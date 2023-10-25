use quinn::{ClientConfig, Endpoint, EndpointConfig};
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::net::UdpSocket;

pub fn configure_client(trusted_keys: Vec<Vec<u8>>) -> ClientConfig {
    let mut root_store = rustls::RootCertStore::empty();
    for key in trusted_keys {
        let cert = rustls::Certificate(key);
        root_store.add(&cert).unwrap();
    }
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    ClientConfig::new(Arc::new(crypto))
}

pub async fn run_client(
    socket: UdpSocket,
    server_addr: SocketAddr,
    filename: &str,
    auth: &str,
    trusted_keys: Vec<Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    let client_config = configure_client(trusted_keys);
    let mut endpoint = Endpoint::new(
        EndpointConfig::default(),
        None,
        socket.into_std().unwrap(),
        quinn::default_runtime().unwrap(),
    )
    .unwrap();
    endpoint.set_default_client_config(client_config);

    // connect to server
    match endpoint.connect(server_addr, "localhost").unwrap().await {
        Ok(connection) => {
            match connection.open_bi().await {
                Ok((mut tx, mut rx)) => {
                    tx.write_all(format!("{} {}", filename, auth).as_bytes())
                        .await
                        .unwrap();
                    tx.finish().await.unwrap();
                    let data_buffer = rx.read_to_end(usize::MAX).await.unwrap();
                    connection.close(0u32.into(), b"done");
                    // Make sure the server has a chance to clean up
                    endpoint.wait_idle().await;
                    crate::file::dump_to_file(filename, &data_buffer)
                        .await
                        .unwrap();
                    Ok(())
                }
                Err(_) => {
                    eprintln!("This certificate is not trusted! Please mark it as such!");
                    Ok(())
                }
            }
        }
        Err(_) => {
            eprintln!("This certificate is not trusted! Please mark it as such!");
            Ok(())
        }
    }
}
