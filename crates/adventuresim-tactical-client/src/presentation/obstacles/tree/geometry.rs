use super::super::super::*;

pub(in crate::presentation) const TREE_PRIMARY_GROUP_COUNT: u8 = 7;
pub(in crate::presentation) const TREE_SECONDARY_GROUP_STRIDE: u16 = 20;

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
    let trunk_length = 5.4_f32.lerp(9.2, canopy_competition);
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

    // Seven crown sectors fill a low, wide, irregular dome. Four are heavy,
    // load-bearing scaffold axes; the other three are subordinate crown-fill
    // limbs. This avoids both a radial whorl and an implausibly symmetrical
    // crown while retaining an even distribution of terminal foliage.
    let mut dominant_scaffolds: Vec<Vec<Vec3>> = Vec::with_capacity(4);
    for primary_index in 0..u64::from(TREE_PRIMARY_GROUP_COUNT) {
        let dominant = primary_index < 4;
        let rank = if dominant {
            primary_index as f32
        } else {
            (primary_index - 4) as f32
        };
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.42;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let (isolated_attach, competitive_attach) = if dominant {
            (
                [0.38, 0.56, 0.72, 0.86][primary_index as usize],
                [0.58, 0.72, 0.84, 0.93][primary_index as usize],
            )
        } else {
            (0.35 + rank * 0.17, 0.48 + rank * 0.12)
        };
        let attach = isolated_attach.lerp(competitive_attach.min(0.975), canopy_competition);
        let start = if dominant {
            sample_polyline(&trunk_points, attach)
        } else {
            sample_polyline(&dominant_scaffolds[rank as usize], attach)
        };
        // In the open, every principal axis contributes to the same broad,
        // ascending dome. Keeping separate vertical "leader" axes produces
        // a pollarded V silhouette instead of the decurrent habit of a mature
        // oak. Competition progressively shortens these lateral reaches and
        // raises their attachment points, leaving a tall clear bole.
        let (isolated_reach, isolated_lift, isolated_sag) = if dominant {
            // Lower axes carry the widest crown sectors while progressively
            // shorter upper axes close a deep, layered dome. Equal reaches
            // make the crown read as stacked shelves around a horizontal
            // beam, which is especially unconvincing in open-grown oaks.
            let reach_profile = [5.25, 4.95, 4.55, 3.8][primary_index as usize];
            let lift_profile = [1.35, 1.7, 1.55, 2.1][primary_index as usize];
            let sag_profile = [0.48, 0.38, 0.32, 0.24][primary_index as usize];
            (
                reach_profile + unit_hash(primary_seed ^ 1) * 0.28,
                lift_profile + unit_hash(primary_seed ^ 2) * 0.24,
                sag_profile,
            )
        } else {
            (
                2.7 - rank * 0.1 + unit_hash(primary_seed ^ 1) * 0.3,
                1.2 + rank * 0.2 + unit_hash(primary_seed ^ 2) * 0.18,
                0.28,
            )
        };
        let (competitive_reach, competitive_lift) = if dominant {
            (2.75 - rank * 0.14, 2.35 + rank * 0.15)
        } else {
            // High attachment already lifts these crown-fill axes. Keeping
            // their extension below the dominant scaffold tips rounds the
            // competitive crown instead of growing a false central spear.
            (2.1 - rank * 0.1, 2.15 + rank * 0.08)
        };
        let reach = isolated_reach.lerp(competitive_reach, canopy_competition);
        let lift = isolated_lift.lerp(competitive_lift, canopy_competition);
        let sag = isolated_sag.lerp(0.32, canopy_competition);
        let lateral = (unit_hash(primary_seed ^ 3) - 0.5) * 1.25;
        let torsion_phase = unit_hash(primary_seed ^ 0x71) * core::f32::consts::TAU;
        let primary_points = (0..=10)
            .map(|point_index| {
                let t = point_index as f32 / 10.0;
                let eased = t * (0.72 + 0.28 * t);
                start
                    + outward * reach * eased
                    + tangent * lateral * (core::f32::consts::PI * t).sin()
                    + tangent
                        * 0.22
                        * (core::f32::consts::TAU * t + torsion_phase).sin()
                        * (core::f32::consts::PI * t).sin()
                    + Vec3::Y
                        * (-sag * (core::f32::consts::PI * t).sin()
                            + lift * t.powf(1.85)
                            + 0.16
                                * (core::f32::consts::TAU * t + torsion_phase * 0.7).sin()
                                * (core::f32::consts::PI * t).sin())
            })
            .collect::<Vec<_>>();
        let (primary_start_radius, primary_end_radius) = if dominant {
            (
                0.36 - rank * 0.043 + unit_hash(primary_seed ^ 4) * 0.035,
                0.028,
            )
        } else {
            (0.17 + unit_hash(primary_seed ^ 4) * 0.035, 0.02)
        };
        append_branch_curve(
            &mut branches,
            &primary_points,
            primary_start_radius,
            primary_end_radius,
            1,
            primary_index as u8,
            u16::MAX,
        );
        if dominant {
            dominant_scaffolds.push(primary_points.clone());
        }

        let secondary_count = if dominant { 20_u64 } else { 12_u64 };
        for secondary_index in 0..secondary_count {
            let secondary_seed = splitmix64(primary_seed ^ (secondary_index + 0x51));
            // The last axis inherits the scaffold direction and carries
            // foliage over its end. The remaining axes alternate laterally,
            // avoiding the bare antler tips produced by stopping all child
            // branches well before their parent.
            let is_terminal_axis = secondary_index + 1 == secondary_count;
            let attach = if is_terminal_axis {
                0.96
            } else {
                let normalized = secondary_index as f32 / (secondary_count - 2) as f32;
                0.14_f32.lerp(0.24, canopy_competition) + normalized.powf(0.7) * 0.72
            };
            let secondary_start = sample_polyline(&primary_points, attach);
            let inherited = polyline_tangent(&primary_points, attach);
            let (frame_right, frame_up) = branch_frame(inherited);
            let branch_phase =
                secondary_index as f32 * 2.399_963_1 + unit_hash(secondary_seed ^ 0x31) * 0.55;
            let radial = frame_right * branch_phase.cos() + frame_up * branch_phase.sin();
            let inherited_weight = if is_terminal_axis { 0.78 } else { 0.38 };
            let lateral_weight = if is_terminal_axis { 0.2 } else { 0.68 };
            let mut secondary_direction = (inherited
                * (inherited_weight + unit_hash(secondary_seed) * 0.16)
                + radial * (lateral_weight + unit_hash(secondary_seed ^ 1) * 0.2)
                + outward * 0.2
                + Vec3::Y * (0.18 + unit_hash(secondary_seed ^ 2) * 0.34))
                .normalize();
            let maximum_rise = 0.62_f32.lerp(0.72, canopy_competition);
            if secondary_direction.y > maximum_rise {
                let horizontal = secondary_direction.xz().normalize_or_zero();
                secondary_direction =
                    Vec3::new(horizontal.x, maximum_rise, horizontal.y).normalize();
            }
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
            let secondary_group =
                (primary_index * u64::from(TREE_SECONDARY_GROUP_STRIDE) + secondary_index) as u16;
            let (secondary_start_radius, secondary_end_radius) = if dominant {
                (0.072, 0.013)
            } else {
                (0.05, 0.01)
            };
            append_branch_curve(
                &mut branches,
                &secondary_points,
                secondary_start_radius,
                secondary_end_radius,
                2,
                primary_index as u8,
                secondary_group,
            );

            // Short shoots are distributed around the secondary limb rather
            // than combed vertically.  Their terminal leaf rosettes overlap
            // into porous masses while leaving glimpses of the scaffold.
            // Open-grown sectors cover much more crown surface. Allocate
            // current-year shoots in proportion to that larger envelope;
            // competition progressively self-prunes the surplus shoots.
            let shoot_count = if dominant {
                40.0_f32.lerp(24.0, canopy_competition).round() as u64
            } else {
                32.0_f32.lerp(16.0, canopy_competition).round() as u64
            };
            for shoot_index in 0..shoot_count {
                let shoot_seed = splitmix64(secondary_seed ^ (shoot_index + 0xa3));
                let normalized = shoot_index as f32 / (shoot_count - 1) as f32;
                let first_attach = 0.08_f32.lerp(0.22, canopy_competition);
                let attach = first_attach + normalized.powf(0.72) * (0.99 - first_attach);
                let shoot_start = sample_polyline(&secondary_points, attach);
                let inherited = polyline_tangent(&secondary_points, attach);
                let (frame_right, frame_up) = branch_frame(inherited);
                let spiral = shoot_index as f32 * 2.399_963_1
                    + unit_hash(shoot_seed) * core::f32::consts::TAU;
                let radial = frame_right * spiral.cos() + frame_up * spiral.sin();
                let open_vertical = -0.2 + unit_hash(shoot_seed ^ 2) * 0.48;
                let competitive_vertical = 0.04 + unit_hash(shoot_seed ^ 2) * 0.24;
                let vertical = open_vertical.lerp(competitive_vertical, canopy_competition);
                let shoot_direction = (inherited * 0.46
                    + radial * (0.58 + unit_hash(shoot_seed ^ 1) * 0.24)
                    + Vec3::Y * vertical)
                    .normalize();
                let shoot_length = 0.24 + unit_hash(shoot_seed ^ 3) * 0.28;
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
            start_radius: start_radius.lerp(end_radius, t0.powf(0.64)),
            end_radius: start_radius.lerp(end_radius, t1.powf(0.64)),
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
    /// Rotation accumulated from the base to the tip of the blade. Real oak
    /// leaves rarely present as perfectly planar cards, even in still air.
    pub(in crate::presentation) torsion: f32,
}

pub(in crate::presentation) fn procedural_oak_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    canopy_competition: f32,
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    let _ = canopy_competition;
    let leaves_per_shoot = 16_u64;
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
        for leaf_index in 0..leaves_per_shoot {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Alternate leaves along each current-year shoot, then finish in
            // the tighter terminal flush characteristic of pedunculate oak.
            let along = if leaf_index < 3 {
                0.06 + leaf_index as f32 * 0.18
            } else {
                (0.62
                    + (leaf_index - 3) as f32 / (leaves_per_shoot - 4) as f32 * 0.36
                    + (unit_hash(leaf_seed ^ 12) - 0.5) * 0.008)
                    .clamp(0.61, 0.985)
            };
            let alternate = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let spiral = leaf_index as f32 * 2.399_963_1 + (unit_hash(leaf_seed ^ 2) - 0.5) * 0.65;
            let radial = (tangent * spiral.cos() + binormal * spiral.sin()).normalize();
            let leaf_up = (radial * (0.46 + unit_hash(leaf_seed ^ 3) * 0.24)
                + direction * (0.42 + unit_hash(leaf_seed ^ 8) * 0.18)
                + Vec3::Y * (0.08 + unit_hash(leaf_seed ^ 9) * 0.18))
                .normalize();
            let leaf_normal_candidate = direction.cross(radial)
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
            // Pedunculate-oak leaf stalks are only a few millimetres long.
            // Keeping this independent of blade size avoids the long-stalked,
            // bilateral-comb silhouette of other broadleaf genera.
            let petiole_length = 0.003 + unit_hash(leaf_seed ^ 4) * 0.004;
            let blade_base =
                petiole_start + (radial * 0.82 + leaf_up * 0.18).normalize() * petiole_length;
            let leaf_length = 0.1 + unit_hash(leaf_seed ^ 5) * 0.06;
            let leaf_width = 0.065 + unit_hash(leaf_seed ^ 6) * 0.04;
            let shell_exposure = ((shoot.end.xz().length() - 1.25) / 4.75).clamp(0.0, 1.0);
            let shade = if leaf_index < 3 {
                0.58 + shell_exposure * 0.22 + unit_hash(leaf_seed ^ 7) * 0.12
            } else {
                0.68 + shell_exposure * 0.25 + unit_hash(leaf_seed ^ 7) * 0.14
            };
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + leaf_up * leaf_length * 0.5,
                right: leaf_right.normalize(),
                up: leaf_up,
                length: leaf_length,
                width: leaf_width,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                shade,
                torsion: (unit_hash(leaf_seed ^ 13) - 0.5) * 0.42,
            });
        }
    }
    leaves
}

pub(in crate::presentation) fn oak_leaf_card_bounds(leaf: TreeLeaf) -> (Vec3, f32, f32) {
    let blade_tip = leaf.center + leaf.up * leaf.length * 0.5;
    let bottom = leaf.petiole_start.dot(leaf.up);
    let top = blade_tip.dot(leaf.up);
    let height = (top - bottom).max(leaf.length) * 1.04;
    let center = leaf.center + leaf.up * (((top + bottom) * 0.5) - leaf.center.dot(leaf.up));
    (center, leaf.width * 1.08, height)
}

fn leaf_shadow_selector(leaf: TreeLeaf) -> f32 {
    let shoot_key = u64::from(leaf.primary_group)
        | (u64::from(leaf.secondary_group) << 8)
        | (u64::from(leaf.shoot_id) << 24);
    unit_hash(splitmix64(shoot_key ^ 0x5a17_8c3d_2149_b6e0))
}

/// Replaces every cambered production leaf with one alpha-masked quad while
/// retaining its biological attachment, orientation, scale, and wind UV.
pub(in crate::presentation) fn procedural_oak_leaf_card_mesh(leaves: &[TreeLeaf]) -> Mesh {
    let mut positions = Vec::with_capacity(leaves.len() * 4);
    let mut normals = Vec::with_capacity(leaves.len() * 4);
    let mut uvs = Vec::with_capacity(leaves.len() * 4);
    let mut colors = Vec::with_capacity(leaves.len() * 4);
    let mut indices = Vec::with_capacity(leaves.len() * 6);
    for leaf in leaves {
        let (mut center, width, height) = oak_leaf_card_bounds(*leaf);
        // A single plane loses the accepted leaf's projected camber when seen
        // obliquely. Enlarge about the fixed petiole (not the card centre) so
        // the intermediate LOD preserves crown coverage without swimming at
        // its biological attachment.
        const COVERAGE_SCALE: f32 = 1.24;
        let scaled_width = width * COVERAGE_SCALE;
        let scaled_height = height * COVERAGE_SCALE;
        center += leaf.up * (scaled_height - height) * 0.5;
        let right = leaf.right * scaled_width * 0.5;
        let up = leaf.up * scaled_height * 0.5;
        let normal = leaf.right.cross(leaf.up).normalize();
        let base = positions.len() as u32;
        positions.extend_from_slice(&[
            (center - right - up).to_array(),
            (center + right - up).to_array(),
            (center + right + up).to_array(),
            (center - right + up).to_array(),
        ]);
        normals.extend_from_slice(&[normal.to_array(); 4]);
        // Image-space V grows downward. Keep the scanned petiole at the
        // biological attachment and the blade tip at the distal end.
        uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        let shade = (leaf.shade / 0.82).clamp(0.72, 1.18);
        let ambient_visibility = ((leaf.shade - 0.52) / 0.44).clamp(0.32, 1.0);
        let shadow_selector = leaf_shadow_selector(*leaf);
        colors.extend_from_slice(&[[shade, shade, shadow_selector, ambient_visibility]; 4]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
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

/// The near leaf is a small cambered grid: the geometry retains fold, cupping,
/// torsion, and grazing-angle area while the scanned opacity texture owns its
/// fine lobed silhouette. It transitions directly to the flat card.
pub(in crate::presentation) fn procedural_oak_textured_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
    let mut positions = Vec::with_capacity(leaves.len() * 9);
    let mut normals = Vec::with_capacity(leaves.len() * 9);
    let mut uvs = Vec::with_capacity(leaves.len() * 9);
    let mut colors = Vec::with_capacity(leaves.len() * 9);
    let mut indices = Vec::with_capacity(leaves.len() * 24);
    for leaf in leaves {
        let (mut center, width, height) = oak_leaf_card_bounds(*leaf);
        const COVERAGE_SCALE: f32 = 1.10;
        let scaled_width = width * COVERAGE_SCALE;
        let scaled_height = height * COVERAGE_SCALE;
        center += leaf.up * (scaled_height - height) * 0.5;
        let shade = (leaf.shade / 0.82).clamp(0.72, 1.18);
        let ambient_visibility = ((leaf.shade - 0.52) / 0.44).clamp(0.32, 1.0);
        let shadow_selector = leaf_shadow_selector(*leaf);
        let base = positions.len() as u32;
        let curl_sign = if leaf.torsion.is_sign_negative() {
            -1.0
        } else {
            1.0
        };
        for row in 0..3 {
            let v = row as f32 * 0.5;
            // Even an almost-untwisted source leaf needs enough geometric
            // change to justify this near representation over the flat card.
            // Accumulate a small asymmetric twist toward the tip while the
            // source torsion keeps every leaf from curling identically.
            let twist_angle = (leaf.torsion * 1.15 + curl_sign * 0.08) * (v - 0.20);
            let twist = Quat::from_axis_angle(leaf.up, twist_angle);
            let cross_right = (twist * leaf.right).normalize();
            let cross_normal = cross_right.cross(leaf.up).normalize();
            for column in 0..3 {
                let u = column as f32 * 0.5;
                let side = (u - 0.5) * 2.0;
                let lateral = side * scaled_width * 0.5;
                let length_profile = (core::f32::consts::PI * v).sin();
                let midrib_ridge = length_profile * (1.0 - side.abs()) * leaf.width * 0.08;
                let margin_cup = length_profile * side.abs() * leaf.width * 0.11;
                let tip_curl = v * v * v * scaled_height * 0.035 * curl_sign;
                positions.push(
                    (center
                        + leaf.up * (v - 0.5) * scaled_height
                        + cross_right * lateral
                        + cross_normal * (midrib_ridge - margin_cup + tip_curl))
                        .to_array(),
                );
                normals.push(
                    (cross_normal + cross_right * -side * 0.52 - leaf.up * v * curl_sign * 0.08)
                        .normalize()
                        .to_array(),
                );
                uvs.push([u, 1.0 - v]);
                colors.push([shade, shade, shadow_selector, ambient_visibility]);
            }
        }
        for row in 0..2_u32 {
            for column in 0..2_u32 {
                let lower_left = base + row * 3 + column;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 3;
                let upper_right = upper_left + 1;
                indices.extend_from_slice(&[
                    lower_left,
                    lower_right,
                    upper_right,
                    lower_left,
                    upper_right,
                    upper_left,
                ]);
            }
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

pub(in crate::presentation) fn procedural_oak_leaf_card_group_mesh(
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Mesh {
    let group_leaves = leaves
        .iter()
        .filter(|leaf| leaf.primary_group == primary_group)
        .copied()
        .collect::<Vec<_>>();
    procedural_oak_leaf_card_mesh(&group_leaves)
}

pub(in crate::presentation) fn procedural_oak_textured_leaf_group_mesh(
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Mesh {
    let group_leaves = leaves
        .iter()
        .filter(|leaf| leaf.primary_group == primary_group)
        .copied()
        .collect::<Vec<_>>();
    procedural_oak_textured_leaf_mesh(&group_leaves)
}

/// Models the compact, scaled winter bud at every current-year shoot tip.
/// It is a separate production mesh so its warm color and overlapping scale
/// silhouette remain legible instead of disappearing into the bark tube cap.
pub(in crate::presentation) fn procedural_oak_bud_mesh(branches: &[TreeBranchSegment]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    const SIDES: u32 = 6;
    const BUD_LENGTH: f32 = 0.008;
    for branch in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
    {
        let direction = (branch.end - branch.start).normalize();
        let (frame_right, frame_forward) = branch_frame(direction);
        let base = positions.len() as u32;
        let rings = [(0.0_f32, 0.0018_f32), (0.38, 0.0038), (0.78, 0.0025)];
        for (ring_index, (along, radius)) in rings.into_iter().enumerate() {
            let center = branch.end + direction * (along * BUD_LENGTH);
            let phase_offset = if ring_index & 1 == 0 { 0.0 } else { 0.22 };
            for side in 0..SIDES {
                let phase = side as f32 * core::f32::consts::TAU / SIDES as f32 + phase_offset;
                // Alternating ridges suggest overlapping protective scales at
                // the close-review distance without inflating the tiny bud.
                let scale_ridge = if (side + ring_index as u32) & 1 == 0 {
                    1.12
                } else {
                    0.94
                };
                let radial = frame_right * phase.cos() + frame_forward * phase.sin();
                positions.push((center + radial * radius * scale_ridge).to_array());
                normals.push((radial * 0.86 + direction * 0.18).normalize().to_array());
                uvs.push([side as f32 / SIDES as f32, along]);
            }
        }
        for ring in 0..rings.len() as u32 - 1 {
            let from = base + ring * SIDES;
            let to = from + SIDES;
            for side in 0..SIDES {
                let next = (side + 1) % SIDES;
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
        positions.push((branch.end + direction * BUD_LENGTH).to_array());
        normals.push(direction.to_array());
        uvs.push([0.5, 1.0]);
        let last_ring = base + (rings.len() as u32 - 1) * SIDES;
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            indices.extend_from_slice(&[tip, last_ring + side, last_ring + next]);
        }
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

pub(in crate::presentation) fn procedural_oak_bud_group_mesh(
    branches: &[TreeBranchSegment],
    primary_group: u8,
) -> Mesh {
    let group = branches
        .iter()
        .filter(|branch| branch.primary_group == primary_group)
        .copied()
        .collect::<Vec<_>>();
    procedural_oak_bud_mesh(&group)
}

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
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
            (8_u32.saturating_sub(u32::from(curve[0].depth))).max(4),
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
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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
    sides: u32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let first = curve[0];
    let last = curve[curve.len() - 1];
    let first_direction = (first.end - first.start).normalize();
    let last_direction = (last.end - last.start).normalize();
    let mut rings = Vec::with_capacity(curve.len() + 2);
    if first.depth > 0 {
        // One basal collar belongs at the biological attachment. Repeating a
        // collar at every polygon joint makes a smooth axis look assembled.
        rings.push((
            first.start - first_direction * first.start_radius * 0.22,
            first.start_radius * 1.18,
            first_direction,
        ));
        rings.push((
            first.start + first_direction * first.start_radius * 0.5,
            first.start_radius * 0.94,
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
        rings.push((branch.end, branch.end_radius, tangent));
    }
    let base = positions.len() as u32;
    for (ring, (center, radius, tangent)) in rings.iter().copied().enumerate() {
        let (right, forward) = branch_frame(tangent);
        for side in 0..sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([side as f32 / sides as f32, ring as f32]);
        }
    }
    for ring in 0..rings.len() as u32 - 1 {
        let from = base + ring * sides;
        let to = from + sides;
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
    let end_ring = base + (rings.len() as u32 - 1) * sides;
    if last.is_limb_tip {
        // A pair of shrinking rings gives every terminal axis a rounded,
        // natural taper. Flat caps read as sawn-off limbs and become black
        // rectangular artifacts in the descendant renders.
        let shoulder = positions.len() as u32;
        let bud_length = last.end_radius;
        let (right, forward) = branch_frame(last_direction);
        for (ring, (distance, radius_scale)) in [(0.55, 0.58), (0.92, 0.12)].into_iter().enumerate()
        {
            let center = last.end + last_direction * bud_length * distance;
            for side in 0..sides {
                let phase = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * phase.cos() + forward * phase.sin();
                let normal = (radial * 0.75 + last_direction * 0.66).normalize();
                positions.push((center + radial * last.end_radius * radius_scale).to_array());
                normals.push(normal.to_array());
                uvs.push([side as f32 / sides as f32, 1.0 + ring as f32 * 0.25]);
            }
        }
        for ring in 0..2_u32 {
            let from = if ring == 0 { end_ring } else { shoulder };
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
        positions.push((last.end + last_direction * bud_length).to_array());
        normals.push(last_direction.to_array());
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
        assert_eq!(counts, vec![22, 70, 348, 8_704]);
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
    fn production_oak_has_finite_cambered_leaf_geometry() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        assert_eq!(leaves.len(), 69_632);
        assert!(leaves.iter().all(|leaf| {
            leaf.petiole_start.is_finite()
                && leaf.center.is_finite()
                && leaf.right.is_finite()
                && leaf.up.is_finite()
                && leaf.right.length_squared() > 0.9
                && leaf.up.length_squared() > 0.9
                && leaf.right.cross(leaf.up).length_squared() > 0.5
                && leaf.torsion.is_finite()
        }));
        let mesh = procedural_oak_textured_leaf_mesh(&leaves);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("leaf mesh has float positions");
        assert_eq!(positions.len(), leaves.len() * 9);
        assert_eq!(
            mesh.indices().expect("leaf mesh has indices").len() / 3,
            leaves.len() * 8
        );
        assert!(
            positions
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        let buds = procedural_oak_bud_mesh(&branches);
        let bud_positions = buds
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("bud mesh has float positions");
        assert_eq!(bud_positions.len(), 4_352 * 19);
        assert_eq!(
            buds.indices().expect("bud mesh has indices").len() / 3,
            4_352 * 30
        );
        assert!(
            bud_positions
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        let leaf_triangles = mesh.indices().expect("leaf mesh has indices").len() / 3;
        let bud_triangles = buds.indices().expect("bud mesh has indices").len() / 3;
        let branch_mesh = procedural_tree_branch_mesh(&branches, 3);
        let branch_triangles = branch_mesh
            .indices()
            .expect("branch mesh has indices")
            .len()
            / 3;
        assert!(
            leaf_triangles + bud_triangles + branch_triangles <= 3_600_000,
            "LOD0 exceeds its 3.6M triangle budget"
        );
        let group_triangles = (0..TREE_PRIMARY_GROUP_COUNT)
            .map(|primary_group| {
                procedural_oak_textured_leaf_group_mesh(&leaves, primary_group)
                    .indices()
                    .expect("sector has indices")
                    .len()
                    / 3
            })
            .sum::<usize>();
        assert_eq!(group_triangles, leaf_triangles);
    }

    #[test]
    fn alpha_leaf_lod_uses_exactly_two_triangles_per_leaf() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mesh = procedural_oak_leaf_card_mesh(&leaves);
        assert_eq!(mesh.count_vertices(), leaves.len() * 4);
        assert_eq!(
            mesh.indices().expect("leaf cards are indexed").len(),
            leaves.len() * 6
        );
    }

    #[test]
    fn textured_leaf_lod_uses_exactly_eight_triangles_per_leaf() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mesh = procedural_oak_textured_leaf_mesh(&leaves);
        assert_eq!(mesh.count_vertices(), leaves.len() * 9);
        assert_eq!(
            mesh.indices().expect("textured leaves are indexed").len(),
            leaves.len() * 24
        );
    }

    #[test]
    fn leaf_shadow_transmission_is_stable_per_shoot_and_well_distributed() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mut shoots = std::collections::BTreeMap::new();
        for leaf in leaves {
            let key = (leaf.primary_group, leaf.secondary_group, leaf.shoot_id);
            let selector = leaf_shadow_selector(leaf);
            let previous = shoots.entry(key).or_insert(selector);
            assert_eq!(
                *previous, selector,
                "one shoot must not fragment its shadow"
            );
        }
        let transmitting = shoots.values().filter(|selector| **selector < 0.42).count();
        let fraction = transmitting as f32 / shoots.len() as f32;
        assert!(
            (0.37..=0.47).contains(&fraction),
            "expected roughly 42% transmitting shoots, got {fraction:.3}"
        );
    }

    #[test]
    fn every_live_axis_is_connected_and_terminates_in_descendants() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
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
