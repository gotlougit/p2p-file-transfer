use anyhow::Result;
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
    let encoded_public_key = mnemonic::to_string(public_key);
    let encoded_private_key = mnemonic::to_string(private_key);
    let trusted_keys: Vec<String> = Vec::new();
    write_config(
        config_path,
        &encoded_public_key,
        &encoded_private_key,
        trusted_keys,
    )
}

fn write_config(
    config_path: &str,
    public_key: &str,
    private_key: &str,
    trusted_keys: Vec<String>,
) -> Result<()> {
    let mut config = Table::new();
    config.insert("public_key".to_string(), public_key.into());
    config.insert("private_key".to_string(), private_key.into());
    config.insert("trusted_keys".to_string(), trusted_keys.into());
    let rawconf = config.to_string();
    let conffilename = config_path.to_string() + "/filetransfer.toml";
    let mut file = fs::File::create(conffilename)?;
    file.write_all(rawconf.as_bytes())?;
    Ok(())
}

pub fn add_trusted_key(key: Vec<u8>) -> Result<()> {
    let config_path = get_config_path();
    let mut conf = get_all_vars().unwrap();
    conf.trusted_keys.push(key);
    let encoded_public_key = mnemonic::to_string(conf.public_key);
    let encoded_private_key = mnemonic::to_string(conf.private_key);
    let encoded_trusted_keys: Vec<_> = conf.trusted_keys.iter().map(mnemonic::to_string).collect();

    write_config(
        &config_path,
        &encoded_public_key,
        &encoded_private_key,
        encoded_trusted_keys,
    )
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

    let mut public_key = Vec::new();
    mnemonic::decode(encoded_public_key, &mut public_key)?;
    let encoded_private_key = conf
        .get("private_key")
        .unwrap()
        .as_str()
        .unwrap()
        .replace('\"', "");
    let mut private_key = Vec::new();
    mnemonic::decode(encoded_private_key, &mut private_key)?;

    let trusted_keys: Vec<_> = conf
        .get("trusted_keys")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|val| {
            let mut decoded_val = Vec::new();
            mnemonic::decode(val.as_str().unwrap(), &mut decoded_val).unwrap();
            decoded_val
        })
        .collect();

    Ok(Config {
        public_key,
        private_key,
        trusted_keys,
    })
}
