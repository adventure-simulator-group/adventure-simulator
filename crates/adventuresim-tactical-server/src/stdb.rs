use std::sync::{Arc, Mutex};

use adventuresim_stdb_client::*;
use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Identity, Table};

use crate::Args;

/// Plugin for spacetimedb x bevy integration.
pub struct SpacetimeDbPlugin;

impl Plugin for SpacetimeDbPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, connect_spacetimedb)
            .add_systems(Update, update_spacetimedb);
    }
}

/// Connection to SpacetimeDB.
///
/// SDK callbacks never touch the Bevy world — they only push rows into a
/// mailbox, which ordinary systems drain on their own schedule.
#[derive(Resource)]
pub struct SpacetimeDb {
    conn: DbConnection,
    connected_players: Arc<Mutex<Vec<ConnectedPlayer>>>,
}

impl SpacetimeDb {
    /// Access spacetime db reducers.
    pub fn reducers(&self) -> &RemoteReducers {
        &self.conn.reducers
    }

    /// Get the server's SpacetimeDB identity.
    pub fn identity(&self) -> Identity {
        self.conn.identity()
    }

    /// Subscribe to `connected_players` and start collecting inserted rows.
    ///
    /// For native subscriptions, see: https://spacetimedb.com/docs/sdks/rust/quickstart/#subscribe-to-queries.
    pub fn subscribe_connected_players(&self) -> SubscriptionHandle {
        let mailbox = self.connected_players.clone();
        self.conn.db.connected_players().on_insert(move |_ctx, row| {
            mailbox.lock().unwrap().push(row.clone());
        });

        self.conn
            .subscription_builder()
            .on_error(|ctx, error| {
                error!(
                    "SpacetimeDB subscription failed: {error} (event: {:?})",
                    ctx.event
                );
            })
            .add_query(|query| query.from.connected_players())
            .subscribe()
    }

    /// Take every `connected_players` row inserted since the last call.
    pub fn take_connected_players(&self) -> Vec<ConnectedPlayer> {
        std::mem::take(&mut *self.connected_players.lock().unwrap())
    }
}

/// Pump the connection; SDK callbacks fire here and fill the mailboxes.
pub fn update_spacetimedb(stdb: Res<SpacetimeDb>) -> Result {
    stdb.conn.frame_tick()?;
    Ok(())
}

fn connect_spacetimedb(mut commands: Commands, args: Res<Args>) -> Result {
    info!("Connecting to SpacetimeDB: {}", args.spacetimedb_url);
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_database_name(&args.spacetimedb_module)
        .on_connect(move |_, i, _| {
            info!("SpacetimeDB connected: {i}");
        })
        .on_connect_error(|_, err| {
            // TODO: probably shouldn't insert resource if there is an error ?
            error!("SpacetimeDB connection failed: {err}");
        })
        .on_disconnect(|_, err| {
            if let Some(err) = err {
                warn!("SpacetimeDB disconnected: {err}")
            } else {
                warn!("SpacetimeDB disconnected: no error")
            }
        })
        .build()?;

    commands.insert_resource(SpacetimeDb {
        conn,
        connected_players: default(),
    });

    Ok(())
}
