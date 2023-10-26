use crate::config::Config;
use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod config;
mod file;
mod nat;
mod server;

#[derive(Subcommand, PartialEq)]
enum Command {
    /// Request a file from a server
    Client {
        #[arg(required = true)]
        filename: String,
        #[arg(required = true)]
        auth: String,
    },
    /// Host a server
    Server {
        #[arg(required = true)]
        filename: String,
        #[arg(required = true)]
        auth: String,
    },
    /// Add a certificate to key store
    AddKey {
        #[arg(required = true)]
        key: String,
    },
}

/// Transfer files using UDP hole punching
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    if let Some(cmd) = args.command {
        let Config {
            public_key,
            private_key,
            trusted_keys,
        } = crate::config::get_all_vars()?;
        match cmd {
            Command::Server { filename, auth } => {
                let encoded_public_key = mnemonic::to_string(&public_key);
                println!("Your public key is: {}", encoded_public_key);
                println!(
                    "Please share this key to recipients for establishing a secure connection"
                );
                let (serversock, _addr) = crate::nat::get_nat_traversed_socket().await.unwrap();
                crate::server::run_server(serversock, &filename, &auth, public_key, private_key)
                    .await?;
            }
            Command::Client { filename, auth } => {
                let (sock, server_addr) = crate::nat::get_nat_traversed_socket().await.unwrap();
                crate::client::run_client(sock, server_addr, &filename, &auth, trusted_keys)
                    .await?;
            }
            Command::AddKey { key } => {
                let mut decoded_key = Vec::new();
                mnemonic::decode(key, &mut decoded_key)?;
                crate::config::add_trusted_key(decoded_key)?;
            }
        };
    }
    Ok(())
}
