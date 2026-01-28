use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use crate::prelude::ProtocolSettings;
use crate::{DEFAULT_SERVER_ADDR, FIXED_TICK_DURATION};
use bevy::ecs::world::DeferredWorld;

#[derive(Default)]
pub struct AdventureSimulatorServerPlugin;

impl Plugin for AdventureSimulatorServerPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(FIXED_TICK_DURATION);
        app.add_plugins(ServerPlugins { tick_duration });
    }
}

#[derive(Component, Debug, Clone)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Server"))]
pub struct AdventureSimulatorServer {
    pub addr: SocketAddr,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self {
            addr: DEFAULT_SERVER_ADDR,
            protocol_settings: default(),
        }
    }
}

impl AdventureSimulatorServer {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let Self {
                addr,
                protocol_settings,
            } = entity_mut.take::<Self>().unwrap();

            entity_mut.insert((
                NetcodeServer::new(NetcodeConfig {
                    protocol_id: protocol_settings.id,
                    private_key: protocol_settings.private_key.0,
                    ..Default::default()
                }),
                LocalAddr(addr),
                ServerUdpIo::default(),
            ));

            world.trigger(Start { entity });
            Ok(())
        });
    }
}
