//! Feature-gated library target: exists purely so a separate binary
//! (`adventuresim-tactical-brp-generator`) can link in every
//! `#[derive(Reflect)]` type this crate defines and have them
//! auto-register (`reflect_auto_register`, enabled workspace-wide) onto a
//! bare `App::new()` - no need to duplicate type paths by hand, or build
//! and run the real server.
//!
//! Gated behind `remote-types` (off by default, not implied by `debug`)
//! so a normal `cargo build`/`check` of this package - which always builds
//! every target, lib included - doesn't pay to compile this module tree a
//! second time. Only `--features remote-types` does, and nothing in the
//! real `adventuresim-tactical-server` binary depends on this target.
#![cfg(feature = "remote-types")]

pub mod bot;
pub mod combat;
pub mod equipment;
pub mod mission;
pub mod player_projection;
pub mod stdb;

/// Stand-in for `main.rs`'s real (clap-derived) `Args` - `player_projection`
/// and `stdb` need *a* type named `Args` with these fields to compile (they
/// take `Res<Args>`), but nothing in this library ever runs a system that
/// reads it, so field *values* never matter. If one of those files starts
/// reading a field this doesn't have, that's a compile error here, same as
/// real `Args` drifting out from under them would be.
#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct Args {
    pub(crate) enemy_combat_scale_bps: u32,
    pub(crate) spacetimedb_url: String,
    pub(crate) spacetimedb_module: String,
}

/// Stand-in for `main.rs`'s real `SceneVistaBundleResource`, for the same
/// reason as `Args` above.
#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct SceneVistaBundleResource(
    pub(crate) Option<adventuresim_tactical_netcode::prelude::SceneVistaBundle>,
);
