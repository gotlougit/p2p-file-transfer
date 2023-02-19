#![allow(dead_code)]

use std::env;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use stunclient::StunClient;
use tokio::net::UdpSocket;
use tracing::error;

mod auth;
mod client;
mod connection;
mod parsing;
mod server;
mod socket;

//TODO: create control plane which will replace calling STUN servers
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
            error!("Error at protocol.rs: STUN");
            socket.local_addr().unwrap()
        }
    }
}

//TODO: standardize location of external STUN servers
async fn get_external_and_nat(socket: &UdpSocket) {
    println!("Internal IP:port is {}", socket.local_addr().unwrap());
    let ip1 = get_external_info(socket, "5.178.34.84:3478".to_string()).await;
    let ip2 = get_external_info(socket, "stun2.l.google.com:19302".to_string()).await;
    if ip1 == ip2 {
        println!("NAT is easy, can transfer files easily");
    } else {
        println!("NAT is hard! Cannot transfer files over hard NAT!");
    }
}

fn get_other_ip(message: String) -> SocketAddr {
    //get client's external IP and port
    //TODO: add control plane which will automate this to support multiple clients
    println!("{message}");
    let stdin = io::stdin();
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

async fn serve(filename: &String, authtoken: &String) {
    //get random port from OS to serve on
    let interface = "0.0.0.0:0";

    //open socket and start networking!
    let socket = UdpSocket::bind(interface)
        .await
        .expect("Couldn't bind to specified port!");

    //NAT traversal

    //get our external IP and port
    get_external_and_nat(&socket).await;

    //get client's external IP and port
    let client_int = get_other_ip("Enter client IP:".to_string());

    let wrappersocket = socket::ActualSocket { socket };

    let mut connection = connection::init_conn::<socket::ActualSocket>(wrappersocket);

    //wait 5 seconds, try connecting to server, then wait 5 more seconds
    connection.sync_nat_traversal();
    connection.init_nat_traversal(&client_int).await;

    //construct Server object
    let mut server_obj = server::init::<socket::ActualSocket>(
        connection,
        filename.to_string(),
        authtoken.to_string(),
    );

    //main loop which listens for connections and serves data depending on stage
    server_obj.mainloop().await;
}

async fn client(file_to_get: &String, authtoken: &String) {
    let interface = "0.0.0.0:0";
    //open socket and start networking!
    let socket = UdpSocket::bind(interface).await.expect("Couldn't connect!");

    //NAT traversal

    //get our external IP and port
    get_external_and_nat(&socket).await;

    //get server's external IP and port
    let server_int = get_other_ip("Enter server IP:".to_string());

    let wrappersocket = socket::ActualSocket { socket };

    let mut connection = connection::init_conn::<socket::ActualSocket>(wrappersocket);
    //wait 5 seconds, try connecting to server, then wait 5 more seconds
    connection.sync_nat_traversal();
    connection.init_nat_traversal(&server_int).await;

    //create Client object and send initial request to server
    let mut client_obj = client::init(connection, file_to_get, authtoken, server_int);
    client_obj.init_connection().await;
    //listen for server responses and deal with them accordingly
    client_obj.mainloop().await;
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        error!("Insufficient args entered! Usage: ./program client <file_to_get> or ./program server <file_to_serve>");
    }

    let mode = &args[1];

    let auth = String::from("11111111"); //for testing purposes only!

    if mode == "server" {
        let filename = &args[2];
        serve(filename, &auth).await;
    } else if mode == "client" {
        let file_to_get = &args[2];
        client(file_to_get, &auth).await;
    } else {
        error!("Incorrect args entered!");
    }
}
