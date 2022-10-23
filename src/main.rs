use std::env;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::net::UdpSocket;
use std::str;
use std::sync::Arc;
use std::thread;

//can be dynamic, set as constant for testing
const MTU: usize = 1280;

fn get_file_to_serve(filename: &String) -> Arc<Vec<u8>> {
    let mut file = match File::open(filename) {
        Err(e) => panic!("couldn't open given file! {}", e),
        Ok(file) => file,
    };
    let metadata = std::fs::metadata(filename).expect("Couldn't get metadata!");
    let mut data = vec![0; metadata.len() as usize];
    file.read_exact(&mut data)
        .expect("buffer overflow while reading file!");
    Arc::new(data)
}

fn save_data_to_file(socket : UdpSocket, file : &mut File) {
    let mut buf = [0u8; MTU];
    loop {
        let (amt, _) = socket
            .recv_from(&mut buf)
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

fn send_data_in_chunks(sock: UdpSocket, src: std::net::SocketAddr, data: Arc<Vec<u8>>) {
    println!("Got connection from {}", src);
    let mut start: usize = 0;
    //send file in chunks at first
    while start + MTU < data.len() {
        sock.send_to(&data[start..start + MTU], &src)
            .expect("Failed to send response");
        start += MTU;
    }
    //when last chunk is smaller than MTU, just send remaining data
    sock.send_to(&data[start..data.len()], &src)
        .expect("Failed to send response");
}

//will be useful to help implement authentication
fn is_valid_request(request_body: [u8; MTU], validreq: &String) -> bool {
    let req = String::from(str::from_utf8(&request_body).expect("Couldn't write buffer as string"));
    req[..validreq.len()].eq(validreq)
}

fn serve(filename: &String) {
    //server only responds to requests with this particular body
    let validreq = String::from("GET ") + filename + "\n";

    //test values, will be dynamic later on
    let interface = "0.0.0.0:8888";

    //file handling; just reads the file into vector
    let data = get_file_to_serve(filename);

    //open socket and start networking!
    let socket = UdpSocket::bind(interface).expect("Couldn't bind to specified port!");

    //print to screen what port we're using here
    println!("I am serving at {}", interface);

    //main loop which listens for connections and serves file
    loop {
        let mut buf = [0u8; MTU];
        let sock = socket.try_clone().expect("Failed to clone socket");
        let data_arc_copy = Arc::clone(&data);
        match socket.recv_from(&mut buf) {
            //create new thread and send our data to the client
            Ok((_, src)) => {
                //make sure request is valid
                if is_valid_request(buf, &validreq) {
                    thread::spawn(move || {
                        send_data_in_chunks(sock, src, data_arc_copy);
                    });
                } else {
                    eprintln!("Bad request made");
                }
            }
            Err(e) => {
                eprintln!("Couldn't receive datagram: {}", e);
            }
        }
    }
}

fn client(server_interface: &String, file_to_get: &String, filename: &String) {
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

    socket.send(rawreq).expect("Couldn't write to server!");

    //keep reading till there's nothing left to read
    save_data_to_file(socket, &mut file);
    println!("Wrote received data to {}", filename);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Insufficient args entered! Usage: ./program client <server_interface> <file_to_get> <filename> or ./program server <file_to_serve>");
    }

    let mode = &args[1];

    if mode == "server" {
        let filename = &args[2];
        serve(filename);
    } else if mode == "client" {
        let server_interface = &args[2];
        let file_to_get = &args[3];
        let filename = &args[4];
        client(server_interface, file_to_get, filename);
    } else {
        eprintln!("Incorrect args entered!");
    }
}
