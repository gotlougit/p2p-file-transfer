//implements server object which is capable of handling multiple clients at once
use std::fs::{remove_file, File};
use std::io;
use std::io::Write;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
//use tokio::task;

use crate::protocol;

pub struct Client {
    socket: Arc<UdpSocket>,
    file: Arc<Mutex<File>>,
    filename: String,
    file_save_as: String,
    authtoken: String,
    lastpacket: usize,
    filesize: usize,
    state: protocol::ClientState,
}

pub fn init(
    socket: Arc<UdpSocket>,
    file_to_get: &String,
    filename: &String,
    authtoken: &String,
) -> Arc<Mutex<Client>> {
    let fd = Arc::new(Mutex::new(
        File::create(&filename).expect("Couldn't create file!"),
    ));

    let client_obj = Client {
        socket,
        file: fd,
        filename: file_to_get.to_string(),
        file_save_as: filename.to_string(),
        authtoken: authtoken.to_string(),
        lastpacket: 0,
        filesize: 0,
        state: protocol::ClientState::NoState,
    };
    Arc::new(Mutex::new(client_obj))
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
                    return self.end_connection();
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
                    println!("Sending ACK");
                    protocol::send(&self.socket, &protocol::ACK.to_vec()).await;
                    println!("Sent ACK");
                    self.state = protocol::ClientState::SendFile;
                    true
                } else {
                    println!("Stopping transfer");
                    protocol::send(&self.socket, &protocol::NACK.to_vec()).await;
                    println!("Sent NACK");
                    self.state = protocol::ClientState::EndConn;
                    //delete open file
                    remove_file(&self.file_save_as).expect("Couldn't remove file!");
                    self.end_connection();
                    false
                }
            }
            protocol::ClientState::SendFile => {
                println!("Client has to receive the file");
                /*
                task::spawn(async move {
                    selfcopy.lock().await.save_data_to_file(message, size).await
                });
                */
                self.save_data_to_file(message, size).await;
                true
            }
            protocol::ClientState::EndConn => {
                println!("Client has received file completely...");
                self.end_connection()
            }
        }
    }

    fn end_connection(&mut self) -> bool {
        //the end
        println!("Ending Client object...");
        false
    }

    async fn save_data_to_file(&mut self, message: [u8; protocol::MTU], size: usize) {
        if size < protocol::MTU {
            match self.file.lock().await.write_all(&message[..size]) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}", e),
            };
            self.state = protocol::ClientState::EndConn;
            self.end_connection();
        } else {
            match self.file.lock().await.write_all(&message) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}", e),
            };
            println!(
                "Sending server msg that we have received offset {}",
                self.lastpacket
            );
            self.lastpacket += protocol::MTU;
            protocol::send(
                &self.socket,
                &protocol::last_received_packet(self.lastpacket),
            )
            .await;
        }
    }
}
