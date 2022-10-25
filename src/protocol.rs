//define both messages that client and server exchange and the interfaces they will use to do so
use std::net::SocketAddr;
use tokio::net::UdpSocket;

//some constants defined for convenience

pub const MTU: usize = 1280;

//two important messages implemented as constants
pub const ACK: [u8; 3] = *b"ACK";
pub const NACK: [u8; 4] = *b"NACK";

//initial request client sends; consists of auth token and name of file to get
pub fn send_req(filename: &String, auth: &String) -> Vec<u8> {
    let r = String::from("AUTH ") + auth + &String::from("\nGET ") + filename;
    r.as_bytes().to_vec()
}

//actual message server sends with filesize
pub fn filesize_packet(filesize: usize) -> Vec<u8> {
    let s = String::from("SIZE ") + &filesize.to_string();
    s.as_bytes().to_vec()
}

//abstractions implemented to later make easier to modify if needed
pub async fn send_to(socket: &UdpSocket, src: &SocketAddr, message: &Vec<u8>) {
    socket
        .send_to(&message, src)
        .await
        .expect("protocol.rs: Send request failed!");
}

pub async fn send(socket: &UdpSocket, message: &Vec<u8>) {
    socket
        .send(&message)
        .await
        .expect("protocol.rs: Send request failed!");
}

pub async fn recv(socket: &UdpSocket, buffer: &mut [u8; MTU]) -> (usize, SocketAddr) {
    socket
        .recv_from(&mut buffer[..])
        .await
        .expect("protocol.rs: Failed to receive data!")
}
