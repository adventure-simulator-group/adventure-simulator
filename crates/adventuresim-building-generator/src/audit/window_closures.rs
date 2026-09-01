/// Closure-policy checks kept separate from structural opening geometry.
fn window_closure_is_legal(
    opening: &crate::OpeningAssembly,
    archetype: BuildingArchetype,
) -> bool {
    use crate::{ClosureKind, ClosureState};

    matches!(
        opening.closure.layers.as_slice(),
        [ClosureKind::LeadedGlazing] | [ClosureKind::IronBars, ClosureKind::LeadedGlazing]
    ) && (archetype != BuildingArchetype::Cathedral
        || opening.closure.state == ClosureState::Closed)
}
