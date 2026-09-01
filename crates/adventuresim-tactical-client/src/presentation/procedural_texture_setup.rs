use adventuresim_procedural_textures::generate_procedural_textures;
use bevy::prelude::*;

use super::ClientStartupTiming;

pub(super) fn setup_procedural_texture_assets(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    startup: Option<Res<ClientStartupTiming>>,
) {
    let started = web_time::Instant::now();
    info!("Generating procedural texture assets");
    commands.insert_resource(generate_procedural_textures(&mut images));
    info!(
        elapsed_ms = started.elapsed().as_millis(),
        "Generated procedural texture assets"
    );
    if let Some(startup) = startup {
        startup.mark("procedural texture assets generated");
    }
}
