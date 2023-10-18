use anyhow::Result;
use quinn::{Endpoint, EndpointConfig, ServerConfig};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::error;

/// Constructs a QUIC endpoint configured to listen for incoming connections on a certain address
/// and port.
///
/// ## Returns
///
/// - a stream of incoming QUIC connections
/// - server certificate serialized into DER format
fn make_server_endpoint(socket: UdpSocket) -> Result<(Endpoint, Vec<u8>)> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::new(
        EndpointConfig::default(),
        Some(server_config),
        socket.into_std().unwrap(),
        quinn::default_runtime().unwrap(),
    )
    .unwrap();
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, Vec<u8>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}

/// Runs a QUIC server bound to given address.
pub async fn run_server(socket: UdpSocket, filename: &str, auth: &str) {
    let (endpoint, _server_cert) = make_server_endpoint(socket).unwrap();
    let conn = endpoint.accept().await.unwrap();
    let (mut tx, mut rx) = conn.await.unwrap().accept_bi().await.unwrap();
    let buf = rx.read_to_end(usize::max_value()).await.unwrap();
    let msg: Vec<_> = std::str::from_utf8(&buf)
        .unwrap()
        .split_whitespace()
        .collect();
    if msg.len() == 2 && msg[0] == filename && msg[1] == auth {
        eprintln!("Got good message from client");
    } else {
        error!("Bad message; could not authenticate: {} {}", msg[0], msg[1]);
        return;
    }
    let buffer = crate::file::get_file_contents(filename).await.unwrap();
    tx.write_all(&buffer).await.unwrap();
    tx.finish().await.unwrap();
}
