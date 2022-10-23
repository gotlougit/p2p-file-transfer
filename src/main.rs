use std::env;
use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;
use std::str;
use std::sync::Arc;
use std::thread;

//can be dynamic, set as constant for testing
const MTU: usize = 1280;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        panic!("Insufficient args entered! USAGE: ./server <filenametotransmit>");
    }
    //test values, will be dynamic later on
    let interface = "0.0.0.0:8888";
    let filename = &args[1];

    //server only responds to these requests
    let validreq = String::from("GET ") + filename + "\n";

    //file handling; just reads the file into vector
    let mut file = match File::open(&filename) {
        Err(e) => panic!("couldn't open given file! {}", e),
        Ok(file) => file,
    };
    let metadata = std::fs::metadata(&filename).expect("Couldn't get metadata!");
    let mut data = vec![0; metadata.len() as usize];
    file.read_exact(&mut data)
        .expect("buffer overflow while reading file!");
    let data_as_arc = Arc::new(data); //this is used from now on

    //open socket and start networking!
    let socket = UdpSocket::bind(interface).expect("Couldn't bind to specified port!");

    //print to screen what port we're using here
    println!("I am serving at {}", interface);

    loop {
        let mut buf = [0u8; MTU];
        let sock = socket.try_clone().expect("Failed to clone socket");
        let d = Arc::clone(&data_as_arc);
        match socket.recv_from(&mut buf) {
            //create new thread and send our data to the client
            Ok((_, src)) => {
                //make sure request is valid
                let req =
                    String::from(str::from_utf8(&buf).expect("Couldn't write buffer as string"));
                if req[..validreq.len()].eq(&validreq) {
                    thread::spawn(move || {
                        println!("Got connection from {}", src);
                        let mut start: usize = 0;
                        //send file in chunks at first
                        while start + MTU < d.len() {
                            sock.send_to(&d[start..start + MTU], &src)
                                .expect("Failed to send response");
                            start += MTU;
                        }
                        //when last chunk is smaller than MTU, just send remaining data
                        sock.send_to(&d[start..d.len()], &src)
                            .expect("Failed to send response");
                    });
                } else {
                    print!("Bad request made: {}", req);
                }
            }
            Err(e) => {
                eprintln!("Couldn't receive datagram: {}", e);
            }
        }
    }
}
