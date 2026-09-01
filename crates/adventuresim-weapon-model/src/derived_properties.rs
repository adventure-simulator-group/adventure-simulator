use crate::MaterialClass;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DerivedProperties {
    pub mass_kg: f32,
    pub length_m: f32,
    pub grip_to_tip_m: f32,
    pub striking_head_length_m: f32,
    /// Signed longitudinal center of mass; positive values lie toward the head.
    pub center_of_mass_from_grip_m: f32,
    /// Mean transverse rotational inertia about the controlling hand.
    pub moment_of_inertia_kg_m2: f32,
    /// Radius of gyration divided by grip-to-tip length. Lower is easier to redirect.
    pub balance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DerivedMaterialMass {
    pub material: MaterialClass,
    pub mass_kg: f32,
}
