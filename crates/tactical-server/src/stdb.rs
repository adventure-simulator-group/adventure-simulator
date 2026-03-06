use std::{mem::MaybeUninit, sync::Arc};

use bevy::{
    ecs::{system::RunSystemOnce, world::unsafe_world_cell::UnsafeWorldCell},
    platform::cell::SyncUnsafeCell,
    prelude::*,
};
use spacetimedb_sdk::{DbContext, Table};
use strategic_db_client::*;

use crate::Args;

/// Plugin for spacetimedb x bevy integration.
pub struct SpacetimeDbPlugin;

impl Plugin for SpacetimeDbPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, connect_spacetimedb)
            .add_systems(Update, update_spacetimedb);
    }
}

// FIXME: frame tick and disconnect on shutown
#[derive(Resource)]
pub struct SpacetimeDb {
    conn: DbConnection,
    world: Arc<SyncUnsafeCell<MaybeUninit<UnsafeWorldCell<'static>>>>,
}

impl SpacetimeDb {
    /// Access spacetime db reducers.
    pub fn reducers(&self) -> &RemoteReducers {
        &self.conn.reducers
    }

    /// Subscribe for spacetime db database changes.
    ///
    /// For native subsctiptions, see: https://spacetimedb.com/docs/sdks/rust/quickstart/#subscribe-to-queries.
    pub fn subscribe(&mut self, queries: impl IntoQueries) -> SubscriptionHandle {
        self.conn
            .subscription_builder()
            .on_error(|ctx, error| {
                error!(
                    "SpacetimeDB subscription failed: {error} (event: {:?})",
                    ctx.event
                );
            })
            .subscribe(queries.into_queries())
    }

    /// Create a new bevy system callback for new rows in a table.
    ///
    /// ## Example
    ///
    /// ```
    /// fn system(mut conn: ResMut<SpacetimeDb>) {
    ///     conn.on_insert(RemoteTables::character, |InRef(character): InRef<Character>, mut commands: Commands| {
    ///         commands.spawn(Name::from(character.name.clone()));
    ///     });
    /// }
    /// ```
    pub fn on_insert<'a, T, TGet, S, M>(&'a mut self, table: TGet, system: S)
    where
        T: Table + 'a,
        TGet: FnOnce(&'a RemoteTables) -> T,
        S: IntoSystem<InRef<'static, T::Row>, (), M> + Send + Sync + 'static + Copy,
    {
        let table = table(&self.conn.db);
        let world = self.world.clone();
        table.on_insert(move |_ctx, new| {
            debug!("on_insert: {}", std::any::type_name::<T::Row>());

            // SAFETY: `world` is set and used within one system: `update_spacetimedb`.
            //         Spacetime will call this callback only when the inner connection is mid-update,
            //         which in our case is single instance of `tick_frame` call when the world is valid.
            unsafe {
                let world = world.get().as_mut().unwrap().assume_init_mut().world_mut();

                // FIXME: `run_system_once_with` is extremely ineffecent,
                //        we need to register the system once somehow
                let system = IntoSystem::into_system(system);
                world.run_system_once_with(system, new).unwrap();
            }
        });
    }
}

/// Trait for types that can be used with [`SpacetimeDb::subscribe`].
///
/// NOTE: This exists in spacetimedb-sdk, but is private :(
///       so we have our own small stripped version of it
pub trait IntoQueries {
    fn into_queries(self) -> Box<[Box<str>]>;
}

impl IntoQueries for &'static str {
    fn into_queries(self) -> Box<[Box<str>]> {
        Box::new([self.into()])
    }
}

impl<const N: usize> IntoQueries for [&'static str; N] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.into_iter().map(Box::from).collect()
    }
}

impl IntoQueries for String {
    fn into_queries(self) -> Box<[Box<str>]> {
        Box::new([self.into()])
    }
}

impl<const N: usize> IntoQueries for [String; N] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.into_iter().map(Box::from).collect()
    }
}

fn update_spacetimedb(world: &mut World) -> Result {
    world.resource_scope::<SpacetimeDb, _>(|world, stdb| -> Result {
        // SAFETY: `world` is only used in callbacks, which are invoked in the subsequent
        //         `frame_tick` call. We limit access to spacetime db to ensure that callbacks
        //         will always have valid world attached.
        unsafe {
            stdb.world
                .get()
                .as_mut()
                .unwrap()
                .write(std::mem::transmute(world.as_unsafe_world_cell()));
        }
        stdb.conn.frame_tick()?;
        Ok(())
    })
}

fn connect_spacetimedb(mut commands: Commands, args: Res<Args>) -> Result {
    info!("Connecting to SpacetimeDB: {}", args.spacetimedb_url);
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_module_name(&args.spacetimedb_module)
        .on_connect(|_, identity, _| {
            info!("SpacetimeDB connected: {identity}");
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
        world: Arc::new(SyncUnsafeCell::new(MaybeUninit::uninit())),
    });

    Ok(())
}
