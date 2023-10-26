use anyhow::Result;
use quinn::{ConnectionError, Endpoint, EndpointConfig, ServerConfig};
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
fn make_server_endpoint(
    socket: UdpSocket,
    public_key: Vec<u8>,
    private_key: Vec<u8>,
) -> Result<(Endpoint, Vec<u8>)> {
    let (server_config, server_cert) = configure_server(public_key, private_key)?;
    let endpoint = Endpoint::new(
        EndpointConfig::default(),
        Some(server_config),
        socket.into_std()?,
        quinn::default_runtime().unwrap(),
    )?;
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server(cert_der: Vec<u8>, priv_key: Vec<u8>) -> Result<(ServerConfig, Vec<u8>)> {
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}

/// Runs a QUIC server bound to given address.
pub async fn run_server(
    socket: UdpSocket,
    filename: &str,
    auth: &str,
    public_key: Vec<u8>,
    private_key: Vec<u8>,
) -> Result<()> {
    let (endpoint, _server_cert) = make_server_endpoint(socket, public_key, private_key)?;
    let conn = endpoint.accept().await.unwrap();
    match conn.await {
        Ok(conn) => {
            let (mut tx, mut rx) = conn.accept_bi().await?;
            let buf = rx.read_to_end(usize::max_value()).await?;
            let msg: Vec<_> = std::str::from_utf8(&buf)
                .unwrap()
                .split_whitespace()
                .collect();
            if msg.len() == 2 && msg[0] == filename && msg[1] == auth {
                eprintln!("Got good message from client");
            } else {
                error!("Bad message; could not authenticate: {} {}", msg[0], msg[1]);
            }
            let buffer = crate::file::get_file_contents(filename).await?;
            tx.write_all(&buffer).await?;
            tx.finish().await?;
        }
        Err(ConnectionError::ConnectionClosed(close)) => {
            if close.error_code == quinn_proto::TransportErrorCode::APPLICATION_ERROR {
                eprintln!("This certificate is not trusted by the client! Please have the client mark it as trusted");
            }
        }
        _ => todo!(),
    };
    Ok(())
}
