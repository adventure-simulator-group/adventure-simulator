// These APIs mirror GPU pipeline layouts and shader resource signatures, where
// grouping parameters or hiding cache-key types would make call sites less explicit.
#![allow(
    clippy::new_ret_no_self,
    clippy::too_many_arguments,
    clippy::type_complexity
)]

pub mod data;
pub mod globals;
pub mod prelude;
