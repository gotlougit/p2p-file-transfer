//define both messages that client and server exchange and the interfaces they will use to do so
use std::net::SocketAddr;
use std::str;
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
pub const NACK: [u8; 4] = *b"NACK";
pub const END: [u8; 3] = *b"END";

//initial request client sends; consists of auth token and name of file to get
pub fn send_req(filename: &String, auth: &String) -> Vec<u8> {
    let r = String::from("AUTH ") + auth + &String::from("\nGET ") + filename;
    r.as_bytes().to_vec()
}

pub fn parse_send_req(message: [u8; MTU]) -> (String, String) {
    let req = String::from(
        str::from_utf8(&message).expect("protocol.rs: Couldn't write buffer as string"),
    );

    let file_requested = match req.split("GET ").collect::<Vec<&str>>().get(1) {
        Some(x) => x.to_string(),
        None => String::from(""),
    };

    let giventoken = match req.split("AUTH ").collect::<Vec<&str>>().get(1) {
        Some(x) => x.to_string(),
        None => String::from(""),
    };

    (file_requested, giventoken)
}

//actual message server sends with filesize
pub fn filesize_packet(filesize: usize) -> Vec<u8> {
    let s = String::from("SIZE ") + &filesize.to_string();
    s.as_bytes().to_vec()
}

//tell server which packet has been last received
pub fn last_received_packet(num: usize) -> Vec<u8> {
    let s = String::from("LAST ") + &num.to_string() + &String::from("\n");
    s.as_bytes().to_vec()
}

//builds data packet encapsulated with required info about where packet actually goes
pub fn data_packet(offset: usize, message: &Vec<u8>) -> Vec<u8> {
    let s = String::from("OFFSET: ")
        + &offset.to_string()
        + "\n"
        + &String::from("SIZE: ")
        + &message.len().to_string()
        + "\n";
    let mut b1 = s.as_bytes().to_vec();
    b1.extend(message);
    b1
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
