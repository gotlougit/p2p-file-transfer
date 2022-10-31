use std::env;
use std::fs::File;
use std::io::Read;
use std::str;
use std::sync::Arc;
use tokio::net::UdpSocket;

mod auth;
mod client;
mod protocol;
mod server;

fn get_file_size(filename: &String) -> u64 {
    let metadata = std::fs::metadata(filename).expect("Couldn't get metadata!");
    metadata.len()
}

fn get_file_to_serve(filename: &String, filesize: u64) -> Arc<Vec<u8>> {
    let mut file = match File::open(filename) {
        Err(e) => panic!("couldn't open given file! {}", e),
        Ok(file) => file,
    };
    let mut data = vec![0; filesize as usize];
    file.read_exact(&mut data)
        .expect("buffer overflow while reading file!");
    Arc::new(data)
}

async fn serve(filename: &String, authtoken: &String) {
    //get random port from OS to serve on
    let interface = "0.0.0.0:0";

    //file handling; just reads the file into vector
    let datasize = get_file_size(filename);
    let data = get_file_to_serve(filename, datasize);

    //open socket and start networking!
    let socket = Arc::new(
        UdpSocket::bind(interface)
            .await
            .expect("Couldn't bind to specified port!"),
    );

    //print to screen what port we're using here
    println!("I am serving at {}", socket.local_addr().unwrap());

    //construct Server object
    let server_obj = server::init(
        Arc::clone(&socket),
        Arc::clone(&data),
        filename.to_string(),
        authtoken.to_string(),
    );

    //main loop which listens for connections and serves data depending on stage
    loop {
        let mut buf = [0u8; protocol::MTU];
        let (amt, src) = protocol::recv(&socket, &mut buf).await;
        println!("Server got data: {}", str::from_utf8(&buf).expect("oof"));
        server_obj
            .lock()
            .await
            .process_msg(&src, buf, amt, server_obj.clone())
            .await;
    }
}

async fn client(
    server_interface: &String,
    file_to_get: &String,
    filename: &String,
    authtoken: &String,
) {
    let interface = "0.0.0.0:0";
    //open socket and start networking!
    let socket = Arc::new(UdpSocket::bind(interface).await.expect("Couldn't connect!"));
    socket
        .connect(server_interface)
        .await
        .expect("Couldn't connect to server, is it running?");

    //print to screen what port we're using here
    println!("I am receiving at {}", socket.local_addr().unwrap());

    //create Client object and send initial request to server
    let client_obj = client::init(Arc::clone(&socket), file_to_get, filename, authtoken);
    client_obj.lock().await.init_connection().await;
    //listen for server responses and deal with them accordingly
    loop {
        let mut buf = [0u8; protocol::MTU];
        let (amt, _) = protocol::recv(&socket, &mut buf).await;
        println!("Client got data: {}", str::from_utf8(&buf).expect("oof"));
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
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Insufficient args entered! Usage: ./program client <server_interface> <file_to_get> <filename> or ./program server <file_to_serve>");
    }

    let mode = &args[1];

    let auth = String::from("11111111"); //for testing purposes only!

    if mode == "server" {
        let filename = &args[2];
        serve(filename, &auth).await;
    } else if mode == "client" {
        let server_interface = &args[2];
        let file_to_get = &args[3];
        let filename = &args[4];
        client(server_interface, file_to_get, filename, &auth).await;
    } else {
        eprintln!("Incorrect args entered!");
    }
}
