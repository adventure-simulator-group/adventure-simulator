use super::super::super::*;

pub(in crate::presentation) const TREE_PRIMARY_GROUP_COUNT: u8 = 7;
pub(in crate::presentation) const TREE_SECONDARY_GROUPS_PER_PRIMARY: u16 = 10;

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

pub(in crate::presentation) fn procedural_tree_skeleton(
    seed: u64,
    canopy_competition: f32,
) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let canopy_competition = canopy_competition.clamp(0.0, 1.0);
    let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
    let bend_direction = Vec3::new(crown_phase.cos(), 0.0, crown_phase.sin());

    // Quercus robur is usually short-boled in the open.  The trunk loses
    // dominance inside a broad crown instead of continuing as a conifer-like
    // central spear.
    let trunk_length = 4.95_f32.lerp(9.2, canopy_competition);
    let trunk_points = (0..=6)
        .map(|index| {
            let t = index as f32 / 6.0;
            Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0)
                + Vec3::Y * (trunk_length * t)
                + bend_direction * (0.28 * t.powf(1.45))
        })
        .collect::<Vec<_>>();
    append_branch_curve(
        &mut branches,
        &trunk_points,
        0.72_f32.lerp(0.56, canopy_competition),
        0.045_f32.lerp(0.035, canopy_competition),
        0,
        u8::MAX,
        u16::MAX,
    );

    // Buttress-like surface roots visually carry the weight of the low,
    // spreading scaffold limbs without changing the authoritative collider.
    for root_index in 0..8_u64 {
        let root_seed = splitmix64(seed ^ 0xb077_0000 ^ root_index);
        let phase = crown_phase
            + root_index as f32 * core::f32::consts::TAU / 8.0
            + (unit_hash(root_seed) - 0.5) * 0.22;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let start = trunk_points[0] + Vec3::Y * 0.09;
        let points = [
            start,
            start + outward * 0.48 + Vec3::Y * 0.01,
            start + outward * (0.92 + unit_hash(root_seed ^ 1) * 0.28) - Vec3::Y * 0.12,
        ];
        append_branch_curve(&mut branches, &points, 0.34, 0.045, 0, u8::MAX, u16::MAX);
    }

    // Five unequal scaffold limbs fill a low, wide, irregular dome.  Each
    // has a shallow sag and a recovered tip, a characteristic silhouette of
    // a vigorous open-grown pedunculate oak.
    for primary_index in 0..u64::from(TREE_PRIMARY_GROUP_COUNT) {
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.42;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let isolated_attach = 0.3 + primary_index as f32 * 0.06;
        let competitive_attach = 0.68 + primary_index as f32 * 0.05;
        let attach = isolated_attach.lerp(competitive_attach.min(0.975), canopy_competition);
        let start = sample_polyline(&trunk_points, attach);
        // In the open, every principal axis contributes to the same broad,
        // ascending dome. Keeping separate vertical "leader" axes produces
        // a pollarded V silhouette instead of the decurrent habit of a mature
        // oak. Competition progressively shortens these lateral reaches and
        // raises their attachment points, leaving a tall clear bole.
        let rank = primary_index as f32;
        let isolated_reach = 5.9 - rank * 0.34 + unit_hash(primary_seed ^ 1) * 0.42;
        let isolated_lift = 0.9 + rank * 0.35 + unit_hash(primary_seed ^ 2) * 0.24;
        let isolated_sag = (0.9 - rank * 0.1).max(0.24);
        let competitive_reach = 2.8 - rank * 0.12;
        let competitive_lift = 2.0 + rank * 0.45;
        let reach = isolated_reach.lerp(competitive_reach, canopy_competition);
        let lift = isolated_lift.lerp(competitive_lift, canopy_competition);
        let sag = isolated_sag.lerp(0.32, canopy_competition);
        let lateral = (unit_hash(primary_seed ^ 3) - 0.5) * 1.25;
        let primary_points = (0..=6)
            .map(|point_index| {
                let t = point_index as f32 / 6.0;
                let eased = t * (0.72 + 0.28 * t);
                start
                    + outward * reach * eased
                    + tangent * lateral * (core::f32::consts::PI * t).sin()
                    + Vec3::Y * (-sag * (core::f32::consts::PI * t).sin() + lift * t.powf(1.85))
            })
            .collect::<Vec<_>>();
        append_branch_curve(
            &mut branches,
            &primary_points,
            0.3 + unit_hash(primary_seed ^ 4) * 0.07,
            0.042,
            1,
            primary_index as u8,
            u16::MAX,
        );

        for secondary_index in 0..u64::from(TREE_SECONDARY_GROUPS_PER_PRIMARY) {
            let secondary_seed = splitmix64(primary_seed ^ (secondary_index + 0x51));
            // The last axis inherits the scaffold direction and carries
            // foliage over its end. The remaining axes alternate laterally,
            // avoiding the bare antler tips produced by stopping all child
            // branches well before their parent.
            let is_terminal_axis = secondary_index == 9;
            let attach = if is_terminal_axis {
                0.96
            } else {
                let normalized = secondary_index as f32 / 8.0;
                0.22 + normalized.powf(0.7) * 0.68
            };
            let secondary_start = sample_polyline(&primary_points, attach);
            let inherited = polyline_tangent(&primary_points, attach);
            let (frame_right, frame_up) = branch_frame(inherited);
            let branch_phase =
                secondary_index as f32 * 2.399_963_1 + unit_hash(secondary_seed ^ 0x31) * 0.55;
            let radial = frame_right * branch_phase.cos() + frame_up * branch_phase.sin();
            let inherited_weight = if is_terminal_axis { 0.78 } else { 0.38 };
            let lateral_weight = if is_terminal_axis { 0.2 } else { 0.68 };
            let secondary_direction = (inherited
                * (inherited_weight + unit_hash(secondary_seed) * 0.16)
                + radial * (lateral_weight + unit_hash(secondary_seed ^ 1) * 0.2)
                + outward * 0.2
                + Vec3::Y * (0.18 + unit_hash(secondary_seed ^ 2) * 0.34))
                .normalize();
            let secondary_length = (1.75 + unit_hash(secondary_seed ^ 3) * 0.95)
                * 1.0_f32.lerp(0.76, canopy_competition);
            let secondary_bend = radial * (unit_hash(secondary_seed ^ 4) - 0.5) * 0.55;
            let secondary_points = (0..=3)
                .map(|point_index| {
                    let t = point_index as f32 / 3.0;
                    secondary_start
                        + secondary_direction * secondary_length * t
                        + secondary_bend * (core::f32::consts::PI * t).sin()
                        + Vec3::Y * (0.2 * t.powf(1.7))
                })
                .collect::<Vec<_>>();
            let secondary_group = (primary_index * u64::from(TREE_SECONDARY_GROUPS_PER_PRIMARY)
                + secondary_index) as u16;
            append_branch_curve(
                &mut branches,
                &secondary_points,
                0.072,
                0.013,
                2,
                primary_index as u8,
                secondary_group,
            );

            // Short shoots are distributed around the secondary limb rather
            // than combed vertically.  Their terminal leaf rosettes overlap
            // into porous masses while leaving glimpses of the scaffold.
            for shoot_index in 0..14_u64 {
                let shoot_seed = splitmix64(secondary_seed ^ (shoot_index + 0xa3));
                let normalized = shoot_index as f32 / 13.0;
                let attach = 0.48 + normalized.powf(0.62) * 0.5;
                let shoot_start = sample_polyline(&secondary_points, attach);
                let inherited = polyline_tangent(&secondary_points, attach);
                let (frame_right, frame_up) = branch_frame(inherited);
                let spiral = shoot_index as f32 * 2.399_963_1
                    + unit_hash(shoot_seed) * core::f32::consts::TAU;
                let radial = frame_right * spiral.cos() + frame_up * spiral.sin();
                let shoot_direction = (inherited * 0.5
                    + radial * (0.5 + unit_hash(shoot_seed ^ 1) * 0.24)
                    + Vec3::Y * (0.06 + unit_hash(shoot_seed ^ 2) * 0.22))
                    .normalize();
                let shoot_length = 0.5 + unit_hash(shoot_seed ^ 3) * 0.42;
                let shoot_mid =
                    shoot_start + shoot_direction * shoot_length * 0.52 + radial * 0.035;
                let shoot_end = shoot_start
                    + shoot_direction * shoot_length
                    + radial * (unit_hash(shoot_seed ^ 4) - 0.5) * 0.11;
                append_branch_curve(
                    &mut branches,
                    &[shoot_start, shoot_mid, shoot_end],
                    0.012,
                    0.0032,
                    3,
                    primary_index as u8,
                    secondary_group,
                );
            }
        }
    }
    branches
}

fn append_branch_curve(
    branches: &mut Vec<TreeBranchSegment>,
    points: &[Vec3],
    start_radius: f32,
    end_radius: f32,
    depth: u8,
    primary_group: u8,
    secondary_group: u16,
) {
    let segment_count = points.len() - 1;
    for index in 0..segment_count {
        let t0 = index as f32 / segment_count as f32;
        let t1 = (index + 1) as f32 / segment_count as f32;
        branches.push(TreeBranchSegment {
            start: points[index],
            end: points[index + 1],
            start_radius: start_radius.lerp(end_radius, t0.powf(0.82)),
            end_radius: start_radius.lerp(end_radius, t1.powf(0.82)),
            depth,
            primary_group,
            secondary_group,
            is_limb_tip: index + 1 == segment_count,
        });
    }
}

fn sample_polyline(points: &[Vec3], t: f32) -> Vec3 {
    let scaled = t.clamp(0.0, 1.0) * (points.len() - 1) as f32;
    let index = scaled.floor().min((points.len() - 2) as f32) as usize;
    points[index].lerp(points[index + 1], scaled - index as f32)
}

fn polyline_tangent(points: &[Vec3], t: f32) -> Vec3 {
    let scaled = t.clamp(0.0, 1.0) * (points.len() - 1) as f32;
    let index = scaled.floor().min((points.len() - 2) as f32) as usize;
    (points[index + 1] - points[index]).normalize()
}

fn branch_frame(direction: Vec3) -> (Vec3, Vec3) {
    let reference = if direction.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = direction.cross(reference).normalize();
    (right, right.cross(direction).normalize())
}

#[allow(dead_code)]
pub(in crate::presentation) fn legacy_procedural_tree_skeleton(
    seed: u64,
) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
    let trunk_bend = Vec3::new(crown_phase.cos(), 0.0, crown_phase.sin())
        * (0.22 + unit_hash(seed ^ 0x51c7_329d) * 0.18);
    let trunk_points = (0..=7)
        .map(|index| {
            let t = index as f32 / 7.0;
            Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0)
                + Vec3::Y * (6.7 * t)
                + trunk_bend * t.powf(1.3)
        })
        .collect::<Vec<_>>();
    for index in 0..7 {
        let t0 = index as f32 / 7.0;
        let t1 = (index + 1) as f32 / 7.0;
        branches.push(TreeBranchSegment {
            start: trunk_points[index],
            end: trunk_points[index + 1],
            start_radius: 0.62 * (1.0 - t0 * 0.76),
            end_radius: (0.62 * (1.0 - t1 * 0.9)).max(0.025),
            depth: 0,
            primary_group: u8::MAX,
            secondary_group: u16::MAX,
            is_limb_tip: index == 6,
        });
    }

    for root_index in 0..8_u64 {
        let phase = crown_phase + root_index as f32 * core::f32::consts::TAU / 8.0;
        let direction = Vec3::new(phase.cos(), -0.1, phase.sin());
        branches.push(TreeBranchSegment {
            start: trunk_points[0] + Vec3::Y * 0.08,
            end: trunk_points[0] + direction * (0.72 + unit_hash(seed ^ root_index) * 0.35),
            start_radius: 0.3,
            end_radius: 0.055,
            depth: 0,
            primary_group: u8::MAX,
            secondary_group: u16::MAX,
            is_limb_tip: false,
        });
    }

    // A mature open-grown English oak has a few unequal, load-bearing scaffold
    // limbs rather than a radial whorl of equivalent ascending rods.
    for primary_index in 0..7_u64 {
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.52;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let trunk_t = 0.29 + primary_index as f32 * 0.062;
        let trunk_scaled = trunk_t * 7.0;
        let trunk_segment = trunk_scaled.floor().min(6.0) as usize;
        let start =
            trunk_points[trunk_segment].lerp(trunk_points[trunk_segment + 1], trunk_scaled.fract());
        let lower_crown = 1.0 - primary_index as f32 / 7.0;
        let reach = 3.8 + lower_crown * 2.05 + unit_hash(primary_seed ^ 1) * 0.65;
        let rise = 1.0 + (1.0 - lower_crown) * 2.0 + unit_hash(primary_seed ^ 2) * 0.45;
        let sag = 0.85 + lower_crown * 0.7;
        let curve = (unit_hash(primary_seed ^ 3) - 0.5) * 1.15;
        let mut primary_points = [Vec3::ZERO; 6];
        for (point_index, point) in primary_points.iter_mut().enumerate() {
            let t = point_index as f32 / 5.0;
            let gravity = -sag * (core::f32::consts::PI * t).sin();
            let recovery = rise * t.powf(2.15);
            *point = start
                + outward * reach * t
                + Vec3::Y * (gravity + recovery)
                + tangent * curve * (core::f32::consts::PI * t).sin()
                + outward * 0.22 * (t * core::f32::consts::TAU).sin();
        }
        let primary_base_radius = 0.3 + lower_crown * 0.16 + unit_hash(primary_seed ^ 9) * 0.05;
        for segment_index in 0..5 {
            let t0 = segment_index as f32 / 5.0;
            let t1 = (segment_index + 1) as f32 / 5.0;
            branches.push(TreeBranchSegment {
                start: primary_points[segment_index],
                end: primary_points[segment_index + 1],
                start_radius: primary_base_radius * (1.0 - t0 * 0.78),
                end_radius: (primary_base_radius * (1.0 - t1 * 0.9)).max(0.018),
                depth: 1,
                primary_group: primary_index as u8,
                secondary_group: u16::MAX,
                is_limb_tip: segment_index == 4,
            });
        }

        for secondary_index in 0..8_u64 {
            let secondary_seed = splitmix64(primary_seed ^ (secondary_index + 11));
            let attach_t = 0.25 + secondary_index as f32 * 0.09;
            let primary_scaled = attach_t * 5.0;
            let segment = primary_scaled.floor().min(4.0) as usize;
            let secondary_start =
                primary_points[segment].lerp(primary_points[segment + 1], primary_scaled.fract());
            let side = if secondary_index & 1 == 0 { 1.0 } else { -1.0 };
            let yaw = phase
                + (secondary_index as f32 - 3.5) * 0.19
                + side * (0.5 + unit_hash(secondary_seed) * 0.38);
            let secondary_outward = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            let inherited = (primary_points[segment + 1] - primary_points[segment]).normalize();
            let secondary_direction = (inherited * 0.34
                + secondary_outward * 0.68
                + Vec3::Y * (0.28 + unit_hash(secondary_seed ^ 1) * 0.3))
                .normalize();
            let secondary_length = 1.5 + unit_hash(secondary_seed ^ 2) * 0.85;
            let bend = tangent * side * (0.22 + unit_hash(secondary_seed ^ 3) * 0.3);
            let mut secondary_points = [Vec3::ZERO; 4];
            for (point_index, point) in secondary_points.iter_mut().enumerate() {
                let t = point_index as f32 / 3.0;
                *point = secondary_start
                    + secondary_direction * secondary_length * t
                    + bend * (core::f32::consts::PI * t).sin()
                    + Vec3::Y * (0.55 * t.powf(2.0) - 0.2 * (core::f32::consts::PI * t).sin());
            }
            let secondary_group = (primary_index * 8 + secondary_index) as u16;
            for segment_index in 0..3 {
                let t0 = segment_index as f32 / 3.0;
                let t1 = (segment_index + 1) as f32 / 3.0;
                branches.push(TreeBranchSegment {
                    start: secondary_points[segment_index],
                    end: secondary_points[segment_index + 1],
                    start_radius: 0.095 * (1.0 - t0 * 0.75),
                    end_radius: (0.095 * (1.0 - t1 * 0.75)).max(0.018),
                    depth: 2,
                    primary_group: primary_index as u8,
                    secondary_group,
                    is_limb_tip: segment_index == 2,
                });
            }
            for twig_index in 0..32_u64 {
                let twig_seed = splitmix64(secondary_seed ^ (twig_index + 23));
                let twig_start = secondary_points[2]
                    .lerp(secondary_points[3], 0.08 + twig_index as f32 / 31.0 * 0.9);
                let twig_yaw = yaw + (twig_index as f32 - 15.5) * 0.14;
                let twig_direction = (secondary_direction * 0.5
                    + Vec3::new(
                        twig_yaw.cos(),
                        0.22 + unit_hash(twig_seed ^ 8) * 0.62,
                        twig_yaw.sin(),
                    ) * 0.68)
                    .normalize();
                let twig_length = 0.62 + unit_hash(twig_seed) * 0.28;
                let twig_mid = twig_start + twig_direction * twig_length * 0.54;
                let twig_end = twig_start
                    + twig_direction * twig_length
                    + Vec3::Y * (0.1 + unit_hash(twig_seed ^ 4) * 0.1);
                for (segment_index, (start, end)) in [(twig_start, twig_mid), (twig_mid, twig_end)]
                    .into_iter()
                    .enumerate()
                {
                    branches.push(TreeBranchSegment {
                        start,
                        end,
                        start_radius: if segment_index == 0 { 0.018 } else { 0.011 },
                        end_radius: if segment_index == 0 { 0.011 } else { 0.0045 },
                        depth: 3,
                        primary_group: primary_index as u8,
                        secondary_group,
                        is_limb_tip: segment_index == 1,
                    });
                }
            }
        }
    }
    branches
}

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct TreeLeaf {
    pub(in crate::presentation) petiole_start: Vec3,
    pub(in crate::presentation) center: Vec3,
    pub(in crate::presentation) right: Vec3,
    pub(in crate::presentation) up: Vec3,
    pub(in crate::presentation) length: f32,
    pub(in crate::presentation) width: f32,
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) secondary_group: u16,
    pub(in crate::presentation) shoot_id: u16,
    pub(in crate::presentation) shade: f32,
}

pub(in crate::presentation) fn procedural_oak_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let tangent = direction.cross(Vec3::Y).normalize_or_zero();
        let tangent = if tangent.length_squared() < 0.25 {
            Vec3::X
        } else {
            tangent
        };
        let binormal = direction.cross(tangent).normalize();
        for leaf_index in 0..24_u64 {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Alternate leaves along each current-year shoot, then finish in
            // the tighter terminal flush characteristic of pedunculate oak.
            let along = if leaf_index < 4 {
                0.18 + leaf_index as f32 * 0.15
            } else {
                0.74 + (leaf_index - 4) as f32 / 19.0 * 0.24
            };
            let alternate = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let spiral = leaf_index as f32 * 2.399_963_1 + (unit_hash(leaf_seed ^ 2) - 0.5) * 0.65;
            let radial = (tangent * spiral.cos() + binormal * spiral.sin()).normalize();
            let leaf_up = (radial * (0.46 + unit_hash(leaf_seed ^ 3) * 0.24)
                + direction * (0.42 + unit_hash(leaf_seed ^ 8) * 0.18)
                + Vec3::Y * (0.08 + unit_hash(leaf_seed ^ 9) * 0.18))
                .normalize();
            let leaf_normal_candidate = direction.cross(radial) * alternate
                + radial * (unit_hash(leaf_seed ^ 10) - 0.5) * 0.7
                + Vec3::Y * (unit_hash(leaf_seed ^ 11) - 0.5) * 0.35;
            let leaf_normal = if leaf_normal_candidate.length_squared() > 0.001 {
                leaf_normal_candidate.normalize()
            } else {
                branch_frame(leaf_up).1
            };
            let right_candidate = leaf_up.cross(leaf_normal);
            let leaf_right = if right_candidate.length_squared() > 0.001 {
                right_candidate.normalize()
            } else {
                branch_frame(leaf_up).0 * alternate
            };
            let petiole_start = shoot.start.lerp(shoot.end, along.min(0.98));
            let petiole_length = 0.025 + unit_hash(leaf_seed ^ 4) * 0.025;
            let blade_base =
                petiole_start + (radial * 0.82 + leaf_up * 0.18).normalize() * petiole_length;
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + leaf_up * (0.05 + unit_hash(leaf_seed ^ 5) * 0.03),
                right: leaf_right.normalize(),
                up: leaf_up,
                length: 0.1 + unit_hash(leaf_seed ^ 5) * 0.06,
                width: 0.065 + unit_hash(leaf_seed ^ 6) * 0.04,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                shade: if leaf_index < 4 {
                    0.72 + unit_hash(leaf_seed ^ 7) * 0.18
                } else {
                    0.9 + unit_hash(leaf_seed ^ 7) * 0.22
                },
            });
        }
    }
    leaves
}

pub(in crate::presentation) fn oak_leaf_outline() -> Vec<Vec2> {
    let samples = 18;
    let mut outline = Vec::with_capacity(samples * 2 + 2);
    for side in [-1.0_f32, 1.0] {
        let range: Box<dyn Iterator<Item = usize>> = if side < 0.0 {
            Box::new(0..=samples)
        } else {
            Box::new((0..=samples).rev())
        };
        for index in range {
            let t = index as f32 / samples as f32;
            // Floating-point sin(PI) can be a tiny negative value; clamp it
            // before the fractional power so leaf tips never become NaN.
            let blade = (core::f32::consts::PI * t).sin().max(0.0).powf(0.72);
            let lobes = (t * core::f32::consts::PI * 5.0).sin().abs().powf(0.48);
            let auricle = (-((t - 0.08) / 0.055).powi(2)).exp() * 0.34;
            let taper = if t > 0.86 { (1.0 - t) / 0.14 } else { 1.0 };
            let half_width = (0.18 + 0.82 * lobes + auricle) * blade * taper.max(0.0);
            outline.push(Vec2::new(side * half_width * 0.5, t - 0.5));
        }
    }
    outline
}

pub(in crate::presentation) fn procedural_oak_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
    let outline = oak_leaf_outline();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for leaf in leaves {
        let normal = leaf.right.cross(leaf.up).normalize();
        let blade_base = leaf.center - leaf.up * leaf.length * 0.5;
        let petiole_half_width = 0.0025;
        let petiole_base = positions.len() as u32;
        for position in [
            leaf.petiole_start - leaf.right * petiole_half_width,
            leaf.petiole_start + leaf.right * petiole_half_width,
            blade_base + leaf.right * petiole_half_width,
            blade_base - leaf.right * petiole_half_width,
        ] {
            positions.push(position.to_array());
            normals.push(normal.to_array());
            uvs.push([0.5, 0.5]);
            colors.push([0.52, 0.65, 0.36, 1.0]);
        }
        indices.extend_from_slice(&[
            petiole_base,
            petiole_base + 1,
            petiole_base + 2,
            petiole_base,
            petiole_base + 2,
            petiole_base + 3,
        ]);
        let base = positions.len() as u32;
        positions.push(leaf.center.to_array());
        normals.push(normal.to_array());
        uvs.push([0.5, 0.5]);
        colors.push([leaf.shade * 0.84, leaf.shade, leaf.shade * 0.72, 1.0]);
        for point in &outline {
            let cup = normal * (point.x.abs() * 0.018 + (point.y * 3.0).sin() * 0.002);
            let position = leaf.center
                + leaf.right * point.x * leaf.width
                + leaf.up * point.y * leaf.length
                + cup;
            positions.push(position.to_array());
            normals.push((normal + leaf.up * point.y * 0.14).normalize().to_array());
            uvs.push([point.x + 0.5, point.y + 0.5]);
            colors.push([leaf.shade * 0.84, leaf.shade, leaf.shade * 0.72, 1.0]);
        }
        for index in 0..outline.len() as u32 {
            let next = (index + 1) % outline.len() as u32;
            indices.extend_from_slice(&[base, base + 1 + index, base + 1 + next]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for branch in branches
        .iter()
        .filter(|branch| branch.depth <= maximum_depth)
    {
        append_branch_tube(
            *branch,
            (8_u32.saturating_sub(u32::from(branch.depth))).max(4),
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
        );
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(in crate::presentation) fn append_branch_tube(
    branch: TreeBranchSegment,
    sides: u32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let direction = (branch.end - branch.start).normalize();
    let reference = if direction.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = direction.cross(reference).normalize();
    let forward = right.cross(direction).normalize();
    let start_center = branch.start - direction * branch.start_radius * 0.18;
    let end_center_position =
        branch.end + direction * branch.end_radius * if branch.is_limb_tip { 0.0 } else { 0.32 };
    let base = positions.len() as u32;
    for ring in 0..2 {
        let (center, radius) = if ring == 0 {
            (start_center, branch.start_radius)
        } else {
            (end_center_position, branch.end_radius)
        };
        for side in 0..sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([side as f32 / sides as f32, ring as f32]);
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
    if branch.is_limb_tip {
        // A pair of shrinking rings gives every terminal axis a rounded,
        // natural taper. Flat caps read as sawn-off limbs and become black
        // rectangular artifacts in the descendant renders.
        let shoulder = positions.len() as u32;
        for (ring, (distance, radius_scale)) in [(0.55, 0.58), (0.92, 0.12)].into_iter().enumerate()
        {
            let center = branch.end + direction * branch.end_radius * distance;
            for side in 0..sides {
                let phase = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * phase.cos() + forward * phase.sin();
                let normal = (radial * 0.75 + direction * 0.66).normalize();
                positions.push((center + radial * branch.end_radius * radius_scale).to_array());
                normals.push(normal.to_array());
                uvs.push([side as f32 / sides as f32, 1.0 + ring as f32 * 0.25]);
            }
        }
        for ring in 0..2_u32 {
            let from = if ring == 0 { base + sides } else { shoulder };
            let to = shoulder + ring * sides;
            for side in 0..sides {
                let next = (side + 1) % sides;
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
        positions.push((branch.end + direction * branch.end_radius).to_array());
        normals.push(direction.to_array());
        uvs.push([0.5, 1.5]);
        for side in 0..sides {
            let next = (side + 1) % sides;
            indices.extend_from_slice(&[tip, shoulder + sides + side, shoulder + sides + next]);
        }
    }
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
    fn procedural_tree_has_a_deterministic_four_order_branch_hierarchy() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let counts = (0..=3)
            .map(|depth| {
                branches
                    .iter()
                    .filter(|branch| branch.depth == depth)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(counts, vec![22, 42, 210, 1_960]);
        assert!(branches.iter().all(|branch| {
            branch.start.is_finite()
                && branch.end.is_finite()
                && branch.start.distance(branch.end) > 0.0
        }));
        assert!(
            branches.iter().all(|branch| {
                branch.start_radius > branch.end_radius && branch.end_radius > 0.0
            })
        );
    }

    #[test]
    fn high_resolution_oak_has_finite_individual_leaf_geometry() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches);
        assert_eq!(leaves.len(), 23_520);
        assert!(leaves.iter().all(|leaf| {
            leaf.petiole_start.is_finite()
                && leaf.center.is_finite()
                && leaf.right.is_finite()
                && leaf.up.is_finite()
                && leaf.right.length_squared() > 0.9
                && leaf.up.length_squared() > 0.9
                && leaf.right.cross(leaf.up).length_squared() > 0.5
        }));
        let mesh = procedural_oak_leaf_mesh(&leaves);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("leaf mesh has float positions");
        assert_eq!(positions.len(), leaves.len() * 43);
        assert!(
            positions
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
    }

    #[test]
    fn every_live_axis_is_connected_and_terminates_in_descendants() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches);
        for (index, branch) in branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.depth > 0)
        {
            assert!(branches.iter().enumerate().any(|(other_index, other)| {
                index != other_index
                    && other.depth <= branch.depth
                    && point_segment_distance(branch.start, other.start, other.end) < 0.002
            }));
        }
        for branch in branches
            .iter()
            .filter(|branch| branch.is_limb_tip && branch.depth > 0)
        {
            if branch.depth < 3 {
                assert!(
                    branches.iter().any(|child| {
                        child.depth > branch.depth
                            && point_segment_distance(child.start, branch.start, branch.end) < 0.002
                            && child.start.distance(branch.end) < 0.55
                    }),
                    "live terminal has no nearby descendant: {branch:?}"
                );
            } else {
                assert!(leaves.iter().any(|leaf| {
                    leaf.secondary_group == branch.secondary_group
                        && leaf.center.distance(branch.end) < 0.55
                }));
            }
        }
    }

    #[test]
    fn canopy_competition_raises_the_clear_bole_and_narrows_the_crown() {
        let isolated = procedural_tree_skeleton(42, 0.0);
        let competitive = procedural_tree_skeleton(42, 1.0);
        let isolated_crown = tree_crown_bounds(&isolated, |branch| branch.depth > 0);
        let competitive_crown = tree_crown_bounds(&competitive, |branch| branch.depth > 0);
        let isolated_first_branch = isolated
            .iter()
            .filter(|branch| branch.depth == 1)
            .map(|branch| branch.start.y)
            .fold(f32::INFINITY, f32::min);
        let competitive_first_branch = competitive
            .iter()
            .filter(|branch| branch.depth == 1)
            .map(|branch| branch.start.y)
            .fold(f32::INFINITY, f32::min);
        assert!(competitive_first_branch > isolated_first_branch + 3.0);
        assert!(competitive_crown.maximum.y > isolated_crown.maximum.y + 2.0);
        assert!(isolated_crown.horizontal_span() > competitive_crown.horizontal_span() * 1.3);
    }

    #[test]
    fn canopy_competition_changes_oak_architecture_continuously_and_monotonically() {
        let metrics = [0.0, 0.25, 0.5, 0.75, 1.0].map(|competition| {
            let branches = procedural_tree_skeleton(42, competition);
            let crown = tree_crown_bounds(&branches, |branch| branch.depth > 0);
            let crown_base = branches
                .iter()
                .filter(|branch| branch.depth == 1)
                .map(|branch| branch.start.y)
                .fold(f32::INFINITY, f32::min);
            (crown.horizontal_span(), crown.maximum.y, crown_base)
        });
        assert!(metrics.windows(2).all(|pair| pair[1].0 < pair[0].0));
        assert!(metrics.windows(2).all(|pair| pair[1].1 > pair[0].1));
        assert!(metrics.windows(2).all(|pair| pair[1].2 > pair[0].2));
    }

    fn point_segment_distance(point: Vec3, start: Vec3, end: Vec3) -> f32 {
        let segment = end - start;
        let along = ((point - start).dot(segment) / segment.length_squared()).clamp(0.0, 1.0);
        point.distance(start + segment * along)
    }
}
