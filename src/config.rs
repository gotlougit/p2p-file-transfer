use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use toml::Table;

pub struct Config {
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
    pub trusted_keys: Vec<Vec<u8>>,
}

fn get_config_path() -> String {
    match env::var_os("XDG_CONFIG_HOME") {
        Some(path) => {
            let parentconfdir = path.to_str().unwrap_or_default().to_string();
            parentconfdir + "/filetransfer"
        }
        None => {
            let home_dir = env::var_os("HOME").unwrap();
            let parentconfdir = home_dir.into_string().unwrap().to_string();
            parentconfdir + ".config/filetransfer"
        }
    }
}

fn create_config_folder(config_path: &str, public_key: &[u8], private_key: &[u8]) -> Result<()> {
    eprintln!("Creating the config folder and default config");
    fs::create_dir(config_path)?;
    let mut config = Table::new();
    let encoded_public_key = general_purpose::STANDARD.encode(public_key);
    let encoded_private_key = general_purpose::STANDARD.encode(private_key);
    let trusted_keys: Vec<String> = Vec::new();
    config.insert("public_key".to_string(), encoded_public_key.into());
    config.insert("private_key".to_string(), encoded_private_key.into());
    config.insert("trusted_keys".to_string(), trusted_keys.into());
    let rawconf = config.to_string();
    let conffilename = config_path.to_string() + "/filetransfer.toml";
    let mut file = fs::File::create(conffilename)?;
    file.write_all(rawconf.as_bytes())?;
    Ok(())
}

pub fn get_all_vars() -> Result<Config> {
    let config_path = get_config_path();
    if !Path::new(&config_path).exists() {
        // generate a new certificate and store in config file
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.serialize_der().unwrap();
        let priv_key = cert.serialize_private_key_der();
        create_config_folder(&config_path, &cert_der, &priv_key)?;
    }
    let file_path = config_path + "/filetransfer.toml";
    let rawconf = fs::read_to_string(file_path)?;
    let conf: Table = rawconf.parse()?;
    // Get the keys from the config file
    // TODO: Figure out a way to remove these quotation marks elegantly
    let encoded_public_key = conf
        .get("public_key")
        .unwrap()
        .as_str()
        .unwrap()
        .replace('\"', "");
    let public_key = general_purpose::STANDARD.decode(encoded_public_key)?;

    let encoded_private_key = conf
        .get("private_key")
        .unwrap()
        .as_str()
        .unwrap()
        .replace('\"', "");
    let private_key = general_purpose::STANDARD.decode(encoded_private_key)?;

    let trusted_keys: Vec<_> = conf
        .get("trusted_keys")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|val| {
            general_purpose::STANDARD
                .decode(val.as_str().unwrap())
                .unwrap()
        })
        .collect();

    Ok(Config {
        public_key,
        private_key,
        trusted_keys,
    })
}
