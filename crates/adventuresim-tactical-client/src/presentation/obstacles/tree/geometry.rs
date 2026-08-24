use bevy::math::{Quat, Vec3};

pub(in crate::presentation) const TREE_PRIMARY_GROUP_COUNT: u8 = 7;
pub(in crate::presentation) const TREE_SECONDARY_GROUP_STRIDE: u16 = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum WoodyPlantForm {
    MatureOak,
    MatureBeech,
    MultiStemShrub,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct WoodyPlantParameters {
    pub(in crate::presentation) form: WoodyPlantForm,
    pub(in crate::presentation) height_metres: f32,
    pub(in crate::presentation) crown_radius_metres: f32,
    pub(in crate::presentation) basal_stems: u8,
    pub(in crate::presentation) leaves_per_shoot: u8,
    pub(in crate::presentation) leaf_length_metres: [f32; 2],
    pub(in crate::presentation) leaf_width_ratio: [f32; 2],
    pub(in crate::presentation) petiole_length_metres: [f32; 2],
}

pub(in crate::presentation) const ENGLISH_OAK_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MatureOak,
        height_metres: 13.0,
        crown_radius_metres: 6.0,
        basal_stems: 1,
        leaves_per_shoot: 16,
        leaf_length_metres: [0.10, 0.16],
        leaf_width_ratio: [0.65, 0.7],
        petiole_length_metres: [0.003, 0.007],
    };

pub(in crate::presentation) const COMMON_HAZEL_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MultiStemShrub,
        height_metres: 2.65,
        crown_radius_metres: 1.55,
        basal_stems: 9,
        leaves_per_shoot: 10,
        leaf_length_metres: [0.082, 0.12],
        leaf_width_ratio: [0.72, 0.84],
        petiole_length_metres: [0.012, 0.023],
    };

pub(in crate::presentation) const COMMON_BEECH_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MatureBeech,
        height_metres: 16.0,
        crown_radius_metres: 4.6,
        basal_stems: 1,
        // Close beech crowns must retain the overlapping two-ranked sprays
        // that make the species read as a closed canopy before the aggregate
        // crown takes over. This cost exists only in the short LOD0 band.
        leaves_per_shoot: 12,
        leaf_length_metres: [0.055, 0.1],
        leaf_width_ratio: [0.48, 0.66],
        petiole_length_metres: [0.008, 0.018],
    };

pub(in crate::presentation) const BLACKTHORN_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MultiStemShrub,
        height_metres: 2.25,
        crown_radius_metres: 1.35,
        basal_stems: 11,
        leaves_per_shoot: 9,
        leaf_length_metres: [0.035, 0.064],
        leaf_width_ratio: [0.46, 0.62],
        petiole_length_metres: [0.004, 0.011],
    };

pub(in crate::presentation) const COMMON_HAWTHORN_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MultiStemShrub,
        height_metres: 3.15,
        crown_radius_metres: 1.7,
        basal_stems: 6,
        leaves_per_shoot: 9,
        leaf_length_metres: [0.04, 0.072],
        leaf_width_ratio: [0.64, 0.82],
        petiole_length_metres: [0.008, 0.018],
    };

/// Independent, normalized controls for the growth history that makes an oak
/// read as gnarled. Keeping these causes separate lets authored presets express
/// a wind-shaped tree, an ancient pollard, or a root-bound veteran without one
/// overloaded "gnarliness" slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::presentation) struct OakGnarlingParameters {
    /// Shared direction of persistent asymmetric loading, normally prevailing
    /// wind, in world-space radians from +X toward +Z.
    pub(in crate::presentation) stress_azimuth_radians: f32,
    pub(in crate::presentation) root_spread: f32,
    pub(in crate::presentation) root_meander: f32,
    pub(in crate::presentation) root_exposure: f32,
    pub(in crate::presentation) root_forking: f32,
    pub(in crate::presentation) trunk_lean: f32,
    pub(in crate::presentation) trunk_sweep: f32,
    pub(in crate::presentation) trunk_twist: f32,
    pub(in crate::presentation) trunk_crooks: f32,
    pub(in crate::presentation) taper_irregularity: f32,
    pub(in crate::presentation) knot_frequency: f32,
    pub(in crate::presentation) knot_scale: f32,
    pub(in crate::presentation) burl_scale: f32,
    pub(in crate::presentation) scaffold_droop: f32,
    pub(in crate::presentation) scaffold_sweep: f32,
    pub(in crate::presentation) scaffold_contortion: f32,
    pub(in crate::presentation) crown_asymmetry: f32,
}

pub(in crate::presentation) const NATURAL_OAK_GNARLING: OakGnarlingParameters =
    OakGnarlingParameters {
        stress_azimuth_radians: 0.0,
        root_spread: 0.0,
        root_meander: 0.0,
        root_exposure: 0.0,
        root_forking: 0.0,
        trunk_lean: 0.0,
        trunk_sweep: 0.0,
        trunk_twist: 0.0,
        trunk_crooks: 0.0,
        taper_irregularity: 0.0,
        knot_frequency: 0.0,
        knot_scale: 0.0,
        burl_scale: 0.0,
        scaffold_droop: 0.0,
        scaffold_sweep: 0.0,
        scaffold_contortion: 0.0,
        crown_asymmetry: 0.0,
    };

pub(in crate::presentation) const WIND_SHAPED_OAK_GNARLING: OakGnarlingParameters =
    OakGnarlingParameters {
        stress_azimuth_radians: 0.0,
        root_spread: 0.48,
        root_meander: 0.36,
        root_exposure: 0.3,
        root_forking: 0.36,
        trunk_lean: 0.72,
        trunk_sweep: 0.68,
        trunk_twist: 0.34,
        trunk_crooks: 0.3,
        taper_irregularity: 0.34,
        knot_frequency: 0.3,
        knot_scale: 0.28,
        burl_scale: 0.12,
        scaffold_droop: 0.48,
        scaffold_sweep: 0.82,
        scaffold_contortion: 0.46,
        crown_asymmetry: 0.86,
    };

pub(in crate::presentation) const ANCIENT_OAK_GNARLING: OakGnarlingParameters =
    OakGnarlingParameters {
        stress_azimuth_radians: 0.0,
        root_spread: 0.86,
        root_meander: 0.72,
        root_exposure: 0.8,
        root_forking: 0.72,
        trunk_lean: 0.28,
        trunk_sweep: 0.46,
        trunk_twist: 0.62,
        trunk_crooks: 0.68,
        taper_irregularity: 0.74,
        knot_frequency: 0.82,
        knot_scale: 0.7,
        burl_scale: 0.76,
        scaffold_droop: 0.78,
        scaffold_sweep: 0.58,
        scaffold_contortion: 0.72,
        crown_asymmetry: 0.54,
    };

pub(in crate::presentation) const EXTREME_OAK_GNARLING: OakGnarlingParameters =
    OakGnarlingParameters {
        stress_azimuth_radians: 0.0,
        root_spread: 1.0,
        root_meander: 1.0,
        root_exposure: 1.0,
        root_forking: 1.0,
        trunk_lean: 0.7,
        trunk_sweep: 1.0,
        trunk_twist: 1.0,
        trunk_crooks: 1.0,
        taper_irregularity: 1.0,
        knot_frequency: 1.0,
        knot_scale: 1.0,
        burl_scale: 1.0,
        scaffold_droop: 1.0,
        scaffold_sweep: 1.0,
        scaffold_contortion: 1.0,
        crown_asymmetry: 1.0,
    };

pub(in crate::presentation) const OAK_GNARLING_SHOWCASE: [OakGnarlingParameters; 4] = [
    NATURAL_OAK_GNARLING,
    WIND_SHAPED_OAK_GNARLING,
    ANCIENT_OAK_GNARLING,
    EXTREME_OAK_GNARLING,
];

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct BarkRecipe {
    pub(in crate::presentation) fissure_depth_metres: f32,
    pub(in crate::presentation) fissure_width_metres: f32,
    pub(in crate::presentation) lip_height_metres: f32,
    pub(in crate::presentation) plate_height_metres: f32,
    pub(in crate::presentation) mature_radius_metres: f32,
    pub(in crate::presentation) minimum_radius_metres: f32,
    pub(in crate::presentation) root_lobe_height_metres: f32,
    /// Typical uninterrupted length of a mature bark fissure before a plate
    /// closes it or diverts it sideways.
    pub(in crate::presentation) plate_length_metres: f32,
    pub(in crate::presentation) branch_depth_attenuation: [f32; 4],
}

pub(in crate::presentation) const ENGLISH_OAK_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.017,
    fissure_width_metres: 0.013,
    lip_height_metres: 0.014,
    plate_height_metres: 0.012,
    mature_radius_metres: 0.38,
    minimum_radius_metres: 0.045,
    root_lobe_height_metres: 0.032,
    plate_length_metres: 0.72,
    branch_depth_attenuation: [1.0, 0.62, 0.24, 0.06],
};

pub(in crate::presentation) const COMMON_HAZEL_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.0015,
    fissure_width_metres: 0.006,
    lip_height_metres: 0.001,
    plate_height_metres: 0.001,
    mature_radius_metres: 0.12,
    minimum_radius_metres: 0.035,
    root_lobe_height_metres: 0.0,
    plate_length_metres: 0.34,
    branch_depth_attenuation: [0.45, 0.2, 0.05, 0.0],
};

pub(in crate::presentation) const COMMON_BEECH_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.00035,
    fissure_width_metres: 0.009,
    lip_height_metres: 0.0002,
    plate_height_metres: 0.00015,
    mature_radius_metres: 0.48,
    minimum_radius_metres: 0.055,
    root_lobe_height_metres: 0.003,
    plate_length_metres: 1.2,
    branch_depth_attenuation: [0.16, 0.06, 0.01, 0.0],
};

pub(in crate::presentation) const BLACKTHORN_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.0018,
    fissure_width_metres: 0.004,
    lip_height_metres: 0.0012,
    plate_height_metres: 0.001,
    mature_radius_metres: 0.09,
    minimum_radius_metres: 0.025,
    root_lobe_height_metres: 0.0,
    plate_length_metres: 0.22,
    branch_depth_attenuation: [0.5, 0.22, 0.05, 0.0],
};

pub(in crate::presentation) const COMMON_HAWTHORN_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.0028,
    fissure_width_metres: 0.005,
    lip_height_metres: 0.0018,
    plate_height_metres: 0.0015,
    mature_radius_metres: 0.1,
    minimum_radius_metres: 0.03,
    root_lobe_height_metres: 0.0,
    plate_length_metres: 0.26,
    branch_depth_attenuation: [0.62, 0.28, 0.06, 0.0],
};

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct TreeBranchSegment {
    pub(in crate::presentation) start: Vec3,
    pub(in crate::presentation) end: Vec3,
    pub(in crate::presentation) start_radius: f32,
    pub(in crate::presentation) end_radius: f32,
    pub(in crate::presentation) depth: u8,
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) secondary_group: u16,
    pub(in crate::presentation) is_limb_tip: bool,
}

mod leaves;
mod skeleton;
mod wood_mesh;

pub(in crate::presentation) use leaves::{
    TreeLeaf, procedural_oak_bud_group_mesh, procedural_oak_bud_mesh,
    procedural_oak_leaf_card_group_mesh, procedural_oak_leaf_card_mesh, procedural_oak_leaves,
    procedural_oak_textured_leaf_group_mesh, procedural_oak_textured_leaf_mesh,
    procedural_woody_cambered_leaf_mesh, procedural_woody_plant_leaves,
    procedural_woody_sparse_leaf_card_mesh,
};
pub(in crate::presentation) use skeleton::{
    procedural_oak_skeleton_with_gnarling, procedural_tree_skeleton,
    procedural_woody_plant_skeleton,
};
pub(in crate::presentation) use wood_mesh::{
    WoodyBranchMeshQuality, procedural_tree_branch_group_mesh, procedural_tree_branch_mesh,
    procedural_woody_branch_bake_mesh, procedural_woody_branch_mesh, procedural_woody_crown_mesh,
    procedural_woody_mid_trunk_mesh,
};

fn branch_frame(direction: Vec3) -> (Vec3, Vec3) {
    let reference = if direction.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = direction.cross(reference).normalize();
    (right, right.cross(direction).normalize())
}

/// Carries a cylindrical frame along a curved woody axis without the abrupt
/// reference-axis changes produced by rebuilding each ring independently.
fn transport_branch_frame(
    previous_tangent: Vec3,
    previous_right: Vec3,
    tangent: Vec3,
) -> (Vec3, Vec3) {
    let rotated = if previous_tangent.dot(tangent) < -0.999 {
        branch_frame(tangent).0
    } else {
        Quat::from_rotation_arc(previous_tangent, tangent) * previous_right
    };
    let mut right = (rotated - tangent * rotated.dot(tangent)).normalize_or_zero();
    if right.length_squared() < 0.5 {
        right = branch_frame(tangent).0;
    }
    if right.dot(previous_right) < 0.0 {
        right = -right;
    }
    (right, right.cross(tangent).normalize())
}

#[derive(Clone, Copy)]
pub(in crate::presentation) struct TreeCrownBounds {
    pub(in crate::presentation) minimum: Vec3,
    pub(in crate::presentation) maximum: Vec3,
}

impl TreeCrownBounds {
    pub(in crate::presentation) fn center(self) -> Vec3 {
        (self.minimum + self.maximum) * 0.5
    }

    pub(in crate::presentation) fn horizontal_span(self) -> f32 {
        (self.maximum.x - self.minimum.x).max(self.maximum.z - self.minimum.z)
    }

    pub(in crate::presentation) fn vertical_span(self) -> f32 {
        self.maximum.y - self.minimum.y
    }
}

pub(in crate::presentation) fn tree_crown_bounds(
    branches: &[TreeBranchSegment],
    mut includes: impl FnMut(&TreeBranchSegment) -> bool,
) -> TreeCrownBounds {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for branch in branches.iter().filter(|branch| includes(branch)) {
        minimum = minimum.min(branch.start).min(branch.end);
        maximum = maximum.max(branch.start).max(branch.end);
    }
    debug_assert!(minimum.is_finite() && maximum.is_finite());
    TreeCrownBounds { minimum, maximum }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transported_branch_frames_remain_orthonormal_across_reference_axis_boundary() {
        let previous_tangent = Vec3::new(0.44, 0.895, 0.07).normalize();
        let (previous_right, _) = branch_frame(previous_tangent);
        let tangent = Vec3::new(0.41, 0.91, 0.06).normalize();
        let (right, forward) = transport_branch_frame(previous_tangent, previous_right, tangent);

        assert!(right.is_normalized() && forward.is_normalized());
        assert!(right.dot(tangent).abs() < 1.0e-5);
        assert!(forward.dot(tangent).abs() < 1.0e-5);
        assert!(right.dot(previous_right) > 0.98);
        assert!(right.cross(tangent).dot(forward) > 0.999);
    }
}
