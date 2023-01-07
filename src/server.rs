//implements server object which is capable of handling multiple clients at once
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::auth;
use crate::protocol;
use crate::protocol::ClientState;

pub struct Server {
    socket: Arc<UdpSocket>,
    data: Mmap,
    size_msg: Vec<u8>,
    src_state_map: HashMap<SocketAddr, ClientState>,
    authchecker: auth::AuthChecker,
}

pub fn init(socket: Arc<UdpSocket>, filename: String, authtoken: String) -> Server {
    let fd = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(&filename)
        .unwrap();
    unsafe {
        let mmap = Mmap::map(&fd).unwrap();
        let filesize = mmap.len();
        let server_obj = Server {
            socket,
            data: mmap,
            size_msg: protocol::filesize_packet(filesize),
            src_state_map: HashMap::new(),
            authchecker: auth::init(authtoken, filename),
        };
        server_obj
    }
}

//one object which spins up tasks depending on what stage the client is at (first connection,
//deciding to get file, receiving file etc)
//TODO: have server be able to serve multiple files on demand
impl Server {
    pub async fn mainloop(&mut self) {
        loop {
            let mut buf = [0u8; protocol::MTU];
            if let Ok((amt, src)) = timeout(
                protocol::MAX_WAIT_TIME,
                protocol::recv(&self.socket, &mut buf),
            )
            .await
            {
                if protocol::parse_resend(buf, amt) {
                    println!("Need to resend!");
                    protocol::resend(&self.socket).await;
                } else {
                    self.process_msg(&src, buf, amt).await;
                }
            } else {
                println!("Timeout occurred, asking all clients to resend!");
                //ask all clients for resend
                self.ask_all_to_resend().await;
            }
        }
    }

    //pass message received here to determine what to do; action will be taken asynchronously
    async fn process_msg(&mut self, src: &SocketAddr, message: [u8; protocol::MTU], amt: usize) {
        if self.src_state_map.contains_key(src) {
            println!("Found prev connection, checking state and handling corresponding call..");
            if protocol::parse_resend(message, amt) {
                println!("Client may not have received last part of file! Sending last chunk...");
                let n = protocol::read_n();
                for i in 0..n {
                    let offset: usize = self.data.len() - protocol::DATA_SIZE * (n - i);
                    let mut len: usize = protocol::DATA_SIZE;
                    if offset + len > self.data.len() {
                        len = self.data.len() - offset;
                    }
                    let data =
                        protocol::data_packet(offset, &self.data[offset..offset + len].to_vec());
                    protocol::send_to(&self.socket, src, &data).await;
                }
            }
            //we are already communicating with client
            if let Some(curstate) = self.src_state_map.get(src) {
                match curstate {
                    ClientState::NoState => {
                        self.initiate_transfer_server(&src, message, amt).await;
                    }
                    ClientState::ACKorNACK => {
                        self.check_ack_or_nack(&src, message, amt).await;
                    }
                    ClientState::SendFile => {
                        self.send_data_in_chunks(&src, message, amt).await;
                    }
                    ClientState::EndConn => {
                        //don't make a new thread for this
                        if protocol::parse_end(message, amt) {
                            self.end_connection(src).await;
                            return;
                        }
                        self.end_connection_with_resend(src).await;
                    }
                    ClientState::EndedConn => {
                        if protocol::parse_end(message, amt) {
                            self.end_connection(src).await;
                            return;
                        }
                        if protocol::parse_resend(message, amt) {
                            println!("Client may not have received last part of file! Sending last chunk...");
                            let n = protocol::read_n();
                            for i in 0..n {
                                let offset: usize = self.data.len() - protocol::DATA_SIZE * (n - i);
                                let mut len: usize = protocol::DATA_SIZE;
                                if offset + len > self.data.len() {
                                    len = self.data.len() - offset;
                                }
                                let data = protocol::data_packet(
                                    offset,
                                    &self.data[offset..offset + len].to_vec(),
                                );
                                protocol::send_to(&self.socket, src, &data).await;
                            }
                        } else {
                            self.send_data_in_chunks(&src, message, amt).await;
                        }
                    }
                }
            }
        } else {
            println!("New connection detected, adding to the list..");
            //TODO: implement authentication check here
            self.src_state_map.insert(*src, ClientState::NoState); //start from scratch
            self.initiate_transfer_server(src, message, amt).await; //initialize function
        }
    }

    pub async fn ask_all_to_resend(&self) {
        for src in self.src_state_map.keys() {
            protocol::ask_for_resend(&self.socket, src).await;
        }
    }

    fn change_src_state(&mut self, src: &SocketAddr, newstate: ClientState) {
        if self.src_state_map.remove(src).is_some() {
            self.src_state_map.insert(*src, newstate);
        }
    }

    async fn end_connection(&mut self, src: &SocketAddr) {
        println!("Sending END (permanent end to connection) to {}", src);
        protocol::send_to(&self.socket, src, protocol::END.as_ref()).await;
        self.src_state_map.remove(src);
    }

    async fn end_connection_with_resend(&mut self, src: &SocketAddr) {
        println!("Sending END (with resend allowed) to {}", src);
        protocol::send_to(&self.socket, src, protocol::END.as_ref()).await;
        self.change_src_state(src, ClientState::EndedConn);
    }

    async fn initiate_transfer_server(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        amt: usize,
    ) {
        if self.authchecker.is_valid_request(message, amt) {
            //send size of data
            println!("Client authentication check succeeded...");
            println!("Sending client size of file");
            protocol::send_to(&self.socket, src, &self.size_msg).await;
            println!("Awaiting response from client...");
            self.change_src_state(src, ClientState::ACKorNACK);
        } else {
            //send dummy message as client failed to authenticate
            println!("Client was not able to be authenticated!");
            println!("Sending 0 size file...");
            protocol::send_to(&self.socket, src, &protocol::filesize_packet(0)).await;
            //end connection
            self.end_connection(src).await;
        }
    }

    async fn check_ack_or_nack(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        amt: usize,
    ) {
        if protocol::parse_ack(message, amt) {
            println!("Client sent ACK");
            self.change_src_state(src, ClientState::SendFile);
            //for now we can do this directly
            self.send_data_in_chunks(src, message, amt).await;
        } else if protocol::parse_nack(message, amt) {
            println!("Client sent NACK");
            self.change_src_state(src, ClientState::EndConn);
            //directly do this since connection needs to be closed anyway
            self.end_connection(src).await;
        } else {
            println!("Client sent unknown message type for its stage! Ignoring");
        }
    }

    async fn send_one_chunk(&mut self, src: &SocketAddr, offset: usize) {
        if offset >= self.data.len() {
            return;
        }
        if offset + protocol::DATA_SIZE < self.data.len() {
            let packet = self.data[offset..offset + protocol::DATA_SIZE].to_vec();
            //send DATA_SIZE size chunk
            println!("Sending a chunk...");
            protocol::send_to(&self.socket, &src, &protocol::data_packet(offset, &packet)).await;
        } else {
            let packet =
                protocol::data_packet(offset, &self.data[offset..self.data.len()].to_vec());
            protocol::send_to(&self.socket, src, &packet).await;
            println!("File sent completely");
            self.end_connection_with_resend(src).await;
        }
    }

    async fn send_data_in_chunks(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        amt: usize,
    ) {
        if protocol::parse_end(message, amt) {
            self.end_connection(src).await;
            return;
        }
        let mut offset = protocol::parse_last_received(message, amt);
        if offset >= self.data.len() {
            //send an END packet
            self.end_connection_with_resend(src).await;
            return;
        }
        //send PROTOCOL_N number of chunks at once and implement go back N if they have not been received
        let n = protocol::read_n();
        for _ in 0..n {
            self.send_one_chunk(src, offset).await;
            offset += protocol::DATA_SIZE;
        }
    }
}
