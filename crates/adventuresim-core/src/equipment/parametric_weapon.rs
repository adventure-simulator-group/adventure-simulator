/// Combat-facing geometry derived from one physical weapon recipe.
///
/// Mass and rotational inertia come from the same component volumes that
/// generate the mesh. Effective reach preserves the default recipe's catalog
/// envelope while applying custom grip-to-tip deltas one-for-one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParametricWeaponCombatGeometry {
    pub mass_kg: f32,
    pub total_length_m: f32,
    pub grip_to_tip_m: f32,
    pub striking_head_length_m: f32,
    pub moment_of_inertia_kg_m2: f32,
    pub balance: f32,
    pub effective_melee_reach_m: f32,
}

impl ParametricWeaponCombatGeometry {
    #[expect(
        clippy::too_many_arguments,
        reason = "the boundary validates each independent authored physical measurement"
    )]
    pub fn new(
        mass_kg: f32,
        total_length_m: f32,
        grip_to_tip_m: f32,
        striking_head_length_m: f32,
        moment_of_inertia_kg_m2: f32,
        balance: f32,
        catalog_melee_reach_m: f32,
        default_recipe_grip_to_tip_m: f32,
    ) -> Option<Self> {
        let values = [
            mass_kg,
            total_length_m,
            grip_to_tip_m,
            striking_head_length_m,
            moment_of_inertia_kg_m2,
            balance,
            catalog_melee_reach_m,
            default_recipe_grip_to_tip_m,
        ];
        let effective_melee_reach_m =
            catalog_melee_reach_m + (grip_to_tip_m - default_recipe_grip_to_tip_m);
        (values.into_iter().all(f32::is_finite)
            && mass_kg > 0.0
            && total_length_m > 0.0
            && grip_to_tip_m > 0.0
            && grip_to_tip_m <= total_length_m
            && (0.0..=total_length_m).contains(&striking_head_length_m)
            && moment_of_inertia_kg_m2 >= 0.0
            && balance >= 0.0
            && effective_melee_reach_m > 0.0)
            .then_some(Self {
                mass_kg,
                total_length_m,
                grip_to_tip_m,
                striking_head_length_m,
                moment_of_inertia_kg_m2,
                balance,
                effective_melee_reach_m,
            })
    }

    pub const fn melee_reach_m(self) -> f32 {
        self.effective_melee_reach_m
    }
}
