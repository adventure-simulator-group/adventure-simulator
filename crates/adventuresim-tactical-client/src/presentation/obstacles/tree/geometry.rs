use super::super::super::*;

pub(in crate::presentation) const TREE_PRIMARY_GROUP_COUNT: u8 = 7;
pub(in crate::presentation) const TREE_SECONDARY_GROUP_STRIDE: u16 = 20;

const OAK_ROOT_MIN_COUNT: usize = 5;
const OAK_ROOT_MAX_COUNT: usize = 10;
const OAK_ROOT_MAX_FORKS: usize = 2;
const OAK_ROOT_MAX_SEGMENTS: usize = OAK_ROOT_MAX_COUNT * 2 + OAK_ROOT_MAX_FORKS;
const OAK_ROOT_MIN_ANGULAR_GAP: f32 = 0.22;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OakRootFork {
    attach: f32,
    angle_offset: f32,
    reach: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OakRootSpec {
    angle: f32,
    reach: f32,
    base_radius: f32,
    tip_radius: f32,
    shoulder_lift: f32,
    burial: f32,
    dominant: bool,
    fork: Option<OakRootFork>,
}

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
    procedural_woody_plant_skeleton(seed, canopy_competition, ENGLISH_OAK_PARAMETERS)
}

pub(in crate::presentation) fn procedural_woody_plant_skeleton(
    seed: u64,
    canopy_competition: f32,
    parameters: WoodyPlantParameters,
) -> Vec<TreeBranchSegment> {
    match parameters.form {
        WoodyPlantForm::MatureOak => procedural_oak_skeleton(seed, canopy_competition),
        WoodyPlantForm::CommonHazel => procedural_hazel_skeleton(seed, parameters),
    }
}

fn oak_primary_scaffold_phase(crown_phase: f32, primary_index: u64, primary_seed: u64) -> f32 {
    crown_phase + primary_index as f32 * 2.399_963_1 + (unit_hash(primary_seed) - 0.5) * 0.42
}

fn signed_angular_delta(from: f32, to: f32) -> f32 {
    let mut delta = (to - from) % core::f32::consts::TAU;
    if delta > core::f32::consts::PI {
        delta -= core::f32::consts::TAU;
    } else if delta < -core::f32::consts::PI {
        delta += core::f32::consts::TAU;
    }
    delta
}

fn procedural_oak_root_specs(seed: u64, crown_phase: f32) -> Vec<OakRootSpec> {
    let plan_seed = splitmix64(seed ^ 0x4f41_4b52_4f4f_5453);
    let root_count = OAK_ROOT_MIN_COUNT
        + (splitmix64(plan_seed ^ 0x01) as usize % (OAK_ROOT_MAX_COUNT - OAK_ROOT_MIN_COUNT + 1));
    let dominant_count = 2 + (splitmix64(plan_seed ^ 0x02) as usize & 1);

    // Allocate the full circle as unequal positive gaps. Normalizing the
    // weights keeps complete coverage without returning to equal radial rays.
    let gap_weights = (0..root_count)
        .map(|index| 0.62 + unit_hash(splitmix64(plan_seed ^ 0x100 ^ index as u64)) * 0.82)
        .collect::<Vec<_>>();
    let gap_total = gap_weights.iter().sum::<f32>();
    let rotation = crown_phase + unit_hash(plan_seed ^ 0x200) * 0.74;
    let mut cursor = rotation;
    let mut angles = Vec::with_capacity(root_count);
    for gap in gap_weights {
        angles.push(cursor);
        cursor += gap / gap_total * core::f32::consts::TAU;
    }

    // Pull distinct nearby roots toward the heaviest scaffold axes. This is a
    // restrained azimuthal bias, not a one-root-per-branch radial layout.
    let mut dominant = vec![false; root_count];
    for primary_index in 0..dominant_count as u64 {
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let load_phase = oak_primary_scaffold_phase(crown_phase, primary_index, primary_seed);
        let nearest = angles
            .iter()
            .enumerate()
            .filter(|(index, _)| !dominant[*index])
            .min_by(|(_, left), (_, right)| {
                signed_angular_delta(**left, load_phase)
                    .abs()
                    .total_cmp(&signed_angular_delta(**right, load_phase).abs())
            })
            .map(|(index, _)| index)
            .expect("an oak root plan always has more roots than dominant scaffolds");
        let previous = if nearest == 0 {
            angles[root_count - 1] - core::f32::consts::TAU
        } else {
            angles[nearest - 1]
        };
        let next = if nearest + 1 == root_count {
            angles[0] + core::f32::consts::TAU
        } else {
            angles[nearest + 1]
        };
        let desired = angles[nearest] + signed_angular_delta(angles[nearest], load_phase) * 0.68;
        angles[nearest] = desired.clamp(
            previous + OAK_ROOT_MIN_ANGULAR_GAP,
            next - OAK_ROOT_MIN_ANGULAR_GAP,
        );
        dominant[nearest] = true;
    }

    let mut fork_count = 0;
    let mut roots = angles
        .into_iter()
        .enumerate()
        .map(|(index, angle)| {
            let root_seed = splitmix64(plan_seed ^ 0x300 ^ index as u64);
            let is_dominant = dominant[index];
            let reach = if is_dominant {
                1.18 + unit_hash(root_seed ^ 0x01) * 0.3
            } else {
                0.82 + unit_hash(root_seed ^ 0x01) * 0.4
            };
            let base_radius = if is_dominant {
                0.34 + unit_hash(root_seed ^ 0x02) * 0.08
            } else {
                0.25 + unit_hash(root_seed ^ 0x02) * 0.075
            };
            let fork = (fork_count < OAK_ROOT_MAX_FORKS && unit_hash(root_seed ^ 0x06) > 0.72)
                .then(|| {
                    fork_count += 1;
                    OakRootFork {
                        attach: 0.54 + unit_hash(root_seed ^ 0x07) * 0.17,
                        angle_offset: if root_seed & 1 == 0 {
                            0.5 + unit_hash(root_seed ^ 0x08) * 0.35
                        } else {
                            -0.5 - unit_hash(root_seed ^ 0x08) * 0.35
                        },
                        reach: 0.24 + unit_hash(root_seed ^ 0x09) * 0.23,
                    }
                });
            OakRootSpec {
                angle: angle.rem_euclid(core::f32::consts::TAU),
                reach,
                base_radius,
                tip_radius: 0.04 + unit_hash(root_seed ^ 0x03) * 0.025,
                shoulder_lift: 0.015 + unit_hash(root_seed ^ 0x04) * 0.045,
                burial: 0.11 + unit_hash(root_seed ^ 0x05) * 0.09,
                dominant: is_dominant,
                fork,
            }
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| left.angle.total_cmp(&right.angle));
    roots
}

fn oak_root_segment_count(roots: &[OakRootSpec]) -> usize {
    roots.len() * 2 + roots.iter().filter(|root| root.fork.is_some()).count()
}

fn curve_radius_at(start_radius: f32, end_radius: f32, t: f32) -> f32 {
    start_radius.lerp(end_radius, t.clamp(0.0, 1.0).powf(0.64))
}

fn child_base_radius(authored: f32, parent_radius: f32) -> f32 {
    authored.min(parent_radius * 0.8)
}

fn oak_root_points(trunk_base: Vec3, root: OakRootSpec) -> [Vec3; 3] {
    let outward = Vec3::new(root.angle.cos(), 0.0, root.angle.sin());
    let tangent = Vec3::new(-root.angle.sin(), 0.0, root.angle.cos());
    let contact_radius = 0.48 + (root.base_radius - 0.25) * 0.35;
    let contact =
        trunk_base + outward * contact_radius + tangent * (root.shoulder_lift - 0.0375) * 1.4;
    [
        contact,
        contact
            + outward * (0.19 + root.reach * 0.18)
            + tangent * (root.tip_radius - 0.0525) * 2.2
            + Vec3::Y * root.shoulder_lift,
        trunk_base + outward * root.reach - Vec3::Y * root.burial,
    ]
}

fn oak_root_fork_points(parent: &[Vec3; 3], root: OakRootSpec, fork: OakRootFork) -> [Vec3; 2] {
    let fork_start = sample_polyline(parent, fork.attach);
    let parent_tangent = polyline_tangent(parent, fork.attach);
    let horizontal = Vec3::new(parent_tangent.x, 0.0, parent_tangent.z).normalize();
    let lateral = Vec3::new(-horizontal.z, 0.0, horizontal.x);
    let fork_direction =
        (horizontal * 0.76 + lateral * fork.angle_offset.signum() * 0.42).normalize();
    [
        fork_start,
        fork_start + fork_direction * fork.reach - Vec3::Y * root.burial * 0.55,
    ]
}

fn procedural_oak_skeleton(seed: u64, canopy_competition: f32) -> Vec<TreeBranchSegment> {
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

    // Unequal surface roots visually carry the weight of the low, spreading
    // scaffold limbs without changing the authoritative collider. Their plan
    // is presentation-only: a bounded irregular partition avoids a radial
    // star, while the broadest buttresses share the major scaffold azimuths.
    let root_specs = procedural_oak_root_specs(seed, crown_phase);
    debug_assert!(oak_root_segment_count(&root_specs) <= OAK_ROOT_MAX_SEGMENTS);
    for root in root_specs {
        let points = oak_root_points(trunk_points[0] + Vec3::Y * 0.09, root);
        append_branch_curve(
            &mut branches,
            &points,
            root.base_radius,
            root.tip_radius,
            0,
            u8::MAX,
            u16::MAX,
        );
        if let Some(fork) = root.fork {
            let fork_points = oak_root_fork_points(&points, root, fork);
            append_branch_curve(
                &mut branches,
                &fork_points,
                root.base_radius * 0.34,
                root.tip_radius * 0.72,
                0,
                u8::MAX,
                u16::MAX,
            );
        }
    }

    // Seven crown sectors fill a low, wide, irregular dome. Four are heavy,
    // load-bearing scaffold axes; the other three are subordinate crown-fill
    // limbs. This avoids both a radial whorl and an implausibly symmetrical
    // crown while retaining an even distribution of terminal foliage.
    let mut dominant_scaffolds: Vec<(Vec<Vec3>, f32, f32)> = Vec::with_capacity(4);
    for primary_index in 0..u64::from(TREE_PRIMARY_GROUP_COUNT) {
        let dominant = primary_index < 4;
        let rank = if dominant {
            primary_index as f32
        } else {
            (primary_index - 4) as f32
        };
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = oak_primary_scaffold_phase(crown_phase, primary_index, primary_seed);
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
            sample_polyline(&dominant_scaffolds[rank as usize].0, attach)
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
        let (authored_primary_start_radius, primary_end_radius) = if dominant {
            (
                0.36 - rank * 0.043 + unit_hash(primary_seed ^ 4) * 0.035,
                0.028,
            )
        } else {
            (0.17 + unit_hash(primary_seed ^ 4) * 0.035, 0.02)
        };
        let parent_radius = if dominant {
            curve_radius_at(
                0.72_f32.lerp(0.56, canopy_competition),
                0.045_f32.lerp(0.035, canopy_competition),
                attach,
            )
        } else {
            let parent = &dominant_scaffolds[rank as usize];
            curve_radius_at(parent.1, parent.2, attach)
        };
        let primary_start_radius = child_base_radius(authored_primary_start_radius, parent_radius);
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
            dominant_scaffolds.push((
                primary_points.clone(),
                primary_start_radius,
                primary_end_radius,
            ));
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
            let (authored_secondary_start_radius, secondary_end_radius) = if dominant {
                (0.072, 0.013)
            } else {
                (0.05, 0.01)
            };
            let secondary_start_radius = child_base_radius(
                authored_secondary_start_radius,
                curve_radius_at(primary_start_radius, primary_end_radius, attach),
            );
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
                    child_base_radius(
                        0.012,
                        curve_radius_at(secondary_start_radius, secondary_end_radius, attach),
                    ),
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

fn procedural_hazel_skeleton(
    seed: u64,
    parameters: WoodyPlantParameters,
) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let stem_count = u64::from(parameters.basal_stems.max(3));
    for stem_index in 0..stem_count {
        let stem_seed = splitmix64(seed ^ 0xc0a1_0000 ^ stem_index);
        let phase = stem_index as f32 * 2.399_963_1 + unit_hash(stem_seed) * 0.55;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let height = parameters.height_metres * (0.76 + unit_hash(stem_seed ^ 1) * 0.24);
        let lean = parameters.crown_radius_metres * (0.34 + unit_hash(stem_seed ^ 2) * 0.32);
        let stem_points = (0..=6)
            .map(|point_index| {
                let t = point_index as f32 / 6.0;
                Vec3::Y * height * t
                    + outward * lean * t.powf(1.35)
                    + tangent * 0.11 * (core::f32::consts::PI * t).sin()
            })
            .collect::<Vec<_>>();
        append_branch_curve(
            &mut branches,
            &stem_points,
            0.035 + unit_hash(stem_seed ^ 3) * 0.018,
            0.008,
            0,
            stem_index as u8,
            u16::MAX,
        );
        for branch_index in 0..10_u64 {
            let branch_seed = splitmix64(stem_seed ^ 0x51a7 ^ branch_index);
            let attach = 0.2 + branch_index as f32 / 10.0 * 0.76;
            let start = sample_polyline(&stem_points, attach);
            let inherited = polyline_tangent(&stem_points, attach);
            let spiral = phase + branch_index as f32 * 2.399_963_1;
            let radial = Vec3::new(spiral.cos(), 0.0, spiral.sin());
            let direction =
                (inherited * 0.26 + radial * 0.88 + Vec3::Y * (0.3 - attach * 0.17)).normalize();
            let length = parameters.crown_radius_metres
                * (0.42 + unit_hash(branch_seed) * 0.34)
                * (1.0 - attach * 0.22);
            let branch_points = [
                start,
                start + direction * length * 0.5 + Vec3::Y * 0.08,
                start + direction * length + Vec3::Y * 0.16,
            ];
            let secondary_group = (stem_index * 16 + branch_index) as u16;
            let branch_start_radius = child_base_radius(
                0.014,
                curve_radius_at(0.035 + unit_hash(stem_seed ^ 3) * 0.018, 0.008, attach),
            );
            append_branch_curve(
                &mut branches,
                &branch_points,
                branch_start_radius,
                0.0036,
                2,
                stem_index as u8,
                secondary_group,
            );
            for shoot_index in 0..5_u64 {
                let shoot_seed = splitmix64(branch_seed ^ 0xa3 ^ shoot_index);
                let along = 0.18 + shoot_index as f32 * 0.19;
                let shoot_start = sample_polyline(&branch_points, along);
                let (right, up) = branch_frame(direction);
                let shoot_phase = shoot_index as f32 * 2.399_963_1 + unit_hash(shoot_seed);
                let shoot_direction = (direction * 0.34
                    + right * shoot_phase.cos() * 0.62
                    + up * shoot_phase.sin() * 0.42
                    + Vec3::Y * 0.22)
                    .normalize();
                let shoot_length = 0.18 + unit_hash(shoot_seed ^ 1) * 0.14;
                append_branch_curve(
                    &mut branches,
                    &[
                        shoot_start,
                        shoot_start + shoot_direction * shoot_length * 0.52,
                        shoot_start + shoot_direction * shoot_length,
                    ],
                    child_base_radius(0.005, curve_radius_at(branch_start_radius, 0.0036, along)),
                    0.0015,
                    3,
                    stem_index as u8,
                    secondary_group,
                );
            }
        }
        // The stem itself ends in a short leafy flush. Without this terminal
        // continuation the shared shrub reads as a bundle of pruned rods and
        // develops an artificial flat top.
        let stem_tip = *stem_points.last().unwrap();
        let stem_direction = polyline_tangent(&stem_points, 1.0);
        let (stem_right, stem_up) = branch_frame(stem_direction);
        for tip_index in 0..5_u64 {
            let tip_seed = splitmix64(stem_seed ^ 0x7e21 ^ tip_index);
            let phase = tip_index as f32 * 2.399_963_1 + unit_hash(tip_seed) * 0.4;
            let direction = (stem_direction * 0.5
                + stem_right * phase.cos() * 0.58
                + stem_up * phase.sin() * 0.42)
                .normalize();
            let length = 0.16 + unit_hash(tip_seed ^ 1) * 0.12;
            append_branch_curve(
                &mut branches,
                &[
                    stem_tip,
                    stem_tip + direction * length * 0.5,
                    stem_tip + direction * length,
                ],
                0.0045,
                0.0014,
                3,
                stem_index as u8,
                (stem_index * 16 + 15) as u16,
            );
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

pub(in crate::presentation) fn procedural_woody_plant_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    canopy_competition: f32,
    parameters: WoodyPlantParameters,
) -> Vec<TreeLeaf> {
    match parameters.form {
        WoodyPlantForm::MatureOak => procedural_oak_leaves(seed, branches, canopy_competition),
        WoodyPlantForm::CommonHazel => procedural_hazel_leaves(seed, branches, parameters),
    }
}

fn procedural_hazel_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    parameters: WoodyPlantParameters,
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    let leaves_per_shoot = u64::from(parameters.leaves_per_shoot.max(4));
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let (frame_right, frame_up) = branch_frame(direction);
        for leaf_index in 0..leaves_per_shoot {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Common hazel leaves are alternate and loosely distichous. The
            // golden-angle perturbation prevents a flat bilateral comb while
            // retaining opposite-side succession along each current shoot.
            let along = 0.08
                + leaf_index as f32 / (leaves_per_shoot - 1) as f32 * 0.84
                + (unit_hash(leaf_seed ^ 1) - 0.5) * 0.025;
            let side = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let phase = side * (0.82 + unit_hash(leaf_seed ^ 2) * 0.28) + leaf_index as f32 * 0.32;
            let radial = (frame_right * phase.cos() + frame_up * phase.sin()).normalize();
            let leaf_up = (radial * 0.72 + direction * 0.48 + Vec3::Y * 0.16).normalize();
            let azimuth_normal = direction.cross(radial).normalize_or_zero();
            let azimuth_normal = if azimuth_normal.length_squared() > 0.25 {
                azimuth_normal
            } else {
                frame_up
            };
            // Hazel blades are generally held obliquely upward rather than as
            // vertical fins around an upright shoot. Bias the generated plane
            // normal toward the sky while retaining azimuthal variation, then
            // project it perpendicular to the blade's midrib. This lets the
            // ordinary PBR response to the sun light the shrub naturally.
            let posture_normal = (Vec3::Y * 0.82 + azimuth_normal * 0.38).normalize();
            let right = leaf_up.cross(posture_normal).normalize_or_zero();
            let right = if right.length_squared() > 0.25 {
                right
            } else {
                leaf_up.cross(frame_right).normalize()
            };
            let petiole_start = shoot.start.lerp(shoot.end, along.clamp(0.04, 0.96));
            let petiole_length = 0.012 + unit_hash(leaf_seed ^ 3) * 0.011;
            let length = 0.082 + unit_hash(leaf_seed ^ 4) * 0.038;
            let width = length * (0.72 + unit_hash(leaf_seed ^ 5) * 0.12);
            let blade_base = petiole_start + radial * petiole_length;
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + leaf_up * length * 0.5,
                right,
                up: leaf_up,
                length,
                width,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                shade: 0.68 + unit_hash(leaf_seed ^ 6) * 0.24,
                torsion: (unit_hash(leaf_seed ^ 7) - 0.5) * 0.28,
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
    procedural_woody_leaf_card_mesh(leaves)
}

pub(in crate::presentation) fn procedural_woody_leaf_card_mesh(leaves: &[TreeLeaf]) -> Mesh {
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
    procedural_woody_cambered_leaf_mesh(leaves)
}

pub(in crate::presentation) fn procedural_woody_cambered_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
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
    procedural_woody_leaf_card_mesh(&group_leaves)
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
    procedural_woody_cambered_leaf_mesh(&group_leaves)
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
    mesh.generate_tangents()
        .expect("procedural woody axes have valid metric UVs and normals");
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
    const BARK_TEXTURE_WIDTH_METRES: f32 = 1.0;
    const BARK_TEXTURE_HEIGHT_METRES: f32 = 2.0;

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
        for side in 0..=sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([
                side as f32 / sides as f32 * circumference_tiles,
                accumulated_distance / BARK_TEXTURE_HEIGHT_METRES,
            ]);
        }
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
    if last.is_limb_tip {
        // A pair of shrinking rings gives every terminal axis a rounded,
        // natural taper. Flat caps read as sawn-off limbs and become black
        // rectangular artifacts in the descendant renders.
        let shoulder = positions.len() as u32;
        let bud_length = last.end_radius;
        let (right, forward) = transport_branch_frame(previous_tangent, right, last_direction);
        let mut terminal_distance = accumulated_distance;
        let mut terminal_center = last.end;
        for (distance, radius_scale) in [(0.55, 0.58), (0.92, 0.12)] {
            let center = last.end + last_direction * bud_length * distance;
            terminal_distance += center.distance(terminal_center);
            let radius = last.end_radius * radius_scale;
            for side in 0..=sides {
                let phase = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * phase.cos() + forward * phase.sin();
                let normal = (radial * 0.75 + last_direction * 0.66).normalize();
                positions.push((center + radial * radius).to_array());
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

    #[test]
    fn branch_mesh_has_metric_seam_safe_uvs_and_valid_tangents() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let mesh = procedural_tree_branch_mesh(&branches, 0);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("branch mesh has float UVs");
        };
        let Some(VertexAttributeValues::Float32x4(tangents)) =
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("normal-mapped branch mesh has float tangents");
        };
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("branch mesh has float positions");

        assert_eq!(uvs.len(), tangents.len());
        assert!(uvs.iter().flatten().all(|component| component.is_finite()));
        assert!(
            tangents
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        assert!(tangents.iter().all(|tangent| {
            let length = Vec3::from_array([tangent[0], tangent[1], tangent[2]]).length();
            (length - 1.0).abs() < 1.0e-3 && tangent[3].abs() == 1.0
        }));
        assert!(Vec3::from_array(positions[0]).distance(Vec3::from_array(positions[8])) < 1.0e-5);
        assert_eq!(uvs[0][0], 0.0);
        assert!(uvs[8][0] >= 1.0 && uvs[8][0].fract().abs() < f32::EPSILON);
        assert!(
            uvs.iter().map(|uv| uv[1]).fold(0.0_f32, f32::max) > 0.5,
            "a two-metre-long axis must advance a full physical bark tile"
        );
    }

    #[test]
    fn procedural_tree_has_a_deterministic_four_order_branch_hierarchy() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let crown_phase = unit_hash(42 ^ 0x9182_64ac) * core::f32::consts::TAU;
        let roots = procedural_oak_root_specs(42, crown_phase);
        let counts = (0..=3)
            .map(|depth| {
                branches
                    .iter()
                    .filter(|branch| branch.depth == depth)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            counts,
            vec![6 + oak_root_segment_count(&roots), 70, 348, 8_704]
        );
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
    fn oak_root_plan_is_deterministic_bounded_and_irregular() {
        for seed in 0..4_096 {
            let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
            let roots = procedural_oak_root_specs(seed, crown_phase);
            assert_eq!(roots, procedural_oak_root_specs(seed, crown_phase));
            assert!((OAK_ROOT_MIN_COUNT..=OAK_ROOT_MAX_COUNT).contains(&roots.len()));
            assert!(oak_root_segment_count(&roots) <= OAK_ROOT_MAX_SEGMENTS);
            assert!(roots.iter().filter(|root| root.fork.is_some()).count() <= OAK_ROOT_MAX_FORKS);
            assert!((2..=3).contains(&roots.iter().filter(|root| root.dominant).count()));
            assert!(roots.iter().all(|root| {
                (0.82..=1.48).contains(&root.reach)
                    && (0.25..=0.42).contains(&root.base_radius)
                    && (0.04..=0.065).contains(&root.tip_radius)
                    && (0.11..=0.2).contains(&root.burial)
            }));

            let mut angles = roots
                .iter()
                .map(|root| root.angle.rem_euclid(core::f32::consts::TAU))
                .collect::<Vec<_>>();
            angles.sort_by(f32::total_cmp);
            let gaps = (0..angles.len())
                .map(|index| {
                    let next = if index + 1 == angles.len() {
                        angles[0] + core::f32::consts::TAU
                    } else {
                        angles[index + 1]
                    };
                    next - angles[index]
                })
                .collect::<Vec<_>>();
            let smallest = gaps.iter().copied().fold(f32::INFINITY, f32::min);
            let largest = gaps.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                smallest >= OAK_ROOT_MIN_ANGULAR_GAP - 0.0001,
                "seed {seed} has colliding root azimuths: {smallest}"
            );
            assert!(
                largest - smallest > 0.08,
                "seed {seed} has uniform root gaps"
            );
        }
    }

    #[test]
    fn dominant_buttresses_follow_major_scaffold_loads_and_vary_in_scale() {
        for seed in [7, 42, 91, 4_096] {
            let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
            let roots = procedural_oak_root_specs(seed, crown_phase);
            let dominant = roots
                .iter()
                .filter(|root| root.dominant)
                .collect::<Vec<_>>();
            for primary_index in 0..dominant.len() as u64 {
                let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
                let load_phase =
                    oak_primary_scaffold_phase(crown_phase, primary_index, primary_seed);
                assert!(
                    dominant
                        .iter()
                        .any(|root| { signed_angular_delta(root.angle, load_phase).abs() < 0.48 })
                );
            }
            assert!(dominant.iter().all(|root| root.reach >= 1.18));
            assert!(dominant.iter().all(|root| root.base_radius >= 0.34));
            let reach_span = roots.iter().map(|root| root.reach).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
            let radius_span = roots.iter().map(|root| root.base_radius).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
            assert!(reach_span.1 - reach_span.0 > 0.18);
            assert!(radius_span.1 - radius_span.0 > 0.06);
        }
    }

    #[test]
    fn root_forks_attach_to_parent_curves_and_contacts_circle_the_trunk() {
        let mut observed_forks = 0;
        for seed in 0..256 {
            let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
            let roots = procedural_oak_root_specs(seed, crown_phase);
            let root_points = roots
                .iter()
                .map(|root| oak_root_points(Vec3::ZERO, *root))
                .collect::<Vec<_>>();
            for (root, points) in roots.iter().zip(&root_points) {
                if let Some(fork) = root.fork {
                    observed_forks += 1;
                    let fork_points = oak_root_fork_points(points, *root, fork);
                    assert!(points.windows(2).any(|segment| point_segment_distance(
                        fork_points[0],
                        segment[0],
                        segment[1]
                    ) < 0.00001));
                }
            }
            for (index, points) in root_points.iter().enumerate() {
                assert!(points[0].length() >= 0.47);
                assert!(
                    root_points[index + 1..]
                        .iter()
                        .all(|other| points[0].distance(other[0]) > 0.025)
                );
            }
        }
        assert!(observed_forks > 0);
    }

    #[test]
    fn visual_root_geometry_can_extend_beyond_the_authoritative_trunk_proxy() {
        let crown_phase = unit_hash(42 ^ 0x9182_64ac) * core::f32::consts::TAU;
        let roots = procedural_oak_root_specs(42, crown_phase);
        assert!(
            roots
                .iter()
                .any(|root| root.reach > TREE_TRUNK_RADIUS_METRES)
        );
        // These constants are imported from tactical-core and are consumed by
        // server/viewer obstacle spawning. Root specs contain only visual mesh
        // dimensions and cannot replace that collider descriptor.
        assert_eq!(TREE_TRUNK_RADIUS_METRES, 0.35);
        assert_eq!(TREE_TRUNK_HEIGHT_METRES, 5.0);
    }

    #[test]
    fn woody_plant_parameters_preserve_oak_and_generate_bounded_multistem_hazel() {
        let legacy_oak = procedural_tree_skeleton(42, 0.4);
        let parameterized_oak = procedural_woody_plant_skeleton(42, 0.4, ENGLISH_OAK_PARAMETERS);
        assert_eq!(legacy_oak.len(), parameterized_oak.len());
        assert!(
            legacy_oak
                .iter()
                .zip(&parameterized_oak)
                .all(|(left, right)| {
                    left.start == right.start
                        && left.end == right.end
                        && left.start_radius == right.start_radius
                        && left.end_radius == right.end_radius
                        && left.depth == right.depth
                        && left.primary_group == right.primary_group
                })
        );

        let hazel = procedural_woody_plant_skeleton(42, 0.0, COMMON_HAZEL_PARAMETERS);
        let basal_stems = hazel
            .iter()
            .filter(|branch| branch.depth == 0 && branch.start.length_squared() < 0.0001)
            .count();
        let bounds = tree_crown_bounds(&hazel, |_| true);
        assert_eq!(
            basal_stems,
            usize::from(COMMON_HAZEL_PARAMETERS.basal_stems)
        );
        assert!(bounds.vertical_span() > 1.8 && bounds.vertical_span() < 3.25);
        assert!(bounds.horizontal_span() > 1.5 && bounds.horizontal_span() < 3.6);
        let leaves = procedural_woody_plant_leaves(42, &hazel, 0.0, COMMON_HAZEL_PARAMETERS);
        assert!(!leaves.is_empty());
        assert!(leaves.iter().all(|leaf| {
            (0.08..=0.125).contains(&leaf.length)
                && leaf.width / leaf.length > 0.7
                && leaf.width / leaf.length < 0.86
        }));
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
