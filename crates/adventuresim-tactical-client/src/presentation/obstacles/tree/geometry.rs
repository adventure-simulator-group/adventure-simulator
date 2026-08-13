use bevy::math::{Quat, Vec3};

pub(in crate::presentation) const TREE_PRIMARY_GROUP_COUNT: u8 = 7;
pub(in crate::presentation) const TREE_SECONDARY_GROUP_STRIDE: u16 = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum WoodyPlantForm {
    MatureOak,
    CommonHazel,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct WoodyPlantParameters {
    pub(in crate::presentation) form: WoodyPlantForm,
    pub(in crate::presentation) height_metres: f32,
    pub(in crate::presentation) crown_radius_metres: f32,
    pub(in crate::presentation) basal_stems: u8,
    pub(in crate::presentation) leaves_per_shoot: u8,
}

pub(in crate::presentation) const ENGLISH_OAK_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::MatureOak,
        height_metres: 13.0,
        crown_radius_metres: 6.0,
        basal_stems: 1,
        leaves_per_shoot: 16,
    };

pub(in crate::presentation) const COMMON_HAZEL_PARAMETERS: WoodyPlantParameters =
    WoodyPlantParameters {
        form: WoodyPlantForm::CommonHazel,
        height_metres: 2.65,
        crown_radius_metres: 1.55,
        basal_stems: 9,
        leaves_per_shoot: 10,
    };

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct BarkRecipe {
    pub(in crate::presentation) fissure_depth_metres: f32,
    pub(in crate::presentation) fissure_width_metres: f32,
    pub(in crate::presentation) lip_height_metres: f32,
    pub(in crate::presentation) plate_height_metres: f32,
    pub(in crate::presentation) mature_radius_metres: f32,
    pub(in crate::presentation) minimum_radius_metres: f32,
    pub(in crate::presentation) root_lobe_height_metres: f32,
    pub(in crate::presentation) branch_depth_attenuation: [f32; 4],
}

pub(in crate::presentation) const ENGLISH_OAK_BARK: BarkRecipe = BarkRecipe {
    fissure_depth_metres: 0.017,
    fissure_width_metres: 0.013,
    lip_height_metres: 0.016,
    plate_height_metres: 0.014,
    mature_radius_metres: 0.38,
    minimum_radius_metres: 0.045,
    root_lobe_height_metres: 0.038,
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
    branch_depth_attenuation: [0.45, 0.2, 0.05, 0.0],
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
    procedural_woody_cambered_leaf_mesh, procedural_woody_leaf_card_mesh,
    procedural_woody_plant_leaves,
};
pub(in crate::presentation) use skeleton::{
    procedural_tree_skeleton, procedural_woody_plant_skeleton,
};
pub(in crate::presentation) use wood_mesh::{
    procedural_tree_branch_group_mesh, procedural_tree_branch_mesh, procedural_woody_branch_mesh,
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
