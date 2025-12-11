use std::net::SocketAddr;

use adventure_simulator_net::{
    client::AdventureSimulatorClient,
    lightyear::prelude::{client::InputDelayConfig, *},
    AdventureSimulatorNetPlugins,
};
use bevy::prelude::*;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about)]
struct AppConfig {
    /// Client ID
    #[arg(long)]
    id: u64,
    /// Server addr
    #[arg(long)]
    server_addr: SocketAddr,
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, AdventureSimulatorNetPlugins))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 2000.,
            ..default()
        })
        .add_systems(Startup, setup_client_system)
        .run();
}

fn setup_client_system(mut commands: Commands) -> Result {
    let AppConfig { id, server_addr } = AppConfig::try_parse()?;

    commands.spawn((
        AdventureSimulatorClient {
            id,
            server_addr,
            ..default()
        },
        InputTimeline(Timeline::from(
            Input::default().with_input_delay(InputDelayConfig::fixed_input_delay(0)),
        )),
    ));

    Ok(())
}
