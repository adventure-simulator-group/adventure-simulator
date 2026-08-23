use anyhow::Result;
use std::sync::Arc;

use crate::globals::WgpuContext;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl Image {
    pub async fn new(_context: WgpuContext, uri: String) -> Result<Image> {
        let bytes = fabelgeist_fs::read_bytes(&uri).await?;

        let img = image::load_from_memory(&bytes)?;
        let img = img.to_rgba8();
        let (width, height) = img.dimensions();

        Ok(Image {
            data: Arc::new(img.into_raw()),
            width,
            height,
        })
    }

    pub fn from_pixels(pixels: Vec<u8>, width: u32, height: u32) -> Result<Image> {
        Ok(Image {
            data: Arc::new(pixels),
            width,
            height,
        })
    }
}
