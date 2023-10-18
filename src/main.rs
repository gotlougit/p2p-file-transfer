use anyhow::Result;
use std::env;
use tracing::error;

mod client;
mod nat;
mod server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        error!("Insufficient args entered! Usage: ./program client <file_to_get> or ./program server <file_to_serve>");
    }

    let mode = &args[1];

    let auth = String::from("11111111"); //for testing purposes only!

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
