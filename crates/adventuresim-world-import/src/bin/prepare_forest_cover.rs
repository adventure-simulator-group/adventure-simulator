//! Download and prepare the bounded Copernicus 2018 forest-cover source.
//!
//! This program intentionally writes only the importer-facing, git-ignored
//! prepared rasters. OAuth client credentials are read from a local `.env`
//! file and are never written to disk or emitted in diagnostics.

use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::WorldBounds;
use clap::Parser;
use reqwest::{
    blocking::{Client, Response},
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tiff::{
    decoder::{Decoder, DecodingResult},
    encoder::{TiffEncoder, colortype::Gray8},
    tags::Tag,
};

const FORMAT: &str = "adventuresim-copernicus-forest-2018-v2";
const PIXELS_PER_DEGREE: u32 = 1_000;
const NODATA: u8 = 255;
const TOKEN_URL: &str =
    "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";
const PROCESS_URL: &str = "https://sh.dataspace.copernicus.eu/api/v1/process";
const TCD_COLLECTION: &str = "edd3c5f5-da8e-463f-8c9a-712aa451d37e";
const BCD_COLLECTION: &str = "a06a42ae-f899-4a07-a5cd-fb7fd920d6c1";
const CCD_COLLECTION: &str = "a0edd575-c763-4c4a-a910-631df3df4506";

#[derive(Debug, Parser)]
#[command(about = "Download bounded 2018 Copernicus forest-cover rasters from CDSE")]
struct Args {
    /// JSON file defining the southwest and northeast WGS84 corners to prepare.
    #[arg(long, value_name = "PATH")]
    world_bounds: PathBuf,
    /// Local OAuth settings. Only COPERNICUS_CLIENT_ID and COPERNICUS_CLIENT_SECRET are read.
    #[arg(long, default_value = ".env", value_name = "PATH")]
    env_file: PathBuf,
    #[arg(long, default_value_os_t = default_output_directory())]
    output_dir: PathBuf,
    /// Replace an existing prepared source directory only after every download succeeds.
    #[arg(long)]
    force: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DegreeTile {
    south: i16,
    west: i16,
}

impl DegreeTile {
    fn label(self) -> String {
        format!(
            "{}{:02}_{}{:03}",
            if self.south >= 0 { 'N' } else { 'S' },
            self.south.unsigned_abs(),
            if self.west >= 0 { 'E' } else { 'W' },
            self.west.unsigned_abs(),
        )
    }

    fn path(self, directory: &Path, layer: &str) -> PathBuf {
        directory.join(format!("{layer}_{}.tif", self.label()))
    }

    fn bbox(self) -> [f64; 4] {
        [
            f64::from(self.west),
            f64::from(self.south),
            f64::from(self.west) + 1.0,
            f64::from(self.south) + 1.0,
        ]
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    run(Args::parse())
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let bounds: WorldBounds = serde_json::from_slice(&fs::read(&args.world_bounds)?)?;
    let tiles = bounds
        .intersecting_one_degree_tiles()
        .into_iter()
        .map(|coordinate| DegreeTile {
            south: coordinate.latitude as i16,
            west: coordinate.longitude as i16,
        })
        .collect::<Vec<_>>();
    if tiles.is_empty() {
        return Err("world bounds did not select any one-degree tiles".into());
    }

    let required = required_paths(&args.output_dir, &tiles);
    if !args.force && required.iter().any(|path| path.exists()) {
        return Err(format!(
            "prepared forest source already exists under {}; use --force to replace it",
            args.output_dir.display()
        )
        .into());
    }
    let credentials = read_credentials(&args.env_file)?;
    let client = Client::builder()
        .user_agent("adventure-simulator-forest-preparer/1.0")
        .build()?;
    let token = access_token(&client, &credentials)?;
    let mut prepared = BTreeMap::new();
    for tile in tiles {
        println!(
            "Downloading Copernicus forest sources for {}...",
            tile.label()
        );
        let density = download_raster(&client, &token, tile, TCD_COLLECTION, "TCD")?;
        let broadleaf = download_raster(&client, &token, tile, BCD_COLLECTION, "BCD")?;
        let coniferous = download_raster(&client, &token, tile, CCD_COLLECTION, "CCD")?;
        let leaves = leaf_types(&density, &broadleaf, &coniferous)?;
        prepared.insert(tile, (density, leaves));
    }

    let staging = args.output_dir.with_extension("staging");
    if staging.exists() {
        return Err(format!("staging path already exists: {}", staging.display()).into());
    }
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        for (tile, (density, leaves)) in &prepared {
            write_raster(&tile.path(&staging, "TCD"), *tile, density)?;
            write_raster(&tile.path(&staging, "DLT"), *tile, leaves)?;
        }
        fs::write(
            staging.join("forest-cover-manifest.json"),
            format!("{{\"format\":\"{FORMAT}\"}}\n"),
        )?;
        if args.output_dir.exists() {
            fs::remove_dir_all(&args.output_dir)?;
        }
        fs::rename(&staging, &args.output_dir)?;
        Ok(())
    })();
    if result.is_err() && staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    result?;
    println!(
        "Prepared {} Copernicus forest tile pair(s) in {}.",
        prepared.len(),
        args.output_dir.display()
    );
    Ok(())
}

fn default_output_directory() -> PathBuf {
    PathBuf::from("target/world-data-sources/raw/forest-cover")
}

fn required_paths(directory: &Path, tiles: &[DegreeTile]) -> Vec<PathBuf> {
    let mut paths = vec![directory.join("forest-cover-manifest.json")];
    for tile in tiles {
        paths.push(tile.path(directory, "TCD"));
        paths.push(tile.path(directory, "DLT"));
    }
    paths
}

fn read_credentials(path: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let mut values = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("{} contains an invalid .env line", path.display()).into());
        };
        let value = value.trim().trim_matches(['\'', '"']);
        values.insert(key.trim().to_owned(), value.to_owned());
    }
    for name in ["COPERNICUS_CLIENT_ID", "COPERNICUS_CLIENT_SECRET"] {
        if values.get(name).is_none_or(String::is_empty) {
            return Err(format!("{} is missing {name}", path.display()).into());
        }
    }
    Ok(values)
}

fn access_token(
    client: &Client,
    credentials: &BTreeMap<String, String>,
) -> Result<String, Box<dyn Error>> {
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", credentials["COPERNICUS_CLIENT_ID"].as_str()),
            (
                "client_secret",
                credentials["COPERNICUS_CLIENT_SECRET"].as_str(),
            ),
        ])
        .send()?;
    let response = require_success(response, "CDSE OAuth token request")?;
    let body: Value = response.json()?;
    body.get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "CDSE OAuth token response did not contain an access token".into())
}

fn download_raster(
    client: &Client,
    token: &str,
    tile: DegreeTile,
    collection: &str,
    band: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let request = json!({
        "input": {
            "bounds": { "bbox": tile.bbox() },
            "data": [{
                "type": format!("byoc-{collection}"),
                "dataFilter": { "timeRange": {
                    "from": "2018-01-01T00:00:00Z",
                    "to": "2018-12-31T23:59:59Z"
                }},
                "processing": { "downsampling": "NEAREST" }
            }]
        },
        "output": {
            "width": PIXELS_PER_DEGREE,
            "height": PIXELS_PER_DEGREE,
            "responses": [{ "identifier": "default", "format": { "type": "image/tiff" } }]
        },
        "evalscript": evalscript(band),
    });
    let response = client
        .post(PROCESS_URL)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&request)
        .send()?;
    let response = require_success(
        response,
        &format!("CDSE {band} raster request for {}", tile.label()),
    )?;
    decode_raster(response.bytes()?.as_ref(), band)
}

fn require_success(response: Response, operation: &str) -> Result<Response, Box<dyn Error>> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().unwrap_or_default();
    let detail = body.chars().take(500).collect::<String>();
    Err(format!("{operation} failed with HTTP {status}: {detail}").into())
}

fn evalscript(band: &str) -> String {
    format!(
        "//VERSION=3\nfunction setup() {{ return {{ input: ['{band}', 'dataMask'], output: {{ bands: 1, sampleType: 'UINT8' }} }}; }}\nfunction evaluatePixel(sample) {{ return [sample.dataMask ? sample.{band} : 255]; }}"
    )
}

fn decode_raster(bytes: &[u8], label: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut decoder = Decoder::new(Cursor::new(bytes))?;
    let (width, height) = decoder.dimensions()?;
    if (width, height) != (PIXELS_PER_DEGREE, PIXELS_PER_DEGREE) {
        return Err(format!(
            "CDSE {label} response is {width}x{height}; expected {PIXELS_PER_DEGREE}x{PIXELS_PER_DEGREE}"
        )
        .into());
    }
    match decoder.read_image()? {
        DecodingResult::U8(values) if values.len() == (width as usize * height as usize) => {
            Ok(values)
        }
        DecodingResult::U8(_) => Err(format!("CDSE {label} response is not single-band").into()),
        _ => Err(format!("CDSE {label} response is not an UInt8 TIFF").into()),
    }
}

fn leaf_types(
    density: &[u8],
    broadleaf: &[u8],
    coniferous: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    if density.len() != broadleaf.len() || density.len() != coniferous.len() {
        return Err("CDSE forest layers have mismatched raster sizes".into());
    }
    density
        .iter()
        .zip(broadleaf)
        .zip(coniferous)
        .map(|((&density, &broadleaf), &coniferous)| {
            if density == NODATA || broadleaf == NODATA || coniferous == NODATA {
                return Ok(NODATA);
            }
            if density > 100 || broadleaf > 100 || coniferous > 100 {
                return Err("CDSE forest source returned a value outside 0..=100 or 255".into());
            }
            let total = u16::from(broadleaf) + u16::from(coniferous);
            Ok(match total {
                0 => NODATA,
                total if u16::from(broadleaf) * 4 >= total * 3 => 1,
                total if u16::from(coniferous) * 4 >= total * 3 => 2,
                _ => 3,
            })
        })
        .collect()
}

fn write_raster(path: &Path, tile: DegreeTile, values: &[u8]) -> Result<(), Box<dyn Error>> {
    if values.len() != (PIXELS_PER_DEGREE as usize).pow(2) {
        return Err(format!("{} has an invalid pixel count", path.display()).into());
    }
    let temporary = path.with_extension("tif.tmp");
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut encoder = TiffEncoder::new(File::create(&temporary)?)?;
        let mut image = encoder.new_image::<Gray8>(PIXELS_PER_DEGREE, PIXELS_PER_DEGREE)?;
        image
            .encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[0.001_f64, 0.001, 0.0][..])?;
        image.encoder().write_tag(
            Tag::ModelTiepointTag,
            &[
                0.0_f64,
                0.0,
                0.0,
                f64::from(tile.west),
                f64::from(tile.south) + 1.0,
                0.0,
            ][..],
        )?;
        image.encoder().write_tag(
            Tag::GeoKeyDirectoryTag,
            &[
                1_u16, 1, 0, 3, 1024, 0, 1, 2, 1025, 0, 1, 1, 2048, 0, 1, 4326,
            ][..],
        )?;
        image.write_data(values)?;
        Ok(())
    })();
    if result.is_ok() {
        fs::rename(&temporary, path)?;
    } else if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{NODATA, leaf_types};

    #[test]
    fn official_cover_densities_map_to_the_prepared_leaf_contract() {
        assert_eq!(
            leaf_types(
                &[40, 40, 40, 0, NODATA],
                &[75, 25, 50, 0, 100],
                &[25, 75, 50, 0, 0]
            )
            .unwrap(),
            vec![1, 2, 3, NODATA, NODATA],
        );
    }
}
