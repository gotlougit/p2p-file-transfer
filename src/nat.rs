use anyhow::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use stunclient::StunClient;
use tokio::net::UdpSocket;
use tracing::{info, warn};

const DUMMY_MSG_NUM: usize = 5;
const MAX_WAIT_TIME: Duration = Duration::from_millis(500);
const NAT_SYNC_TIME: Duration = Duration::from_secs(15);

// tries to perform NAT traversal and returns the socket and remote address
// if successful
pub async fn get_nat_traversed_socket() -> Option<(UdpSocket, SocketAddr)> {
    // get random port from OS to serve on
    let interface = "0.0.0.0:0";

    // open socket and start networking!
    let socket = UdpSocket::bind(interface)
        .await
        .expect("Couldn't bind to specified port!");

    // NAT traversal

    // get our external IP and port
    get_external_and_nat(&socket).await;

    // get remote IP and port
    let other_machine_addr = get_other_ip("Enter IP:port of other machine:".to_string());

    // wait a few seconds, try connecting to other machine
    tokio::time::sleep(NAT_SYNC_TIME).await;
    match init_nat_traversal(&socket, &other_machine_addr).await {
        Ok(status) => match status {
            true => Some((socket, other_machine_addr)),
            false => None,
        },
        Err(_) => None,
    }
}

// sends dummy packets to other machine and returns connection state
async fn init_nat_traversal(sock: &UdpSocket, ip: &SocketAddr) -> Result<bool> {
    let mut connected = false;
    let dummymsg = *b"HELLOWORLD";
    for _ in 0..DUMMY_MSG_NUM {
        sock.send_to(&dummymsg, ip).await?;
        let mut buf = Vec::new();
        let recv_future = sock.recv_from(&mut buf);
        match tokio::time::timeout(MAX_WAIT_TIME, recv_future).await {
            Ok(res) => match res {
                Ok((_, src)) => {
                    connected = connected || src == *ip;
                    info!("Seemed to get data from {}", src);
                }
                Err(_) => warn!("A socket error occurred"),
            },
            Err(_) => {
                warn!("Did not receive any data, retrying...");
            }
        }
    }
    Ok(connected)
}

// TODO: create control plane which will replace calling STUN servers
async fn get_external_info(socket: &UdpSocket, ip: String) -> SocketAddr {
    let stun_addr = ip.to_socket_addrs().unwrap().find(|x| x.is_ipv4()).unwrap();
    let c = StunClient::new(stun_addr);
    let f = c.query_external_address_async(socket).await;
    match f {
        Ok(x) => {
            println!("Program is externally at: {x}");
            x
        }
        Err(_) => {
            eprintln!("Error at protocol.rs: STUN");
            socket.local_addr().unwrap()
        }
    }
}

// TODO: standardize location of external STUN servers
async fn get_external_and_nat(socket: &UdpSocket) {
    println!("Internal IP:port is {}", socket.local_addr().unwrap());
    let ip1 = get_external_info(socket, "5.178.34.84:3478".to_string()).await;
    let ip2 = get_external_info(socket, "stun2.l.google.com:19302".to_string()).await;
    if ip1 == ip2 {
        println!("NAT is easy, can transfer files easily");
    } else {
        eprintln!("NAT is hard! Cannot transfer files over hard NAT!");
    }
}

// get client's external IP and port
// TODO: add control plane which will automate this to support multiple clients
fn get_other_ip(message: String) -> SocketAddr {
    println!("{message}");
    let stdin = std::io::stdin();
    let mut other_interface = String::new();
    stdin
        .read_line(&mut other_interface)
        .expect("Couldn't read from stdin");
    let other_int = other_interface[..other_interface.len() - 1].to_string();
    other_int
        .to_socket_addrs()
        .unwrap()
        .find(|x| x.is_ipv4())
        .unwrap()
}
