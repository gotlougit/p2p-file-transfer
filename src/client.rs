use std::env;
use std::fs::File;
use std::io::Write;
use std::net::UdpSocket;

//can be dynamic, set as constant for testing
const MTU: usize = 1280;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        panic!("Insufficient args entered! USAGE: ./client <ip_address_of_server:port> <file_to_get> <save_file_as>");
    }
    let server_interface = &args[1];
    let file_to_get = &args[2];
    let filename = &args[3];

    //test values, will be dynamic later on
    let interface = "0.0.0.0:8000";

    //way to get the server to serve a particular file
    let request = String::from("GET ") + file_to_get + "\n";
    let rawreq = request.as_bytes();

    //keep file open for writing to interface
    let mut file = File::create(filename).expect("Couldn't create file!");

    //open socket and start networking!
    let socket = UdpSocket::bind(interface).expect("Couldn't bind to specified port!");
    socket
        .connect(server_interface)
        .expect("Couldn't connect to server, is it running?");

    socket.send(&rawreq).expect("Couldn't write to server!");

    //keep reading till there's nothing left to read
    let mut buf = [0u8; MTU];
    loop {
        let (amt, _) = socket
            .recv_from(&mut buf)
            .expect("Couldn't read from socket!");
        if amt < MTU {
            match file.write_all(&buf[..amt]) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}",e),
            };
            break;
        } else {
            match file.write_all(&buf) {
                Ok(v) => v,
                Err(e) => eprint!("Encountered an error while writing: {}",e),
            };
        }
    }
    println!("Wrote received data to {}", filename);
}
