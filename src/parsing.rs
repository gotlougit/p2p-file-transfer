//used to convert from variables to raw bytes to be sent through network and vice versa
use std::str;
use tracing::{debug, error};

//use this instead of comparing raw bytes every time
#[derive(PartialEq, Debug)]
pub enum PrimitiveMessage {
    ACK,
    NACK,
    END,
    RESEND,
    INVALID,
}

//helps keep track of client states
#[derive(PartialEq, Debug)]
pub enum ClientState {
    NoState,
    ACKorNACK,
    SendFile,
    EndConn,
    EndedConn,
}

//small message types are convenient as consts
const ACK: [u8; 3] = *b"ACK";
const NACK: [u8; 4] = *b"NACK";
const RESEND: [u8; 6] = *b"RESEND";
const END: [u8; 3] = *b"END";

//get the plaintext parts of packet out into a String if possible
fn parse_generic_req(message: &[u8], amt: usize) -> Option<String> {
    if amt > message.len() {
        return None;
    }
    let req = match str::from_utf8(&message[..amt]) {
        Ok(x) => x.to_string(),
        Err(_) => {
            error!("Generic parse error detected!");
            return None;
        }
    };
    Some(req)
}

//used externally to get simple message types
pub fn get_primitive(msgtype: PrimitiveMessage) -> Vec<u8> {
    match msgtype {
        PrimitiveMessage::ACK => {
            debug!("Requested ACK");
            ACK.to_vec()
        }
        PrimitiveMessage::NACK => {
            debug!("Requested NACK");
            NACK.to_vec()
        }
        PrimitiveMessage::END => {
            debug!("Requested END");
            END.to_vec()
        }
        PrimitiveMessage::RESEND => {
            debug!("Requested RESEND");
            RESEND.to_vec()
        }
        _ => {
            error!("Requested INVALID? Returning ACK");
            ACK.to_vec()
        }
    }
}

//get primitive message type
pub fn parse_primitive(message: &[u8], amt: usize) -> PrimitiveMessage {
    match amt {
        3 => {
            if message[..3] == ACK {
                debug!("Parsed ACK");
                return PrimitiveMessage::ACK;
            } else if message[..3] == END {
                debug!("Parsed END");
                return PrimitiveMessage::END;
            }
            error!("Parsed invalid primitive message");
            return PrimitiveMessage::INVALID;
        }
        4 => {
            if message[..4] == NACK {
                debug!("Parsed NACK");
                return PrimitiveMessage::NACK;
            }
            return PrimitiveMessage::INVALID;
        }
        6 => {
            if message[..6] == RESEND {
                debug!("Parsed RESEND");
                return PrimitiveMessage::RESEND;
            }
            return PrimitiveMessage::INVALID;
        }
        _ => {
            error!("Parsed invalid primitive message");
            return PrimitiveMessage::INVALID;
        }
    }
}

//initial request client sends; consists of auth token and name of file to get
pub fn send_req(filename: &String, auth: &String) -> Vec<u8> {
    let r = String::from("AUTH ") + auth + &String::from("\nGET ") + filename;
    debug!(
        "Built send request with an authtoken and filename {}",
        filename
    );
    r.as_bytes().to_vec()
}

pub fn parse_send_req(message: &[u8], amt: usize) -> Option<(String, String)> {
    if let Some(req) = parse_generic_req(message, amt) {
        let file_requested = match req.split("GET ").collect::<Vec<&str>>().get(1) {
            Some(x) => x.to_string(),
            None => {
                error!("File request parsed unsuccessfully");
                return None;
            }
        };

        let giventoken = match req.split("AUTH ").collect::<Vec<&str>>().get(1) {
            Some(x) => match x.to_string().split('\n').collect::<Vec<&str>>().first() {
                Some(y) => y.to_string(),
                None => {
                    error!("Token parse error");
                    return None;
                }
            },
            None => {
                error!("Token parse error");
                return None;
            }
        };
        debug!("Parsed send request");
        return Some((file_requested, giventoken));
    }
    None
}

//server then sends filesize if everything checked out
pub fn filesize_packet(filesize: usize) -> Vec<u8> {
    let s = String::from("SIZE ") + &filesize.to_string();
    debug!("Built filesize packet with size {}", filesize);
    s.as_bytes().to_vec()
}

pub fn parse_filesize_packet(message: &[u8], amt: usize) -> Option<usize> {
    if let Some(req) = parse_generic_req(message, amt) {
        let size = match req.split("SIZE ").collect::<Vec<&str>>().get(1) {
            Some(x) => x.to_string().parse::<usize>().unwrap(),
            None => {
                error!("Filesize parse error");
                return None;
            }
        };
        debug!("Parsed filesize packet");
        return Some(size);
    }
    None
}

//here client will tell server if it wants the file or not

//tell server which packet has been last received
pub fn last_received_packet(num: usize) -> Vec<u8> {
    let s = String::from("LAST ") + &num.to_string();
    debug!("Built last packet with offset {}", num);
    s.as_bytes().to_vec()
}

pub fn parse_last_received(message: &[u8], amt: usize) -> Option<usize> {
    if parse_primitive(message, amt) == PrimitiveMessage::ACK {
        return None;
    }
    if let Some(req) = parse_generic_req(message, amt) {
        let lastrecv = match req.split("LAST ").collect::<Vec<&str>>().get(1) {
            Some(x) => x.to_string().parse::<usize>().unwrap(),
            None => {
                error!("Last receive parse error");
                return None;
            }
        };
        debug!("Parsed last received packet");
        return Some(lastrecv);
    }
    None
}

//data packet is basically a small header and some amount of payload
pub fn data_packet(offset: usize, message: &Vec<u8>) -> Vec<u8> {
    let s = String::from("OFFSET: ") + &offset.to_string() + "\n";
    let mut b1 = s.as_bytes().to_vec();
    b1.extend(message);
    debug!("Built data packet with offset {}", offset);
    b1
}

pub fn parse_data_packet(message: &[u8], amt: usize) -> Option<(usize, Vec<u8>)> {
    let mut sizeofheader = 0;
    for i in 0..amt {
        if message[i] == 10 {
            sizeofheader = i + 1;
            break;
        }
    }
    if let Some(req) = parse_generic_req(message, sizeofheader) {
        let offset = match req.split("OFFSET: ").collect::<Vec<&str>>().get(1) {
            Some(x) => match x.to_string().split('\n').collect::<Vec<&str>>().first() {
                Some(y) => y.to_string().parse::<usize>().unwrap(),
                None => {
                    error!("Offset parse error");
                    return None;
                }
            },
            None => {
                error!("Offset parse error");
                return None;
            }
        };
        debug!("Parsed data packet");
        return Some((offset, message[sizeofheader..amt].to_vec()));
    }
    None
}

pub fn resend_offset(offset: usize) -> Vec<u8> {
    let s = String::from("RESEND ") + &offset.to_string();
    s.as_bytes().to_vec()
}

pub fn parse_resend_offset(message: &[u8], amt: usize) -> Option<usize> {
    if let Some(req) = parse_generic_req(message, amt) {
        let offset_requested = match req.split("RESEND ").collect::<Vec<&str>>().get(1) {
            Some(x) => x.to_string().parse::<usize>().unwrap(),
            None => {
                error!("Resend offset parse error!");
                return None;
            }
        };
        return Some(offset_requested);
    }
    None
}

//test private functions
#[cfg(test)]
mod test {
    use crate::parsing::parse_generic_req;
    #[test]
    fn test_generic_parser() {
        let ok_input = *b"HELLO WORLD";
        let bad_input = [129u8; 1000];
        assert_eq!(parse_generic_req(&ok_input, 11).is_some(), true);
        assert_eq!(parse_generic_req(&bad_input, 1000).is_some(), false);
        assert_eq!(parse_generic_req(&bad_input, 1).is_some(), false);
        assert_eq!(parse_generic_req(&bad_input, 1001).is_some(), false);
    }
}
