//! Accelerated deterministic melee iteration command.

mod melee_combat_iteration;

fn main() -> Result<(), String> {
    melee_combat_iteration::run()
}
