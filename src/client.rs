//implements client object which is capable of handling one file from one server
use memmap2::MmapMut;
use std::collections::HashMap;
use std::fs::{OpenOptions, remove_file, File};
use std::io::{stdin, Seek, SeekFrom, Write};
use std::process::exit;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::protocol;

pub struct Client {
    socket: Arc<UdpSocket>,
    file: File,
    filename: String,
    authtoken: String,
    filesize: usize,
    state: protocol::ClientState,
    counter: usize,
    packets_recv: HashMap<usize, bool>,
    packet_cache: HashMap<usize, Vec<u8>>,
}

pub fn init(socket: Arc<UdpSocket>, file_to_get: &String, authtoken: &String) -> Client {
    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(file_to_get)
        .unwrap();

    let client_obj = Client {
        socket,
        file: fd,
        filename: file_to_get.to_string(),
        authtoken: authtoken.to_string(),
        filesize: 0,
        state: protocol::ClientState::NoState,
        counter: 0,
        packets_recv: HashMap::new(),
        packet_cache: HashMap::new(),
    };
    client_obj
}

//one object which spins up tasks depending on what stage the server is at (first connection,
//deciding to get file, receiving file etc)
impl Client {
    //initialize connection to server
    pub async fn init_connection(&mut self) {
        println!("Client has new connection to make!");
        let filereq = protocol::send_req(&self.filename, &self.authtoken);
        protocol::send(&self.socket, &filereq).await;
        self.state = protocol::ClientState::ACKorNACK;
    }

    pub async fn mainloop(&mut self) {
        let mut last_recv = true;
        let mut is_connected = false;
        loop {
            if !last_recv && is_connected {
                protocol::resend(&self.socket).await;
                println!("Sent resent packet");
                last_recv = true;
            }

            let mut buf = [0u8; protocol::MTU];
            if let Ok((amt, _)) = timeout(
                protocol::MAX_WAIT_TIME,
                protocol::recv(&self.socket, &mut buf),
            )
            .await
            {
                if protocol::parse_resend(buf, amt) {
                    protocol::resend(&self.socket).await;
                    continue;
                }
                //make sure program exits gracefully
                let continue_with_loop = self.process_msg(buf, amt).await;
                if !continue_with_loop {
                    println!("Client exiting...");
                    break;
                }
                is_connected = true;
            } else {
                if !is_connected {
                    println!("Initial connection request may have been lost! Resending...");
                    self.init_connection().await;
                }
                println!("Client could not receive data in time!");
                last_recv = false;
            }
        }
    }

    fn get_user_decision(&self) -> bool {
        if self.filesize == 0 {
            eprintln!("Server sent file size 0!");
            return false;
        }
        println!("Size of file is: {}", self.filesize);
        println!("Initiate transfer? (Y/N)");
        let stdin = stdin();
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
    async fn process_msg(&mut self, message: [u8; protocol::MTU], size: usize) -> bool {
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
                    //setup HashMap to keep track of all received packets
                    let max_packets = self.filesize / protocol::DATA_SIZE + 1;
                    for i in 0..max_packets {
                        self.packets_recv.insert(protocol::DATA_SIZE * i, false);
                    }
                    //fix size of file
                    self.file
                        .seek(SeekFrom::Start(self.filesize as u64 - 1))
                        .unwrap();
                    self.file.write_all(&[0]).unwrap();
                    self.file.seek(SeekFrom::Start(0)).unwrap();
                    println!("Sending ACK");
                    protocol::send(&self.socket, protocol::ACK.as_ref()).await;
                    self.state = protocol::ClientState::SendFile;
                    true
                } else {
                    println!("Stopping transfer");
                    protocol::send(&self.socket, protocol::NACK.as_ref()).await;
                    println!("Sent NACK");
                    //delete open file
                    remove_file(&self.filename).expect("Couldn't remove file!");
                    self.end_connection().await;
                    false
                }
            }
            protocol::ClientState::SendFile => {
                println!("Client has to receive the file");
                if protocol::parse_end(message, size) && self.packets_recv.is_empty() {
                    println!("END received...");
                    self.end_connection().await;
                    false
                } else {
                    self.save_data_to_file(message, size).await;
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

    //writes to file from packets in cache and clears map
    fn write_to_file(&mut self) -> usize {
        let mut last = 0;
        for (offset, data) in self.packet_cache.iter() {
            last = offset + data.len();
            unsafe {
                let mut mmap = MmapMut::map_mut(&self.file).unwrap();
                mmap[*offset..last].copy_from_slice(&data[..]);
            };
        }
        self.packet_cache.clear();
        last
    }

    async fn end_connection(&mut self) -> bool {
        self.state = protocol::ClientState::EndConn;
        protocol::send(&self.socket, protocol::END.as_ref()).await;
        //write remaining packets in packet_cache
        self.write_to_file();
        //the end
        println!("Ending Client object...");
        exit(0);
    }

    async fn save_data_to_file(&mut self, message: [u8; protocol::MTU], size: usize) {
        let (offset, data) = protocol::parse_data_packet(message, size);
        if self.packets_recv.get(&offset).is_none() {
            //already been received, assume we already have it inside memory
            println!("Got already received packet");
        } else {
            //get rid of received packet from HashMap
            self.packets_recv.remove(&offset);
            //save to packet_cache
            self.packet_cache.insert(offset, data);
        }
        println!("Need {} more packets", self.packets_recv.len());
        self.counter += 1;
        println!("Received offset {}", offset);
        if self.packets_recv.is_empty() {
            println!("Client received entire file, ending...");
            //client received entire file, end connection
            self.end_connection().await;
        } else {
            //keep track of whether we received all PROTOCOL_N packets or not
            //send request for next packet only if this is PROTOCOL_Nth packet
            //else server will automatically assume to resend packets
            if self.counter != 0 && self.counter % protocol::read_n() == 0 {
                self.counter = 0;
                //write to file in batches only
                let last = self.write_to_file();
                protocol::send(&self.socket, &protocol::last_received_packet(last)).await;
            }
        }
    }
}
