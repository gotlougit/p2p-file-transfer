//implements client object which is capable of handling one file from one server
use memmap2::MmapMut;
use std::collections::HashMap;
use std::fs::{remove_file, File, OpenOptions};
use std::io::{stdin, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::process::exit;

use crate::connection;
use crate::parsing;
use crate::parsing::{ClientState, PrimitiveMessage};

use tracing::{debug, error, info, warn};

pub struct Client {
    connection: connection::Connection,
    file: File,
    filename: String,
    authtoken: String,
    filesize: usize,
    state: ClientState,
    counter: usize,
    packets_left: HashMap<usize, bool>,
    packet_cache: HashMap<usize, Vec<u8>>,
    server: SocketAddr,
}

pub fn init(
    conn: connection::Connection,
    file_to_get: &String,
    authtoken: &String,
    server: SocketAddr,
) -> Client {
    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(file_to_get)
        .unwrap();

    let client_obj = Client {
        connection: conn,
        file: fd,
        filename: file_to_get.to_string(),
        authtoken: authtoken.to_string(),
        filesize: 0,
        state: ClientState::NoState,
        counter: 0,
        packets_left: HashMap::new(),
        packet_cache: HashMap::new(),
        server,
    };
    client_obj
}

//one object which spins up tasks depending on what stage the server is at (first connection,
//deciding to get file, receiving file etc)
impl Client {
    //initialize connection to server
    pub async fn init_connection(&mut self) {
        info!("Client has new connection to make!");
        let filereq = parsing::send_req(&self.filename, &self.authtoken);
        self.connection.send_to(&self.server, &filereq).await;
        self.state = ClientState::ACKorNACK;
    }

    pub async fn mainloop(&mut self) {
        loop {
            let mut buffer = [0u8; connection::MTU];
            if let Some((amt, src)) = self.connection.reliable_recv(&mut buffer).await {
                //server wants client to resend
                if parsing::parse_primitive(&buffer, amt) == PrimitiveMessage::RESEND {
                    warn!("Server asked for resend!");
                    self.connection.resend_to(&self.server).await;
                    continue;
                }
                if src != self.server {
                    error!("Client received from unknown IP! {}", src);
                    continue;
                }
                //proceed normally
                let continue_with_loop = self.process_msg(buffer, amt).await;
                if !continue_with_loop {
                    info!("Client exiting...");
                    break;
                }
            } else {
                //retry again in next iteration of loop; we have already sent resend request
                continue;
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
    async fn process_msg(&mut self, message: [u8; connection::MTU], size: usize) -> bool {
        if size == 0 {
            //maybe new connection
            match self.state {
                ClientState::NoState => {
                    self.init_connection().await;
                    return true;
                }
                _ => {
                    error!("An error has occurred!");
                    return self.end_connection().await;
                }
            }
        }
        match self.state {
            ClientState::NoState => {
                //new connection
                self.init_connection().await;
                true
            }
            ClientState::ACKorNACK => {
                //ask user whether they want the file or not
                if let Some(fz) = parsing::parse_filesize_packet(&message[..], size) {
                    self.filesize = fz;
                    let decision = self.get_user_decision();
                    //send ACK/NACK
                    if decision {
                        //setup HashMap to keep track of all received packets
                        let max_packets = self.filesize / connection::DATA_SIZE + 1;
                        for i in 0..max_packets {
                            self.packets_left.insert(connection::DATA_SIZE * i, false);
                        }
                        //fix size of file
                        self.file
                            .seek(SeekFrom::Start(self.filesize as u64 - 1))
                            .unwrap();
                        self.file.write_all(&[0]).unwrap();
                        self.file.seek(SeekFrom::Start(0)).unwrap();

                        debug!("Sending ACK");
                        self.connection
                            .send_to(&self.server, &parsing::get_primitive(PrimitiveMessage::ACK))
                            .await;
                        self.state = ClientState::SendFile;
                        //send request for first offset
                        self.connection.send_to(&self.server, &parsing::last_received_packet(0)).await;
                        return true;
                    } else {
                        info!("Stopping transfer");
                        self.connection
                            .send_to(
                                &self.server,
                                &parsing::get_primitive(PrimitiveMessage::NACK),
                            )
                            .await;
                        debug!("Sent NACK");
                        //delete open file
                        remove_file(&self.filename).expect("Couldn't remove file!");
                        self.end_connection().await;
                        return false;
                    }
                }
                false
            }
            ClientState::SendFile => {
                println!("Client has to receive the file");
                if parsing::parse_primitive(&message[..], size) == PrimitiveMessage::END
                    && self.packets_left.is_empty()
                {
                    info!("END received...");
                    self.end_connection().await;
                    false
                } else {
                    self.save_data_to_file(message, size).await;
                    true
                }
            }
            ClientState::EndConn => {
                info!("Client has received file completely...");
                self.end_connection().await
            }
            ClientState::EndedConn => {
                //do nothing
                self.end_connection().await
            }
        }
    }

    //writes to file from packets in cache and clears map
    fn write_to_file(&mut self) -> usize {
        debug!("Writing packet_cache to file...");
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
        self.state = ClientState::EndConn;
        self.connection
            .send_to(&self.server, &parsing::get_primitive(PrimitiveMessage::END))
            .await;
        //write remaining packets in packet_cache
        self.write_to_file();
        //the end
        info!("Ending Client object...");
        exit(0);
    }

    async fn save_data_to_file(&mut self, message: [u8; connection::MTU], size: usize) {
        if let Some((offset, data)) = parsing::parse_data_packet(&message[..], size) {
            if self.packets_left.get(&offset).is_none() {
                //already been received, assume we already have it inside memory
                println!("Got already received packet");
            } else {
                //get rid of received packet from HashMap
                self.packets_left.remove(&offset);
                //save to packet_cache
                self.packet_cache.insert(offset, data);
            }
            info!("Need {} more packets", self.packets_left.len());
            self.counter += 1;
            debug!("Received offset {}", offset);
            if self.packets_left.is_empty() {
                info!("Client received entire file, ending...");
                //client received entire file, end connection
                self.end_connection().await;
            } else {
                //keep track of whether we received all PROTOCOL_N packets or not
                //send request for next packet only if this is PROTOCOL_Nth packet
                //else server will automatically assume to resend packets
                let n = self.connection.read_n(&self.server);
                if self.counter != 0 && self.counter % n == 0 {
                    self.counter = 0;
                    //write to file in batches only
                    let last = self.write_to_file();
                    self.connection
                        .send_to(&self.server, &parsing::last_received_packet(last))
                        .await;
                }
            }
        }
    }
}
