use quinn::{Endpoint, EndpointConfig, ServerConfig};
use std::{error::Error, sync::Arc};
use tokio::net::UdpSocket;

/// Constructs a QUIC endpoint configured to listen for incoming connections on a certain address
/// and port.
///
/// ## Returns
///
/// - a stream of incoming QUIC connections
/// - server certificate serialized into DER format
pub fn make_server_endpoint(socket: UdpSocket) -> Result<(Endpoint, Vec<u8>), Box<dyn Error>> {
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
pub fn configure_server() -> Result<(ServerConfig, Vec<u8>), Box<dyn Error>> {
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
pub async fn run_server(socket: UdpSocket) {
    let (endpoint, _server_cert) = make_server_endpoint(socket).unwrap();
    let conn = endpoint.accept().await.unwrap();
    let (mut tx, mut rx) = conn.await.unwrap().accept_bi().await.unwrap();
    let buf = rx.read_to_end(usize::max_value()).await.unwrap();
    let msg = std::str::from_utf8(&buf).unwrap();
    if msg.len() != 0 {
        println!("Got message: {}", msg);
    }
    tx.write_all(b"Hello world!\n").await.unwrap();
    tx.finish().await.unwrap();
}
