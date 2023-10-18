use anyhow::Result;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

pub async fn get_file_contents(filename: &str) -> Result<Vec<u8>> {
    let mut fd = File::open(filename).await?;
    let mut buffer = Vec::new();
    fd.read(&mut buffer).await?;
    Ok(buffer)
}

pub async fn dump_to_file(filename: &str, bytes: &[u8]) -> Result<()> {
    let mut fd = File::create(filename).await?;
    fd.write(bytes).await?;
    Ok(())
}
