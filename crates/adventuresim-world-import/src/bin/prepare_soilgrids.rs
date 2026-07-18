//! Download one WCS 2.0.1 coverage subset per required SoilGrids layer.
//!
//! SoilGrids has no stable small-tile REST contract.  This deliberately asks
//! its official WCS for exactly the canonical `WorldBounds` rectangle and
//! stores the service's native bounded GeoTIFF response unchanged.

use std::{
    error::Error,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::WorldBounds;
use clap::Parser;
use reqwest::blocking::{Client, Response};
use serde_json::json;
use tiff::decoder::{Decoder, DecodingResult};

const FORMAT: &str = "adventuresim-soilgrids-2.0-0-5cm-v1";
const SOURCE_URL: &str = "https://maps.isric.org/";
const WCS_ROOT: &str = "https://maps.isric.org/mapserv";
const LAYERS: [&str; 6] = ["sand", "silt", "clay", "soc", "cfvo", "bdod"];

#[derive(Debug, Parser)]
#[command(about = "Download bounded SoilGrids 2.0 physical-layer WCS subsets")]
struct Args {
    /// JSON file defining the southwest and northeast WGS84 corners.
    #[arg(long, value_name = "PATH")]
    world_bounds: PathBuf,
    #[arg(long, default_value_os_t = default_output_directory())]
    output_dir: PathBuf,
    /// Replace the previous cache only after all six WCS requests succeed.
    #[arg(long)]
    force: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    run(Args::parse())
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let bounds: WorldBounds = serde_json::from_slice(&fs::read(&args.world_bounds)?)?;
    let required = required_paths(&args.output_dir);
    if !args.force && required.iter().any(|path| path.exists()) {
        return Err(format!(
            "prepared SoilGrids cache already exists under {}; use --force to replace it",
            args.output_dir.display()
        )
        .into());
    }
    let staging = args.output_dir.with_extension("staging");
    if staging.exists() {
        return Err(format!("staging path already exists: {}", staging.display()).into());
    }
    fs::create_dir_all(&staging)?;
    let client = Client::builder()
        .user_agent("adventure-simulator-soilgrids-preparer/1.0")
        .build()?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        for layer in LAYERS {
            println!("Downloading SoilGrids {layer} 0--5cm WCS subset...");
            let bytes = download_layer(&client, &bounds, layer)?;
            validate_wcs_tiff(&bytes, layer)?;
            fs::write(staging.join(filename(layer)), bytes)?;
        }
        fs::write(
            staging.join("soilgrids-manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "format": FORMAT, "source_url": SOURCE_URL, "layers": LAYERS, "world_bounds": bounds,
            }))?,
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
        "Prepared {} SoilGrids 2.0 rasters in {}.",
        LAYERS.len(),
        args.output_dir.display()
    );
    Ok(())
}

fn default_output_directory() -> PathBuf {
    PathBuf::from("target/world-data-sources/raw/soilgrids")
}
fn filename(layer: &str) -> String {
    format!("{layer}_0-5cm_mean.tif")
}
fn required_paths(directory: &Path) -> Vec<PathBuf> {
    std::iter::once(directory.join("soilgrids-manifest.json"))
        .chain(
            LAYERS
                .into_iter()
                .map(|layer| directory.join(filename(layer))),
        )
        .collect()
}

fn download_layer(
    client: &Client,
    bounds: &WorldBounds,
    layer: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let bounds_json = serde_json::to_value(bounds)?;
    let south = bounds_json["south_west"]["latitude"]
        .as_f64()
        .ok_or("invalid south latitude")?;
    let west = bounds_json["south_west"]["longitude"]
        .as_f64()
        .ok_or("invalid west longitude")?;
    let north = bounds_json["north_east"]["latitude"]
        .as_f64()
        .ok_or("invalid north latitude")?;
    let east = bounds_json["north_east"]["longitude"]
        .as_f64()
        .ok_or("invalid east longitude")?;
    let map = format!("/map/{layer}.map");
    let coverage = format!("{layer}_0-5cm_mean");
    let x_subset = format!("X({west},{east})");
    let y_subset = format!("Y({south},{north})");
    let url = reqwest::Url::parse_with_params(
        WCS_ROOT,
        [
            ("map", map.as_str()),
            ("SERVICE", "WCS"),
            ("VERSION", "2.0.1"),
            ("REQUEST", "GetCoverage"),
            ("COVERAGEID", coverage.as_str()),
            ("FORMAT", "GEOTIFF_INT16"),
            (
                "SUBSETTINGCRS",
                "http://www.opengis.net/def/crs/EPSG/0/4326",
            ),
            ("OUTPUTCRS", "http://www.opengis.net/def/crs/EPSG/0/4326"),
            ("SUBSET", x_subset.as_str()),
            ("SUBSET", y_subset.as_str()),
        ],
    )?;
    let response = client.get(url).send()?;
    require_success(response, &format!("SoilGrids WCS {layer}"))?
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(Into::into)
}
fn require_success(response: Response, operation: &str) -> Result<Response, Box<dyn Error>> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let detail = response
        .text()
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect::<String>();
    Err(format!("{operation} failed with HTTP {status}: {detail}").into())
}
fn validate_wcs_tiff(bytes: &[u8], layer: &str) -> Result<(), Box<dyn Error>> {
    let mut decoder = Decoder::new(Cursor::new(bytes))
        .map_err(|error| format!("SoilGrids WCS {layer} did not return TIFF: {error}"))?;
    let (width, height) = decoder.dimensions()?;
    if width == 0 || height == 0 {
        return Err(format!("SoilGrids WCS {layer} response is empty").into());
    }
    match decoder.read_image()? {
        DecodingResult::I16(values) if values.len() == width as usize * height as usize => Ok(()),
        _ => {
            Err(format!("SoilGrids WCS {layer} response is not a single-band Int16 GeoTIFF").into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{filename, required_paths};
    use std::path::Path;
    #[test]
    fn cache_contract_has_exact_six_layer_names() {
        let paths = required_paths(Path::new("cache"));
        assert_eq!(paths.len(), 7);
        assert_eq!(filename("sand"), "sand_0-5cm_mean.tif");
    }
}
