//! Export the authoritative mesh and bounded CPU-generation evidence for review.

use std::{fs, path::PathBuf, time::Instant};

use adventuresim_tactical_core::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input_path = PathBuf::from(args.next().ok_or("expected scene input path")?);
    let output = PathBuf::from(args.next().ok_or("expected fresh output directory")?);
    if output.exists() {
        return Err("output already exists".into());
    }
    let input = TacticalSceneInput::load(&input_path)?;
    let mut durations = Vec::new();
    let mut generated = None;
    for _ in 0..3 {
        let start = Instant::now();
        let candidate = input.generate()?;
        durations.push(start.elapsed().as_secs_f64() * 1000.0);
        if let Some(previous) = &generated {
            let previous: &GeneratedTacticalScene = previous;
            if previous.terrain_patch != candidate.terrain_patch {
                return Err("nondeterministic mesh".into());
            }
        }
        generated = Some(candidate);
    }
    let generated = generated.ok_or("missing generated scene")?;
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("mesh.json"),
        serde_json::to_vec(&generated.terrain_patch)?,
    )?;
    fs::write(
        output.join("metrics.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "input": input_path, "digest": generated.digest,
            "recipe": input.landform, "generation_ms": durations,
            "triangles": generated.terrain_patch.as_ref().map(SceneTerrainPatch::triangle_count),
            "vertices": generated.terrain_patch.as_ref().map(|mesh| mesh.positions.len()),
            "obstacles": generated.obstacles.len(),
            "adjusted_height_samples": generated.repairs.adjusted_height_samples,
            "deterministic": true,
        }))?,
    )?;
    println!("{}", output.display());
    Ok(())
}
