use anyhow::Result;
use tokio::{
    fs::{read, File},
    io::AsyncWriteExt,
};

pub async fn get_file_contents(filename: &str) -> Result<Vec<u8>> {
    let contents = read(filename).await?;
    Ok(contents)
}

pub async fn dump_to_file(filename: &str, bytes: &[u8]) -> Result<()> {
    let mut fd = File::create(filename).await?;
    fd.write_all(bytes).await?;
    fd.flush().await?;
    Ok(())
}
