use anyhow::{Result, anyhow};
use std::sync::Arc;

use crate::globals::WgpuContext;

#[derive(Clone, Debug, Default)]
pub struct Image {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl Image {
    pub fn new(_context: &WgpuContext, uri: String) -> Result<Image> {
        let bytes = if uri.starts_with("data:") {
            load_data_uri(&uri)?
        } else if uri.starts_with("http") {
            #[cfg(not(target_arch = "wasm32"))]
            {
                pollster::block_on(async {
                    let response = reqwest::get(&uri).await?;
                    Ok::<Vec<u8>, anyhow::Error>(response.bytes().await?.to_vec())
                })
                .map_err(|e| anyhow!("Failed to fetch image: {}", e))?
            }
            #[cfg(target_arch = "wasm32")]
            {
                return Err(anyhow!(
                    "Async HTTP loading not yet supported on WASM in this synchronous runtime. Please use data:// for now."
                ));
            }
        } else if uri.starts_with("file://") {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = uri.strip_prefix("file://").unwrap();
                // Handle file:///C:/path
                let path = if path.starts_with("/") && path.len() > 3 && path.as_bytes()[2] == b':'
                {
                    &path[1..]
                } else {
                    path
                };
                std::fs::read(path).map_err(|e| anyhow!("Failed to read file: {}", e))?
            }
            #[cfg(target_arch = "wasm32")]
            {
                return Err(anyhow!("file:// is not supported on web"));
            }
        } else {
            return Err(anyhow!("Unsupported URI scheme: {}", uri));
        };

        let img = image::load_from_memory(&bytes)?;
        let img = img.to_rgba8();
        let (width, height) = img.dimensions();

        Ok(Image {
            data: Arc::new(img.into_raw()),
            width,
            height,
        })
    }
}

fn load_data_uri(uri: &str) -> anyhow::Result<Vec<u8>> {
    let comma_pos = uri.find(',').ok_or_else(|| anyhow!("Invalid data URI"))?;
    let data = &uri[comma_pos + 1..];
    if uri[..comma_pos].contains(";base64") {
        use base64::{Engine as _, engine::general_purpose};
        Ok(general_purpose::STANDARD.decode(data)?)
    } else {
        Ok(percent_encoding::percent_decode_str(data).collect())
    }
}
