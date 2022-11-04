//implements server object which is capable of handling multiple clients at once
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task;

use crate::auth;
use crate::protocol;
use crate::protocol::ClientState;

pub struct Server {
    socket: Arc<UdpSocket>,
    data: Arc<Vec<u8>>,
    size_msg: Vec<u8>,
    dummy_size_msg: Vec<u8>,
    src_state_map: HashMap<SocketAddr, ClientState>,
    authchecker: auth::AuthChecker,
    lastmsgs: HashMap<SocketAddr, Vec<u8>>,
}

pub fn init(
    socket: Arc<UdpSocket>,
    data: Arc<Vec<u8>>,
    filename: String,
    authtoken: String,
) -> Arc<Mutex<Server>> {
    let emptyvec = Vec::<u8>::new();
    let filesize = data.len();
    let server_obj = Server {
        socket,
        data,
        size_msg: protocol::filesize_packet(filesize),
        dummy_size_msg: protocol::filesize_packet(0),
        src_state_map: HashMap::new(),
        authchecker: auth::init(authtoken, filename),
        lastmsgs: HashMap::new(),
    };
    Arc::new(Mutex::new(server_obj))
}

//one object which spins up tasks depending on what stage the client is at (first connection,
//deciding to get file, receiving file etc)
//TODO: have server be able to serve multiple files on demand
//TODO: have server read file on demand instead of keeping a single file around forever in memory
impl Server {
    //pass message received here to determine what to do; action will be taken asynchronously
    pub async fn process_msg(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        amt: usize,
        selfcopy: Arc<Mutex<Server>>,
    ) {
        if self.src_state_map.contains_key(src) {
            println!("Found prev connection, checking state and handling corresponding call..");
            //we are already communicating with client
            if let Some(curstate) = self.src_state_map.get(src) {
                let srcclone = src.clone();
                match curstate {
                    ClientState::NoState => {
                        task::spawn(async move {
                            println!("Running initial server response...");
                            selfcopy
                                .lock()
                                .await
                                .initiate_transfer_server(&srcclone, message, amt)
                                .await;
                        });
                    }
                    ClientState::ACKorNACK => {
                        task::spawn(async move {
                            println!("Checking ACK or NACK...");
                            selfcopy
                                .lock()
                                .await
                                .check_ack_or_nack(&srcclone, message, amt)
                                .await;
                        });
                    }
                    ClientState::SendFile => {
                        task::spawn(async move {
                            println!("Sending data in chunks...");
                            selfcopy
                                .lock()
                                .await
                                .send_data_in_chunks(&srcclone, message, amt)
                                .await;
                        });
                    }
                    ClientState::EndConn => {
                        //don't make a new thread for this
                        selfcopy.lock().await.end_connection(src).await;
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

    pub async fn resend_msg(&mut self, src : &SocketAddr) {
        println!("Resending data...");
        if let Some(msg) = self.lastmsgs.get(src) {
            protocol::send_to(&self.socket, src, msg).await;
        }
    }

    fn change_src_state(&mut self, src: &SocketAddr, newstate: ClientState) {
        if let Some(_v) = self.src_state_map.remove(src) {
            self.src_state_map.insert(*src, newstate);
        }
    }

    fn change_lastmsg(&mut self, src: &SocketAddr, msg : Vec<u8>) {
        if let Some(_v) = self.lastmsgs.remove(src) {
            self.lastmsgs.insert(*src, msg);
        }
    }

    async fn end_connection(&mut self, src: &SocketAddr) {
        println!("Sending END to {}", src);
        protocol::send_to(&self.socket, src, &protocol::END.to_vec()).await;
        self.src_state_map.remove(src);
        self.lastmsgs.remove(src);
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
            self.change_lastmsg(src, self.size_msg.to_vec());
            println!("Awaiting response from client...");
            self.change_src_state(src, ClientState::ACKorNACK);
        } else {
            //send dummy message as client failed to authenticate
            println!("Client was not able to be authenticated!");
            println!("Sending 0 size file...");
            protocol::send_to(&self.socket, src, &self.dummy_size_msg).await;
            self.change_lastmsg(src, self.dummy_size_msg.to_vec());

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
            println!("Client snet unknown message type for it's stage! Ignoring");
        }
    }

    async fn send_data_in_chunks(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        amt: usize,
    ) {
        let offset = protocol::parse_last_received(message, amt);
        if offset + protocol::MTU < self.data.len() {
            //send MTU size chunk
            println!("Sending a chunk...");
            protocol::send_to(
                &self.socket,
                src,
                &self.data[offset..offset + protocol::MTU].to_vec(),
            )
            .await;
            self.change_lastmsg(src, self.data[offset..offset + protocol::MTU].to_vec());
        } else {
            //send remaining data and end connection
            protocol::send_to(
                &self.socket,
                src,
                &self.data[offset..self.data.len()].to_vec(),
            )
            .await;
            self.change_lastmsg(src, self.data[offset..self.data.len()].to_vec());
            println!("File sent completely");
            self.end_connection(src).await;
        }
    }
}
