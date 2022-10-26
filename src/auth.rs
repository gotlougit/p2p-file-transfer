//module to help implement authentication
use std::str;

//helper function to check if request is legitimate
//will be useful to help implement authentication
pub fn is_valid_request(request_body: [u8; MTU], validreq: &[u8]) -> bool {
    let req = String::from(str::from_utf8(&request_body).expect("Couldn't write buffer as string"));
    let vreq = String::from(str::from_utf8(&validreq).expect("Couldn't write buffer as string"));
    req[..validreq.len()].eq(&vreq)
}
