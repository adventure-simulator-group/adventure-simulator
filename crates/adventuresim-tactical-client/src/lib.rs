//! Feature-gated library target: exists purely so a separate binary
//! (`adventuresim-tactical-brp-generator`) can link in every
//! `#[derive(Reflect)]` type this crate defines and have them
//! auto-register (`reflect_auto_register`, enabled workspace-wide) onto a
//! bare `App::new()` - no need to duplicate type paths by hand, or build
//! and run the real client.
//!
//! Gated behind `remote-types` (off by default) so a normal `cargo
//! build`/`check` of this package - which always builds every target, lib
//! included - doesn't pay to compile this module tree a second time. Only
//! `--features remote-types` does, and nothing in the real
//! `adventuresim-tactical-client` binary depends on this target.
#![cfg(feature = "remote-types")]

pub mod animation;
pub mod camera;
pub mod debug;
pub mod player;
