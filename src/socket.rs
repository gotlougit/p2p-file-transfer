//implements trait that can be used to create fake socket for testing
use async_trait::async_trait;
use std::{net::SocketAddr, str::FromStr};
use tokio::{io::Error, net::UdpSocket};

pub struct ActualSocket {
    pub socket: UdpSocket,
}

pub struct DummySocket {
    pub send_proper: bool,
    pub recv_proper: bool,
}

#[async_trait]
pub trait Socket {
    async fn send_to<'a>(&self, message: &[u8], src: &SocketAddr) -> Result<usize, Error>;
    async fn recv_from(&self, message: &mut [u8]) -> Result<(usize, SocketAddr), Error>;
}

#[async_trait]
impl Socket for ActualSocket {
    async fn send_to<'a>(&self, message: &[u8], src: &SocketAddr) -> Result<usize, Error> {
        self.socket.send_to(message, src).await
    }
    async fn recv_from(&self, message: &mut [u8]) -> Result<(usize, SocketAddr), Error> {
        self.socket.recv_from(message).await
    }
}

#[async_trait]
impl Socket for DummySocket {
    async fn send_to<'a>(&self, message: &[u8], src: &SocketAddr) -> Result<usize, Error> {
        if self.send_proper {
            Ok(message.len())
        } else {
            Ok(0)
        }
    }
    async fn recv_from(&self, message: &mut [u8]) -> Result<(usize, SocketAddr), Error> {
        if self.recv_proper {
            Ok((
                message.len(),
                SocketAddr::from_str("127.0.0.1:1025").unwrap(),
            ))
        } else {
            Ok((0, SocketAddr::from_str("127.0.0.1:1025").unwrap()))
        }
    }
}
