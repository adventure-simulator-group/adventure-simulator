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
            .add_systems(Update, update_spacetimedb)
            .add_systems(Last, disconnect_spacetimedb);
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
    terminal_results: Arc<Mutex<Vec<TerminalSubmissionResult>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalSubmissionResult {
    Accepted,
    Rejected(String),
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
        // This is the callback handler. Standard Arc<Mutex<>> pattern for sharing state between threads.
        let connected_players = self.connected_players.clone();
        self.conn
            .db
            .connected_players()
            .on_insert(move |_ctx, row| {
                connected_players.lock().unwrap().push(row.clone());
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

    /// Queue a terminal reducer and mailbox its eventual nested callback
    /// result. Queue success is not reducer acceptance.
    pub fn submit_terminal(
        &self,
        resolution: TacticalMissionResolution,
        receipt: TacticalConsequenceReceipt,
    ) -> spacetimedb_sdk::Result<()> {
        let terminal_results = self.terminal_results.clone();
        self.conn
            .reducers
            .end_tactical_server_then(resolution, receipt, move |_, result| {
                let result = match result {
                    Ok(Ok(())) => TerminalSubmissionResult::Accepted,
                    Ok(Err(error)) => TerminalSubmissionResult::Rejected(error),
                    Err(error) => TerminalSubmissionResult::Rejected(format!(
                        "internal reducer callback error: {error:?}"
                    )),
                };
                terminal_results.lock().unwrap().push(result);
            })
    }

    pub fn take_terminal_results(&self) -> Vec<TerminalSubmissionResult> {
        std::mem::take(&mut *self.terminal_results.lock().unwrap())
    }
}

/// Pump the connection; SDK callbacks fire here and fill the mailboxes.
pub fn update_spacetimedb(stdb: Res<SpacetimeDb>) -> Result {
    stdb.conn.frame_tick()?;
    Ok(())
}

/// On app exit, close the connection cleanly.
fn disconnect_spacetimedb(mut exit: MessageReader<AppExit>, stdb: Res<SpacetimeDb>) {
    if exit.read().next().is_none() {
        return;
    }

    info!("Disconnecting from SpacetimeDB...");
    if stdb.conn.disconnect().is_ok() {
        // `disconnect` only queues the close; pump until the stream ends so
        // queued reducer calls (e.g. mission results) reach the server first.
        while stdb.conn.advance_one_message_blocking().is_ok() {}
    }
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
            error!("SpacetimeDB connection failed: {err}");
            // A mission server without its database is useless;
            // die and let the orchestrator deal with it.
            std::process::exit(1);
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
        terminal_results: default(),
    });

    Ok(())
}
