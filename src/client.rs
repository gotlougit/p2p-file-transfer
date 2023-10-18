use quinn::{ClientConfig, Endpoint, EndpointConfig};
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::net::UdpSocket;

/// Dummy certificate verifier that treats any certificate as valid.
/// NOTE, such verification is vulnerable to MITM attacks, but convenient for testing.
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

pub fn configure_client() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    ClientConfig::new(Arc::new(crypto))
}

pub async fn run_client(socket: UdpSocket, server_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    let client_config = configure_client();
    let mut endpoint = Endpoint::new(
        EndpointConfig::default(),
        None,
        socket.into_std().unwrap(),
        quinn::default_runtime().unwrap(),
    )
    .unwrap();
    endpoint.set_default_client_config(client_config);

    // connect to server
    let connection = endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();

    println!("[client] connected: addr={}", connection.remote_address());
    let (mut tx, mut rx) = connection.open_bi().await.unwrap();
    tx.write_all(b"Hello world this is test").await.unwrap();
    tx.finish().await.unwrap();
    let buf = rx.read_to_end(usize::max_value()).await.unwrap();
    let xyz = std::str::from_utf8(&buf).unwrap();
    if xyz.len() != 0 {
        println!("Got message: {}", xyz);
    }
    connection.close(0u32.into(), b"done");
    // Make sure the server has a chance to clean up
    endpoint.wait_idle().await;

    Ok(())
}
