use std::{net::SocketAddr, path::PathBuf};

use adventure_simulator_net::{prelude::*, protocol::PrivateKey, server::AdventureSimulatorServer};
use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
};
use clap::{Parser, ValueHint};
use serde::Deserialize;

#[derive(Parser, Debug, Default, Deserialize)]
#[command(version, about)]
#[serde(deny_unknown_fields, default)]
struct AppConfig {
    /// Path to a TOML config file
    #[arg(short, long, value_hint = ValueHint::FilePath, env = "ADVENTURE_SIMULATOR_CONFIG")]
    #[serde(skip)]
    config: Option<PathBuf>,

    /// Bind server to the adress
    #[arg(long, env = "ADVENTURE_SIMULATOR_ADDR")]
    addr: Option<SocketAddr>,

    /// Enable UDP protocol for the server
    #[arg(long, env = "ADVENTURE_SIMULATOR_UDP", default_value_t = true)]
    enable_udp: bool,

    /// Enalbe WebSocket protocol for the server
    #[arg(long, env = "ADVENTURE_SIMULATOR_WEBSOCKET")]
    enable_websocket: bool,

    /// Enable WebTransport protocol for the server
    #[arg(long, env = "ADVENTURE_SIMULATOR_WEBTRANSPORT")]
    enable_webtransport: bool,

    /// Set server's protocol id
    #[arg(long, env = "ADVENTURE_SIMULATOR_PROTOCOL_ID")]
    protocol_id: Option<u64>,

    /// Set server's protocol private key
    #[arg(long, env = "ADVENTURE_SIMULATOR_PRIVATE_KEY")]
    protocol_private_key: Option<PrivateKey>,
}

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            AdventureSimulatorNetPlugins,
            LogPlugin {
                level: Level::DEBUG,
                filter: "bevy_ecs=warn,bevy_time=warn".to_string(),
                ..default()
            },
        ))
        .add_systems(Startup, setup_server_system)
        .run();
}

fn setup_server_system(mut commands: Commands) -> Result {
    let config = AppConfig::try_parse()?;

    let AppConfig {
        config: _,
        addr,
        enable_udp,
        enable_websocket,
        enable_webtransport,
        protocol_id,
        protocol_private_key,
    } = if let Some(path) = config.config {
        let toml_str = std::fs::read_to_string(path)?;
        let file_config = toml::from_str::<AppConfig>(&toml_str)?;
        AppConfig {
            config: None,
            addr: config.addr.or(file_config.addr),
            enable_udp: config.enable_udp || file_config.enable_udp,
            enable_websocket: config.enable_websocket || file_config.enable_websocket,
            enable_webtransport: config.enable_webtransport || file_config.enable_webtransport,
            protocol_id: config.protocol_id.or(file_config.protocol_id),
            protocol_private_key: config
                .protocol_private_key
                .or(file_config.protocol_private_key),
        }
    } else {
        config
    };

    let mut default_server = AdventureSimulatorServer::default();
    if let Some(addr) = addr {
        default_server.addr = addr;
    }
    if let Some(id) = protocol_id {
        default_server.protocol_settings.id = id;
    }
    if let Some(private_key) = protocol_private_key {
        default_server.protocol_settings.private_key = private_key;
    }

    if enable_udp {
        commands.spawn(AdventureSimulatorServer {
            protocol: ServerProtocol::Udp,
            ..default_server.clone()
        });
    }
    if enable_websocket {
        commands.spawn(AdventureSimulatorServer {
            protocol: ServerProtocol::WebSocket,
            ..default_server.clone()
        });
    }
    if enable_webtransport {
        commands.spawn(AdventureSimulatorServer {
            protocol: ServerProtocol::WebTransport {
                // TODO: parse certificates in args
                certificate: default(),
            },
            ..default_server.clone()
        });
    }

    Ok(())
}
