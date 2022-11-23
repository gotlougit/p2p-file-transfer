//define both messages that client and server exchange and the interfaces they will use to do so
use std::net::{SocketAddr, ToSocketAddrs};
use std::str;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use stunclient::StunClient;
use tokio::net::UdpSocket;
use tokio::time::timeout;

pub enum ClientState {
    NoState,
    ACKorNACK,
    SendFile,
    EndConn,
}

static LASTMSG: Mutex<Vec<u8>> = Mutex::new(Vec::new());

pub const MAX_WAIT_TIME: Duration = Duration::from_secs(5);

pub async fn init_nat_traversal(socket: Arc<UdpSocket>, other_machine: &String) {
    thread::sleep(MAX_WAIT_TIME);
    let om = &other_machine.to_string()[..other_machine.len() - 1]
        .to_string()
        .to_socket_addrs()
        .unwrap()
        .filter(|x| x.is_ipv4())
        .next()
        .unwrap();

    let x = 5; //number of test packets to send

    let mut connected = false;

    while !connected {
        for _ in 0..x {
            println!("Sending useless message to get firewall to open up...");
            send_to(&socket, &om, &ACK.to_vec()).await;
            println!("Sent useless message to get firewall to open up...");
            let mut buf = [0u8; MTU];
            let f = recv(&socket, &mut buf);
            match timeout(MAX_WAIT_TIME, f).await {
                Ok(_) => {
                    println!("Seemed to get some data from somewhere, perhaps it is other machine");
                    connected = true;
                    println!(
                        "Will send more requests in case other machine's ports are not open yet..."
                    );
                }
                Err(_) => {
                    println!("Did not receive any data, retrying...");
                }
            }
        }
    }

    if connected {
        println!("Seems we are connected to other machine...");
    } else {
        eprintln!("Direct connection was NOT able to be established!");
    }
    thread::sleep(MAX_WAIT_TIME);
}

async fn get_external_info(socket: &UdpSocket, ip: String) -> SocketAddr {
    println!("Internal IP:port is {}", socket.local_addr().unwrap());
    let stun_addr = ip
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

pub async fn get_external_and_nat(socket: Arc<UdpSocket>) {
    let ip1 = get_external_info(&socket, "5.178.34.84:3478".to_string()).await;
    let ip2 = get_external_info(&socket, "stun2.l.google.com:19302".to_string()).await;
    if ip1 == ip2 {
        println!("NAT is easy, can transfer files easily");
    } else {
        eprintln!("NAT is hard! Cannot transfer files over hard NAT!");
    }
}
//some constants defined for convenience

pub const MTU: usize = 1280;
pub const DATA_SIZE: usize = 1000;

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

pub const RESEND: [u8; 6] = *b"RESEND";

pub fn parse_resend(message: [u8; MTU], amt: usize) -> bool {
    if amt == 6 && message[..6] == RESEND {
        return true;
    }
    false
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
fn build_data_packet_header(offset: usize) -> String {
    String::from("OFFSET: ") + &offset.to_string() + "\n"
}

pub fn data_packet(offset: usize, message: &Vec<u8>) -> Vec<u8> {
    let s = build_data_packet_header(offset);
    let mut b1 = s.as_bytes().to_vec();
    b1.extend(message);
    b1
}

//get the required data from the data packet
pub fn parse_data_packet(message: [u8; MTU], amt: usize) -> (usize, Vec<u8>) {
    let mut sizeofheader = 0;
    for i in 0..amt {
        if message[i] == 10 {
            sizeofheader = i + 1;
            break;
        }
    }
    let req = parse_generic_req(message, sizeofheader);
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
    (offset, message[sizeofheader..amt].to_vec())
}

//deal with retransmission of last packet that apparently was never received
pub async fn resend(socket: &UdpSocket) {
    let msg = LASTMSG.lock().unwrap().to_owned();
    send(socket, &msg).await
}

fn set_last_msg(message: &Vec<u8>) {
    let mut t = LASTMSG.lock().expect("Could not acquire lock for LASTMSG");
    if *t != message.to_vec() {
        *t = message.to_vec();
    }
}

//abstractions implemented to later make easier to modify if needed
pub async fn send_to(socket: &UdpSocket, src: &SocketAddr, message: &Vec<u8>) {
    socket
        .send_to(message, src)
        .await
        .expect("protocol.rs: Send request failed!");
    set_last_msg(message);
}

pub async fn send(socket: &UdpSocket, message: &Vec<u8>) {
    socket
        .send(message)
        .await
        .expect("protocol.rs: Send request failed!");
    set_last_msg(message);
}

pub async fn recv(socket: &UdpSocket, buffer: &mut [u8; MTU]) -> (usize, SocketAddr) {
    socket
        .recv_from(&mut buffer[..])
        .await
        .expect("protocol.rs: Failed to receive data!")
}
