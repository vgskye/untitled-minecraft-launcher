use std::path::Path;

use sha1::Digest;
use tauri::api::http::{ClientBuilder, HttpRequestBuilder, ResponseType};

pub async fn get_file(
    path: &Path,
    url: &str,
    redownload: bool,
    sha1: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    if !redownload {
        if let Ok(file) = tokio::fs::read(path).await {
            if let Some(sha1) = sha1 {
                let sha1 = hex::decode(sha1)?;
                let hash = ::sha1::Sha1::digest(&file);
                if sha1 == hash.as_slice() {
                    return Ok(file);
                }
            } else {
                return Ok(file);
            }
        }
    }
    let client = ClientBuilder::new().build()?;
    let file = client
        .send(HttpRequestBuilder::new("GET", url)?.response_type(ResponseType::Binary))
        .await?
        .bytes()
        .await?;
    if file.status != 200 {
        return Err(anyhow::anyhow!("Got status {} instead of 200", file.status));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, &file.data).await?;
    Ok(file.data)
}
