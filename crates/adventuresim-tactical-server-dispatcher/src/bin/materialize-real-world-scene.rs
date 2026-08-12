//! Materialize a deterministic tactical scene at a geographic coordinate.

use std::path::PathBuf;

use adventuresim_tactical_server_dispatcher::scene_input::{
    build_imported_scene, materialize_scene_input,
};
use adventuresim_terrain::{TerrainPack, TerrainPurpose};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Sample the final real-world terrain pack into a tactical scene document")]
struct Args {
    /// Latitude in signed WGS84 decimal degrees.
    #[arg(long, allow_hyphen_values = true)]
    latitude: f64,

    /// Longitude in signed WGS84 decimal degrees.
    #[arg(long, allow_hyphen_values = true)]
    longitude: f64,

    /// Absolute world minute used for deterministic weather and celestial state.
    #[arg(long, default_value_t = 340_320)]
    absolute_minute: u64,

    /// Stable semantic name included in the scene document.
    #[arg(long, default_value = "real-world-coordinate")]
    scene_key: String,

    /// Final terrain manifest produced by build-strategic-map.
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v3.json")]
    terrain_manifest: PathBuf,

    /// Final compressed terrain pack paired with terrain-manifest.
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v3.pack")]
    terrain_pack: PathBuf,

    /// Directory for immutable coordinate-derived scene documents.
    #[arg(long, default_value = "target/tactical-real-world-scenes")]
    output_dir: PathBuf,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let latitude_e7 = coordinate_e7("latitude", args.latitude, -90.0, 90.0)?;
    let longitude_e7 = coordinate_e7("longitude", args.longitude, -180.0, 180.0)?;
    let terrain = TerrainPack::load(&args.terrain_manifest, &args.terrain_pack)
        .map_err(|error| format!("failed to load final terrain pack: {error}"))?;
    if terrain.purpose() != TerrainPurpose::Final {
        return Err("real-world scenes require the final terrain pack".into());
    }
    let mission_id = format!(
        "capture:{}:{latitude_e7}:{longitude_e7}:{}:{}",
        terrain.digest(),
        args.absolute_minute,
        args.scene_key
    );
    let input = build_imported_scene(
        &terrain,
        &mission_id,
        &args.scene_key,
        latitude_e7,
        longitude_e7,
        args.absolute_minute,
    )?;
    let path = materialize_scene_input(&args.output_dir, &mission_id, &input)?;
    println!(
        "TACTICAL_REAL_WORLD_SCENE={}",
        path.canonicalize().unwrap_or(path).display()
    );
    println!("TERRAIN_PACKAGE_SHA256={}", terrain.digest());
    println!("COORDINATES={:.7},{:.7}", args.latitude, args.longitude);
    Ok(())
}

fn coordinate_e7(name: &str, value: f64, minimum: f64, maximum: f64) -> Result<i32, String> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(format!(
            "{name} must be finite and within {minimum}..={maximum}"
        ));
    }
    let scaled = (value * 10_000_000.0).round();
    if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&scaled) {
        return Err(format!("{name} cannot be represented as signed E7 degrees"));
    }
    Ok(scaled as i32)
}

#[cfg(test)]
mod tests {
    use super::coordinate_e7;

    #[test]
    fn decimal_coordinates_round_to_stable_e7_values() {
        assert_eq!(
            coordinate_e7("latitude", 53.5503412, -90.0, 90.0),
            Ok(535_503_412)
        );
        assert_eq!(
            coordinate_e7("longitude", 9.992_345_67, -180.0, 180.0),
            Ok(99_923_457)
        );
        assert!(coordinate_e7("latitude", 90.000_000_1, -90.0, 90.0).is_err());
        assert!(coordinate_e7("longitude", f64::NAN, -180.0, 180.0).is_err());
    }
}
