//module to help implement authentication
use std::str;

use crate::protocol;

pub struct AuthChecker {
    valid_tokens: Vec<String>,
    valid_files: Vec<String>,
}

//TODO: AuthChecker should eventually support multiple files and tokens
pub fn init(valid_token: String, valid_file: String) -> AuthChecker {
    let v = vec![valid_token];
    let w = vec![valid_file];
    AuthChecker {
        valid_tokens: v,
        valid_files: w,
    }
}

impl AuthChecker {
    pub fn is_valid_request(&self, request_body: [u8; protocol::MTU]) -> bool {
        let (file_requested, giventoken) = protocol::parse_send_req(request_body);

        let mut does_file_exist = false;
        if self
            .valid_files
            .iter()
            .any(|file| file == &file_requested[..file.len()])
        {
            does_file_exist = true;
        }
        if !does_file_exist {
            return false;
        }
        let mut is_token_allowed = false;
        if self.valid_tokens.iter().any(|token| token == &giventoken) {
            is_token_allowed = true;
        }
        is_token_allowed
    }
}
