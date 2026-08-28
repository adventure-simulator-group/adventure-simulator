use adventuresim_tactical_core::prelude::TREE_TRUNK_HEIGHT_METRES;
use bevy::math::{FloatExt, Vec3, Vec3Swizzles};
use fabelgeist_determinism::splitmix64;

use crate::presentation::unit_hash;

use super::{
    ENGLISH_OAK_PARAMETERS, NATURAL_OAK_GNARLING, OakGnarlingParameters, TREE_PRIMARY_GROUP_COUNT,
    TREE_SECONDARY_GROUP_STRIDE, TreeBranchSegment, WoodyPlantForm, WoodyPlantParameters,
    branch_frame,
};

const OAK_ROOT_MIN_COUNT: usize = 4;
const OAK_ROOT_MAX_COUNT: usize = 5;
const OAK_ROOT_MAX_FORKS: usize = 1;
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
        WoodyPlantForm::MatureBeech => {
            procedural_beech_skeleton(seed, canopy_competition, parameters)
        }
        WoodyPlantForm::MultiStemShrub => procedural_multistem_shrub_skeleton(seed, parameters),
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

#[cfg(test)]
fn procedural_oak_root_specs(seed: u64, crown_phase: f32) -> Vec<OakRootSpec> {
    procedural_oak_root_specs_with_gnarling(seed, crown_phase, NATURAL_OAK_GNARLING)
}

fn procedural_oak_root_specs_with_gnarling(
    seed: u64,
    crown_phase: f32,
    gnarling: OakGnarlingParameters,
) -> Vec<OakRootSpec> {
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
            let reach = (if is_dominant {
                0.88 + unit_hash(root_seed ^ 0x01) * 0.16
            } else {
                0.55 + unit_hash(root_seed ^ 0x01) * 0.3
            }) * (1.0 + gnarling.root_spread.clamp(0.0, 1.0) * 0.45);
            let base_radius = if is_dominant {
                0.22 + unit_hash(root_seed ^ 0x02) * 0.05
            } else {
                0.14 + unit_hash(root_seed ^ 0x02) * 0.05
            };
            let fork_threshold = 0.82 - gnarling.root_forking.clamp(0.0, 1.0) * 0.72;
            let fork = (fork_count < OAK_ROOT_MAX_FORKS
                && unit_hash(root_seed ^ 0x06) > fork_threshold)
                .then(|| {
                    fork_count += 1;
                    OakRootFork {
                        attach: 0.54 + unit_hash(root_seed ^ 0x07) * 0.17,
                        angle_offset: if root_seed & 1 == 0 {
                            0.5 + unit_hash(root_seed ^ 0x08) * 0.35
                        } else {
                            -0.5 - unit_hash(root_seed ^ 0x08) * 0.35
                        },
                        reach: 0.16 + unit_hash(root_seed ^ 0x09) * 0.16,
                    }
                });
            OakRootSpec {
                angle: angle.rem_euclid(core::f32::consts::TAU),
                reach,
                base_radius,
                tip_radius: 0.008 + unit_hash(root_seed ^ 0x03) * 0.012,
                shoulder_lift: unit_hash(root_seed ^ 0x04) * 0.012,
                burial: 0.24 + unit_hash(root_seed ^ 0x05) * 0.12,
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

fn oak_root_points(
    trunk_base: Vec3,
    root: OakRootSpec,
    gnarling: OakGnarlingParameters,
) -> [Vec3; 3] {
    let outward = Vec3::new(root.angle.cos(), 0.0, root.angle.sin());
    let tangent = Vec3::new(-root.angle.sin(), 0.0, root.angle.cos());
    let contact_radius = 0.36 + (root.base_radius - 0.14) * 0.32;
    let meander = (gnarling.root_meander.clamp(0.0, 1.0) * root.reach * 0.32)
        * (root.angle * 2.7 + root.reach * 3.1).sin();
    let exposure = gnarling.root_exposure.clamp(0.0, 1.0);
    let contact =
        trunk_base + outward * contact_radius + tangent * (root.shoulder_lift - 0.0375) * 1.4;
    [
        contact,
        contact
            + outward * (0.19 + root.reach * 0.18)
            + tangent * ((root.tip_radius - 0.016) * 3.0 + meander)
            // Keep only the buttress shoulder visible. The continuation
            // descends immediately so a smooth root capsule cannot read as a
            // long toe laid on top of the soil.
            + Vec3::Y * (root.shoulder_lift - 0.16 + exposure * 0.03),
        trunk_base + outward * root.reach + tangent * meander * 0.42
            - Vec3::Y * root.burial * (1.0 - exposure * 0.3),
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

pub(in crate::presentation) fn procedural_oak_skeleton_with_gnarling(
    seed: u64,
    canopy_competition: f32,
    gnarling: OakGnarlingParameters,
) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let canopy_competition = canopy_competition.clamp(0.0, 1.0);
    let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
    let bend_direction = Vec3::new(crown_phase.cos(), 0.0, crown_phase.sin());

    // Quercus robur is usually short-boled in the open.  The trunk loses
    // dominance inside a broad crown instead of continuing as a conifer-like
    // central spear.
    let trunk_length = 5.4_f32.lerp(9.2, canopy_competition);
    let individual_bias = (unit_hash(seed ^ 0x4c45_414e) - 0.5) * 0.34;
    let lean_phase = gnarling.stress_azimuth_radians + individual_bias;
    let lean_direction = Vec3::new(lean_phase.cos(), 0.0, lean_phase.sin());
    let trunk_deformation =
        gnarling.trunk_lean + gnarling.trunk_sweep + gnarling.trunk_twist + gnarling.trunk_crooks;
    let trunk_steps = if trunk_deformation > 0.001 { 24 } else { 6 };
    let trunk_points = (0..=trunk_steps)
        .map(|index| {
            let t = index as f32 / trunk_steps as f32;
            let window = (core::f32::consts::PI * t).sin();
            let sweep = lean_direction * (gnarling.trunk_lean.clamp(0.0, 1.0) * 2.6 * t.powf(1.25));
            let crook = bend_direction
                * (gnarling.trunk_crooks.clamp(0.0, 1.0)
                    * 0.72
                    * (t * core::f32::consts::TAU * 2.4 + crown_phase).sin()
                    * window);
            let helical = Vec3::new(
                (crown_phase + t * core::f32::consts::TAU * 1.7).cos(),
                0.0,
                (crown_phase + t * core::f32::consts::TAU * 1.7).sin(),
            ) * (gnarling.trunk_twist.clamp(0.0, 1.0) * 0.48 * window);
            let lateral_sweep = Vec3::new(-lean_direction.z, 0.0, lean_direction.x)
                * (gnarling.trunk_sweep.clamp(0.0, 1.0)
                    * 0.8
                    * (t * core::f32::consts::PI).sin().powi(2));
            Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0)
                + Vec3::Y * (trunk_length * t)
                + bend_direction * (0.28 * t.powf(1.45))
                + sweep
                + crook
                + helical
                + lateral_sweep
        })
        .collect::<Vec<_>>();
    append_branch_curve(
        &mut branches,
        &trunk_points,
        0.72_f32.lerp(0.56, canopy_competition)
            * (1.0 + gnarling.taper_irregularity.clamp(0.0, 1.0) * 0.16),
        0.045_f32.lerp(0.035, canopy_competition),
        0,
        u8::MAX,
        u16::MAX,
    );

    // Unequal surface roots visually carry the weight of the low, spreading
    // scaffold limbs without changing the authoritative collider. Their plan
    // is presentation-only: a bounded irregular partition avoids a radial
    // star, while the broadest buttresses share the major scaffold azimuths.
    let root_specs = procedural_oak_root_specs_with_gnarling(seed, crown_phase, gnarling);
    debug_assert!(oak_root_segment_count(&root_specs) <= OAK_ROOT_MAX_SEGMENTS);
    for root in root_specs {
        // Root capsules begin just below grade. Their smooth union leaves an
        // irregular trunk flare, while every radial continuation and fork is
        // buried before it can terminate as a visible toe.
        let points = oak_root_points(trunk_points[0] - Vec3::Y * 0.07, root, gnarling);
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

    append_oak_knots(&mut branches, seed, &trunk_points, gnarling);

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
            let reach_profile = [5.55, 5.15, 4.65, 3.75][primary_index as usize];
            let lift_profile = [0.72, 1.0, 1.2, 1.55][primary_index as usize];
            let sag_profile = [0.62, 0.52, 0.4, 0.28][primary_index as usize];
            (
                reach_profile + unit_hash(primary_seed ^ 1) * 0.28,
                lift_profile + unit_hash(primary_seed ^ 2) * 0.24,
                sag_profile,
            )
        } else {
            (
                2.9 - rank * 0.14 + unit_hash(primary_seed ^ 1) * 0.28,
                0.78 + rank * 0.18 + unit_hash(primary_seed ^ 2) * 0.16,
                0.36,
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
        let asymmetry =
            1.0 + gnarling.crown_asymmetry.clamp(0.0, 1.0) * 0.38 * (phase - lean_phase).cos();
        let reach = isolated_reach.lerp(competitive_reach, canopy_competition) * asymmetry;
        let lift = isolated_lift.lerp(competitive_lift, canopy_competition);
        let sag = isolated_sag.lerp(0.32, canopy_competition);
        let lateral = (unit_hash(primary_seed ^ 3) - 0.5)
            * (1.25 + gnarling.scaffold_sweep.clamp(0.0, 1.0) * 2.8);
        let torsion_phase = unit_hash(primary_seed ^ 0x71) * core::f32::consts::TAU;
        let primary_points = (0..=10)
            .map(|point_index| {
                let t = point_index as f32 / 10.0;
                let eased = t * (0.72 + 0.28 * t);
                start
                    + outward * reach * eased
                    + tangent * lateral * (core::f32::consts::PI * t).sin()
                    + tangent
                        * (0.22 + gnarling.scaffold_contortion.clamp(0.0, 1.0) * 0.62)
                        * (core::f32::consts::TAU * t + torsion_phase).sin()
                        * (core::f32::consts::PI * t).sin()
                    + Vec3::Y
                        * (-sag * (core::f32::consts::PI * t).sin()
                            - gnarling.scaffold_droop.clamp(0.0, 1.0)
                                * reach
                                * 0.16
                                * (core::f32::consts::PI * t).sin().powi(2)
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

        // Organize the crown into readable scaffold masses rather than a
        // uniformly filled wire cage. Fewer secondary axes leave deliberate
        // windows and remove wood hidden behind several alpha-tested leaves.
        let secondary_count = if dominant { 18_u64 } else { 11_u64 };
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
                + Vec3::Y * (0.08 + unit_hash(secondary_seed ^ 2) * 0.26))
                .normalize();
            let maximum_rise = 0.5_f32.lerp(0.68, canopy_competition);
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
                34.0_f32.lerp(24.0, canopy_competition).round() as u64
            } else {
                22.0_f32.lerp(15.0, canopy_competition).round() as u64
            };
            for shoot_index in 0..shoot_count {
                let shoot_seed = splitmix64(secondary_seed ^ (shoot_index + 0xa3));
                let first_attach = 0.08_f32.lerp(0.22, canopy_competition);
                let attach =
                    oak_clustered_shoot_attach(shoot_index, shoot_count, first_attach, shoot_seed);
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

/// Distributes current-year shoots in five separated pulses along a secondary
/// axis. Stable pulses retain terminal coverage while leaving recognizable sky
/// and interior windows between foliage masses.
fn oak_clustered_shoot_attach(
    shoot_index: u64,
    shoot_count: u64,
    first_attach: f32,
    shoot_seed: u64,
) -> f32 {
    const CLUSTER_COUNT: u64 = 5;
    let cluster = (shoot_index * CLUSTER_COUNT / shoot_count.max(1)).min(CLUSTER_COUNT - 1);
    let cluster_start = cluster * shoot_count / CLUSTER_COUNT;
    let cluster_end = ((cluster + 1) * shoot_count / CLUSTER_COUNT).max(cluster_start + 1);
    let within = (shoot_index - cluster_start) as f32 / (cluster_end - cluster_start) as f32;
    let center = first_attach.lerp(0.965, cluster as f32 / (CLUSTER_COUNT - 1) as f32);
    let spread = 0.052_f32.lerp(0.082, cluster as f32 / (CLUSTER_COUNT - 1) as f32);
    (center + (within - 0.5) * spread + (unit_hash(shoot_seed ^ 0xc10d) - 0.5) * 0.018)
        .clamp(0.04, 0.992)
}

fn procedural_oak_skeleton(seed: u64, canopy_competition: f32) -> Vec<TreeBranchSegment> {
    procedural_oak_skeleton_with_gnarling(seed, canopy_competition, NATURAL_OAK_GNARLING)
}

fn append_oak_knots(
    branches: &mut Vec<TreeBranchSegment>,
    seed: u64,
    trunk: &[Vec3],
    gnarling: OakGnarlingParameters,
) {
    let count = (gnarling.knot_frequency.clamp(0.0, 1.0) * 6.0).round() as u64;
    for index in 0..count {
        let knot_seed = splitmix64(seed ^ 0x4b4e_4f54 ^ index);
        let attach = 0.1 + unit_hash(knot_seed) * 0.68;
        let center = sample_polyline(trunk, attach);
        let axis = polyline_tangent(trunk, attach);
        let (right, forward) = branch_frame(axis);
        let phase = unit_hash(knot_seed ^ 1) * core::f32::consts::TAU;
        let radial = right * phase.cos() + forward * phase.sin();
        let scale = 0.055
            + gnarling.knot_scale.clamp(0.0, 1.0) * 0.18
            + gnarling.burl_scale.clamp(0.0, 1.0) * unit_hash(knot_seed ^ 2) * 0.24;
        append_branch_curve(
            branches,
            &[
                center,
                center + radial * scale * 0.75,
                center + radial * scale + axis * scale * 0.18,
            ],
            scale,
            scale * 0.38,
            0,
            u8::MAX,
            u16::MAX,
        );
    }
}

fn procedural_beech_skeleton(
    seed: u64,
    canopy_competition: f32,
    parameters: WoodyPlantParameters,
) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let competition = canopy_competition.clamp(0.0, 1.0);
    let height = parameters.height_metres * (1.0 + competition * 0.13);
    let crown_radius = parameters.crown_radius_metres * (1.0 - competition * 0.18);
    let clear_bole_fraction = 0.38 + competition * 0.14;
    let crown_phase = unit_hash(seed ^ 0xbeec_0001) * core::f32::consts::TAU;
    let trunk_base = Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0);
    let trunk_points = (0..=10)
        .map(|index| {
            let t = index as f32 / 10.0;
            let sweep = (core::f32::consts::PI * t).sin() * (0.045 + 0.025 * competition);
            Vec3::new(
                crown_phase.cos() * sweep,
                trunk_base.y + height * t,
                crown_phase.sin() * sweep,
            )
        })
        .collect::<Vec<_>>();
    append_branch_curve(
        &mut branches,
        &trunk_points,
        0.5,
        0.055,
        0,
        u8::MAX,
        u16::MAX,
    );

    // Beech commonly shows a broad, shallow root plate rather than a set of
    // exposed radial cables. A few low shoulders taper below the soil within
    // a metre, softening the trunk-ground junction without creating oak-like
    // buttresses or changing the authoritative cylindrical collider.
    for root_index in 0..3_u64 {
        let root_seed = splitmix64(seed ^ 0xbeec_5000 ^ root_index);
        let phase =
            crown_phase + root_index as f32 * 2.399_963_1 + (unit_hash(root_seed) - 0.5) * 0.32;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let reach = 0.3 + unit_hash(root_seed ^ 1) * 0.16;
        let shoulder = trunk_base
            + outward * (0.3 + unit_hash(root_seed ^ 2) * 0.025)
            + Vec3::Y * (0.004 + unit_hash(root_seed ^ 3) * 0.008);
        let root_points = [
            shoulder,
            shoulder + outward * reach * 0.42 + tangent * (unit_hash(root_seed ^ 4) - 0.5) * 0.08
                - Vec3::Y * 0.11,
            trunk_base + outward * reach + tangent * (unit_hash(root_seed ^ 5) - 0.5) * 0.12
                - Vec3::Y * (0.25 + unit_hash(root_seed ^ 6) * 0.08),
        ];
        append_branch_curve(
            &mut branches,
            &root_points,
            0.11 + unit_hash(root_seed ^ 7) * 0.025,
            0.006 + unit_hash(root_seed ^ 8) * 0.004,
            0,
            u8::MAX,
            u16::MAX,
        );
    }

    for primary_index in 0..20_u64 {
        let primary_seed = splitmix64(seed ^ 0xbeec_1000 ^ primary_index);
        let layer = primary_index as f32 / 19.0;
        let height_fraction = (clear_bole_fraction
            + layer.powf(0.88) * (0.95 - clear_bole_fraction)
            + (unit_hash(primary_seed ^ 0x19) - 0.5) * 0.032)
            .clamp(clear_bole_fraction, 0.96);
        let start = sample_polyline(&trunk_points, height_fraction);
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.42;
        let radial = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        // A continuous ovate crown is widest below mid-crown and closes
        // steadily toward the leader. Unequal reach prevents shelf tiers from
        // reading as repeated horizontal plates.
        let crown_profile = (core::f32::consts::PI * (0.12 + layer * 0.8))
            .sin()
            .max(0.28);
        let reach = crown_radius * crown_profile * (0.78 + unit_hash(primary_seed ^ 1) * 0.36);
        let primary_points = (0..=5)
            .map(|index| {
                let t = index as f32 / 5.0;
                let arch = (core::f32::consts::PI * t).sin();
                start
                    + radial * reach * (t * (0.86 + 0.14 * t))
                    + Vec3::Y * (reach * (0.18 * arch + (0.5 + layer * 0.3) * t.powf(1.35)))
                    + tangent * (unit_hash(primary_seed ^ 2) - 0.5) * 0.42 * arch
            })
            .collect::<Vec<_>>();
        // LOD clusters are spatial crown sectors, not every seventh scaffold
        // in generation order. The latter mixed opposing azimuths into each
        // card and made a beech collapse into repeated full-height shelves at
        // aggregate distances.
        let primary_group = ((phase.rem_euclid(core::f32::consts::TAU) / core::f32::consts::TAU
            * f32::from(TREE_PRIMARY_GROUP_COUNT))
        .floor() as u8)
            .min(TREE_PRIMARY_GROUP_COUNT - 1);
        append_branch_curve(
            &mut branches,
            &primary_points,
            0.24 - layer * 0.095,
            0.018,
            1,
            primary_group,
            u16::MAX,
        );

        for secondary_index in 0..5_u64 {
            let secondary_seed = splitmix64(primary_seed ^ 0xbeec_2000 ^ secondary_index);
            let along = (0.12
                + secondary_index as f32 * 0.17
                + (unit_hash(secondary_seed ^ 0x17) - 0.5) * 0.055)
                .clamp(0.08, 0.84);
            let secondary_start = sample_polyline(&primary_points, along);
            let side = if secondary_index & 1 == 0 { 1.0 } else { -1.0 };
            let vertical_bias = 0.06 + unit_hash(secondary_seed ^ 1) * 0.42 + layer * 0.12;
            let direction = (radial * (0.38 + along * 0.25)
                + tangent * side * (0.62 + unit_hash(secondary_seed) * 0.22)
                + Vec3::Y * vertical_bias)
                .normalize();
            let length =
                crown_radius * crown_profile * (0.3 + unit_hash(secondary_seed ^ 2) * 0.15);
            let secondary_points = [
                secondary_start,
                secondary_start + direction * length * 0.5 + Vec3::Y * 0.04,
                secondary_start + direction * length + Vec3::Y * (0.16 + layer * 0.12),
            ];
            let secondary_group = (primary_index * 8 + secondary_index) as u16;
            append_branch_curve(
                &mut branches,
                &secondary_points,
                0.048,
                0.007,
                2,
                primary_group,
                secondary_group,
            );
            for twig_index in 0..6_u64 {
                let twig_seed = splitmix64(secondary_seed ^ 0xbeec_3000 ^ twig_index);
                let twig_start = sample_polyline(&secondary_points, 0.1 + twig_index as f32 * 0.15);
                let (right, up) = branch_frame(direction);
                let phase = twig_index as f32 * 2.399_963_1 + unit_hash(twig_seed);
                let twig_direction = (direction * 0.48
                    + right * phase.cos() * 0.52
                    + up * phase.sin() * 0.3
                    + Vec3::Y * (0.08 + layer * 0.08))
                    .normalize();
                let twig_length = 0.4 + unit_hash(twig_seed ^ 1) * 0.27;
                append_branch_curve(
                    &mut branches,
                    &[
                        twig_start,
                        twig_start + twig_direction * twig_length * 0.52,
                        twig_start + twig_direction * twig_length,
                    ],
                    0.008,
                    0.002,
                    3,
                    primary_group,
                    secondary_group,
                );
            }
        }
    }
    branches
}

fn procedural_multistem_shrub_skeleton(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::geometry::{
        BLACKTHORN_PARAMETERS, COMMON_BEECH_BARK, COMMON_BEECH_PARAMETERS,
        COMMON_HAWTHORN_PARAMETERS, COMMON_HAZEL_PARAMETERS, ENGLISH_OAK_PARAMETERS,
        procedural_oak_leaves, procedural_woody_plant_leaves, tree_crown_bounds,
    };
    use adventuresim_tactical_core::prelude::{TREE_TRUNK_HEIGHT_METRES, TREE_TRUNK_RADIUS_METRES};

    #[test]
    fn oak_shoot_pulses_leave_four_stable_canopy_windows() {
        let attaches = (0..24_u64)
            .map(|index| oak_clustered_shoot_attach(index, 24, 0.08, splitmix64(42 ^ index)))
            .collect::<Vec<_>>();
        assert!(
            attaches
                .iter()
                .all(|attach| (0.04..=0.992).contains(attach))
        );
        let mut ordered = attaches.clone();
        ordered.sort_by(f32::total_cmp);
        assert_eq!(
            ordered
                .windows(2)
                .filter(|pair| pair[1] - pair[0] > 0.1)
                .count(),
            4
        );
        let repeated = (0..24_u64)
            .map(|index| oak_clustered_shoot_attach(index, 24, 0.08, splitmix64(42 ^ index)))
            .collect::<Vec<_>>();
        assert_eq!(attaches, repeated);
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
            vec![6 + oak_root_segment_count(&roots), 70, 315, 6_348]
        );
        assert!(branches.iter().all(|branch| branch.start.is_finite()
            && branch.end.is_finite()
            && branch.start.distance(branch.end) > 0.0));
        assert!(
            branches
                .iter()
                .all(|branch| branch.start_radius > branch.end_radius && branch.end_radius > 0.0)
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
            assert!(roots.iter().all(|root| (0.55..=1.04).contains(&root.reach)
                && (0.14..=0.27).contains(&root.base_radius)
                && (0.008..=0.02).contains(&root.tip_radius)
                && (0.24..=0.36).contains(&root.burial)));
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
            assert!(smallest >= OAK_ROOT_MIN_ANGULAR_GAP - 0.0001);
            assert!(largest - smallest > 0.08);
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
                        .any(|root| signed_angular_delta(root.angle, load_phase).abs() < 0.48)
                );
            }
            assert!(
                dominant
                    .iter()
                    .all(|root| root.reach >= 0.88 && root.base_radius >= 0.22)
            );
            let span = |values: Vec<f32>| {
                let bounds = values
                    .into_iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                        (min.min(value), max.max(value))
                    });
                bounds.1 - bounds.0
            };
            assert!(span(roots.iter().map(|root| root.reach).collect()) > 0.12);
            assert!(span(roots.iter().map(|root| root.base_radius).collect()) > 0.035);
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
                .map(|root| oak_root_points(Vec3::ZERO, *root, NATURAL_OAK_GNARLING))
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
                assert!(points[0].length() >= 0.35);
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
    fn natural_oak_roots_have_no_above_grade_continuations() {
        for seed in 0..256 {
            let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
            for root in procedural_oak_root_specs(seed, crown_phase) {
                let points = oak_root_points(-Vec3::Y * 0.07, root, NATURAL_OAK_GNARLING);
                // The contact capsule may break grade as a short trunk flare;
                // both continuation controls and their conservative radii
                // remain below grade, so no radial toe can terminate visibly.
                assert!(points[1].y + root.base_radius * 0.65 < 0.0);
                assert!(points[2].y + root.tip_radius < 0.0);
                if let Some(fork) = root.fork {
                    let fork_points = oak_root_fork_points(&points, root, fork);
                    assert!(
                        fork_points
                            .iter()
                            .all(|point| point.y + root.base_radius * 0.34 < 0.0)
                    );
                }
            }
        }
    }

    #[test]
    fn visual_root_geometry_can_extend_beyond_the_authoritative_trunk_proxy() {
        let roots =
            procedural_oak_root_specs(42, unit_hash(42 ^ 0x9182_64ac) * core::f32::consts::TAU);
        assert!(
            roots
                .iter()
                .any(|root| root.reach > TREE_TRUNK_RADIUS_METRES)
        );
        assert_eq!(TREE_TRUNK_RADIUS_METRES, 0.35);
        assert_eq!(TREE_TRUNK_HEIGHT_METRES, 5.0);
    }

    #[test]
    fn default_tree_is_english_oak_and_hazel_is_bounded_multistem() {
        let default_oak = procedural_tree_skeleton(42, 0.4);
        let oak = procedural_woody_plant_skeleton(42, 0.4, ENGLISH_OAK_PARAMETERS);
        assert_eq!(default_oak.len(), oak.len());
        assert!(
            default_oak
                .iter()
                .zip(&oak)
                .all(|(left, right)| left.start == right.start
                    && left.end == right.end
                    && left.start_radius == right.start_radius
                    && left.end_radius == right.end_radius
                    && left.depth == right.depth
                    && left.primary_group == right.primary_group)
        );
        let hazel = procedural_woody_plant_skeleton(42, 0.0, COMMON_HAZEL_PARAMETERS);
        assert_eq!(
            hazel
                .iter()
                .filter(|branch| branch.depth == 0 && branch.start.length_squared() < 0.0001)
                .count(),
            usize::from(COMMON_HAZEL_PARAMETERS.basal_stems)
        );
        let bounds = tree_crown_bounds(&hazel, |_| true);
        assert!(bounds.vertical_span() > 1.8 && bounds.vertical_span() < 3.25);
        assert!(bounds.horizontal_span() > 1.5 && bounds.horizontal_span() < 3.6);
        let leaves = procedural_woody_plant_leaves(42, &hazel, 0.0, COMMON_HAZEL_PARAMETERS);
        assert!(!leaves.is_empty());
        assert!(
            leaves
                .iter()
                .all(|leaf| (0.08..=0.125).contains(&leaf.length)
                    && leaf.width / leaf.length > 0.7
                    && leaf.width / leaf.length < 0.86)
        );
    }

    #[test]
    fn central_german_shrub_presets_are_deterministic_and_morphologically_distinct() {
        let presets = [
            COMMON_HAZEL_PARAMETERS,
            BLACKTHORN_PARAMETERS,
            COMMON_HAWTHORN_PARAMETERS,
        ];
        let mut metrics = Vec::new();
        for parameters in presets {
            let first = procedural_woody_plant_skeleton(91, 0.0, parameters);
            let repeated = procedural_woody_plant_skeleton(91, 0.0, parameters);
            assert_eq!(first.len(), repeated.len());
            assert!(
                first
                    .iter()
                    .zip(&repeated)
                    .all(|(a, b)| a.start == b.start && a.end == b.end)
            );
            let bounds = tree_crown_bounds(&first, |_| true);
            let leaves = procedural_woody_plant_leaves(91, &first, 0.0, parameters);
            assert!(leaves.iter().all(|leaf| {
                (parameters.leaf_length_metres[0]..=parameters.leaf_length_metres[1])
                    .contains(&leaf.length)
                    && (parameters.leaf_width_ratio[0]..=parameters.leaf_width_ratio[1])
                        .contains(&(leaf.width / leaf.length))
            }));
            metrics.push((
                bounds.vertical_span(),
                bounds.horizontal_span(),
                leaves[0].length,
            ));
        }
        assert!(metrics[1].0 < metrics[0].0 && metrics[0].0 < metrics[2].0);
        assert!(metrics[1].2 < metrics[2].2 && metrics[2].2 < metrics[0].2);
    }

    #[test]
    fn common_beech_has_a_straight_clear_bole_smooth_bark_and_ovate_leaves() {
        let branches = procedural_woody_plant_skeleton(91, 0.65, COMMON_BEECH_PARAMETERS);
        let repeated = procedural_woody_plant_skeleton(91, 0.65, COMMON_BEECH_PARAMETERS);
        assert!(
            branches
                .iter()
                .zip(&repeated)
                .all(|(a, b)| a.start == b.start
                    && a.end == b.end
                    && a.start_radius == b.start_radius
                    && a.end_radius == b.end_radius)
        );
        let trunk = branches
            .iter()
            .filter(|branch| branch.depth == 0 && (branch.end - branch.start).normalize().y > 0.9)
            .collect::<Vec<_>>();
        assert_eq!(trunk.len(), 10);
        assert!(
            (trunk[0].start.y + TREE_TRUNK_HEIGHT_METRES * 0.5).abs() < f32::EPSILON,
            "the beech mesh root must share the collider-centred tree origin"
        );
        assert!(
            trunk
                .iter()
                .all(|segment| segment.end.xz().length() < 0.075)
        );
        let basal_roots = branches
            .iter()
            .filter(|branch| {
                branch.depth == 0
                    && branch.end.y < -TREE_TRUNK_HEIGHT_METRES * 0.5
                    && branch.end_radius < 0.03
            })
            .collect::<Vec<_>>();
        assert_eq!(basal_roots.len(), 3);
        assert!(basal_roots.iter().all(|root| root.end.xz().length() < 0.55));
        let crown_base = branches
            .iter()
            .filter(|branch| branch.depth == 1)
            .map(|branch| branch.start.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            crown_base + TREE_TRUNK_HEIGHT_METRES * 0.5
                > COMMON_BEECH_PARAMETERS.height_metres * 0.4
        );
        const {
            assert!(COMMON_BEECH_BARK.fissure_depth_metres < 0.0005);
        }
        assert_eq!(COMMON_BEECH_BARK.root_lobe_height_metres, 0.003);
        let branch_counts = (0..=3)
            .map(|depth| {
                branches
                    .iter()
                    .filter(|branch| branch.depth == depth)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(branch_counts, vec![16, 100, 200, 1_200]);
        assert!((0..TREE_PRIMARY_GROUP_COUNT).all(|group| {
            branches
                .iter()
                .any(|branch| branch.depth == 1 && branch.primary_group == group)
        }));
        assert!(
            branches
                .iter()
                .filter(|branch| (branch.depth == 1 || branch.depth == 2) && branch.is_limb_tip)
                .all(|branch| branch.end.y > branch.start.y)
        );
        let leaves = procedural_woody_plant_leaves(91, &branches, 0.65, COMMON_BEECH_PARAMETERS);
        assert_eq!(leaves.len(), 7_200);
        assert!(leaves.iter().all(|leaf| {
            (0.165..=0.301).contains(&leaf.length)
                && (0.48..=0.66).contains(&(leaf.width / leaf.length))
        }));
        let horizontal_laminae = leaves
            .iter()
            .filter(|leaf| leaf.right.cross(leaf.up).normalize().dot(Vec3::Y).abs() > 0.72)
            .count();
        assert!(horizontal_laminae * 5 > leaves.len() * 2);
        let crown = tree_crown_bounds(&branches, |branch| branch.depth > 0);
        // A closed-stand beech crown should remain a coherent dome, but need
        // not be taller than its maximum lateral scaffold span.
        assert!(crown.vertical_span() > crown.horizontal_span() * 0.65);
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
            assert!(
                branches
                    .iter()
                    .enumerate()
                    .any(|(other_index, other)| index != other_index
                        && other.depth <= branch.depth
                        && point_segment_distance(branch.start, other.start, other.end) < 0.002)
            );
        }
        for branch in branches
            .iter()
            .filter(|branch| branch.is_limb_tip && branch.depth > 0)
        {
            if branch.depth < 3 {
                assert!(branches.iter().any(|child| child.depth > branch.depth
                    && point_segment_distance(child.start, branch.start, branch.end) < 0.002
                    && child.start.distance(branch.end) < 0.55));
            } else {
                assert!(
                    leaves
                        .iter()
                        .any(|leaf| leaf.secondary_group == branch.secondary_group
                            && leaf.center.distance(branch.end) < 0.55)
                );
            }
        }
    }

    #[test]
    fn canopy_competition_raises_the_clear_bole_and_narrows_the_crown() {
        let isolated = procedural_tree_skeleton(42, 0.0);
        let competitive = procedural_tree_skeleton(42, 1.0);
        let isolated_crown = tree_crown_bounds(&isolated, |branch| branch.depth > 0);
        let competitive_crown = tree_crown_bounds(&competitive, |branch| branch.depth > 0);
        let first = |branches: &[TreeBranchSegment]| {
            branches
                .iter()
                .filter(|branch| branch.depth == 1)
                .map(|branch| branch.start.y)
                .fold(f32::INFINITY, f32::min)
        };
        assert!(first(&competitive) > first(&isolated) + 3.0);
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

    #[test]
    fn gnarling_recipe_is_deterministic_bounded_and_changes_every_growth_system() {
        let baseline = procedural_oak_skeleton_with_gnarling(42, 0.0, NATURAL_OAK_GNARLING);
        let extreme =
            procedural_oak_skeleton_with_gnarling(42, 0.0, super::super::EXTREME_OAK_GNARLING);
        let repeated =
            procedural_oak_skeleton_with_gnarling(42, 0.0, super::super::EXTREME_OAK_GNARLING);
        assert_eq!(extreme.len(), repeated.len());
        assert!(
            extreme
                .iter()
                .zip(&repeated)
                .all(|(left, right)| left.start == right.start
                    && left.end == right.end
                    && left.start_radius == right.start_radius
                    && left.end_radius == right.end_radius)
        );
        assert!(extreme.len() > baseline.len());
        let root_span = |branches: &[TreeBranchSegment]| {
            branches
                .iter()
                .filter(|branch| branch.depth == 0 && branch.end.y < -2.0)
                .map(|branch| branch.end.xz().length())
                .fold(0.0_f32, f32::max)
        };
        assert!(root_span(&extreme) > root_span(&baseline) * 1.3);
        let trunk_horizontal_span = |branches: &[TreeBranchSegment]| {
            branches
                .iter()
                .filter(|branch| branch.depth == 0 && branch.start.y > -2.1)
                .map(|branch| branch.end.xz().length())
                .fold(0.0_f32, f32::max)
        };
        assert!(trunk_horizontal_span(&extreme) > trunk_horizontal_span(&baseline) + 0.7);
        assert!(extreme.iter().all(|branch| branch.start.is_finite()
            && branch.end.is_finite()
            && branch.start_radius.is_finite()
            && branch.end_radius.is_finite()
            && branch.start_radius > branch.end_radius
            && branch.end_radius > 0.0));
    }

    fn point_segment_distance(point: Vec3, start: Vec3, end: Vec3) -> f32 {
        let segment = end - start;
        let along = ((point - start).dot(segment) / segment.length_squared()).clamp(0.0, 1.0);
        point.distance(start + segment * along)
    }
}
