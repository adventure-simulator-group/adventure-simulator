//! Dependency-free escalation limits shared by build-time content validation
//! and runtime combat math.

/// Weakest authored threat baseline: one tenth of an unscaled orc.
pub const MIN_BASELINE_ENEMY_POWER: u32 = 1_000;
/// Global escalation ceiling: thirty baseline-orc equivalents.
pub const MAX_ORC_EQUIVALENT_POWER: u32 = 300_000;
