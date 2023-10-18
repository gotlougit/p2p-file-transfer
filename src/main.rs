use anyhow::Result;
use std::env;
use tracing::error;

mod client;
mod file;
mod nat;
mod server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    // TODO: use clap crate for CLI arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        error!("Insufficient args entered! Usage: ./program client <file_to_get> or ./program server <file_to_serve> <pre_shared_secret>");
    }

    // client mode or server mode
    let mode = &args[1];
    // pre shared secret which acts as a deterrent against MITM attacks
    let auth = &args[3];

    if mode == "server" {
        let filename = &args[2];
        let (serversock, _addr) = crate::nat::get_nat_traversed_socket().await.unwrap();
        crate::server::run_server(serversock).await;
    } else if mode == "client" {
        let file_to_get = &args[2];
        let (sock, server_addr) = crate::nat::get_nat_traversed_socket().await.unwrap();
        crate::client::run_client(sock, server_addr).await.unwrap();
    } else {
        error!("Incorrect args entered!");
    }
    Ok(())
}
