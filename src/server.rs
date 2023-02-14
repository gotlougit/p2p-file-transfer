//implements server object which is capable of handling multiple clients at once
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::net::SocketAddr;
use tracing::{debug, error, info, warn};

use crate::auth;
use crate::connection;
use crate::connection::change_map_value;
use crate::parsing;
use crate::parsing::{ClientState, PrimitiveMessage};
use crate::socket::Socket;

pub struct Server<T: Socket> {
    connection: connection::Connection<T>,
    data: Mmap,
    size_msg: Vec<u8>,
    src_state_map: HashMap<SocketAddr, ClientState>,
    authchecker: auth::AuthChecker,
}

pub fn init<T: Socket>(
    connection: connection::Connection<T>,
    filename: String,
    authtoken: String,
) -> Server<T> {
    let fd = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(&filename)
        .unwrap();
    unsafe {
        let mmap = Mmap::map(&fd).unwrap();
        let filesize = mmap.len();
        Server {
            connection,
            data: mmap,
            size_msg: parsing::filesize_packet(filesize),
            src_state_map: HashMap::new(),
            authchecker: auth::init(authtoken, filename),
        }
    }
}

//one object which spins up tasks depending on what stage the client is at (first connection,
//deciding to get file, receiving file etc)
//TODO: have server be able to serve multiple files on demand
impl<T: Socket> Server<T> {
    pub async fn mainloop(&mut self) {
        loop {
            let mut buffer = [0u8; connection::MTU];
            if let Some((amt, src)) = self.connection.reliable_recv(&mut buffer).await {
                //client wants server to resend
                if parsing::parse_primitive(&buffer, amt) == PrimitiveMessage::RESEND {
                    warn!("Client asked for resend!");
                    match self.src_state_map.get(&src) {
                        Some(state) => {
                            if *state == ClientState::SendFile {
                                //ask client to resend, this tricks it into sending the last packet size
                                self.connection
                                    .send_to(
                                        &src,
                                        &parsing::get_primitive(PrimitiveMessage::RESEND),
                                    )
                                    .await;
                            } else {
                                self.connection.resend_to(&src).await;
                            }
                        }
                        _ => self.connection.resend_to(&src).await,
                    }
                } else {
                    //proceed normally
                    self.process_msg(&src, buffer, amt).await;
                }
            }
            //retry again in next iteration of loop; we have already sent resend request
        }
    }

    //pass message received here to determine what to do; action will be taken asynchronously
    async fn process_msg(&mut self, src: &SocketAddr, message: [u8; connection::MTU], amt: usize) {
        if let Some(curstate) = self.src_state_map.get(src) {
            debug!("Found prev connection, checking state and handling corresponding call..");
            //we are already communicating with client
            match curstate {
                ClientState::NoState => {
                    self.initiate_transfer_server(src, message, amt).await;
                }
                ClientState::ACKorNACK => {
                    self.check_ack_or_nack(src, message, amt).await;
                }
                ClientState::SendFile => {
                    self.send_data_in_chunks(src, message, amt).await;
                }
                ClientState::EndConn => {
                    if parsing::parse_primitive(&message[..], amt) == PrimitiveMessage::END {
                        self.end_connection(src).await;
                        return;
                    }
                    self.end_connection_with_resend(src).await;
                }
                ClientState::EndedConn => {
                    match parsing::parse_primitive(&message[..], amt) {
                        PrimitiveMessage::END => self.end_connection(src).await,
                        PrimitiveMessage::RESEND => {
                            //doubtful this ever executes
                            warn!("Client may not have received last part of file! Sending last chunk...");
                            let n = self.connection.read_n(src);
                            let offset = self.data.len() - connection::DATA_SIZE * n;
                            self.send_n_chunks(src, offset).await;
                        }
                        _ => self.send_data_in_chunks(src, message, amt).await,
                    }
                }
            }
        } else {
            info!("New connection detected, adding to the list..");
            //TODO: implement authentication check here
            self.src_state_map.insert(*src, ClientState::NoState); //start from scratch
            self.initiate_transfer_server(src, message, amt).await; //initialize function
        }
    }

    async fn end_connection(&mut self, src: &SocketAddr) {
        info!("Sending END (permanent end to connection) to {}", src);
        self.connection
            .send_to(src, &parsing::get_primitive(PrimitiveMessage::END))
            .await;
        self.src_state_map.remove(src);
    }

    async fn end_connection_with_resend(&mut self, src: &SocketAddr) {
        info!("Sending END (with resend allowed) to {}", src);
        self.connection
            .send_to(src, &parsing::get_primitive(PrimitiveMessage::END))
            .await;
        change_map_value::<SocketAddr, ClientState>(
            &mut self.src_state_map,
            *src,
            ClientState::EndedConn,
        );
    }

    async fn initiate_transfer_server(
        &mut self,
        src: &SocketAddr,
        message: [u8; connection::MTU],
        amt: usize,
    ) {
        if self.authchecker.is_valid_request(&message[..], amt) {
            //send size of data
            info!("Client authentication check succeeded...");
            debug!("Sending client size of file");
            self.connection.send_to(src, &self.size_msg).await;
            info!("Awaiting response from client...");
            change_map_value::<SocketAddr, ClientState>(
                &mut self.src_state_map,
                *src,
                ClientState::ACKorNACK,
            );
        } else {
            //send dummy message as client failed to authenticate
            error!("Client was not able to be authenticated!");
            debug!("Sending 0 size file...");
            self.connection
                .send_to(src, &parsing::filesize_packet(0))
                .await;
            //end connection
            self.end_connection(src).await;
        }
    }

    async fn check_ack_or_nack(
        &mut self,
        src: &SocketAddr,
        message: [u8; connection::MTU],
        amt: usize,
    ) {
        match parsing::parse_primitive(&message[..], amt) {
            PrimitiveMessage::ACK => {
                debug!("Client sent ACK");
                change_map_value::<SocketAddr, ClientState>(
                    &mut self.src_state_map,
                    *src,
                    ClientState::SendFile,
                );
            }
            PrimitiveMessage::NACK => {
                debug!("Client sent NACK");
                change_map_value::<SocketAddr, ClientState>(
                    &mut self.src_state_map,
                    *src,
                    ClientState::EndConn,
                );
                self.end_connection(src).await;
            }
            _ => {
                warn!("Client sent unknown message type for its stage! Ignoring");
            }
        }
    }

    async fn send_one_chunk(&mut self, src: &SocketAddr, offset: usize) {
        if offset >= self.data.len() {
            return;
        }
        let mut len = connection::DATA_SIZE;
        let mut end_afterwards = false;
        if offset + len > self.data.len() {
            len = self.data.len() - offset;
            end_afterwards = true;
        }
        debug!("Sending a chunk...");
        let packet = self.data[offset..offset + len].to_vec();
        self.connection
            .send_to(src, &parsing::data_packet(offset, &packet))
            .await;
        if end_afterwards {
            info!("File sent completely");
            self.end_connection_with_resend(src).await;
        }
    }

    async fn send_n_chunks(&mut self, src: &SocketAddr, offset: usize) {
        let mut offset = offset;
        let n = self.connection.read_n(src);
        for _ in 0..n {
            self.send_one_chunk(src, offset).await;
            offset += connection::DATA_SIZE;
        }
    }

    async fn send_data_in_chunks(
        &mut self,
        src: &SocketAddr,
        message: [u8; connection::MTU],
        amt: usize,
    ) {
        if parsing::parse_primitive(&message[..], amt) == PrimitiveMessage::END {
            self.end_connection(src).await;
            return;
        }
        if let Some(offset) = parsing::parse_last_received(&message[..], amt) {
            if offset >= self.data.len() {
                //send an END packet
                self.end_connection_with_resend(src).await;
                return;
            }
            self.send_n_chunks(src, offset).await;
        } else {
            warn!("Ignoring incorrect chunk request");
        }
    }
}
