use super::*;

pub(super) const OAK_BARK_TILE_METRES: f32 = 0.5;
const OAK_BARK_HEIGHT_RANGE_METRES: f32 = 0.032;
#[cfg(test)]
pub(super) const OAK_BARK_COLUMNS: i32 = 10;
#[cfg(test)]
pub(super) const OAK_BARK_ROWS: i32 = 6;
#[cfg(test)]
pub(super) const OAK_BARK_FISSURE_WIDTH_MIN: f32 = 0.007;
#[cfg(test)]
pub(super) const OAK_BARK_FISSURE_WIDTH_SPAN: f32 = 0.003;
#[cfg(test)]
pub(super) const OAK_BARK_VALLEY_WIDTH_MIN: f32 = 0.014;

#[cfg(test)]
pub(super) fn bark_random(cell_x: i32, cell_y: i32, salt: u64) -> f32 {
    let hash = splitmix64(bark_cell_id(cell_x, cell_y) | salt.rotate_left(21));
    unit_hash(hash)
}

#[cfg(test)]
fn bark_cell_id(cell_x: i32, cell_y: i32) -> u64 {
    let wrapped_x = cell_x.rem_euclid(OAK_BARK_COLUMNS) as u64;
    let wrapped_y = cell_y.rem_euclid(OAK_BARK_ROWS) as u64;
    wrapped_x | (wrapped_y << 8)
}

#[cfg(test)]
fn bark_edge_random(first: (i32, i32), second: (i32, i32), salt: u64) -> f32 {
    let first = bark_cell_id(first.0, first.1);
    let second = bark_cell_id(second.0, second.1);
    let (lower, upper) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    let hash = splitmix64(lower | (upper << 16) | salt.rotate_left(37));
    unit_hash(hash)
}

pub(super) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
pub(super) fn bark_segment_modulation(point: Vec2, first: (i32, i32), second: (i32, i32)) -> f32 {
    let tau = core::f32::consts::TAU;
    let frequency = 2.0 + (bark_edge_random(first, second, 0x8d31) * 3.0).floor();
    let phase = bark_edge_random(first, second, 0xc4b7);
    let meander = 0.13 * (tau * (point.x * 3.0 + bark_edge_random(first, second, 0x724d))).sin();
    let wave = (tau * (point.y * frequency + phase + meander)).sin();
    0.48 + 0.52 * smoothstep(-0.55, 0.30, wave)
}

#[cfg(test)]
pub(super) fn oak_bark_major_profile(
    edge_distance: f32,
    core_width: f32,
    valley_width: f32,
    run_strength: f32,
    crown_height: f32,
    shoulder_height: f32,
) -> f32 {
    let core = (-0.5 * (edge_distance / core_width).powi(2)).exp();
    let valley = (-0.5 * (edge_distance / valley_width).powi(2)).exp();
    let shoulder_distance = (edge_distance - valley_width * 1.15) / (valley_width * 0.38);
    let shoulder = (-0.5 * shoulder_distance.powi(2)).exp();
    crown_height + shoulder_height * run_strength * shoulder
        - 0.24 * run_strength * valley
        - (0.035 + 0.24 * run_strength) * core
}

#[cfg(test)]
pub(super) fn oak_bark_crack_x(crack: i32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let phase = bark_random(crack, 0, 0xd32f);
    let secondary_phase = bark_random(crack, 0, 0x82b5);
    let offset = (bark_random(crack, 0, 0x4c19) - 0.5) * 0.16 / OAK_BARK_COLUMNS as f32;
    crack as f32 / OAK_BARK_COLUMNS as f32
        + offset
        + 0.0065 * (tau * (v * 2.0 + phase)).sin()
        + 0.0028 * (tau * (v * 5.0 + secondary_phase)).sin()
}

#[cfg(test)]
pub(super) fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let axis = end - start;
    let along = ((point - start).dot(axis) / axis.length_squared().max(1.0e-6)).clamp(0.0, 1.0);
    point.distance(start + axis * along)
}

const OAK_BARK_PLATE_COUNT: i32 = 38;

fn oak_bark_plate_random(index: i32, salt: u64) -> f32 {
    unit_hash(splitmix64(index as u64 | salt.rotate_left(29)))
}

#[derive(Clone, Copy)]
struct OakBarkPlateSite {
    id: u64,
    position: Vec2,
}

fn oak_bark_plate_site(index: i32) -> OakBarkPlateSite {
    let wrapped_index = index.rem_euclid(OAK_BARK_PLATE_COUNT);
    let x = (0.5
        + wrapped_index as f32 * 0.618_034
        + (oak_bark_plate_random(wrapped_index, 0x4c19) - 0.5) * 0.012)
        .fract();
    let y = (0.5
        + wrapped_index as f32 * 0.754_878
        + (oak_bark_plate_random(wrapped_index, 0x9bd7) - 0.5) * 0.012)
        .fract();
    OakBarkPlateSite {
        id: wrapped_index as u64,
        position: Vec2::new(x, y),
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct OakBarkPlateField {
    sites: [OakBarkPlateSite; 3],
    offsets: [Vec2; 3],
    distances: [f32; 3],
}

fn oak_bark_toroidal_offset(point: Vec2, site: Vec2) -> Vec2 {
    Vec2::new(
        (point.x - site.x + 0.5).rem_euclid(1.0) - 0.5,
        (point.y - site.y + 0.5).rem_euclid(1.0) - 0.5,
    )
}

#[cfg(test)]
fn oak_bark_plate_field(point: Vec2) -> OakBarkPlateField {
    let empty = OakBarkPlateSite {
        id: 0,
        position: Vec2::ZERO,
    };
    let mut field = OakBarkPlateField {
        sites: [empty; 3],
        offsets: [Vec2::ZERO; 3],
        distances: [f32::INFINITY; 3],
    };
    for index in 0..OAK_BARK_PLATE_COUNT {
        let site = oak_bark_plate_site(index);
        let offset = oak_bark_toroidal_offset(point, site.position);
        let distance = (offset.x.powi(2) + (offset.y * 0.46).powi(2)).sqrt();
        if distance < field.distances[0] {
            field.distances[2] = field.distances[1];
            field.sites[2] = field.sites[1];
            field.offsets[2] = field.offsets[1];
            field.distances[1] = field.distances[0];
            field.sites[1] = field.sites[0];
            field.offsets[1] = field.offsets[0];
            field.distances[0] = distance;
            field.sites[0] = site;
            field.offsets[0] = offset;
        } else if distance < field.distances[1] {
            field.distances[2] = field.distances[1];
            field.sites[2] = field.sites[1];
            field.offsets[2] = field.offsets[1];
            field.distances[1] = distance;
            field.sites[1] = site;
            field.offsets[1] = offset;
        } else if distance < field.distances[2] {
            field.distances[2] = distance;
            field.sites[2] = site;
            field.offsets[2] = offset;
        }
    }
    field
}

fn oak_bark_site_value(id: u64, salt: u64) -> f32 {
    unit_hash(splitmix64(id | salt.rotate_left(31)))
}

const OAK_BARK_CHECK_COUNT: i32 = 17;
const OAK_BARK_FIBER_COUNT: i32 = 11;
#[cfg(test)]
const OAK_BARK_MAJOR_FURROWS: i32 = 4;
#[cfg(test)]
#[allow(dead_code)]
const OAK_BARK_BRANCH_COUNT: i32 = 9;
#[cfg(test)]
#[allow(dead_code)]
const OAK_BARK_HANDOFF_COUNT: i32 = 2;

#[cfg(test)]
fn oak_bark_major_furrow_x(furrow: i32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let wrapped = furrow.rem_euclid(OAK_BARK_MAJOR_FURROWS);
    let cycle = furrow.div_euclid(OAK_BARK_MAJOR_FURROWS) as f32;
    let irregular_positions = [0.0, 0.16, 0.48, 0.77];
    cycle
        + irregular_positions[wrapped as usize]
        + (oak_bark_plate_random(wrapped, 0xd32f) - 0.5) * 0.055
        + (0.010 + 0.011 * oak_bark_plate_random(wrapped, 0x82b5))
            * (tau * (v * 2.0 + oak_bark_plate_random(wrapped, 0x4c19))).sin()
        + (0.002 + 0.005 * oak_bark_plate_random(wrapped, 0x73d9))
            * (tau * (v * 5.0 + oak_bark_plate_random(wrapped, 0xa7c1))).sin()
}

#[cfg(test)]
fn oak_bark_major_distance(point: Vec2) -> (f32, i32) {
    let approximate = (point.x * OAK_BARK_MAJOR_FURROWS as f32).round() as i32;
    let mut best = (f32::INFINITY, approximate);
    for furrow in (approximate - 2)..=(approximate + 2) {
        let distance = (point.x - oak_bark_major_furrow_x(furrow, point.y)).abs();
        if distance < best.0 {
            best = (distance, furrow);
        }
    }
    best
}

#[derive(Clone, Copy)]
struct OakBarkCurve {
    start: Vec2,
    control: Vec2,
    end: Vec2,
}

#[derive(Clone, Copy)]
struct OakBarkGraphEdge {
    start: Vec2,
    end: Vec2,
    bend: f32,
    width: f32,
    depth: f32,
}

const OAK_BARK_GRAPH_EDGES: [OakBarkGraphEdge; 20] = [
    graph_edge(0.05, 0.00, 0.12, 0.18, -0.018, 0.0074, 0.86),
    graph_edge(0.43, 0.00, 0.35, 0.18, 0.021, 0.0074, 0.86),
    graph_edge(0.78, 0.00, 0.88, 0.18, -0.024, 0.0074, 0.86),
    graph_edge(0.24, 0.05, 0.29, 0.23, 0.018, 0.0074, 0.86),
    graph_edge(0.12, 0.18, 0.20, 0.38, 0.025, 0.0074, 0.86),
    graph_edge(0.35, 0.18, 0.52, 0.38, -0.031, 0.0074, 0.86),
    graph_edge(0.88, 0.18, 0.76, 0.38, 0.028, 0.0074, 0.86),
    graph_edge(0.29, 0.23, 0.20, 0.38, -0.020, 0.0074, 0.86),
    graph_edge(0.20, 0.38, 0.08, 0.58, -0.026, 0.0074, 0.86),
    graph_edge(0.52, 0.38, 0.45, 0.58, 0.023, 0.0074, 0.86),
    graph_edge(0.52, 0.38, 0.66, 0.56, -0.029, 0.0074, 0.86),
    graph_edge(0.76, 0.38, 0.84, 0.58, 0.021, 0.0074, 0.86),
    graph_edge(0.08, 0.58, 0.17, 0.78, 0.024, 0.0074, 0.86),
    graph_edge(0.45, 0.58, 0.31, 0.78, -0.030, 0.0074, 0.86),
    graph_edge(0.84, 0.58, 0.72, 0.78, 0.027, 0.0074, 0.86),
    graph_edge(0.55, 0.62, 0.72, 0.78, -0.025, 0.0074, 0.86),
    graph_edge(0.17, 0.78, 0.05, 1.00, -0.028, 0.0074, 0.86),
    graph_edge(0.31, 0.78, 0.43, 1.00, 0.030, 0.0074, 0.86),
    graph_edge(0.72, 0.78, 0.78, 1.00, -0.022, 0.0074, 0.86),
    graph_edge(0.92, 0.72, 0.90, 0.92, 0.019, 0.0074, 0.86),
];

const fn graph_edge(
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
    bend: f32,
    width: f32,
    depth: f32,
) -> OakBarkGraphEdge {
    OakBarkGraphEdge {
        start: Vec2::new(start_x, start_y),
        end: Vec2::new(end_x, end_y),
        bend,
        width,
        depth,
    }
}

fn oak_bark_graph_curve(edge: OakBarkGraphEdge, vertical_copy: i32, point_x: f32) -> OakBarkCurve {
    let vertical = Vec2::Y * vertical_copy as f32;
    let start = edge.start + vertical;
    let end = edge.end + vertical;
    let horizontal_copy = (point_x - (start.x + end.x) * 0.5).round();
    let copy = Vec2::X * horizontal_copy;
    OakBarkCurve {
        start: start + copy,
        control: (start + end) * 0.5 + Vec2::X * (edge.bend * 1.9) + copy,
        end: end + copy,
    }
}

fn oak_bark_graph_distance(point: Vec2) -> (f32, f32, f32) {
    let mut best = (f32::INFINITY, 0.007, 0.8);
    for (edge_index, edge) in OAK_BARK_GRAPH_EDGES.iter().copied().enumerate() {
        for vertical_copy in -1..=1 {
            let curve = oak_bark_graph_curve(edge, vertical_copy, point.x);
            let (distance, along) = oak_bark_curve_distance(point, curve);
            let amplitude =
                0.12 + 0.42 * unit_hash(splitmix64(edge_index as u64 | 0x94d3_0000_0000_0000));
            let mut width_scale = 1.0 + amplitude * (core::f32::consts::PI * along).sin().powi(2);
            let mut terminal_envelope = 1.0;
            if matches!(edge_index, 3 | 15 | 19) {
                terminal_envelope *= smoothstep(0.0, 0.18, along);
            }
            if matches!(edge_index, 10 | 19) {
                terminal_envelope *= 1.0 - smoothstep(0.78, 1.0, along);
            }
            width_scale *= 0.35 + 0.65 * terminal_envelope;
            let local_width = edge.width * width_scale.max(0.08);
            let normalized = distance / local_width;
            if normalized < best.0 {
                best = (normalized, local_width, edge.depth * terminal_envelope);
            }
        }
    }
    best
}

fn oak_bark_curve_distance(point: Vec2, curve: OakBarkCurve) -> (f32, f32) {
    let mut best = (f32::INFINITY, 0.0);
    let mut previous = curve.start;
    for segment in 1..=8 {
        let end_t = segment as f32 / 8.0;
        let inverse = 1.0 - end_t;
        let next = inverse.powi(2) * curve.start
            + 2.0 * inverse * end_t * curve.control
            + end_t.powi(2) * curve.end;
        let axis = next - previous;
        let local =
            ((point - previous).dot(axis) / axis.length_squared().max(1.0e-6)).clamp(0.0, 1.0);
        let distance = point.distance(previous + axis * local);
        if distance < best.0 {
            best = (distance, (segment as f32 - 1.0 + local) / 8.0);
        }
        previous = next;
    }
    best
}

fn oak_bark_check_curve(index: i32, vertical_copy: i32, point_x: f32) -> OakBarkCurve {
    let edge_index =
        (oak_bark_plate_random(index, 0x51d7) * OAK_BARK_GRAPH_EDGES.len() as f32).floor() as usize;
    let graph_curve = oak_bark_graph_curve(
        OAK_BARK_GRAPH_EDGES[edge_index.min(OAK_BARK_GRAPH_EDGES.len() - 1)],
        vertical_copy,
        point_x,
    );
    let graph_t = 0.18 + 0.64 * oak_bark_plate_random(index, 0xc927);
    let inverse = 1.0 - graph_t;
    let start = inverse.powi(2) * graph_curve.start
        + 2.0 * inverse * graph_t * graph_curve.control
        + graph_t.powi(2) * graph_curve.end;
    let side = if oak_bark_plate_random(index, 0x917d) >= 0.5 {
        1.0
    } else {
        -1.0
    };
    let run = side * (0.034 + 0.078 * oak_bark_plate_random(index, 0x3e29));
    let rise = -0.035 + 0.105 * oak_bark_plate_random(index, 0xd815);
    let end = start + Vec2::new(run, rise);
    let control = start
        + Vec2::new(
            run * (0.36 + 0.30 * oak_bark_plate_random(index, 0x27f1)),
            rise * 0.45 + (oak_bark_plate_random(index, 0xa563) - 0.5) * 0.025,
        );
    OakBarkCurve {
        start,
        control,
        end,
    }
}

fn oak_bark_check_distance(point: Vec2) -> (f32, f32) {
    let mut minimum = f32::INFINITY;
    let mut taper = 0.0_f32;
    for index in 0..OAK_BARK_CHECK_COUNT {
        for vertical_copy in -1..=1 {
            let curve = oak_bark_check_curve(index, vertical_copy, point.x);
            let (distance, along) = oak_bark_curve_distance(point, curve);
            if distance < minimum {
                minimum = distance;
                taper = 1.0 - smoothstep(0.42, 1.0, along);
            }
        }
    }
    (minimum, taper)
}

#[cfg(test)]
#[allow(dead_code)]
fn oak_bark_branch_distance(point: Vec2) -> (f32, f32) {
    let mut minimum = f32::INFINITY;
    let mut taper = 0.0;
    for index in 0..OAK_BARK_BRANCH_COUNT {
        let furrow =
            (oak_bark_plate_random(index, 0xb831) * OAK_BARK_MAJOR_FURROWS as f32).floor() as i32;
        for vertical_copy in -1..=1 {
            let origin_v = oak_bark_plate_random(index, 0x682d) + vertical_copy as f32;
            let direction = if oak_bark_plate_random(index, 0x44f9) >= 0.5 {
                1.0
            } else {
                -1.0
            };
            let start_x = oak_bark_major_furrow_x(furrow, origin_v);
            let start = Vec2::new(start_x + (point.x - start_x).round(), origin_v);
            let length = 0.12 + 0.20 * oak_bark_plate_random(index, 0xf291);
            let reach = direction * (0.024 + 0.055 * oak_bark_plate_random(index, 0x93a7));
            let end = start + Vec2::new(reach, length);
            let curve = OakBarkCurve {
                start,
                control: start
                    + Vec2::new(
                        reach * (0.55 + 0.25 * oak_bark_plate_random(index, 0x391f)),
                        length * 0.43,
                    ),
                end,
            };
            let (distance, along) = oak_bark_curve_distance(point, curve);
            if distance < minimum {
                minimum = distance;
                taper = 1.0 - smoothstep(0.58, 1.0, along);
            }
        }
    }
    (minimum, taper)
}

#[cfg(test)]
#[allow(dead_code)]
fn oak_bark_handoff_curve(index: i32, vertical_copy: i32, point_x: f32) -> OakBarkCurve {
    let (furrow, origin_v, length, reach) = match index {
        0 => (1, 0.20, 0.48, 0.13),
        _ => (3, 0.53, 0.45, -0.11),
    };
    let origin_v = origin_v + vertical_copy as f32;
    let start_x = oak_bark_major_furrow_x(furrow, origin_v);
    let start = Vec2::new(start_x + (point_x - start_x).round(), origin_v);
    let end = start + Vec2::new(reach, length);
    OakBarkCurve {
        start,
        control: start + Vec2::new(reach * if index == 0 { 0.28 } else { 0.78 }, length * 0.53),
        end,
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn oak_bark_handoff_distance(point: Vec2) -> (f32, f32, i32) {
    let mut best = (f32::INFINITY, 0.0, 0);
    for index in 0..OAK_BARK_HANDOFF_COUNT {
        for vertical_copy in -1..=1 {
            let curve = oak_bark_handoff_curve(index, vertical_copy, point.x);
            let (distance, along) = oak_bark_curve_distance(point, curve);
            if distance < best.0 {
                let envelope =
                    smoothstep(0.02, 0.11, along) * (1.0 - smoothstep(0.84, 0.99, along));
                best = (distance, envelope, index);
            }
        }
    }
    best
}

fn oak_bark_fiber_distance(point: Vec2) -> (f32, f32) {
    let mut minimum = f32::INFINITY;
    let mut envelope = 0.0;
    for index in 0..OAK_BARK_FIBER_COUNT {
        let site = oak_bark_plate_site((index * 11 + 5).rem_euclid(OAK_BARK_PLATE_COUNT));
        for filament in 0..3 {
            for vertical_copy in -1..=1 {
                let strand = index * 3 + filament;
                let centre = Vec2::new(
                    site.position.x
                        + (oak_bark_plate_random(strand, 0x4a21) - 0.5) * 0.018
                        + (point.x - site.position.x).round(),
                    site.position.y
                        + (oak_bark_plate_random(strand, 0xd927) - 0.5) * 0.026
                        + vertical_copy as f32,
                );
                let length = 0.040 + 0.075 * oak_bark_plate_random(strand, 0x18e3);
                let slant = (oak_bark_plate_random(strand, 0x7d2b) - 0.5) * 0.022;
                let curve = OakBarkCurve {
                    start: centre - Vec2::new(slant * 0.5, length * 0.5),
                    control: centre
                        + Vec2::new((oak_bark_plate_random(strand, 0xe5b1) - 0.5) * 0.007, 0.0),
                    end: centre + Vec2::new(slant * 0.5, length * 0.5),
                };
                let (distance, along) = oak_bark_curve_distance(point, curve);
                if distance < minimum {
                    minimum = distance;
                    envelope = smoothstep(0.0, 0.22, along) * (1.0 - smoothstep(0.72, 1.0, along));
                }
            }
        }
    }
    (minimum, envelope)
}

/// Periodic oak relief with continuous longitudinal furrows. A shared plate
/// field contributes only smoothly blended crown variation; subordinate checks
/// begin at major furrows and taper before they can outline closed cells.
pub(super) fn oak_bark_height(u: f32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let point = Vec2::new(u.rem_euclid(1.0), v.rem_euclid(1.0));
    let mut weight_sum = 0.0;
    let mut crown_height = 0.0;
    let mut tilt = 0.0;
    let mut fracture_phase = 0.0;
    // The coherent shared metric is sampled with a smooth compact-looking
    // kernel over every site. No nearest-site rank switch can introduce an
    // ownership seam into crown, tilt, or fracture phase.
    for index in 0..OAK_BARK_PLATE_COUNT {
        let site = oak_bark_plate_site(index);
        let offset = oak_bark_toroidal_offset(point, site.position);
        let distance = (offset.x.powi(2) + (offset.y * 0.46).powi(2)).sqrt();
        let weight = (-(distance / 0.15).powi(4)).exp() + 1.0e-5;
        let id = site.id;
        weight_sum += weight;
        crown_height += weight * (0.070 + 0.110 * oak_bark_site_value(id, 0x61e3));
        tilt += weight
            * ((oak_bark_site_value(id, 0x19d7) - 0.5) * offset.x * 0.30
                + (oak_bark_site_value(id, 0x2d91) - 0.5) * offset.y * 0.13);
        fracture_phase += weight * oak_bark_site_value(id, 0x8d31);
    }
    crown_height /= weight_sum;
    tilt /= weight_sum;
    fracture_phase /= weight_sum;

    let (graph_normalized, graph_width, graph_depth) = oak_bark_graph_distance(point);
    let graph_core = (-graph_normalized.powi(4)).exp();
    let graph_valley = (-0.5 * (graph_normalized / 3.0).powi(2)).exp();
    let graph_shoulder =
        (0.035 + 0.024 * graph_depth) * (-0.5 * ((graph_normalized - 3.25) / 0.82).powi(2)).exp();
    let graph_relief = graph_depth * (-0.52 * graph_core - 0.16 * graph_valley);
    let physical_graph_distance = graph_normalized * graph_width;
    let face_mask = smoothstep(0.012, 0.040, physical_graph_distance);
    let asymmetric_crown = (0.028 + 0.060 * fracture_phase)
        * smoothstep(0.010, 0.085, physical_graph_distance)
        * (0.72 + 0.28 * (tau * (point.y * 2.1 + fracture_phase)).sin());
    let (check_distance, check_taper) = oak_bark_check_distance(point);
    let check_relief = -0.085 * (-(check_distance / 0.0045).powi(4)).exp() * check_taper;
    let (fiber_distance, fiber_envelope) = oak_bark_fiber_distance(point);
    let fiber_relief =
        -0.032 * (-(fiber_distance / 0.0030).powi(2)).exp() * fiber_envelope * face_mask;
    let broad_breakup = 0.018
        * (tau * (point.x * 7.0 + point.y * 2.0 + fracture_phase)).sin()
        * (tau * (point.y * 5.0 - point.x + 0.73)).sin()
        * face_mask;
    let fine_breakup = 0.006
        * (tau * (point.x * 17.0 + point.y * 11.0 + fracture_phase * 2.0)).sin()
        * (tau * (point.y * 13.0 - point.x * 3.0 + 0.29)).sin()
        * face_mask;
    let fissure_relief = graph_relief.min(check_relief);

    (crown_height
        + tilt
        + asymmetric_crown
        + graph_shoulder
        + fissure_relief
        + fiber_relief
        + broad_breakup
        + fine_breakup)
        .clamp(-0.5, 0.32)
}

pub(super) fn periodic_sample(field: &[f32], size: u32, x: i32, y: i32) -> f32 {
    let size = size as i32;
    let wrapped_x = x.rem_euclid(size) as usize;
    let wrapped_y = y.rem_euclid(size) as usize;
    field[wrapped_y * size as usize + wrapped_x]
}

pub(super) fn oak_bark_horizon_ao(field: &[f32], x: i32, y: i32) -> f32 {
    debug_assert_eq!(field.len(), (OAK_BARK_TEXTURE_SIZE.pow(2)) as usize);
    let source_scale = (OAK_BARK_TEXTURE_SIZE / OAK_BARK_AO_SIZE) as i32;
    let source_x = x * source_scale + source_scale / 2;
    let source_y = y * source_scale + source_scale / 2;
    let centre = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, source_x, source_y)
        * OAK_BARK_HEIGHT_RANGE_METRES;
    let ao_texel_metres = OAK_BARK_TILE_METRES / OAK_BARK_AO_SIZE as f32;
    let mut visibility = 0.0;
    for (direction_x, direction_y) in OAK_BARK_AO_DIRECTIONS {
        let mut maximum_slope = 0.0_f32;
        for ao_step in OAK_BARK_AO_STEPS {
            let source_step = ao_step * source_scale;
            let neighbor = periodic_sample(
                field,
                OAK_BARK_TEXTURE_SIZE,
                source_x + direction_x * source_step,
                source_y + direction_y * source_step,
            ) * OAK_BARK_HEIGHT_RANGE_METRES;
            let run = ao_step as f32 * ao_texel_metres;
            maximum_slope = maximum_slope.max(((neighbor - centre) / run).max(0.0));
        }
        visibility += 1.0 / (1.0 + maximum_slope * maximum_slope).sqrt();
    }
    (visibility / OAK_BARK_AO_DIRECTIONS.len() as f32).clamp(0.36, 1.0)
}

pub(super) fn periodic_bilinear_sample(field: &[f32], size: u32, u: f32, v: f32) -> f32 {
    let x = u * size as f32 - 0.5;
    let y = v * size as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let blend_x = x - x.floor();
    let blend_y = y - y.floor();
    let lower = periodic_sample(field, size, x0, y0)
        .lerp(periodic_sample(field, size, x0 + 1, y0), blend_x);
    let upper = periodic_sample(field, size, x0, y0 + 1)
        .lerp(periodic_sample(field, size, x0 + 1, y0 + 1), blend_x);
    lower.lerp(upper, blend_y)
}

pub(super) fn oak_bark_local_cavity(field: &[f32], x: i32, y: i32) -> f32 {
    let centre = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y);
    let neighbors = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x - 1, y)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x + 1, y)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y - 1)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y + 1);
    let cavity = (neighbors * 0.25 - centre).max(0.0);
    (1.0 - cavity * 1.5).clamp(0.72, 1.0)
}

pub(super) fn generate_oak_bark_texture(images: &mut Assets<Image>) -> BarkTextureSet {
    let size = OAK_BARK_TEXTURE_SIZE;
    let pixel_count = (size * size) as usize;
    let texel = 1.0 / size as f32;
    let heights = (0..size)
        .flat_map(|y| {
            (0..size)
                .map(move |x| oak_bark_height((x as f32 + 0.5) * texel, (y as f32 + 0.5) * texel))
        })
        .collect::<Vec<_>>();
    let horizon_ao = (0..OAK_BARK_AO_SIZE)
        .flat_map(|y| {
            let heights = &heights;
            (0..OAK_BARK_AO_SIZE).map(move |x| oak_bark_horizon_ao(heights, x as i32, y as i32))
        })
        .collect::<Vec<_>>();
    let mut height_ao = Vec::with_capacity(pixel_count * 2);
    for y in 0..size {
        for x in 0..size {
            let height = periodic_sample(&heights, size, x as i32, y as i32);
            let encoded_height = ((height + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let broad_visibility = periodic_bilinear_sample(&horizon_ao, OAK_BARK_AO_SIZE, u, v);
            let local_visibility = oak_bark_local_cavity(&heights, x as i32, y as i32);
            let ao = (broad_visibility * local_visibility * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            height_ao.extend_from_slice(&[encoded_height, ao]);
        }
    }
    BarkTextureSet {
        height_ao: images.add(image_rg_mipped(height_ao, size, true)),
    }
}

#[cfg(test)]
mod oak_bark_tests {
    use std::collections::BTreeSet;

    use super::*;

    fn generated_bark_image() -> Image {
        let mut images = Assets::<Image>::default();
        let textures = generate_oak_bark_texture(&mut images);
        images.remove(&textures.height_ao).expect("oak bark image")
    }

    #[test]
    fn packed_height_and_ao_are_deterministic_and_independently_detailed() {
        let first = generated_bark_image();
        let second = generated_bark_image();
        assert_eq!(first.texture_descriptor.format, TextureFormat::Rg8Unorm);
        assert_eq!(first.data, second.data);

        let data = first.data.as_deref().expect("packed mip data");
        let base = &data[..(OAK_BARK_TEXTURE_SIZE.pow(2) * 2) as usize];
        let height_values = base.iter().step_by(2).copied().collect::<BTreeSet<_>>();
        let ao_values = base
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(
            height_values.len() > 128,
            "height values: {}",
            height_values.len()
        );
        assert!(ao_values.len() > 96, "AO values: {}", ao_values.len());
        assert!(height_values.first().is_some_and(|minimum| *minimum <= 24));
        assert!(ao_values.first().is_some_and(|minimum| *minimum < 150));
        assert_eq!(ao_values.last(), Some(&255));
    }

    #[test]
    fn tile_edges_are_continuous_at_sub_texel_scale() {
        let epsilon = 0.25 / OAK_BARK_TEXTURE_SIZE as f32;
        let mut maximum_horizontal_error = 0.0_f32;
        let mut maximum_vertical_error = 0.0_f32;
        for sample in 0..256 {
            let coordinate = (sample as f32 + 0.5) / 256.0;
            maximum_horizontal_error = maximum_horizontal_error.max(
                (oak_bark_height(epsilon, coordinate) - oak_bark_height(1.0 - epsilon, coordinate))
                    .abs(),
            );
            maximum_vertical_error = maximum_vertical_error.max(
                (oak_bark_height(coordinate, epsilon) - oak_bark_height(coordinate, 1.0 - epsilon))
                    .abs(),
            );
        }
        assert!(
            maximum_horizontal_error < 0.25,
            "horizontal seam error: {maximum_horizontal_error}"
        );
        assert!(
            maximum_vertical_error < 0.16,
            "vertical seam error: {maximum_vertical_error}"
        );
    }

    #[test]
    fn free_plate_sites_share_one_metric_and_furrows_form_the_primary_hierarchy() {
        let sites = (0..OAK_BARK_PLATE_COUNT)
            .map(oak_bark_plate_site)
            .collect::<Vec<_>>();
        assert_eq!(
            sites
                .iter()
                .map(|site| site.id)
                .collect::<BTreeSet<_>>()
                .len(),
            38
        );
        let minimum_separation = sites
            .iter()
            .enumerate()
            .flat_map(|(index, first)| {
                sites[index + 1..].iter().map(move |second| {
                    oak_bark_toroidal_offset(first.position, second.position).length()
                })
            })
            .fold(f32::INFINITY, f32::min);
        assert!(
            minimum_separation > 0.025,
            "minimum site separation: {minimum_separation}"
        );

        let mut plate_ids = BTreeSet::new();
        let mut boundary_samples = 0;
        let mut junction_samples = 0;
        let mut furrow_core_samples = 0;
        let mut check_samples = 0;
        for y in 0..96 {
            for x in 0..96 {
                let point = Vec2::new((x as f32 + 0.5) / 96.0, (y as f32 + 0.5) / 96.0);
                let field = oak_bark_plate_field(point);
                plate_ids.insert(field.sites[0].id);
                boundary_samples += usize::from(field.distances[1] - field.distances[0] < 0.006);
                junction_samples += usize::from(field.distances[2] - field.distances[1] < 0.006);
                furrow_core_samples += usize::from(oak_bark_major_distance(point).0 < 0.007);
                let (check_distance, taper) = oak_bark_check_distance(point);
                check_samples += usize::from(check_distance < 0.006 && taper > 0.1);
            }
        }
        assert!(plate_ids.len() >= 30, "visible plates: {}", plate_ids.len());
        assert!(
            furrow_core_samples >= 350,
            "furrow core samples: {furrow_core_samples}"
        );
        assert!(
            (60..=500).contains(&check_samples),
            "subordinate check samples: {check_samples}"
        );
        assert!(
            boundary_samples >= 500,
            "boundary samples: {boundary_samples}"
        );
        assert!(
            junction_samples >= 300,
            "junction samples: {junction_samples}"
        );
    }

    #[test]
    fn dense_height_samples_have_no_internal_ownership_jumps() {
        let epsilon = 0.25 / OAK_BARK_TEXTURE_SIZE as f32;
        let mut maximum_jump = 0.0_f32;
        let mut transition_samples = 0_usize;
        for y in 0..192 {
            for x in 0..192 {
                let point = Vec2::new((x as f32 + 0.5) / 192.0, (y as f32 + 0.5) / 192.0);
                let field = oak_bark_plate_field(point);
                let (check_distance, _) = oak_bark_check_distance(point);
                let (fiber_distance, _) = oak_bark_fiber_distance(point);
                let (graph_normalized, _, _) = oak_bark_graph_distance(point);
                if field.distances[1] - field.distances[0] > 0.003
                    || check_distance < 0.012
                    || fiber_distance < 0.005
                    || graph_normalized < 5.0
                {
                    continue;
                }
                transition_samples += 1;
                let horizontal = (oak_bark_height(point.x + epsilon, point.y)
                    - oak_bark_height(point.x - epsilon, point.y))
                .abs();
                let vertical = (oak_bark_height(point.x, point.y + epsilon)
                    - oak_bark_height(point.x, point.y - epsilon))
                .abs();
                maximum_jump = maximum_jump.max(horizontal).max(vertical);
            }
        }
        assert!(
            transition_samples >= 250,
            "dense ownership transition samples: {transition_samples}"
        );
        assert!(
            maximum_jump < 0.025,
            "internal quarter-texel height jump: {maximum_jump}"
        );
    }

    #[test]
    fn graph_edges_are_band_local_and_cross_sections_change_identity() {
        assert!(
            OAK_BARK_GRAPH_EDGES
                .iter()
                .all(|edge| edge.end.y - edge.start.y <= 0.23),
            "a graph edge survives too much of the tile height"
        );
        let layer_counts = [3, 4, 3, 4, 3, 3];
        assert_eq!(
            layer_counts.iter().copied().collect::<BTreeSet<_>>().len(),
            2
        );

        let mut signatures = BTreeSet::new();
        let mut counts = BTreeSet::new();
        for slice in 0..18 {
            let v = (slice as f32 + 0.5) / 18.0;
            let mut valleys = Vec::new();
            let mut inside = false;
            for x in 0..512 {
                let normalized = oak_bark_graph_distance(Vec2::new((x as f32 + 0.5) / 512.0, v)).0;
                let is_valley = normalized < 1.2;
                if is_valley && !inside {
                    valleys.push(x / 8);
                }
                inside = is_valley;
            }
            counts.insert(valleys.len());
            signatures.insert(valleys);
        }
        assert!(counts.len() >= 3, "graph cross-section counts: {counts:?}");
        assert!(
            signatures.len() >= 12,
            "graph cross-section identities: {}",
            signatures.len()
        );
    }

    #[test]
    fn checks_and_fibers_are_free_positioned_instead_of_row_banded() {
        let mut check_bins = BTreeSet::new();
        let mut off_legacy_rows = 0;
        for index in 0..OAK_BARK_CHECK_COUNT {
            let origin = oak_bark_plate_random(index, 0xc927);
            check_bins.insert((origin * 32.0).floor() as i32);
            let legacy_row_distance = (origin * 7.0 - (origin * 7.0).round()).abs() / 7.0;
            off_legacy_rows += i32::from(legacy_row_distance > 0.018);
        }
        assert!(check_bins.len() >= 12, "check origin bins: {check_bins:?}");
        assert!(
            off_legacy_rows >= 12,
            "checks away from legacy rows: {off_legacy_rows}"
        );

        let fiber_bins = (0..OAK_BARK_FIBER_COUNT)
            .map(|index| {
                let site = oak_bark_plate_site((index * 11 + 5).rem_euclid(OAK_BARK_PLATE_COUNT));
                (site.position.y * 32.0).floor() as i32
            })
            .collect::<BTreeSet<_>>();
        assert!(fiber_bins.len() >= 9, "fiber cluster bins: {fiber_bins:?}");
    }

    #[test]
    fn graph_junctions_are_wide_shouldered_cavities_without_additive_voids() {
        for junction in [Vec2::new(0.20, 0.38), Vec2::new(0.72, 0.78)] {
            let centre_height = oak_bark_height(junction.x, junction.y);
            let valley_flank = oak_bark_height(junction.x + 0.014, junction.y);
            let outer_flank = oak_bark_height(junction.x + 0.032, junction.y);
            assert!(centre_height < -0.20, "junction centre: {centre_height}");
            assert!(
                centre_height > -0.49,
                "junction collapsed into a black void: {centre_height}"
            );
            assert!(
                valley_flank > centre_height + 0.10,
                "junction valley is not broad: {centre_height}, {valley_flank}"
            );
            assert!(
                outer_flank > valley_flank,
                "junction lacks a two-sided shoulder: {valley_flank}, {outer_flank}"
            );
        }
    }

    #[test]
    fn box_filtered_mips_preserve_mid_scale_relief_then_converge() {
        let image = generated_bark_image();
        let data = image.data.as_deref().expect("packed mip data");
        let mut size = OAK_BARK_TEXTURE_SIZE;
        let mut offset = 0_usize;
        let mut previous_height_span = u8::MAX;
        let mut span_at_128 = None;
        while size > 0 {
            let byte_count = (size.pow(2) * 2) as usize;
            let mip = &data[offset..offset + byte_count];
            let minimum = mip.iter().step_by(2).copied().min().unwrap();
            let maximum = mip.iter().step_by(2).copied().max().unwrap();
            let span = maximum - minimum;
            assert!(
                span <= previous_height_span,
                "{size}px height span increased"
            );
            if size == 128 {
                span_at_128 = Some(span);
            }
            previous_height_span = span;
            offset += byte_count;
            size /= 2;
        }
        assert!(span_at_128.is_some_and(|span| span > 80));
        assert!(previous_height_span <= 1);
        assert_eq!(offset, data.len());
    }
}
