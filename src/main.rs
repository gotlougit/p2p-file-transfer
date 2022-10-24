use std::env;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::str;
use std::sync::Arc;
use tokio::net::UdpSocket;

//can be dynamic, set as constant for testing
const MTU: usize = 1280;
const ACK: [u8; 3] = *b"ACK";
const NACK: [u8; 4] = *b"NACK";

fn send_req(filename: &String, auth: &String) -> Vec<u8> {
    let r = String::from("AUTH ") + auth + &String::from("\nGET ") + filename;
    r.as_bytes().to_vec()
}

fn get_file_size(filename: &String) -> u64 {
    let metadata = std::fs::metadata(filename).expect("Couldn't get metadata!");
    metadata.len()
}

fn filesize_packet(filesize: usize) -> Vec<u8> {
    let s = String::from("SIZE ") + &filesize.to_string();
    s.as_bytes().to_vec()
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
    let mut buf = [0u8; MTU];
    loop {
        let (amt, _) = socket
            .recv_from(&mut buf)
            .await
            .expect("Couldn't read from socket!");
        if amt < MTU {
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

//this function will handle server side code for data transfer
async fn initiate_transfer_server(
    socket: &Arc<UdpSocket>,
    src: std::net::SocketAddr,
    data: Arc<Vec<u8>>,
) {
    //send size of data
    println!("Sending client size of file");
    socket
        .send_to(&filesize_packet(data.len()), src)
        .await
        .expect("Couldn't send data!");
    println!("Awaiting response from client..");
    //await client sending ACK
    let mut resp = [0u8; MTU];
    match socket.recv_from(&mut resp).await {
        Ok((amt, actualsrc)) => {
            //if ACK call send_data_in_chunks, which will handle the reliable sending of data
            if actualsrc == src {
                if amt == 3 || resp[..4] == ACK {
                    println!("Client sent ACK");
                    println!("Sending data in chunks...");
                    send_data_in_chunks(socket, src, data).await;
                    println!("Data sent successfully!");
                } else {
                    println!("Client sent NACK");
                    eprintln!("Client declined to initiate data transfer");
                }
            } else {
                println!("Something went wrong here!");
            }
        }
        Err(e) => {
            eprintln!("Encountered error: {}", e);
        }
    };
}

async fn initiate_transfer_client(socket: UdpSocket, file: &mut File) {
    //receive data size
    let mut resp = [0u8; MTU];
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
                socket.send(&ACK).await.expect("Error transferring ACK");
                println!("Sent ACK");
                //recieve data in chunks if ACK
                println!("Receiving file in chunks...");
                save_data_to_file(socket, file).await;
                println!("Wrote received data");
            } else {
                println!("Stopping transfer");
                socket.send(&NACK).await.expect("Error transferring NACK");
                println!("Sent NACK");
            }
        }
        Err(e) => eprintln!("Error while reading response! {}", e),
    }
}

async fn send_data_in_chunks(
    socket: &Arc<UdpSocket>,
    src: std::net::SocketAddr,
    data: Arc<Vec<u8>>,
) {
    let mut start: usize = 0;
    //send file in chunks at first
    while start + MTU < data.len() {
        socket
            .send_to(&data[start..start + MTU], &src)
            .await
            .expect("Failed to send response");
        start += MTU;
    }
    //when last chunk is smaller than MTU, just send remaining data
    socket
        .send_to(&data[start..data.len()], &src)
        .await
        .expect("Failed to send response");
}

//will be useful to help implement authentication
fn is_valid_request(request_body: [u8; MTU], validreq: &[u8]) -> bool {
    let req = String::from(str::from_utf8(&request_body).expect("Couldn't write buffer as string"));
    let vreq = String::from(str::from_utf8(&validreq).expect("Couldn't write buffer as string"));
    req[..validreq.len()].eq(&vreq)
}

async fn serve(filename: &String, authtoken: &String) {
    //server only responds to requests with this particular body
    let validreq = send_req(filename, authtoken);

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

    //main loop which listens for connections and serves file
    loop {
        let mut buf = [0u8; MTU];
        let data_arc_copy = Arc::clone(&data);
        match socket.recv_from(&mut buf).await {
            //create new thread and send our data to the client
            Ok((_, src)) => {
                //make sure request is valid
                let socket_arc_copy = Arc::clone(&socket);
                println!("Server got data: {}", str::from_utf8(&buf).expect("oof"));
                if is_valid_request(buf, &validreq) {
                    println!("Got valid request from {}, creating new thread", src);
                    //TODO: figure out concurrency so other users can request files as well
                    initiate_transfer_server(&socket_arc_copy, src, data_arc_copy).await;
                } else {
                    //send 0 size packet; client understands this is bad request
                    socket
                        .send_to(&filesize_packet(0), &src)
                        .await
                        .expect("Couldn't send data");
                    eprintln!("Bad request made");
                }
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
) {
    //test values, will be dynamic later on
    let interface = "0.0.0.0:8000";

    //way to get the server to serve a particular file
    let filereq = send_req(file_to_get, authtoken);

    //keep file open for writing to interface
    let mut file = File::create(filename).expect("Couldn't create file!");

    //open socket and start networking!
    let socket = UdpSocket::bind(interface).await.expect("Couldn't connect!");
    socket
        .connect(server_interface)
        .await
        .expect("Couldn't connect to server, is it running?");

    socket
        .send(&filereq)
        .await
        .expect("Couldn't write to server!");

    //follow necessary steps for file transfer
    initiate_transfer_client(socket, &mut file).await;
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
