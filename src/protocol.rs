//define both messages that client and server exchange and the interfaces they will use to do so
use std::net::{SocketAddr, ToSocketAddrs};
use std::str;
use stunclient::StunClient;
use tokio::net::UdpSocket;

pub enum ClientState {
    NoState,
    ACKorNACK,
    SendFile,
    EndConn,
}

//some constants defined for convenience

pub const MTU: usize = 1280;

//important messages implemented as constants
pub const ACK: [u8; 3] = *b"ACK";

pub fn parse_ack(message: [u8; MTU], amt: usize) -> bool {
    if amt == 3 && message[..3] == ACK {
        return true;
    }
    false
}

pub const NACK: [u8; 4] = *b"NACK";

pub fn parse_nack(message: [u8; MTU], amt: usize) -> bool {
    if amt == 4 && message[..4] == NACK {
        return true;
    }
    false
}

pub const END: [u8; 3] = *b"END";

pub fn parse_end(message: [u8; MTU], amt: usize) -> bool {
    if amt == 3 && message[..3] == END {
        return true;
    }
    false
}

pub async fn get_external(socket: &UdpSocket) -> SocketAddr {
    let stun_addr = "5.178.34.84:3478"
        .to_socket_addrs()
        .unwrap()
        .filter(|x| x.is_ipv4())
        .next()
        .unwrap();
    let c = StunClient::new(stun_addr);
    let f = c.query_external_address_async(socket).await;
    match f {
        Ok(x) => {
            println!("Program is externally at: {}", x);
            x
        }
        Err(_) => {
            println!("Error at protocol.rs: STUN");
            socket.local_addr().unwrap()
        }
    }
}

fn parse_generic_req(message: [u8; MTU], amt: usize) -> String {
    let req = match str::from_utf8(&message[..amt]) {
        Ok(x) => x.to_string(),
        Err(_) => {
            println!("Parse error detected, returning empty string...");
            String::from("")
        }
    };
    req
}

//initial request client sends; consists of auth token and name of file to get
pub fn send_req(filename: &String, auth: &String) -> Vec<u8> {
    let r = String::from("AUTH ") + auth + &String::from("\nGET ") + filename;
    r.as_bytes().to_vec()
}

pub fn parse_send_req(message: [u8; MTU], amt: usize) -> (String, String) {
    let req = parse_generic_req(message, amt);

    if req.is_empty() {
        //invalid request
        return (String::from(""), String::from(""));
    }

    let file_requested = match req.split("GET ").collect::<Vec<&str>>().get(1) {
        Some(x) => x.to_string(),
        None => String::from(""),
    };

    let giventoken = match req.split("AUTH ").collect::<Vec<&str>>().get(1) {
        Some(x) => match x.to_string().split('\n').collect::<Vec<&str>>().first() {
            Some(y) => y.to_string(),
            None => String::from(""),
        },
        None => String::from(""),
    };

    (file_requested, giventoken)
}

//actual message server sends with filesize
pub fn filesize_packet(filesize: usize) -> Vec<u8> {
    let s = String::from("SIZE ") + &filesize.to_string();
    s.as_bytes().to_vec()
}

pub fn parse_filesize_packet(message: [u8; MTU], amt: usize) -> usize {
    let req = parse_generic_req(message, amt);
    if req.is_empty() {
        return 0;
    }
    let size = match req.split("SIZE ").collect::<Vec<&str>>().get(1) {
        Some(x) => x.to_string().parse::<usize>().unwrap(),
        None => 0,
    };
    size
}

//tell server which packet has been last received
pub fn last_received_packet(num: usize) -> Vec<u8> {
    let s = String::from("LAST ") + &num.to_string();
    s.as_bytes().to_vec()
}

pub fn parse_last_received(message: [u8; MTU], amt: usize) -> usize {
    if parse_ack(message, amt) {
        return 0;
    }
    let req = parse_generic_req(message, amt);
    if req.is_empty() {
        return 0;
    }
    let lastrecv = match req.split("LAST ").collect::<Vec<&str>>().get(1) {
        Some(x) => x.to_string().parse::<usize>().unwrap(),
        None => 0,
    };
    lastrecv
}

//builds data packet encapsulated with required info about where packet actually goes
fn build_data_packet_header(offset: usize, size: usize) -> String {
    String::from("OFFSET: ")
        + &offset.to_string()
        + "\n"
        + &String::from("SIZE: ")
        + &size.to_string()
        + "\n"
}

pub fn data_packet(offset: usize, message: &Vec<u8>) -> Vec<u8> {
    let s = build_data_packet_header(offset, message.len());
    let mut b1 = s.as_bytes().to_vec();
    b1.extend(message);
    b1
}

pub fn parse_data_packet(message: [u8; MTU], amt: usize) -> (usize, Vec<u8>) {
    let req = parse_generic_req(message, amt);
    if req.is_empty() {
        let emptyvector: Vec<u8> = Vec::new();
        return (0, emptyvector);
    }
    let offset = match req.split("OFFSET: ").collect::<Vec<&str>>().get(1) {
        Some(x) => match x.to_string().split('\n').collect::<Vec<&str>>().first() {
            Some(y) => y.to_string().parse::<usize>().unwrap(),
            None => 0,
        },
        None => 0,
    };
    let size = match req.split("SIZE: ").collect::<Vec<&str>>().get(1) {
        Some(x) => match x.to_string().split('\n').collect::<Vec<&str>>().first() {
            Some(y) => y.to_string().parse::<usize>().unwrap(),
            None => 0,
        },
        None => 0,
    };

    let sizeofheader = build_data_packet_header(offset, size).len();
    (offset, message[sizeofheader..sizeofheader + size].to_vec())
}

//abstractions implemented to later make easier to modify if needed
pub async fn send_to(socket: &UdpSocket, src: &SocketAddr, message: &Vec<u8>) {
    socket
        .send_to(message, src)
        .await
        .expect("protocol.rs: Send request failed!");
}

pub async fn send(socket: &UdpSocket, message: &Vec<u8>) {
    socket
        .send(message)
        .await
        .expect("protocol.rs: Send request failed!");
}

pub async fn recv(socket: &UdpSocket, buffer: &mut [u8; MTU]) -> (usize, SocketAddr) {
    socket
        .recv_from(&mut buffer[..])
        .await
        .expect("protocol.rs: Failed to receive data!")
}
