//implements client object which is capable of handling one file from one server
use std::fs::{remove_file, File};
use std::io;
use std::io::Write;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task;
use std::collections::HashMap;

use crate::protocol;

pub struct Client {
    socket: Arc<UdpSocket>,
    file: File,
    filename: String,
    file_save_as: String,
    authtoken: String,
    lastpacket: usize,
    filesize: usize,
    state: protocol::ClientState,
    file_in_ram: Vec<u8>,
    counter: usize,
    is_file_written: bool,
    packets_recv : HashMap<usize, bool>
}

pub fn init(
    socket: Arc<UdpSocket>,
    file_to_get: &String,
    filename: &String,
    authtoken: &String,
) -> Arc<Mutex<Client>> {
    let fd = File::create(filename).expect("Couldn't create file!");

    let client_obj = Client {
        socket,
        file: fd,
        filename: file_to_get.to_string(),
        file_save_as: filename.to_string(),
        authtoken: authtoken.to_string(),
        lastpacket: 0,
        filesize: 0,
        state: protocol::ClientState::NoState,
        file_in_ram: Vec::new(),
        counter: 0,
        is_file_written: false,
        packets_recv: HashMap::new(),
    };
    Arc::new(Mutex::new(client_obj))
}

//one object which spins up tasks depending on what stage the server is at (first connection,
//deciding to get file, receiving file etc)
//TODO: write file dynamically to disk/use mmap() instead of keeping file in RAM
impl Client {
    //initialize connection to server

    pub async fn init_connection(&mut self) {
        println!("Client has new connection to make!");
        let filereq = protocol::send_req(&self.filename, &self.authtoken);
        protocol::send(&self.socket, &filereq).await;
        self.state = protocol::ClientState::ACKorNACK;
    }

    fn get_user_decision(&self) -> bool {
        if self.filesize == 0 {
            eprintln!("Server sent file size 0!");
            return false;
        }
        println!("Size of file is: {}", self.filesize);
        println!("Initiate transfer? (Y/N)");
        let stdin = io::stdin();
        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .expect("Couldn't read from STDIN!");
        if input == "Y\n" || input == "\n" {
            return true;
        }
        false
    }

    //pass message received here to determine what to do; action will be taken asynchronously
    pub async fn process_msg(
        &mut self,
        message: [u8; protocol::MTU],
        size: usize,
        selfcopy: Arc<Mutex<Client>>,
    ) -> bool {
        if size == 0 {
            //maybe new connection
            match self.state {
                protocol::ClientState::NoState => {
                    self.init_connection().await;
                    return true;
                }
                _ => {
                    eprintln!("An error has occurred!");
                    return self.end_connection().await;
                }
            }
        }
        match self.state {
            protocol::ClientState::NoState => {
                //new connection
                self.init_connection().await;
                true
            }
            protocol::ClientState::ACKorNACK => {
                //ask user whether they want the file or not
                self.filesize = protocol::parse_filesize_packet(message, size);
                let decision = self.get_user_decision();
                //send ACK/NACK
                if decision {
                    //fix size of vector
                    self.file_in_ram.resize(self.filesize, 0);
                    //setup HashMap to keep track of all received packets
                    let max_packets = self.filesize / protocol::DATA_SIZE + 1;
                    for i in 0..max_packets {
                        self.packets_recv.insert(protocol::DATA_SIZE * i, false);
                    }
                    println!("Sending ACK");
                    protocol::send(&self.socket, protocol::ACK.as_ref()).await;
                    self.state = protocol::ClientState::SendFile;
                    true
                } else {
                    println!("Stopping transfer");
                    protocol::send(&self.socket, protocol::NACK.as_ref()).await;
                    println!("Sent NACK");
                    //delete open file
                    remove_file(&self.file_save_as).expect("Couldn't remove file!");
                    self.end_connection().await;
                    false
                }
            }
            protocol::ClientState::SendFile => {
                println!("Client has to receive the file");
                if protocol::parse_end(message, size) && self.packets_recv.len() == 0 {
                    println!("END received...");
                    self.end_connection().await;
                    false
                } else {
                    task::spawn(async move {
                        selfcopy.lock().await.save_data_to_file(message, size).await
                    });
                    true
                }
            }
            protocol::ClientState::EndConn => {
                println!("Client has received file completely...");
                self.end_connection().await
            }
            protocol::ClientState::EndedConn => {
                //do nothing
                self.end_connection().await
            }
        }
    }

    async fn end_connection(&mut self) -> bool {
        self.state = protocol::ClientState::EndConn;
        //write file all at once
        if !self.is_file_written && self.file.write_all(&self.file_in_ram).is_err() {
            eprintln!("Error! File write failed!");
        }
        self.is_file_written = true;
        protocol::send(&self.socket, protocol::END.as_ref()).await;
        //the end
        println!("Ending Client object...");
        false
    }

    async fn save_data_to_file(&mut self, message: [u8; protocol::MTU], size: usize) {
        let (offset, data) = protocol::parse_data_packet(message, size);
        self.counter += 1;
        if self.packets_recv.get(&offset).is_none() { //already been received
            println!("Got already received packet, ignoring...");
            return;
        }
        //get rid of received packet
        self.packets_recv.remove(&offset);
        //copy data over to file_in_ram
        self.file_in_ram[offset..offset + data.len()].copy_from_slice(&data[..]);
        println!("Received offset {}", offset);
        self.lastpacket = offset;

        if self.lastpacket >= self.filesize {
            println!("Client received entire file, ending...");
            //client received entire file, end connection
            self.end_connection().await;
            return;
        } else {
            //keep track of whether we received all PROTOCOL_N packets or not
            //send request for next packet only if this is PROTOCOL_Nth packet
            //else server will automatically assume to resend packets
            if self.counter != 0 && self.counter % protocol::PROTOCOL_N == 0 {
                self.counter = 0;
                self.lastpacket += data.len();
                protocol::send(
                    &self.socket,
                    &protocol::last_received_packet(self.lastpacket),
                )
                .await;
            }
        }
    }
}
