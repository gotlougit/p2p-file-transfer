//implements server object which is capable of handling multiple clients at once
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task;

use crate::protocol;

pub struct Server {
    socket: Arc<UdpSocket>,
    data: Arc<Vec<u8>>,
    size_msg: Vec<u8>,
    dummy_size_msg: Vec<u8>,
    src_state_map: HashMap<SocketAddr, u8>,
}

pub fn init(socket: Arc<UdpSocket>, data: Arc<Vec<u8>>) -> Arc<Mutex<Server>> {
    let filesize = data.len();
    let server_obj = Server {
        socket,
        data,
        size_msg: protocol::filesize_packet(filesize),
        dummy_size_msg: protocol::filesize_packet(0),
        src_state_map: HashMap::new(),
    };
    Arc::new(Mutex::new(server_obj))
}

impl Server {
    //pass message received here to determine what to do; action will be taken asynchronously
    pub async fn process_msg(
        &mut self,
        src: &SocketAddr,
        message: [u8; protocol::MTU],
        selfcopy: Arc<Mutex<Server>>,
    ) {
        if self.src_state_map.contains_key(src) {
            println!("Found prev connection, checking state and handling corresponding call..");
            //we are already communicating with client
            if let Some(&curstate) = self.src_state_map.get(src) {
                let srcclone = src.clone();
                if curstate == 0 as u8 {
                    task::spawn(async move {
                        println!("Running initial server response...");
                        let _ = selfcopy
                            .lock()
                            .await
                            .initiate_transfer_server(&srcclone)
                            .await;
                    });
                } else if curstate == 1 as u8 {
                    task::spawn(async move {
                        println!("Checking ACK or NACK...");
                        let _ = selfcopy
                            .lock()
                            .await
                            .check_ack_or_nack(&srcclone, message)
                            .await;
                    });
                } else if curstate == 2 as u8 {
                    task::spawn(async move {
                        println!("Sending data in chunks...");
                        let _ = selfcopy.lock().await.send_data_in_chunks(&srcclone).await;
                    });
                } else if curstate == 3 as u8 {
                    //don't make a new thread for this
                    selfcopy.lock().await.end_connection(&src);
                }
            }
        } else {
            println!("New connection detected, adding to the list..");
            //TODO: implement authentication check here
            self.src_state_map.insert(*src, 0); //start from scratch
            self.initiate_transfer_server(&src).await; //initialize function
        }
    }

    fn change_src_state(&mut self, src: &SocketAddr, newstate: u8) {
        if let Some(_v) = self.src_state_map.remove(src) {
            self.src_state_map.insert(*src, newstate);
        }
    }

    fn end_connection(&mut self, src: &SocketAddr) {
        println!("Ending connection with {}", src);
        self.src_state_map.remove(src);
    }

    async fn initiate_transfer_server(&mut self, src: &SocketAddr) {
        //send size of data
        println!("Sending client size of file");
        protocol::send_to(&self.socket, &src, &self.size_msg).await;
        println!("Awaiting response from client..");
        self.change_src_state(src, 1);
    }

    async fn check_ack_or_nack(&mut self, src: &SocketAddr, message: [u8; protocol::MTU]) {
        if message[..3] == protocol::ACK {
            println!("Client sent ACK");
            self.change_src_state(src, 2);
            //for now we can do this directly
            let _ = self.send_data_in_chunks(&src).await;
        } else {
            println!("Client sent NACK");
            self.change_src_state(src, 3);
            //directly do this since connection needs to be closed anyway
            self.end_connection(&src);
        }
    }

    async fn send_data_in_chunks(&mut self, src: &SocketAddr) {
        println!("Sending data in chunks...");
        let mut start: usize = 0;
        //send file in chunks at first
        while start + protocol::MTU < self.data.len() {
            protocol::send_to(
                &self.socket,
                &src,
                &self.data[start..start + protocol::MTU].to_vec(),
            )
            .await;
            start += protocol::MTU;
        }
        //when last chunk is smaller than protocol::MTU, just send remaining data
        protocol::send_to(
            &self.socket,
            &src,
            &self.data[start..self.data.len()].to_vec(),
        )
        .await;
        self.end_connection(&src);
    }
}
