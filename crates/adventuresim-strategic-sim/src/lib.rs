//! Deterministic, native strategic-layer experiment harness.

mod analysis;
mod config;
pub mod investigation_eval;
mod live_core;
mod profile;
mod rng;
mod runner;

pub use analysis::*;
pub use config::*;
pub use investigation_eval::*;
pub use live_core::*;
pub use profile::*;
pub use runner::*;

pub const FORMAT_VERSION: u32 = 3;
/// Maximum accepted config or report JSON input.
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

pub fn validate_input_len(len: u64) -> Result<(), String> {
    if len > MAX_INPUT_BYTES {
        Err(format!("input exceeds {MAX_INPUT_BYTES} bytes"))
    } else {
        Ok(())
    }
}
