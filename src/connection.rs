//provide basic NAT traversal and reliable data transfer abstractions
use std::collections::HashMap;
use std::hash::Hash;
use std::net::SocketAddr;
use std::thread;
use std::time::SystemTime;
use std::time::{Duration, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::time::timeout;

use tracing::{debug, error, info, warn};

//defines how small and large the sliding window can be
const INITIAL_N: usize = 6;
const MAX_N: usize = 75;

//DIVIDER tells the maximum time that both machines will wait before initiating
//NAT traversal
const DIVIDER: u64 = 15;

//amount of time each machine waits before declaring a timeout and initiating
//RESEND
const MAX_WAIT_TIME: Duration = Duration::from_secs(3);

//MTU: maximum raw info in one packet
pub const MTU: usize = 1280;
//DATA_SIZE: amount of file each packet will contain
pub const DATA_SIZE: usize = 1000;

//number of dummy messages to send
const DUMMY_MSG_NUM: usize = 5;

//small function used everywhere here
pub fn change_map_value<K, V>(map: &mut HashMap<K, V>, key: K, newval: V)
where
    K: Eq,
    K: Hash,
{
    if map.remove(&key).is_some() {
        map.insert(key, newval);
    }
}

//main abstraction to handle connections
pub struct Connection {
    socket: UdpSocket,
    protocol_n: HashMap<SocketAddr, usize>,
    encryption_token: HashMap<SocketAddr, String>,
    lastmsg: HashMap<SocketAddr, Vec<Vec<u8>>>,
}

pub fn init_conn(socket: UdpSocket) -> Connection {
    Connection {
        socket,
        protocol_n: HashMap::new(),
        encryption_token: HashMap::new(),
        lastmsg: HashMap::new(),
    }
}

impl Connection {
    //helps sync NAT traversal
    pub fn sync_nat_traversal(&self) {
        //wait till some common time
        let time_to_wait = DIVIDER
            - (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                % DIVIDER);
        info!("Waiting for {} seconds", time_to_wait);
        thread::sleep(Duration::from_secs(time_to_wait));
    }

    //sends dummy packets to other machine and returns connection state
    pub async fn init_nat_traversal(&self, ip: &SocketAddr) -> bool {
        let mut connected = false;
        let dummymsg = *b"HELLOWORLD";
        for _ in 0..DUMMY_MSG_NUM {
            self.basic_send_to(ip, &dummymsg).await;
            let mut buf = [0u8; MTU];
            let recv_future = self.basic_recv(&mut buf);
            match timeout(MAX_WAIT_TIME, recv_future).await {
                Ok((_, src)) => {
                    connected = connected || src == *ip;
                    info!("Seemed to get data from {}", src);
                }
                Err(_) => {
                    warn!("Did not receive any data, retrying...");
                }
            }
        }
        connected
    }

    fn add_ip_to_maps(&mut self, ip: &SocketAddr) {
        debug!("Adding IP {} to maps", ip);
        if self.protocol_n.get(ip) == None {
            self.protocol_n.insert(*ip,INITIAL_N);
        }
        if self.lastmsg.get(ip) == None {
            self.lastmsg.insert(*ip,Vec::new());
        }
    }

    //deal with N for each IP
    fn grow_n(&mut self, ip: &SocketAddr) {
        if let Some(n) = self.protocol_n.get(ip) {
            if n * 2 <= MAX_N {
                let n = &(n * 2);
                debug!("Increased N to {} for IP: {}", n, ip);
                change_map_value::<SocketAddr, usize>(&mut self.protocol_n, *ip, *n);
            }
        } else {
            error!("N could not be read for IP {}, probably not in map", ip);
            self.add_ip_to_maps(ip);
        }
    }

    fn reset_n(&mut self, ip: &SocketAddr) {
        if let Some(_) = self.protocol_n.get(ip) {
            let n = &INITIAL_N;
            debug!("Reset N to {} for IP: {}", n, ip);
            change_map_value::<SocketAddr, usize>(&mut self.protocol_n, *ip, *n);
        } else {
            error!("N could not be read for IP {}, probably not in map", ip);
            self.add_ip_to_maps(ip);
        }
    }

    pub fn read_n(&mut self, ip: &SocketAddr) -> usize {
        if let Some(n) = self.protocol_n.get(ip) {
            debug!("Read N as {} for IP: {}", n, ip);
            return *n;
        } else {
            error!("N could not be read for IP {}, probably not in map", ip);
            self.add_ip_to_maps(ip);
            INITIAL_N
        }
    }

    //get current list of IP addresses connected
    fn get_ip_connected(&self) -> Vec<SocketAddr> {
        let iplist = self.protocol_n.keys().cloned().collect::<Vec<SocketAddr>>();
        return iplist;
    }

    //deal with last messages for each IP
    fn add_last_msg(&mut self, ip: &SocketAddr, message: Vec<u8>) {
        if let Some(v) = self.lastmsg.get(ip) {
            if v.len() >= self.read_n(ip) {
                debug!("Resetting lastmsg for IP {}",ip);
                self.reset_last_msg(ip);
            }
        } else {
            error!("N could not be read for IP {}, probably not in map", ip);
            self.add_ip_to_maps(ip);
        }
        //now that we know a Vec exists for the given IP, let's add message to it
        if let Some(v) = self.lastmsg.get(ip) {
            let mut newv = v.clone();
            newv.push(message);
            change_map_value::<SocketAddr, Vec<Vec<u8>>>(&mut self.lastmsg, *ip, newv);
        }

    }

    fn reset_last_msg(&mut self, ip: &SocketAddr) {
        let emptyvec: Vec<Vec<u8>> = Vec::new();
        change_map_value::<SocketAddr, Vec<Vec<u8>>>(&mut self.lastmsg, *ip, emptyvec);
    }

    //resend last messages
    pub async fn resend_to(&mut self, ip: &SocketAddr) {
        if let Some(messages) = self.lastmsg.get(ip) {
            debug!("Resending {} packets to IP {}", messages.len(), ip);
            for msg in messages {
                self.basic_send_to(ip, msg).await;
            }
        }
        self.reset_n(ip);
        self.reset_last_msg(ip);
    }

    async fn ask_resend_from_all(&mut self) {
        warn!("Asking for resend from all IPs connected");
        let ipaddrs = self.get_ip_connected();
        for ip in ipaddrs {
            self.resend_to(&ip).await;
        }
    }

    //actual APIs used for sending and receiving data
    pub async fn send_to(&mut self, src: &SocketAddr, message: &[u8]) {
        self.grow_n(src);
        self.add_last_msg(src, message.to_vec());
        self.basic_send_to(src, message).await;
    }

    //like recv() but implements one timeout
    pub async fn reliable_recv(&mut self, buffer: &mut [u8; MTU]) -> Option<(usize, SocketAddr)> {
        let future = self.recv(buffer);
        if let Ok((amt, src)) = timeout(MAX_WAIT_TIME, future).await {
            debug!("Received packet in time");
            Some((amt, src))
        } else {
            warn!("Timeout occurred, asking for resend");
            self.ask_resend_from_all().await;
            None
        }
    }

    pub async fn recv(&mut self, buffer: &mut [u8; MTU]) -> (usize, SocketAddr) {
        let (amt, src) = self.basic_recv(buffer).await;
        self.grow_n(&src);
        (amt, src)
    }

    //basic send and recv wrappers
    async fn basic_send_to(&self, src: &SocketAddr, message: &[u8]) {
        //TODO: use encryption_token to encrypt message before sending
        match self.socket.send_to(message, src).await {
            Ok(_) => debug!("Sent message"),
            Err(_) => error!("A send error occured!"),
        }
    }

    async fn basic_recv(&self, buffer: &mut [u8; MTU]) -> (usize, SocketAddr) {
        let (amt, src) = self
            .socket
            .recv_from(&mut buffer[..])
            .await
            .expect("A recv error occurred!");
        //TODO: use encryption_token to decrypt message after recv call is complete
        (amt, src)
    }
}
