//! Download one WCS 2.0.1 coverage subset per required SoilGrids layer.
//!
//! SoilGrids has no stable small-tile REST contract.  This deliberately asks
//! its official WCS for exactly the canonical `WorldBounds` rectangle and
//! stores the service's native bounded GeoTIFF response unchanged.

use std::{
    error::Error,
    fs,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
    time::Duration,
};

use adventuresim_world_schema::WorldBounds;
use clap::Parser;
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};

const FORMAT: &str = "adventuresim-soilgrids-2.0-0-5cm-v1";
const SOURCE_URL: &str = "https://maps.isric.org/";
const WCS_ROOT: &str = "https://maps.isric.org/mapserv";
const LAYERS: [&str; 6] = ["sand", "silt", "clay", "soc", "cfvo", "bdod"];
const MAX_DOWNLOAD_BYTES: usize = 64 * 1024 * 1024;
const MAX_RASTER_PIXELS: u64 = 16_000_000;
const MAX_PIXEL_DEGREES: f64 = 0.01;
const MAX_ENVELOPE_EXPANSION_DEGREES: f64 = 0.05;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedManifest {
    format: String,
    source_url: String,
    layers: Vec<String>,
    world_bounds: Value,
}

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
    if args.output_dir.exists() && !args.force {
        return Err(format!(
            "prepared SoilGrids cache already exists under {}; use --force to replace it",
            args.output_dir.display()
        )
        .into());
    }
    if args.force && args.output_dir.exists() {
        validate_existing_cache(&args.output_dir)?;
    }
    let staging = args.output_dir.with_extension("staging");
    if staging == args.output_dir {
        return Err("output directory must not end with .staging".into());
    }
    if staging.exists() {
        return Err(format!("staging path already exists: {}", staging.display()).into());
    }
    fs::create_dir_all(&staging)?;
    let client = Client::builder()
        .user_agent("adventure-simulator-soilgrids-preparer/1.0")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .build()?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut grid = None;
        for layer in LAYERS {
            println!("Downloading SoilGrids {layer} 0--5cm WCS subset...");
            let bytes = download_layer(&client, &bounds, layer)?;
            let candidate = validate_wcs_tiff(&bytes, layer, &bounds)?;
            if grid
                .replace(candidate)
                .is_some_and(|existing| existing != candidate)
            {
                return Err("SoilGrids WCS layers do not share one exact GeoTIFF grid".into());
            }
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
fn validate_existing_cache(directory: &Path) -> Result<(), Box<dyn Error>> {
    let path = directory.join("soilgrids-manifest.json");
    let manifest: PreparedManifest = serde_json::from_slice(&fs::read(&path)?)?;
    if manifest.format != FORMAT || manifest.source_url != SOURCE_URL || manifest.layers != LAYERS {
        return Err(format!(
            "refusing to replace {}: it is not a recognized SoilGrids cache",
            directory.display()
        )
        .into());
    }
    let _: WorldBounds = serde_json::from_value(manifest.world_bounds)?;
    if required_paths(directory).iter().any(|path| !path.is_file()) {
        return Err(format!(
            "refusing to replace {}: recognized manifest has incomplete cache files",
            directory.display()
        )
        .into());
    }
    Ok(())
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
    let response = require_success(response, &format!("SoilGrids WCS {layer}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES as u64)
    {
        return Err(
            format!("SoilGrids WCS {layer} response exceeds {MAX_DOWNLOAD_BYTES} bytes").into(),
        );
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_DOWNLOAD_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err(
            format!("SoilGrids WCS {layer} response exceeds {MAX_DOWNLOAD_BYTES} bytes").into(),
        );
    }
    Ok(bytes)
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
fn validate_wcs_tiff(
    bytes: &[u8],
    layer: &str,
    bounds: &WorldBounds,
) -> Result<Grid, Box<dyn Error>> {
    let mut decoder = Decoder::new(Cursor::new(bytes))
        .map_err(|error| format!("SoilGrids WCS {layer} did not return TIFF: {error}"))?;
    let (width, height) = decoder.dimensions()?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_RASTER_PIXELS {
        return Err(format!("SoilGrids WCS {layer} response is empty").into());
    }
    match decoder.read_image()? {
        DecodingResult::I16(values) if values.len() == width as usize * height as usize => {
            let grid = Grid::parse(&mut decoder)?;
            grid.validate_bounds(bounds, width, height)?;
            Ok(grid)
        }
        _ => {
            Err(format!("SoilGrids WCS {layer} response is not a single-band Int16 GeoTIFF").into())
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Grid {
    west: f64,
    north: f64,
    x_scale: f64,
    y_scale: f64,
}
impl Grid {
    fn parse(reader: &mut Decoder<impl Read + Seek>) -> Result<Self, Box<dyn Error>> {
        let scale = reader.get_tag_f64_vec(Tag::ModelPixelScaleTag)?;
        let tie = reader.get_tag_f64_vec(Tag::ModelTiepointTag)?;
        let keys = reader.get_tag_u16_vec(Tag::GeoKeyDirectoryTag)?;
        if scale.len() != 3
            || tie.len() != 6
            || geo_key(&keys, 1024) != Some(2)
            || geo_key(&keys, 1025) != Some(1)
            || geo_key(&keys, 2048) != Some(4326)
            || !scale[0].is_finite()
            || !scale[1].is_finite()
            || scale[0] <= 0.0
            || scale[1] <= 0.0
            || scale[0] > MAX_PIXEL_DEGREES
            || scale[1] > MAX_PIXEL_DEGREES
        {
            return Err(
                "SoilGrids WCS response has invalid EPSG:4326 RasterPixelIsArea metadata".into(),
            );
        }
        Ok(Self {
            west: tie[3] - tie[0] * scale[0],
            north: tie[4] + tie[1] * scale[1],
            x_scale: scale[0],
            y_scale: scale[1],
        })
    }
    fn validate_bounds(
        self,
        bounds: &WorldBounds,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        let value = serde_json::to_value(bounds)?;
        let south = value["south_west"]["latitude"]
            .as_f64()
            .ok_or("invalid south latitude")?;
        let west = value["south_west"]["longitude"]
            .as_f64()
            .ok_or("invalid west longitude")?;
        let north = value["north_east"]["latitude"]
            .as_f64()
            .ok_or("invalid north latitude")?;
        let east = value["north_east"]["longitude"]
            .as_f64()
            .ok_or("invalid east longitude")?;
        let raster_east = self.west + self.x_scale * f64::from(width);
        let raster_south = self.north - self.y_scale * f64::from(height);
        let e = 1e-9;
        if self.west < west - self.x_scale - e
            || self.west > west + self.x_scale + e
            || self.north < north - self.y_scale - e
            || self.north > north + self.y_scale + e
            || raster_east < east - self.x_scale - e
            || raster_east > east + self.x_scale + e
            || raster_south < south - self.y_scale - e
            || raster_south > south + self.y_scale + e
            || self.west < west - MAX_ENVELOPE_EXPANSION_DEGREES
            || self.north > north + MAX_ENVELOPE_EXPANSION_DEGREES
            || raster_east > east + MAX_ENVELOPE_EXPANSION_DEGREES
            || raster_south < south - MAX_ENVELOPE_EXPANSION_DEGREES
        {
            return Err(
                "SoilGrids WCS response envelope does not match requested world bounds".into(),
            );
        }
        Ok(())
    }
}
fn geo_key(keys: &[u16], requested: u16) -> Option<u16> {
    let [1, 1, _, count, entries @ ..] = keys else {
        return None;
    };
    if entries.len() != usize::from(*count) * 4 {
        return None;
    };
    entries.as_chunks::<4>().0.iter().find_map(|entry| {
        (entry[0] == requested && entry[1] == 0 && entry[2] == 1).then_some(entry[3])
    })
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
