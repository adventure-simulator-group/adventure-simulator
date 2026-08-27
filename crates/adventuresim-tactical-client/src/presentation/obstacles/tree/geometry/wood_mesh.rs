#[cfg(test)]
use crate::presentation::unit_hash;
use adventuresim_tactical_core::prelude::TREE_TRUNK_HEIGHT_METRES;
use bevy::{
    asset::RenderAssetUsages,
    math::{FloatExt, Vec3, Vec3Swizzles},
    mesh::{Indices, PrimitiveTopology},
    prelude::Mesh,
};

#[cfg(test)]
use super::{BarkRecipe, ENGLISH_OAK_BARK};
use super::{TreeBranchSegment, branch_frame, transport_branch_frame};

#[cfg(test)]
const BRANCH_HASH_INDEX_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;
#[cfg(test)]
const BRANCH_HASH_MULTIPLIER: u64 = 0xbf58_476d_1ce4_e5b9;

/// Geometry budgets for live woody branch sweeps.
///
/// `FullDetail` is the authored trunk/root and LOD0 branch mesh. The mid
/// trunk and aggregate crown tiers deliberately opt into their own budgets so
/// reducing distant geometry cannot silently change close tree silhouettes or
/// root contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum WoodyBranchMeshQuality {
    FullDetail,
    MidDistanceTrunk,
    AggregateLod1,
    AggregateLod2,
}

impl WoodyBranchMeshQuality {
    fn radial_sides(self, depth: u8) -> u32 {
        match self {
            Self::FullDetail => {
                if depth == 0 {
                    16
                } else {
                    (8_u32.saturating_sub(u32::from(depth))).max(4)
                }
            }
            // The middle tier carries only the upright bole. Its faceting is
            // hidden by distance while retaining enough sides for stable bark
            // highlights through the LOD3 handoff.
            Self::MidDistanceTrunk => {
                if depth == 0 {
                    9
                } else {
                    4
                }
            }
            // LOD1 keeps a little more roundness on scaffold branches, while
            // its covered terminal wood falls to four sides.
            Self::AggregateLod1 => match depth {
                0 => 16,
                1 => 5,
                _ => 4,
            },
            // LOD2 contains only depth-one scaffold wood. Four sides are
            // enough behind its larger impostor cards.
            Self::AggregateLod2 => {
                if depth == 0 {
                    16
                } else {
                    4
                }
            }
        }
    }

    fn axial_spacing_metres(self) -> f32 {
        match self {
            Self::FullDetail => 0.45,
            Self::MidDistanceTrunk => 0.9,
            Self::AggregateLod1 => 0.75,
            Self::AggregateLod2 => 1.1,
        }
    }

    fn uses_rounded_terminal(self) -> bool {
        matches!(self, Self::FullDetail)
    }
}

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    procedural_woody_branch_mesh(branches, maximum_depth)
}

pub(in crate::presentation) fn procedural_woody_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    procedural_woody_branch_mesh_with_quality(
        branches,
        maximum_depth,
        WoodyBranchMeshQuality::FullDetail,
    )
}

/// Mid-distance playable-tree trunk mesh.
///
/// This deliberately retains only the upright depth-zero bole. The authored
/// full-detail mesh keeps the root flare and contact geometry near the
/// camera; reproducing those roots in this tier would spend geometry on
/// detail that is no longer readable and would make the near/mid overlap
/// visually muddy.
pub(in crate::presentation) fn procedural_woody_mid_trunk_mesh(
    branches: &[TreeBranchSegment],
) -> Mesh {
    let upright_bole = branches
        .iter()
        .filter(|branch| {
            if branch.depth != 0 {
                return false;
            }
            let axis = branch.end - branch.start;
            axis.length_squared() > 0.000_001 && axis.y.abs() >= axis.xz().length() * 0.7
        })
        .copied()
        .collect::<Vec<_>>();
    procedural_woody_branch_mesh_with_quality(
        &upright_bole,
        0,
        WoodyBranchMeshQuality::MidDistanceTrunk,
    )
}

fn procedural_woody_branch_mesh_with_quality(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
    quality: WoodyBranchMeshQuality,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    // Explicit root curves preserve their authored branching and overlap the
    // continuous trunk sweep below its surface. The former implicit flare
    // collapsed those roots into a bulb and exposed a horizontal handoff.
    let visible = branches
        .iter()
        .filter(|branch| branch.depth <= maximum_depth)
        .copied()
        .collect::<Vec<_>>();
    let mut curve_start = 0;
    while curve_start < visible.len() {
        let curve_end = visible[curve_start..]
            .iter()
            .position(|branch| branch.is_limb_tip)
            .map(|offset| curve_start + offset + 1)
            .unwrap_or(visible.len());
        let curve = &visible[curve_start..curve_end];
        append_branch_curve_tube(
            curve,
            quality,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
        );
        curve_start = curve_end;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    if visible.iter().any(|branch| branch.depth == 0) {
        // The bark shader needs metric height above the root contact plane for
        // its UV-free soil-deposition mask. Vertex colour is an interpolated
        // geometry channel here, not pigment: local Y remains exact across a
        // triangle and avoids creating a unique material for every tree.
        let ground_y = -TREE_TRUNK_HEIGHT_METRES * 0.5;
        let root_heights = positions
            .iter()
            .map(|position| [position[1] - ground_y, 1.0, 1.0, 1.0])
            .collect::<Vec<_>>();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, root_heights);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Cheap woody silhouette used only as input to the software impostor bake.
///
/// The production mesh's implicit root flare, bark displacement, tangents, and
/// high radial tessellation are valuable when viewed directly but invisible
/// after projection into a 64-256 px atlas. Rebuilding that mesh for every
/// card dominated cold startup, so bake cards use independent low-sided tubes
/// while the live near tree keeps the exact production geometry.
pub(in crate::presentation) fn procedural_woody_branch_bake_mesh(
    branches: &[TreeBranchSegment],
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for branch in branches {
        let axis = branch.end - branch.start;
        if axis.length_squared() <= 0.000_001 {
            continue;
        }
        let direction = axis.normalize();
        let (right, forward) = branch_frame(direction);
        let sides = match branch.depth {
            0 => 10_u32,
            1 => 7,
            2 => 5,
            _ => 3,
        };
        let base = positions.len() as u32;
        for (center, radius) in [
            (branch.start, branch.start_radius),
            (branch.end, branch.end_radius),
        ] {
            for side in 0..sides {
                let angle = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * angle.cos() + forward * angle.sin();
                positions.push((center + radial * radius).to_array());
                normals.push(radial.to_array());
            }
        }
        for side in 0..sides {
            let next = (side + 1) % sides;
            indices.extend_from_slice(&[
                base + side,
                base + sides + side,
                base + sides + next,
                base + side,
                base + sides + next,
                base + next,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Aggregate crown wood excludes depth-zero trunk and root geometry because
/// the independently streamed trunk remains resident through LOD2. Besides
/// avoiding duplicate draw geometry, this prevents rebuilding the expensive
/// implicit root flare for each intermediate crown LOD.
pub(in crate::presentation) fn procedural_woody_crown_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
    quality: WoodyBranchMeshQuality,
) -> Mesh {
    let crown = branches
        .iter()
        .filter(|branch| branch.depth > 0 && branch.depth <= maximum_depth)
        .copied()
        .collect::<Vec<_>>();
    procedural_woody_branch_mesh_with_quality(&crown, maximum_depth, quality)
}

#[derive(Clone)]
#[cfg(test)]
/// Retained in test builds as a comparison fixture for the superseded
/// implicit-field root mesher. Production wood uses explicit sweeps only.
struct RootFlareField {
    segments: Vec<TreeBranchSegment>,
    minimum: Vec3,
    maximum: Vec3,
    cell: f32,
    blend: f32,
    bark: BarkRecipe,
    root_directions: Vec<Vec3>,
    sweep_start_y: f32,
}

#[cfg(test)]
impl RootFlareField {
    fn from_branches(
        branches: &[TreeBranchSegment],
        _maximum_depth: u8,
        bark: BarkRecipe,
    ) -> Option<Self> {
        // Hybrid skeleton/sweep/implicit architecture adapted from:
        // https://gist.github.com/halbe/5613d15ecfa84e80c04a56f34a656456
        // Restricting the field to the root crown bounds volumetric sampling;
        // an overlapping trunk sweep hides its deliberately open upper edge.
        let trunk_base = branches
            .iter()
            .filter(|branch| {
                let axis = branch.end - branch.start;
                branch.depth == 0 && axis.y.abs() > axis.xz().length()
            })
            .map(|branch| branch.start.y.min(branch.end.y))
            .fold(f32::INFINITY, f32::min);
        let depth_zero = branches
            .iter()
            .filter(|branch| branch.depth == 0)
            .copied()
            .collect::<Vec<_>>();
        let has_roots = depth_zero.iter().any(|segment| {
            let axis = segment.end - segment.start;
            segment.start.y < trunk_base + 0.5 && axis.xz().length() > axis.y.abs() * 0.7
        });
        if !has_roots || !trunk_base.is_finite() {
            return None;
        }
        const FLARE_HEIGHT_METRES: f32 = 1.25;
        const FLARE_TRUNK_OVERLAP_METRES: f32 = 0.3;
        let flare_top_y = trunk_base + FLARE_HEIGHT_METRES;
        let sweep_start_y = flare_top_y - FLARE_TRUNK_OVERLAP_METRES;
        // Keep the trunk capsule alive above the extraction box so the flare
        // exits through an open plane instead of forming a rounded, visibly
        // scalloped cap around the overlapping swept trunk.
        let field_segment_top_y = flare_top_y + 0.75;
        let segments = depth_zero
            .into_iter()
            .filter_map(|segment| clip_segment_below_y(segment, field_segment_top_y))
            .collect::<Vec<_>>();
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for segment in &segments {
            let extent = Vec3::splat(segment.start_radius.max(segment.end_radius) + 0.22);
            minimum = minimum
                .min(segment.start - extent)
                .min(segment.end - extent);
            maximum = maximum
                .max(segment.start + extent)
                .max(segment.end + extent);
        }
        minimum.y = minimum.y.max(trunk_base - 0.42);
        maximum.y = maximum.y.min(flare_top_y);
        let root_directions = segments
            .iter()
            .filter_map(|segment| {
                let axis = (segment.end - segment.start).xz();
                (axis.length_squared() > 0.04).then(|| Vec3::new(axis.x, 0.0, axis.y).normalize())
            })
            .collect();
        let smooth_thin_bark =
            bark.fissure_depth_metres < 0.001 && bark.root_lobe_height_metres > 0.0;
        Some(Self {
            segments,
            minimum,
            maximum,
            // The implicit field is confined to the root crown and remains
            // deliberately coarse. The swept trunk overlaps its open top,
            // hiding the handoff without polygonizing the full tree height.
            cell: if smooth_thin_bark { 0.2 } else { 0.26 },
            blend: if smooth_thin_bark { 0.09 } else { 0.18 },
            bark,
            root_directions,
            sweep_start_y,
        })
    }

    fn visible_segment(&self, segment: TreeBranchSegment) -> Option<TreeBranchSegment> {
        if segment.depth != 0 {
            return Some(segment);
        }
        clip_segment_above_y(segment, self.sweep_start_y)
    }

    fn macro_distance(&self, point: Vec3) -> f32 {
        self.segments.iter().fold(f32::INFINITY, |field, segment| {
            smooth_min(field, capsule_distance(point, segment), self.blend)
        })
    }

    fn distance(&self, point: Vec3) -> f32 {
        self.macro_distance(point)
    }

    fn root_profile_relief(&self, point: Vec3, outward: Vec3) -> f32 {
        if self.bark.root_lobe_height_metres <= 0.0 {
            return 0.0;
        }
        let horizontal = Vec3::new(outward.x, 0.0, outward.z).normalize_or_zero();
        let alignment = self
            .root_directions
            .iter()
            .map(|direction| horizontal.dot(*direction).max(0.0).powi(8))
            .fold(0.0_f32, f32::max);
        let base = self
            .segments
            .iter()
            .map(|segment| segment.start.y)
            .fold(f32::INFINITY, f32::min);
        let height_fade = (1.0 - ((point.y - base) / 1.35).clamp(0.0, 1.0)).powi(2);
        self.bark.root_lobe_height_metres * alignment * height_fade
    }
}

#[cfg(test)]
fn clip_segment_below_y(
    mut segment: TreeBranchSegment,
    maximum_y: f32,
) -> Option<TreeBranchSegment> {
    if segment.start.y > maximum_y && segment.end.y > maximum_y {
        return None;
    }
    if (segment.start.y > maximum_y) != (segment.end.y > maximum_y) {
        let along =
            ((maximum_y - segment.start.y) / (segment.end.y - segment.start.y)).clamp(0.0, 1.0);
        let point = segment.start.lerp(segment.end, along);
        let radius = segment.start_radius.lerp(segment.end_radius, along);
        if segment.start.y > maximum_y {
            segment.start = point;
            segment.start_radius = radius;
        } else {
            segment.end = point;
            segment.end_radius = radius;
        }
    }
    Some(segment)
}

#[cfg(test)]
fn clip_segment_above_y(
    mut segment: TreeBranchSegment,
    minimum_y: f32,
) -> Option<TreeBranchSegment> {
    if segment.start.y < minimum_y && segment.end.y < minimum_y {
        return None;
    }
    if (segment.start.y < minimum_y) != (segment.end.y < minimum_y) {
        let along =
            ((minimum_y - segment.start.y) / (segment.end.y - segment.start.y)).clamp(0.0, 1.0);
        let point = segment.start.lerp(segment.end, along);
        let radius = segment.start_radius.lerp(segment.end_radius, along);
        if segment.start.y < minimum_y {
            segment.start = point;
            segment.start_radius = radius;
        } else {
            segment.end = point;
            segment.end_radius = radius;
        }
    }
    Some(segment)
}

/// A longitudinal groove with raised lips, evaluated in branch space.
///
/// The angular signal is periodic rather than UV-based, so crossing the
/// cylindrical wrap cannot create a discontinuity. The world-space warp makes
/// the fissures wander without baking any colour variation into the material.
#[cfg(test)]
fn bark_relief(point: Vec3, segment: &TreeBranchSegment, bark: BarkRecipe, bark_phase: f32) -> f32 {
    let axis = segment.end - segment.start;
    let length_squared = axis.length_squared();
    if length_squared <= 1.0e-6 {
        return 0.0;
    }
    let along = ((point - segment.start).dot(axis) / length_squared).clamp(0.0, 1.0);
    let tangent = axis.normalize();
    let center = segment.start + axis * along;
    let radius = segment
        .start_radius
        .lerp(segment.end_radius, along.powf(0.64));
    let radial = (point - center).normalize_or_zero();
    if radial.length_squared() < 0.5 {
        return 0.0;
    }
    let (right, forward) = branch_frame(tangent);
    let theta = radial.dot(forward).atan2(radial.dot(right));
    const CRACK_PHASES: [f32; 13] = [
        0.08, 0.55, 0.91, 1.53, 1.86, 2.44, 2.86, 3.27, 3.82, 4.17, 4.78, 5.31, 5.83,
    ];
    let axial_metres = (point - segment.start).dot(tangent);
    let mut nearest_crack = f32::INFINITY;
    let mut signed_crack = 0.0;
    let mut nearest_crack_index = 0;
    for (index, base_phase) in CRACK_PHASES.into_iter().enumerate() {
        let seed_phase = index as f32 * 1.618_034;
        let drift = 0.11 * (point.dot(Vec3::new(0.17, 0.83, -0.29)) * 1.18 + seed_phase).sin()
            + 0.045 * (point.dot(Vec3::new(-0.53, 0.24, 0.47)) * 2.37 - seed_phase * 0.7).sin();
        let delta = (theta - base_phase - bark_phase - drift + core::f32::consts::PI)
            .rem_euclid(core::f32::consts::TAU)
            - core::f32::consts::PI;
        let distance = delta.abs() * radius;
        if distance < nearest_crack {
            nearest_crack = distance;
            signed_crack = delta * radius;
            nearest_crack_index = index;
        }
    }
    let arc_distance = nearest_crack;
    let depth_index = usize::from(segment.depth.min(3));
    let hierarchy = bark.branch_depth_attenuation[depth_index];
    let maturity = ((radius - bark.minimum_radius_metres)
        / (bark.mature_radius_metres - bark.minimum_radius_metres).max(1.0e-4))
    .clamp(0.0, 1.0);
    let strength = hierarchy * maturity * maturity * (3.0 - 2.0 * maturity);
    if strength <= 1.0e-4 {
        return 0.0;
    }

    // Mature oak fissures are streams, not uninterrupted flutes. Each stream
    // closes on a different, softly staggered cadence so adjacent plates
    // interlock rather than forming horizontal bands. A small residual keeps
    // the relief continuous while the dominant groove visibly terminates.
    let plate_phase = nearest_crack_index as f32 * 2.399_963
        + segment.primary_group as f32 * 0.37
        + bark_phase * 0.73;
    let plate_length = bark.plate_length_metres.max(0.08);
    let break_signal = (axial_metres * core::f32::consts::TAU / plate_length + plate_phase).sin()
        + 0.38
            * (axial_metres * core::f32::consts::TAU / (plate_length * 0.47) - plate_phase * 0.6)
                .sin();
    let fissure_continuity = smoothstep(-0.52, -0.05, break_signal);
    // A closed plate should actually interrupt a fissure. Retaining most of
    // the groove made one angular stream read as a manufactured longitudinal
    // seam even though the cylindrical wrap itself was continuous.
    let fissure_strength = 0.18 + 0.82 * fissure_continuity * fissure_continuity;
    let groove = (-bark.fissure_depth_metres
        * (-0.5 * (arc_distance / bark.fissure_width_metres.max(1.0e-4)).powi(2)).exp())
    .max(-radius * 0.035)
        * fissure_strength;
    let lip_center = bark.fissure_width_metres * 2.25;
    let lip_width = bark.fissure_width_metres * 1.15;
    let lip_distance = (arc_distance - lip_center) / lip_width.max(1.0e-4);
    let lip_asymmetry = 1.0 + 0.18 * (signed_crack / bark.fissure_width_metres.max(1.0e-4)).tanh();
    let lips = (bark.lip_height_metres * lip_asymmetry * (-0.5 * lip_distance.powi(2)).exp())
        .min(radius * 0.035)
        * (0.72 + 0.28 * fissure_continuity);
    let plate_t = (arc_distance / (radius * 0.24).max(0.035)).clamp(0.0, 1.0);
    let plate_crown = bark.plate_height_metres * (core::f32::consts::PI * plate_t).sin().powi(2);
    let transverse_breaks = 0.48
        + 0.32 * (axial_metres * core::f32::consts::TAU / plate_length - plate_phase * 0.4).sin()
        + 0.20 * (theta * 3.0 + plate_phase).sin();
    let broad_warp = 0.27 * (point.dot(Vec3::new(0.21, 0.62, -0.37)) * 0.74).sin();
    let broad_fold = 0.007 * (5.0 * theta + broad_warp).cos() * (radius / 0.7).clamp(0.2, 1.0);
    let closure = 1.0 - fissure_continuity;
    let between_fissures = smoothstep(
        bark.fissure_width_metres * 2.8,
        bark.fissure_width_metres * 5.5,
        arc_distance,
    );
    let transverse_closure = -bark.fissure_depth_metres * 0.16 * closure * between_fissures;
    // Horizontal roots carry broader, quieter folds than the upright trunk.
    // This prevents fine bark relief from turning the root crown into a pile
    // of inflated cords while preserving the same continuous surface field.
    let upright = tangent.y.abs();
    let orientation_attenuation = 0.88 + 0.12 * upright;
    (groove + lips + plate_crown * transverse_breaks.max(0.18) + broad_fold + transverse_closure)
        * strength
        * orientation_attenuation
}

#[cfg(test)]
fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Blend branch-space relief with the same proximity semantics as the smooth
/// implicit union. No nearest-segment switch is exposed at root junctions.
#[cfg(test)]
fn blended_bark_relief(
    point: Vec3,
    segments: &[TreeBranchSegment],
    blend: f32,
    bark: BarkRecipe,
    bark_phase: f32,
) -> f32 {
    let nearest = segments
        .iter()
        .map(|segment| capsule_distance(point, segment))
        .fold(f32::INFINITY, f32::min);
    let mut weighted = 0.0;
    let mut total = 0.0;
    for segment in segments {
        let delta = (capsule_distance(point, segment) - nearest).max(0.0);
        let weight = (-4.0 * delta / blend.max(1.0e-4)).exp();
        if weight > 1.0e-4 {
            weighted += bark_relief(point, segment, bark, bark_phase) * weight;
            total += weight;
        }
    }
    if total > 0.0 { weighted / total } else { 0.0 }
}

#[cfg(test)]
fn capsule_distance(point: Vec3, segment: &TreeBranchSegment) -> f32 {
    let axis = segment.end - segment.start;
    let length_squared = axis.length_squared();
    let along = if length_squared > 1.0e-6 {
        ((point - segment.start).dot(axis) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let radius = segment
        .start_radius
        .lerp(segment.end_radius, along.powf(0.64));
    point.distance(segment.start + axis * along) - radius
}

#[cfg(test)]
fn bark_phase_from_branches(branches: &[TreeBranchSegment]) -> f32 {
    let stride = (branches.len() / 64).max(1);
    let mut hash = 0x6a09_e667_f3bc_c909_u64;
    for (index, branch) in branches.iter().step_by(stride).enumerate() {
        let bits = u64::from(branch.end.x.to_bits())
            ^ u64::from(branch.end.y.to_bits()).rotate_left(17)
            ^ u64::from(branch.end.z.to_bits()).rotate_left(33)
            ^ (index as u64).wrapping_mul(BRANCH_HASH_INDEX_STRIDE);
        hash ^= bits;
        hash = hash.wrapping_mul(BRANCH_HASH_MULTIPLIER);
        hash ^= hash >> 29;
    }
    unit_hash(hash) * core::f32::consts::TAU
}

#[cfg(test)]
fn smooth_min(left: f32, right: f32, blend: f32) -> f32 {
    if !left.is_finite() {
        return right;
    }
    let h = (0.5 + 0.5 * (right - left) / blend).clamp(0.0, 1.0);
    right.lerp(left, h) - blend * h * (1.0 - h)
}

pub(in crate::presentation) fn procedural_tree_branch_group_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
    primary_group: u8,
) -> Mesh {
    let group = branches
        .iter()
        .filter(|branch| {
            branch.depth > 0
                && branch.depth <= maximum_depth
                && branch.primary_group == primary_group
        })
        .copied()
        .collect::<Vec<_>>();
    procedural_tree_branch_mesh(&group, maximum_depth)
}

fn append_branch_curve_tube(
    curve: &[TreeBranchSegment],
    quality: WoodyBranchMeshQuality,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    const BARK_TEXTURE_WIDTH_METRES: f32 = 1.0;
    const BARK_TEXTURE_HEIGHT_METRES: f32 = 2.0;

    let first = curve[0];
    let last = curve[curve.len() - 1];
    let sides = quality.radial_sides(first.depth);
    let first_direction = (first.end - first.start).normalize();
    let last_direction = (last.end - last.start).normalize();
    let surface_root = first.depth == 0
        && first.start.y < -0.75
        && first_direction.xz().length() > first_direction.y.abs() * 0.7;
    let mut rings = Vec::with_capacity(curve.len() + 2);
    if first.depth > 0 {
        // One basal collar belongs at the biological attachment. Repeating a
        // collar at every polygon joint makes a smooth axis look assembled.
        rings.push((
            first.start - first_direction * first.start_radius * 0.12,
            first.start_radius * 1.12,
            first_direction,
        ));
        rings.push((
            first.start + first_direction * first.start_radius * 0.28,
            first.start_radius * 1.06,
            first_direction,
        ));
        rings.push((
            first.start + first_direction * first.start_radius * 0.72,
            first.start_radius * 0.98,
            first_direction,
        ));
    } else {
        rings.push((
            first.start - first_direction * first.start_radius * 0.18,
            first.start_radius,
            first_direction,
        ));
    }
    for (index, branch) in curve.iter().enumerate() {
        let direction = (branch.end - branch.start).normalize();
        let tangent = if index + 1 < curve.len() {
            (direction + (curve[index + 1].end - curve[index + 1].start).normalize()).normalize()
        } else {
            direction
        };
        // Smooth wood needs only enough axial sampling to preserve the authored
        // skeleton curve and taper. Fine bark is intentionally material-only.
        let spacing = quality.axial_spacing_metres();
        let steps = ((branch.end - branch.start).length() / spacing)
            .ceil()
            .max(1.0) as u32;
        for step in 1..=steps {
            let along = step as f32 / steps as f32;
            rings.push((
                branch.start.lerp(branch.end, along),
                branch
                    .start_radius
                    .lerp(branch.end_radius, along.powf(0.64)),
                tangent,
            ));
        }
    }
    if surface_root {
        let ground_y = -TREE_TRUNK_HEIGHT_METRES * 0.5;
        for (center, radius, _) in &mut rings {
            center.y += *radius * 0.75;
            center.y = center.y.max(ground_y + *radius * 0.05);
        }
    }
    let ring_stride = sides + 1;
    // A whole-number wrap keeps the duplicated cylindrical seam texel-exact.
    // Choose it from the biological base circumference so scale is physical
    // and stable along a tapering axis rather than resetting per segment.
    let circumference_tiles = (core::f32::consts::TAU * first.start_radius
        / BARK_TEXTURE_WIDTH_METRES)
        .round()
        .max(1.0);
    let base = positions.len() as u32;
    let mut accumulated_distance = 0.0;
    let (mut right, mut forward) = branch_frame(rings[0].2);
    let mut previous_center = rings[0].0;
    let mut previous_tangent = rings[0].2;
    for (ring, (center, radius, tangent)) in rings.iter().copied().enumerate() {
        if ring > 0 {
            accumulated_distance += center.distance(previous_center);
            (right, forward) = transport_branch_frame(previous_tangent, right, tangent);
        }
        let ring_base = positions.len();
        for side in 0..sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let radial = right * phase.cos() + forward * phase.sin();
            let offset = if surface_root {
                Vec3::new(radial.x * 1.25, radial.y * 0.45, radial.z * 1.25) * radius
            } else {
                radial * radius
            };
            let normal = if surface_root {
                Vec3::new(radial.x / 1.25, radial.y / 0.45, radial.z / 1.25).normalize()
            } else {
                radial
            };
            positions.push((center + offset).to_array());
            normals.push(normal.to_array());
            uvs.push([
                side as f32 / sides as f32 * circumference_tiles,
                accumulated_distance / BARK_TEXTURE_HEIGHT_METRES,
            ]);
        }
        // Duplicate the first evaluated vertex bit-for-bit at the cylindrical
        // wrap. Re-evaluating sin(TAU), relief, and its finite-difference normal
        // can otherwise produce a hairline lighting discontinuity.
        positions.push(positions[ring_base]);
        normals.push(normals[ring_base]);
        uvs.push([
            circumference_tiles,
            accumulated_distance / BARK_TEXTURE_HEIGHT_METRES,
        ]);
        previous_center = center;
        previous_tangent = tangent;
    }
    for ring in 0..rings.len() as u32 - 1 {
        let from = base + ring * ring_stride;
        let to = from + ring_stride;
        for side in 0..sides {
            let next = side + 1;
            indices.extend_from_slice(&[
                from + side,
                to + side,
                to + next,
                from + side,
                to + next,
                from + next,
            ]);
        }
    }
    let end_ring = base + (rings.len() as u32 - 1) * ring_stride;
    if last.is_limb_tip && quality.uses_rounded_terminal() {
        // A pair of shrinking rings gives every terminal axis a rounded,
        // natural taper. Flat caps read as sawn-off limbs and become black
        // rectangular artifacts in the descendant renders.
        let shoulder = positions.len() as u32;
        let bud_length = last.end_radius;
        let (right, forward) = transport_branch_frame(previous_tangent, right, last_direction);
        let mut terminal_distance = accumulated_distance;
        let mut terminal_center = last.end;
        for (distance, radius_scale) in [(0.55, 0.58), (0.92, 0.12)] {
            let radius = last.end_radius * radius_scale;
            let center = last.end
                + last_direction * bud_length * distance
                + Vec3::Y * if surface_root { radius * 0.75 } else { 0.0 };
            let center = if surface_root {
                center.with_y(
                    center
                        .y
                        .max(-TREE_TRUNK_HEIGHT_METRES * 0.5 + radius * 0.05),
                )
            } else {
                center
            };
            terminal_distance += center.distance(terminal_center);
            for side in 0..=sides {
                let phase = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * phase.cos() + forward * phase.sin();
                let offset = if surface_root {
                    Vec3::new(radial.x * 1.25, radial.y * 0.45, radial.z * 1.25) * radius
                } else {
                    radial * radius
                };
                let surface_radial = if surface_root {
                    Vec3::new(radial.x / 1.25, radial.y / 0.45, radial.z / 1.25).normalize()
                } else {
                    radial
                };
                let normal = (surface_radial * 0.75 + last_direction * 0.66).normalize();
                positions.push((center + offset).to_array());
                normals.push(normal.to_array());
                uvs.push([
                    side as f32 / sides as f32 * circumference_tiles,
                    terminal_distance / BARK_TEXTURE_HEIGHT_METRES,
                ]);
            }
            terminal_center = center;
        }
        for ring in 0..2_u32 {
            let from = if ring == 0 { end_ring } else { shoulder };
            let to = shoulder + ring * ring_stride;
            for side in 0..sides {
                let next = side + 1;
                indices.extend_from_slice(&[
                    from + side,
                    to + side,
                    to + next,
                    from + side,
                    to + next,
                    from + next,
                ]);
            }
        }
        let tip = positions.len() as u32;
        positions.push((last.end + last_direction * bud_length).to_array());
        normals.push(last_direction.to_array());
        uvs.push([
            0.0,
            (accumulated_distance + bud_length) / BARK_TEXTURE_HEIGHT_METRES,
        ]);
        for side in 0..sides {
            let next = side + 1;
            indices.extend_from_slice(&[
                tip,
                shoulder + ring_stride + side,
                shoulder + ring_stride + next,
            ]);
        }
    } else if last.is_limb_tip {
        // Aggregate wood sits behind baked canopy cards. A single tapered cap
        // preserves the branch silhouette without the two extra rounded rings
        // that close the near-field mesh.
        let tip = positions.len() as u32;
        let bud_length = last.end_radius;
        positions.push((last.end + last_direction * bud_length * 0.7).to_array());
        normals.push(last_direction.to_array());
        uvs.push([
            0.0,
            (accumulated_distance + bud_length * 0.7) / BARK_TEXTURE_HEIGHT_METRES,
        ]);
        for side in 0..sides {
            let next = side + 1;
            indices.extend_from_slice(&[tip, end_ring + side, end_ring + next]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::geometry::{
        COMMON_BEECH_BARK, COMMON_BEECH_PARAMETERS, procedural_tree_skeleton,
        procedural_woody_plant_skeleton,
    };
    use bevy::{math::Vec3, mesh::VertexAttributeValues};

    #[test]
    fn branch_mesh_has_metric_seam_safe_uvs_without_unused_tangents() {
        let branches = [TreeBranchSegment {
            start: Vec3::ZERO,
            end: Vec3::Y * 2.0,
            start_radius: 0.4,
            end_radius: 0.3,
            depth: 1,
            primary_group: 0,
            secondary_group: 0,
            is_limb_tip: true,
        }];
        let mesh = procedural_tree_branch_mesh(&branches, 1);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("branch mesh has float UVs");
        };
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_none());
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_none(),
            "crown-only wood must not pay for the root deposition channel"
        );
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("branch mesh has float positions");
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attribute| attribute.as_float3())
            .expect("branch mesh has float normals");

        assert!(uvs.iter().flatten().all(|component| component.is_finite()));
        assert_eq!(positions[0], positions[7]);
        assert_eq!(normals[0], normals[7]);
        assert_eq!(uvs[0][0], 0.0);
        assert!(uvs[7][0] >= 1.0 && uvs[7][0].fract().abs() < f32::EPSILON);
        assert!(
            uvs.iter().map(|uv| uv[1]).fold(0.0_f32, f32::max) > 0.5,
            "a two-metre-long axis must advance a full physical bark tile"
        );
    }

    #[test]
    fn aggregate_wood_quality_tiers_reduce_representative_geometry_without_invalid_surface_data() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let full_lod1 =
            procedural_woody_crown_mesh(&branches, 2, WoodyBranchMeshQuality::FullDetail);
        let aggregate_lod1 =
            procedural_woody_crown_mesh(&branches, 2, WoodyBranchMeshQuality::AggregateLod1);
        let full_lod2 =
            procedural_woody_crown_mesh(&branches, 1, WoodyBranchMeshQuality::FullDetail);
        let aggregate_lod2 =
            procedural_woody_crown_mesh(&branches, 1, WoodyBranchMeshQuality::AggregateLod2);
        let repeated_lod1 =
            procedural_woody_crown_mesh(&branches, 2, WoodyBranchMeshQuality::AggregateLod1);

        for mesh in [&aggregate_lod1, &aggregate_lod2] {
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .expect("aggregate mesh has positions");
            let normals = mesh
                .attribute(Mesh::ATTRIBUTE_NORMAL)
                .and_then(VertexAttributeValues::as_float3)
                .expect("aggregate mesh has normals");
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("aggregate mesh has float UVs");
            };
            let indices = mesh.indices().expect("aggregate mesh is indexed");

            assert_eq!(positions.len(), normals.len());
            assert_eq!(positions.len(), uvs.len());
            assert_eq!(indices.len() % 3, 0);
            assert!(indices.iter().all(|index| index < positions.len()));
            assert!(positions.iter().flatten().all(|value| value.is_finite()));
            assert!(uvs.iter().flatten().all(|value| value.is_finite()));
            assert!(normals.iter().all(|normal| {
                let normal = Vec3::from_array(*normal);
                normal.is_finite() && (normal.length() - 1.0).abs() < 1.0e-3
            }));

            let minimum = positions
                .iter()
                .map(|position| Vec3::from_array(*position))
                .reduce(Vec3::min)
                .expect("aggregate mesh has a bound");
            let maximum = positions
                .iter()
                .map(|position| Vec3::from_array(*position))
                .reduce(Vec3::max)
                .expect("aggregate mesh has a bound");
            assert!(minimum.is_finite() && maximum.is_finite());
            assert!(minimum.cmple(maximum).all());
        }

        // Seed 42 is the representative oak fixture used by the tree suite.
        // These exact reductions guard both the LOD budget and the simplified
        // one-cap terminal topology: 9,540/15,523 -> 4,810/7,099 for LOD1,
        // and 1,119/1,897 -> 462/700 for LOD2 (vertices/triangles).
        assert_eq!(full_lod1.count_vertices(), 9_540);
        assert_eq!(
            full_lod1.indices().expect("full LOD1 is indexed").len() / 3,
            15_523
        );
        assert_eq!(aggregate_lod1.count_vertices(), 4_810);
        assert_eq!(
            aggregate_lod1
                .indices()
                .expect("aggregate LOD1 is indexed")
                .len()
                / 3,
            7_099
        );
        assert_eq!(full_lod2.count_vertices(), 1_119);
        assert_eq!(
            full_lod2.indices().expect("full LOD2 is indexed").len() / 3,
            1_897
        );
        assert_eq!(aggregate_lod2.count_vertices(), 462);
        assert_eq!(
            aggregate_lod2
                .indices()
                .expect("aggregate LOD2 is indexed")
                .len()
                / 3,
            700
        );

        assert_eq!(
            aggregate_lod1.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_lod1.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            aggregate_lod1.attribute(Mesh::ATTRIBUTE_NORMAL),
            repeated_lod1.attribute(Mesh::ATTRIBUTE_NORMAL)
        );
        assert_eq!(
            aggregate_lod1.attribute(Mesh::ATTRIBUTE_UV_0),
            repeated_lod1.attribute(Mesh::ATTRIBUTE_UV_0)
        );
        assert_eq!(aggregate_lod1.indices(), repeated_lod1.indices());
    }

    #[test]
    fn trunk_mesh_carries_metric_root_height_without_an_authored_dirt_uv() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let mesh = procedural_tree_branch_mesh(&branches, 0);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("trunk mesh has float positions");
        let Some(VertexAttributeValues::Float32x4(root_data)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("trunk mesh has metric root data");
        };
        let ground_y = -TREE_TRUNK_HEIGHT_METRES * 0.5;

        assert_eq!(positions.len(), root_data.len());
        for (position, root) in positions.iter().zip(root_data) {
            assert!((root[0] - (position[1] - ground_y)).abs() < 1.0e-5);
            assert_eq!(root[1..], [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn mid_distance_trunk_reduces_the_upright_bole_with_valid_deterministic_geometry() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let full = procedural_tree_branch_mesh(&branches, 0);
        let mid = procedural_woody_mid_trunk_mesh(&branches);
        let repeated = procedural_woody_mid_trunk_mesh(&branches);
        let positions = mid
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("mid trunk has positions");
        let normals = mid
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(VertexAttributeValues::as_float3)
            .expect("mid trunk has normals");
        let Some(VertexAttributeValues::Float32x2(uvs)) = mid.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("mid trunk has UVs");
        };
        let Some(VertexAttributeValues::Float32x4(root_data)) =
            mid.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("mid trunk carries metric root height");
        };
        let indices = mid.indices().expect("mid trunk is indexed");

        assert_eq!(positions.len(), normals.len());
        assert_eq!(positions.len(), uvs.len());
        assert_eq!(positions.len(), root_data.len());
        assert_eq!(indices.len() % 3, 0);
        assert!(indices.iter().all(|index| index < positions.len()));
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(uvs.iter().flatten().all(|value| value.is_finite()));
        assert!(normals.iter().all(|normal| {
            let normal = Vec3::from_array(*normal);
            normal.is_finite() && (normal.length() - 1.0).abs() < 1.0e-3
        }));
        let ground_y = -TREE_TRUNK_HEIGHT_METRES * 0.5;
        for (position, root) in positions.iter().zip(root_data) {
            assert!((root[0] - (position[1] - ground_y)).abs() < 1.0e-5);
            assert_eq!(root[1..], [1.0, 1.0, 1.0]);
        }

        // Seed 42 is the representative oak fixture used by the tree suite.
        // Exact counts lock the nine-sided, 0.9-metre, one-cap budget and
        // verify that the root-only close mesh remains substantially denser.
        assert_eq!(full.count_vertices(), 771);
        assert_eq!(
            full.indices().expect("full trunk is indexed").len() / 3,
            1_344
        );
        assert_eq!(mid.count_vertices(), 173);
        assert_eq!(indices.len() / 3, 279);
        assert!(mid.count_vertices() * 2 < full.count_vertices());
        assert!(indices.len() / 3 * 2 < full.indices().unwrap().len() / 3);

        assert_eq!(
            mid.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            mid.attribute(Mesh::ATTRIBUTE_NORMAL),
            repeated.attribute(Mesh::ATTRIBUTE_NORMAL)
        );
        assert_eq!(
            mid.attribute(Mesh::ATTRIBUTE_UV_0),
            repeated.attribute(Mesh::ATTRIBUTE_UV_0)
        );
        assert_eq!(
            mid.attribute(Mesh::ATTRIBUTE_COLOR),
            repeated.attribute(Mesh::ATTRIBUTE_COLOR)
        );
        assert_eq!(mid.indices(), repeated.indices());
    }

    #[test]
    fn explicit_trunk_and_roots_have_bounded_finite_surface_data() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let field = RootFlareField::from_branches(&branches, 0, ENGLISH_OAK_BARK)
            .expect("oak has a root flare");
        assert!(field.maximum.x - field.minimum.x < 4.0);
        assert!(field.maximum.z - field.minimum.z < 4.0);
        assert!((1.0..3.0).contains(&(field.maximum.y - field.minimum.y)));
        let dimensions = ((field.maximum - field.minimum) / field.cell)
            .ceil()
            .as_uvec3();
        assert!(
            u64::from(dimensions.x) * u64::from(dimensions.y) * u64::from(dimensions.z) < 180_000
        );

        assert!(smooth_min(-0.1, -0.1, field.blend) < -0.1);
        for point in [Vec3::ZERO, field.minimum, field.maximum] {
            let hard_union = field
                .segments
                .iter()
                .map(|segment| capsule_distance(point, segment))
                .fold(f32::INFINITY, f32::min);
            assert!(field.distance(point) <= hard_union + 1.0e-6);
        }

        let mesh = procedural_tree_branch_mesh(&branches, 0);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .unwrap();
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attribute| attribute.as_float3())
            .unwrap();
        assert!(positions.len() > 100);
        assert!(
            positions.len() < 50_000,
            "hybrid trunk exceeded its bounded vertex budget: {}",
            positions.len()
        );
        assert_eq!(positions.len(), normals.len());
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(normals.iter().flatten().all(|value| value.is_finite()));
        assert!(
            normals
                .iter()
                .all(|normal| { (Vec3::from_array(*normal).length() - 1.0).abs() < 1.0e-3 })
        );
    }

    #[test]
    fn smooth_beech_base_uses_a_finer_quieter_union_than_oak() {
        let oak = procedural_tree_skeleton(42, 0.0);
        let oak_field =
            RootFlareField::from_branches(&oak, 0, ENGLISH_OAK_BARK).expect("oak has a root flare");
        let beech = procedural_woody_plant_skeleton(42, 0.65, COMMON_BEECH_PARAMETERS);
        let beech_field = RootFlareField::from_branches(&beech, 0, COMMON_BEECH_BARK)
            .expect("beech has a root flare");

        assert!(beech_field.cell < oak_field.cell);
        assert!(beech_field.blend < oak_field.blend);
        assert!((4..=8).contains(&beech_field.segments.len()));
    }

    #[test]
    fn root_flare_is_basal_and_overlaps_the_swept_upper_trunk() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let field = RootFlareField::from_branches(&branches, 0, ENGLISH_OAK_BARK)
            .expect("oak has a root flare");
        assert!(
            field
                .segments
                .iter()
                .any(|segment| segment.start.y > field.maximum.y
                    || segment.end.y > field.maximum.y)
        );
        let visible_depth_zero = branches
            .iter()
            .copied()
            .filter(|segment| segment.depth == 0)
            .filter_map(|segment| field.visible_segment(segment))
            .collect::<Vec<_>>();
        assert!(!visible_depth_zero.is_empty());
        assert!(
            visible_depth_zero
                .iter()
                .all(|segment| segment.start.y >= field.sweep_start_y - 1.0e-5)
        );
    }

    #[test]
    fn bark_phase_is_deterministic_seeded_and_bounded() {
        let phase = bark_phase_from_branches(&procedural_tree_skeleton(42, 0.0));
        assert_eq!(
            phase,
            bark_phase_from_branches(&procedural_tree_skeleton(42, 0.0))
        );
        assert_ne!(
            phase,
            bark_phase_from_branches(&procedural_tree_skeleton(43, 0.0))
        );
        assert!((0.0..=core::f32::consts::TAU).contains(&phase));
    }

    #[test]
    fn bark_relief_is_periodic_and_blends_at_root_influence_boundaries() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let bark_phase = bark_phase_from_branches(&branches);
        let field = RootFlareField::from_branches(&branches, 0, ENGLISH_OAK_BARK)
            .expect("oak has a root flare");
        let trunk = field
            .segments
            .iter()
            .find(|segment| (segment.end - segment.start).xz().length() < 0.1)
            .expect("oak flare includes a vertical trunk segment");
        let axis = trunk.end - trunk.start;
        let center = trunk.start + axis * 0.35;
        let tangent = axis.normalize();
        let (right, forward) = branch_frame(tangent);
        let radius = trunk
            .start_radius
            .lerp(trunk.end_radius, 0.35_f32.powf(0.64));
        let epsilon = 1.0e-5_f32;
        let before = center + (right * epsilon.cos() + forward * (-epsilon).sin()) * radius;
        let after = center + (right * epsilon.cos() + forward * epsilon.sin()) * radius;
        assert!(
            (bark_relief(before, trunk, ENGLISH_OAK_BARK, bark_phase)
                - bark_relief(after, trunk, ENGLISH_OAK_BARK, bark_phase))
            .abs()
                < 1.0e-3
        );

        let junction = field
            .segments
            .iter()
            .filter(|segment| (segment.end - segment.start).xz().length() > 0.2)
            .map(|segment| segment.start)
            .next()
            .expect("oak flare includes a root junction");
        let left = blended_bark_relief(
            junction - Vec3::X * 0.002,
            &field.segments,
            field.blend,
            ENGLISH_OAK_BARK,
            bark_phase,
        );
        let right = blended_bark_relief(
            junction + Vec3::X * 0.002,
            &field.segments,
            field.blend,
            ENGLISH_OAK_BARK,
            bark_phase,
        );
        assert!(left.is_finite() && right.is_finite());
        assert!((left - right).abs() < 0.01);
    }

    #[test]
    fn oak_bark_matures_with_radius_and_fades_on_young_branch_orders() {
        let sample = |radius: f32, depth: u8| {
            let segment = TreeBranchSegment {
                start: Vec3::ZERO,
                end: Vec3::Y * 2.0,
                start_radius: radius,
                end_radius: radius,
                depth,
                primary_group: 0,
                secondary_group: 0,
                is_limb_tip: true,
            };
            (0..96)
                .map(|index| {
                    let phase = index as f32 * core::f32::consts::TAU / 96.0;
                    let point = Vec3::new(phase.cos() * radius, 0.8, phase.sin() * radius);
                    bark_relief(point, &segment, ENGLISH_OAK_BARK, 0.0).abs()
                })
                .fold(0.0_f32, f32::max)
        };
        let mature_trunk = sample(0.6, 0);
        assert!(mature_trunk > sample(0.09, 0) * 2.0);
        assert!(mature_trunk > sample(0.6, 2) * 3.0);
        assert!(sample(0.03, 3) < 1.0e-5);
    }

    #[test]
    fn root_profile_lobes_follow_generated_root_directions_and_fade_upward() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let field = RootFlareField::from_branches(&branches, 0, ENGLISH_OAK_BARK)
            .expect("oak has a root flare");
        let base = field
            .segments
            .iter()
            .map(|segment| segment.start.y)
            .fold(f32::INFINITY, f32::min);
        let lobes = (0..128)
            .map(|index| {
                let phase = index as f32 * core::f32::consts::TAU / 128.0;
                let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
                (
                    outward,
                    field.root_profile_relief(Vec3::new(0.0, base + 0.15, 0.0), outward),
                )
            })
            .collect::<Vec<_>>();
        let (direction, aligned) = lobes
            .iter()
            .copied()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .unwrap();
        let transverse = lobes
            .iter()
            .map(|(_, relief)| *relief)
            .fold(f32::INFINITY, f32::min);
        let high = field.root_profile_relief(Vec3::new(0.0, base + 1.6, 0.0), direction);
        assert!(aligned > transverse + 0.015);
        assert!(aligned > high * 4.0);
    }
}
