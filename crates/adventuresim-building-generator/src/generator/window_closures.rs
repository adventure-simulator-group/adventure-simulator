/// Deterministic historical closure policy for generated window openings.

#[derive(Clone, Copy)]
enum WindowClosureVariant {
    Fixed,
    Casement,
    BarredCasement,
}

fn window_closure_variant(
    program: &BuildingProgram,
    storey_level: u16,
    opening: crate::OpeningAssemblyId,
) -> WindowClosureVariant {
    if program.archetype == BuildingArchetype::Cathedral {
        return WindowClosureVariant::Fixed;
    }
    let sample = fabelgeist_determinism::splitmix64(
        program.seed
            ^ opening.0.rotate_left(17)
            ^ u64::from(storey_level).rotate_left(41),
    );
    if storey_level == 0 && sample.is_multiple_of(5) {
        WindowClosureVariant::BarredCasement
    } else if sample.is_multiple_of(4) {
        WindowClosureVariant::Fixed
    } else {
        WindowClosureVariant::Casement
    }
}

fn closure_policy_for(
    use_kind: crate::OpeningUse,
    window_variant: WindowClosureVariant,
) -> crate::ClosurePolicy {
    use crate::{ClosureKind, ClosureState};
    match use_kind {
        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop => crate::ClosurePolicy {
            layers: vec![ClosureKind::OpenMilitary],
            state: ClosureState::Open,
            thickness_metres: 0.0,
            swing_clearance_metres: 0.0,
        },
        crate::OpeningUse::Door | crate::OpeningUse::Gate => crate::ClosurePolicy {
            layers: vec![ClosureKind::DoorLeaf],
            state: ClosureState::Operable,
            thickness_metres: 0.07,
            swing_clearance_metres: 0.90,
        },
        crate::OpeningUse::Window => window_variant.policy(),
        crate::OpeningUse::BellOpening => crate::ClosurePolicy {
            layers: vec![ClosureKind::TimberLouvre],
            state: ClosureState::Open,
            thickness_metres: 0.08,
            swing_clearance_metres: 0.0,
        },
    }
}

fn opening_closure(
    program: &BuildingProgram,
    storey_level: u16,
    opening: crate::OpeningAssemblyId,
    use_kind: crate::OpeningUse,
) -> crate::ClosurePolicy {
    closure_policy_for(
        use_kind,
        window_closure_variant(program, storey_level, opening),
    )
}

fn closure_solid_layers(
    policy: &crate::ClosurePolicy,
) -> impl Iterator<Item = (usize, crate::ClosureKind)> + '_ {
    policy
        .layers
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, layer)| {
            !matches!(
                layer,
                crate::ClosureKind::OpenMilitary | crate::ClosureKind::IronBars
            )
        })
}

fn fixed_window_closure_policy() -> crate::ClosurePolicy {
    WindowClosureVariant::Fixed.policy()
}

impl WindowClosureVariant {
    fn policy(self) -> crate::ClosurePolicy {
        use crate::{ClosureKind, ClosurePolicy, ClosureState};
        let (layers, state, swing_clearance_metres) = match self {
            Self::Fixed => (vec![ClosureKind::LeadedGlazing], ClosureState::Closed, 0.0),
            Self::Casement => (
                vec![ClosureKind::LeadedGlazing],
                ClosureState::Operable,
                0.55,
            ),
            Self::BarredCasement => (
                vec![ClosureKind::IronBars, ClosureKind::LeadedGlazing],
                ClosureState::Operable,
                0.55,
            ),
        };
        ClosurePolicy {
            layers,
            state,
            thickness_metres: 0.025,
            swing_clearance_metres,
        }
    }
}
