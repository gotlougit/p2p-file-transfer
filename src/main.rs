use std::env;
use std::io;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::timeout;

mod auth;
mod client;
mod protocol;
mod server;

async fn serve(filename: &String, authtoken: &String) {
    //get random port from OS to serve on
    let interface = "0.0.0.0:0";

    //open socket and start networking!
    let socket = Arc::new(
        UdpSocket::bind(interface)
            .await
            .expect("Couldn't bind to specified port!"),
    );

    //NAT traversal

    //get our external IP and port
    protocol::get_external_and_nat(Arc::clone(&socket)).await;

    //get client's external IP and port
    //TODO: add control plane which will automate this to support multiple clients
    println!("Enter client IP info: ");
    let stdin = io::stdin();
    let mut client_interface = String::new();
    stdin
        .read_line(&mut client_interface)
        .expect("Couldn't read from stdin");

    //remove \n from input
    let client_int = client_interface[..client_interface.len() - 1].to_string();

    //wait 5 seconds, try connecting to server, then wait 5 more seconds
    protocol::init_nat_traversal(Arc::clone(&socket), &client_int).await;

    //print to screen what port we're using here just in case
    println!("I am serving locally at {}", socket.local_addr().unwrap());

    //construct Server object
    let server_obj = server::init(
        Arc::clone(&socket),
        filename.to_string(),
        authtoken.to_string(),
    );

    //main loop which listens for connections and serves data depending on stage
    loop {
        let mut buf = [0u8; protocol::MTU];
        if let Ok((amt, src)) =
            timeout(protocol::MAX_WAIT_TIME, protocol::recv(&socket, &mut buf)).await
        {
            if protocol::parse_resend(buf, amt) {
                println!("Need to resend!");
                protocol::resend(&socket).await;
            } else {
                server_obj
                    .lock()
                    .await
                    .process_msg(&src, buf, amt, server_obj.clone())
                    .await;
            }
        } else {
            println!("Timeout occurred, asking all clients to resend!");
            //ask all clients for resend
            server_obj.lock().await.ask_all_to_resend().await;
        }
    }
}

async fn client(file_to_get: &String, filename: &String, authtoken: &String) {
    let interface = "0.0.0.0:0";
    //open socket and start networking!
    let socket = Arc::new(UdpSocket::bind(interface).await.expect("Couldn't connect!"));

    //NAT traversal

    //get our external IP and port
    protocol::get_external_and_nat(Arc::clone(&socket)).await;

    //get server's external IP and port
    println!("Enter server IP info: ");
    let stdin = io::stdin();
    let mut server_interface = String::new();
    stdin
        .read_line(&mut server_interface)
        .expect("Couldn't read from stdin");

    //get rid of \n from input
    let server_int = server_interface[..server_interface.len() - 1].to_string();
    //wait 5 seconds, try connecting to server, then wait 5 more seconds
    protocol::init_nat_traversal(Arc::clone(&socket), &server_int).await;

    //connect to *hopefully* open server port
    socket
        .connect(server_int)
        .await
        .expect("Couldn't connect to server, is it running?");

    //print to screen what local port we're using here just in case
    println!("I am receiving at {}", socket.local_addr().unwrap());

    //create Client object and send initial request to server
    let client_obj = client::init(Arc::clone(&socket), file_to_get, filename, authtoken);
    client_obj.lock().await.init_connection().await;
    let mut last_recv = true;
    let mut is_connected = false;
    //listen for server responses and deal with them accordingly
    loop {
        if !last_recv && is_connected {
            protocol::resend(&socket).await;
            println!("Sent resent packet");
            last_recv = true;
        }

        let mut buf = [0u8; protocol::MTU];
        if let Ok((amt, _)) =
            timeout(protocol::MAX_WAIT_TIME, protocol::recv(&socket, &mut buf)).await
        {
            if protocol::parse_resend(buf, amt) {
                protocol::resend(&socket).await;
                continue;
            }
            //make sure program exits gracefully
            let continue_with_loop = client_obj
                .lock()
                .await
                .process_msg(buf, amt, client_obj.clone())
                .await;
            if !continue_with_loop {
                println!("Client exiting...");
                break;
            }
            is_connected = true;
        } else {
            if !is_connected {
                println!("Initial connection request may have been lost! Resending...");
                client_obj.lock().await.init_connection().await;
            }
            println!("Client could not receive data in time!");
            last_recv = false;
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Insufficient args entered! Usage: ./program client <file_to_get> or ./program server <file_to_serve>");
    }

    let mode = &args[1];

    let auth = String::from("11111111"); //for testing purposes only!

    if mode == "server" {
        let filename = &args[2];
        serve(filename, &auth).await;
    } else if mode == "client" {
        let file_to_get = &args[2];
        client(file_to_get, file_to_get, &auth).await;
    } else {
        eprintln!("Incorrect args entered!");
    }
}
