//! Materialize a deterministic tactical scene at a geographic coordinate.

use std::path::PathBuf;

use adventuresim_tactical_core::prelude::TACTICAL_SCENE_GENERATION_VERSION;
use adventuresim_tactical_server_dispatcher::scene_input::{
    build_imported_scene, materialize_scene_input,
};
use adventuresim_terrain::{TerrainPack, TerrainPurpose};
use adventuresim_world_schema::coordinates::{LatitudeE7, LongitudeE7};
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
    let latitude_e7 = LatitudeE7::from_degrees(args.latitude)
        .ok_or("latitude must be finite and within -90..=90")?
        .get();
    let longitude_e7 = LongitudeE7::from_degrees(args.longitude)
        .ok_or("longitude must be finite and within -180..=180")?
        .get();
    let terrain = TerrainPack::load(&args.terrain_manifest, &args.terrain_pack)
        .map_err(|error| format!("failed to load final terrain pack: {error}"))?;
    if terrain.purpose() != TerrainPurpose::Final {
        return Err("real-world scenes require the final terrain pack".into());
    }
    let mission_id = format!(
        "capture:v{}:{}:{latitude_e7}:{longitude_e7}:{}:{}:{}",
        TACTICAL_SCENE_GENERATION_VERSION,
        terrain.digest(),
        args.absolute_minute,
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

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::coordinates::{LatitudeE7, LongitudeE7};

    #[test]
    fn decimal_coordinates_round_to_stable_e7_values() {
        assert_eq!(
            LatitudeE7::from_degrees(53.5503412).map(LatitudeE7::get),
            Some(535_503_412)
        );
        assert_eq!(
            LongitudeE7::from_degrees(9.992_345_67).map(LongitudeE7::get),
            Some(99_923_457)
        );
        assert!(LatitudeE7::from_degrees(90.000_000_1).is_none());
        assert!(LongitudeE7::from_degrees(f64::NAN).is_none());
    }
}
