use std::env;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::str;
use std::sync::Arc;
use tokio::net::UdpSocket;

mod protocol;
mod server;

pub fn get_file_size(filename: &String) -> u64 {
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

async fn save_data_to_file(socket: UdpSocket, file: &mut File) {
    let mut buf = [0u8; protocol::MTU];
    loop {
        let (amt, _) = protocol::recv(&socket, &mut buf).await;
        if amt < protocol::MTU {
            match file.write_all(&buf[..amt]) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}", e),
            };
            break;
        } else {
            match file.write_all(&buf) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}", e),
            };
        }
    }
}

async fn initiate_transfer_client(socket: UdpSocket, file: &mut File) {
    //receive data size
    let mut resp = [0u8; protocol::MTU];
    match socket.recv_from(&mut resp).await {
        Ok((amt, _)) => {
            let size = String::from(
                str::from_utf8(&resp[5..amt]).expect("Couldn't read buffer into string!"),
            );
            //zero size file means error
            if size == "0" {
                eprintln!("Invalid file entered! Server returned size 0");
            }
            //ask user whether they want the file or not
            println!("Size of file is: {}", size);
            println!("Initiate transfer? (Y/N)");
            let stdin = io::stdin();
            let mut input = String::new();
            stdin
                .read_line(&mut input)
                .expect("Couldn't read from STDIN!");
            //send ACK/NACK
            if input == "Y\n" || input == "\n" {
                println!("Initiating transfer...");
                println!("Sending ACK");
                protocol::send(&socket, &protocol::ACK.to_vec()).await;
                println!("Sent ACK");
                //recieve data in chunks if ACK
                println!("Receiving file in chunks...");
                save_data_to_file(socket, file).await;
                println!("Wrote received data");
            } else {
                println!("Stopping transfer");
                protocol::send(&socket, &protocol::NACK.to_vec()).await;
                println!("Sent NACK");
            }
        }
        Err(e) => eprintln!("Error while reading response! {}", e),
    }
}

async fn serve(filename: &String, authtoken: &String) {
    //server only responds to requests with this particular body
    let validreq = protocol::send_req(filename, authtoken);

    //test values, will be dynamic later on
    let interface = "0.0.0.0:8888";

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
    println!("I am serving at {}", interface);

    //construct Server object
    let server_obj = server::init(Arc::clone(&socket), Arc::clone(&data));

    //main loop which listens for connections and serves data depending on stage
    loop {
        let mut buf = [0u8; protocol::MTU];
        match socket.recv_from(&mut buf).await {
            Ok((_, src)) => {
                println!("Server got data: {}", str::from_utf8(&buf).expect("oof"));
                server_obj
                    .lock()
                    .await
                    .process_msg(&src, buf, server_obj.clone())
                    .await;
            }
            Err(e) => {
                eprintln!("Couldn't receive datagram: {}", e);
            }
        }
    }
}

async fn client(
    server_interface: &String,
    file_to_get: &String,
    filename: &String,
    authtoken: &String,
    interface: &String,
) {
    //test values, will be dynamic later on
    //let interface = "0.0.0.0:8000";

    //way to get the server to serve a particular file
    let filereq = protocol::send_req(file_to_get, authtoken);

    //keep file open for writing to interface
    let mut file = File::create(filename).expect("Couldn't create file!");

    //open socket and start networking!
    let socket = UdpSocket::bind(interface).await.expect("Couldn't connect!");
    socket
        .connect(server_interface)
        .await
        .expect("Couldn't connect to server, is it running?");
    protocol::send(&socket, &filereq).await;
    //follow necessary steps for file transfer
    initiate_transfer_client(socket, &mut file).await;
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Insufficient args entered! Usage: ./program client <server_interface> <file_to_get> <filename> <client_interface_to_use> or ./program server <file_to_serve>");
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
        let interface = &args[5];
        client(server_interface, file_to_get, filename, &auth, interface).await;
    } else {
        eprintln!("Incorrect args entered!");
    }
}
