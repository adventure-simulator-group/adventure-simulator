use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

use crate::{
    Anchor, Attachment, AxeSpec, BillSpec, BladeProfile, BladeSection, BladeSpec, Bounds,
    ComponentDesign, ComponentRole, ComponentShape, CurvedBeakSpec, FigureEightSpec,
    GeneratedWeapon, GeneratedWeaponHolder, GlaiveSpec, GothicMaceSpec, GuardSpec, MaceSpec,
    MaterialClass, MeshPart, PartisanSpec, SocketSpec, TubePathSpec, ValidationError, WeaponDesign,
    WeaponHolderDesign, WeaponHolderKind, derive_holder_properties, derive_properties, design_hash,
    holder_design_hash, validate, validate_holder,
};

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("invalid weapon design")]
    Invalid(Vec<ValidationError>),
    #[error("attachment graph could not resolve component `{0}`")]
    UnresolvedAttachment(String),
    #[error("weapon cannot provide required {0} holder geometry")]
    MissingHolderGeometry(&'static str),
}

#[derive(Default)]
struct RawMesh {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl RawMesh {
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }
    fn append(&mut self, mut other: RawMesh) {
        let base = self.positions.len() as u32;
        self.positions.append(&mut other.positions);
        self.indices
            .extend(other.indices.into_iter().map(|index| index + base));
    }
    fn translate(&mut self, offset: [f32; 3]) {
        for point in &mut self.positions {
            for axis in 0..3 {
                point[axis] += offset[axis];
            }
        }
    }
    fn signed_volume(&self) -> f32 {
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|triangle| {
                let [a, b, c] = [
                    self.positions[triangle[0] as usize],
                    self.positions[triangle[1] as usize],
                    self.positions[triangle[2] as usize],
                ];
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }
    fn orient_positive(&mut self) {
        if self.signed_volume() < 0.0 {
            for triangle in self.indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
    }
}

fn cylinder(length: f32, radius: f32, segments: u16) -> RawMesh {
    frustum(length, radius, radius, segments)
}

fn frustum(length: f32, bottom_radius: f32, top_radius: f32, segments: u16) -> RawMesh {
    let mut mesh = RawMesh::default();
    let count = segments as usize;
    for (y, radius) in [(0.0, bottom_radius), (length, top_radius)] {
        for segment in 0..count {
            let angle = segment as f32 / count as f32 * std::f32::consts::TAU;
            mesh.positions
                .push([angle.cos() * radius, y, angle.sin() * radius]);
        }
    }
    let bottom_center = mesh.positions.len() as u32;
    mesh.positions.push([0.0, 0.0, 0.0]);
    let top_center = mesh.positions.len() as u32;
    mesh.positions.push([0.0, length, 0.0]);
    for segment in 0..count {
        let next = (segment + 1) % count;
        let b0 = segment as u32;
        let b1 = next as u32;
        let t0 = (count + segment) as u32;
        let t1 = (count + next) as u32;
        mesh.triangle(b0, t1, b1);
        mesh.triangle(b0, t0, t1);
        mesh.triangle(bottom_center, b0, b1);
        mesh.triangle(top_center, t1, t0);
    }
    mesh.orient_positive();
    mesh
}

fn elliptical_frustum(
    length: f32,
    bottom_half_width: f32,
    top_half_width: f32,
    thickness_to_width: f32,
    segments: u16,
) -> RawMesh {
    let mut mesh = frustum(length, bottom_half_width, top_half_width, segments);
    for position in &mut mesh.positions {
        position[2] *= thickness_to_width;
    }
    mesh.orient_positive();
    mesh
}

fn blade(spec: &BladeSpec) -> RawMesh {
    let samples = spec.samples.0 as usize;
    let section_count = match spec.section {
        BladeSection::Flat | BladeSection::Diamond => 4,
        BladeSection::Fullered => 8,
    };
    let mut mesh = RawMesh::default();
    let ricasso = spec.ricasso.meters() / spec.length.meters();
    for index in 0..=samples {
        let t = index as f32 / samples as f32;
        let edge_t = ((t - ricasso) / (1.0 - ricasso)).clamp(0.0, 1.0);
        let profile_taper = match spec.profile {
            BladeProfile::Straight | BladeProfile::Curved => 1.0,
            BladeProfile::Spear => 0.78 + 0.22 * (std::f32::consts::PI * edge_t).sin(),
            BladeProfile::Cleaver => 0.9 + 0.24 * (std::f32::consts::PI * edge_t).sin(),
        };
        let point_taper = 0.025 + 0.975 * (1.0 - edge_t).powf(spec.taper.unit());
        let belly = 1.0 + spec.belly.0 as f32 / 1_000.0 * (std::f32::consts::PI * edge_t).sin();
        let half_width = spec.width.meters() * 0.5 * profile_taper * point_taper * belly;
        let asymmetry = half_width * spec.single_edge.unit() * 0.35;
        let center = spec.curvature.meters() * edge_t * edge_t + asymmetry;
        let depth = spec.thickness.meters() * 0.5 * (1.0 - edge_t * 0.72);
        let ring: Vec<[f32; 2]> = match spec.section {
            BladeSection::Fullered => vec![
                [-half_width, 0.0],
                [-half_width * 0.72, depth],
                [-half_width * 0.28, depth * 0.32],
                [0.0, depth * 0.22],
                [half_width * 0.28, depth * 0.32],
                [half_width * 0.72, depth],
                [half_width, 0.0],
                [0.0, -depth * 0.8],
            ],
            BladeSection::Flat => vec![
                [-half_width, 0.0],
                [0.0, depth * 0.45],
                [half_width, 0.0],
                [0.0, -depth * 0.45],
            ],
            BladeSection::Diamond => {
                vec![
                    [-half_width, 0.0],
                    [0.0, depth],
                    [half_width, 0.0],
                    [0.0, -depth],
                ]
            }
        };
        mesh.positions.extend(
            ring.into_iter()
                .map(|point| [center + point[0], t * spec.length.meters(), point[1]]),
        );
    }
    for ring in 0..samples {
        for side in 0..section_count {
            let next = (side + 1) % section_count;
            let a = (ring * section_count + side) as u32;
            let b = (ring * section_count + next) as u32;
            let c = ((ring + 1) * section_count + next) as u32;
            let d = ((ring + 1) * section_count + side) as u32;
            mesh.triangle(a, b, c);
            mesh.triangle(a, c, d);
        }
    }
    let bottom = mesh.positions.len() as u32;
    mesh.positions.push([0.0, 0.0, 0.0]);
    let top = mesh.positions.len() as u32;
    mesh.positions
        .push([spec.curvature.meters(), spec.length.meters(), 0.0]);
    for side in 0..section_count {
        let next = (side + 1) % section_count;
        mesh.triangle(bottom, next as u32, side as u32);
        let a = (samples * section_count + side) as u32;
        let b = (samples * section_count + next) as u32;
        mesh.triangle(top, a, b);
    }
    mesh.orient_positive();
    mesh
}

fn prism(mut outline: Vec<[f32; 2]>, thickness: f32) -> RawMesh {
    let area: f32 = outline
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = outline[(index + 1) % outline.len()];
            point[0] * next[1] - next[0] * point[1]
        })
        .sum();
    if area < 0.0 {
        outline.reverse();
    }
    let half = thickness / 2.0;
    let count = outline.len();
    let mut mesh = RawMesh::default();
    for z in [-half, half] {
        mesh.positions
            .extend(outline.iter().map(|point| [point[0], point[1], z]));
    }
    for [a, b, c] in triangulate(&outline) {
        mesh.triangle((count + a) as u32, (count + b) as u32, (count + c) as u32);
        mesh.triangle(a as u32, c as u32, b as u32);
    }
    for index in 0..count {
        let next = (index + 1) % count;
        mesh.triangle(index as u32, next as u32, (count + next) as u32);
        mesh.triangle(index as u32, (count + next) as u32, (count + index) as u32);
    }
    mesh.orient_positive();
    mesh
}

fn triangulate(outline: &[[f32; 2]]) -> Vec<[usize; 3]> {
    fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    fn inside(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
        let [ab, bc, ca] = [cross(a, b, point), cross(b, c, point), cross(c, a, point)];
        ab >= -1e-8 && bc >= -1e-8 && ca >= -1e-8
    }
    let mut remaining: Vec<_> = (0..outline.len()).collect();
    let mut triangles = Vec::with_capacity(outline.len().saturating_sub(2));
    while remaining.len() > 3 {
        let mut ear = None;
        for index in 0..remaining.len() {
            let a = remaining[(index + remaining.len() - 1) % remaining.len()];
            let b = remaining[index];
            let c = remaining[(index + 1) % remaining.len()];
            if cross(outline[a], outline[b], outline[c]) <= 1e-9 {
                continue;
            }
            if remaining.iter().copied().any(|point| {
                point != a
                    && point != b
                    && point != c
                    && inside(outline[point], outline[a], outline[b], outline[c])
            }) {
                continue;
            }
            ear = Some((index, [a, b, c]));
            break;
        }
        let Some((index, triangle)) = ear else {
            // Validation rejects self-intersecting profiles; this is only a numerical fallback.
            for index in 1..remaining.len() - 1 {
                triangles.push([remaining[0], remaining[index], remaining[index + 1]]);
            }
            return triangles;
        };
        triangles.push(triangle);
        remaining.remove(index);
    }
    triangles.push([remaining[0], remaining[1], remaining[2]]);
    triangles
}

fn socket(spec: &SocketSpec) -> RawMesh {
    let n = spec.segments.0 as usize;
    let outer_bottom = spec.outer_radius.meters();
    let outer_top = spec.top_radius.meters();
    let length = spec.length.meters();
    let mut mesh = RawMesh::default();
    for (y, outer) in [(0.0, outer_bottom), (length, outer_top)] {
        let inner = outer - spec.wall.meters();
        for radius in [outer, inner] {
            for segment in 0..n {
                let a = segment as f32 / n as f32 * std::f32::consts::TAU;
                mesh.positions.push([a.cos() * radius, y, a.sin() * radius]);
            }
        }
    }
    let ob = 0;
    let ib = n;
    let ot = n * 2;
    let it = n * 3;
    for i in 0..n {
        let j = (i + 1) % n;
        for [a, b, c, d] in [
            [ob + i, ot + i, ot + j, ob + j],
            [ib + i, ib + j, it + j, it + i],
            [ob + i, ob + j, ib + j, ib + i],
            [ot + i, it + i, it + j, ot + j],
        ] {
            mesh.triangle(a as u32, b as u32, c as u32);
            mesh.triangle(a as u32, c as u32, d as u32);
        }
    }
    mesh.orient_positive();
    mesh
}

fn spear(spec: &crate::SpearSpec) -> RawMesh {
    let count = spec.samples.0 as usize;
    let belly = spec.belly_position.unit();
    let half_root = spec.root_width.meters() / 2.0;
    let half_max = spec.width.meters() / 2.0;
    let mut left = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let t = index as f32 / count as f32;
        let half = if t <= belly {
            half_root + (half_max - half_root) * (t / belly).powf(0.8)
        } else {
            half_max * ((1.0 - t) / (1.0 - belly)).powf(spec.acuteness.unit())
        };
        left.push([-half, t * spec.length.meters()]);
    }
    let mut outline = left;
    for index in (1..count).rev() {
        let point = outline[index];
        outline.push([-point[0], point[1]]);
    }
    outline.push([half_root, 0.0]);
    prism(outline, spec.thickness.meters())
}

fn profiled_pommel(spec: &crate::ProfiledPommelSpec) -> RawMesh {
    let radial = spec.segments.0 as usize;
    let rings = spec.profile.len();
    let mut mesh = RawMesh::default();
    for point in &spec.profile {
        for segment in 0..radial {
            let angle = segment as f32 / radial as f32 * std::f32::consts::TAU;
            mesh.positions.push([
                angle.cos() * point.radius.meters(),
                point.y.meters(),
                angle.sin() * point.radius.meters(),
            ]);
        }
    }
    for ring in 0..rings - 1 {
        for segment in 0..radial {
            let next = (segment + 1) % radial;
            let a = (ring * radial + segment) as u32;
            let b = (ring * radial + next) as u32;
            let c = ((ring + 1) * radial + next) as u32;
            let d = ((ring + 1) * radial + segment) as u32;
            mesh.triangle(a, c, b);
            mesh.triangle(a, d, c);
        }
    }
    let bottom = mesh.positions.len() as u32;
    mesh.positions.push([0.0, spec.profile[0].y.meters(), 0.0]);
    let top = mesh.positions.len() as u32;
    mesh.positions
        .push([0.0, spec.profile[rings - 1].y.meters(), 0.0]);
    for segment in 0..radial {
        let next = (segment + 1) % radial;
        mesh.triangle(bottom, segment as u32, next as u32);
        let a = ((rings - 1) * radial + segment) as u32;
        let b = ((rings - 1) * radial + next) as u32;
        mesh.triangle(top, b, a);
    }
    mesh.orient_positive();
    mesh
}

fn axe(spec: &AxeSpec) -> RawMesh {
    let s = spec.side as f32;
    let w = spec.reach.meters();
    let h = spec.height.meters();
    let r = spec.root_width.meters();
    let beard = spec.beard.unit();
    let curve = spec.curvature.unit();
    let flare = spec.flare.0 as f32 / 1_000.0;
    let toe = spec.toe.0 as f32 / 1_000.0;
    let heel = spec.heel.0 as f32 / 1_000.0;
    let mut points = vec![
        [-r * s, h * spec.upper_shoulder.unit()],
        [w * (0.68 + flare * 0.12) * s, h * (0.48 + toe)],
    ];
    for i in 0..=12 {
        let t = i as f32 / 12.0;
        points.push([
            w * (0.82 + flare * (t - 0.5) + curve * (std::f32::consts::PI * t).sin()) * s,
            h * (0.42 - 0.9 * t),
        ]);
    }
    points.extend([
        [w * beard * s, -h * (0.34 + spec.beard_drop.unit() + heel)],
        [-r * s, -h * spec.lower_shoulder.unit()],
    ]);
    if s < 0.0 {
        points.reverse();
    }
    prism(points, spec.thickness.meters())
}
fn hammer(spec: &crate::HammerPollSpec) -> RawMesh {
    let s = spec.direction as f32;
    let l = spec.length.meters();
    let f = spec.face.meters() * (1.0 + spec.face_flare.unit());
    let n = spec.neck.meters();
    let crown = spec.crown_length.meters();
    let neck_length = l * spec.neck_ratio.unit();
    prism(
        vec![
            [0.0, -n / 2.0],
            [neck_length * s, -n / 2.0],
            [l * s, -f / 2.0],
            [(l + crown) * s, 0.0],
            [l * s, f / 2.0],
            [neck_length * s, n / 2.0],
            [0.0, n / 2.0],
        ],
        spec.face_thickness.meters(),
    )
}

fn curved_beak(spec: &CurvedBeakSpec) -> RawMesh {
    let n = spec.samples.0 as usize;
    let mut upper = Vec::new();
    let mut lower = Vec::new();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = spec.direction as f32 * spec.length.meters() * t;
        let bp = spec.bend_position.unit().clamp(0.15, 0.85);
        let exponent = 0.5_f32.ln() / bp.ln();
        let bend = spec.curvature.meters() * (std::f32::consts::PI * t.powf(exponent)).sin()
            + spec.droop.meters() * t;
        let half = (spec.root_section.meters() * (1.0 - t) + spec.tip_section.meters() * t) / 2.0;
        upper.push([x, bend + half]);
        lower.push([x, bend - half]);
    }
    lower.reverse();
    upper.extend(lower);
    prism(upper, spec.thickness.meters())
}
fn faceted_beak(spec: &crate::FacetedBeakSpec) -> RawMesh {
    let s = spec.direction as f32;
    let l = spec.length.meters();
    let r = spec.root.meters();
    let t = spec.tip.meters();
    let bend = spec.bend_position.unit();
    let set = spec.set.meters();
    prism(
        vec![
            [0.0, -r / 2.0],
            [l * bend * s, set * bend - r * 0.42],
            [l * s, set - t / 2.0],
            [l * s, set + t / 2.0],
            [l * bend * s, set * bend + r * 0.42],
            [0.0, r / 2.0],
        ],
        spec.tip_thickness.meters(),
    )
}

fn cubic(points: [[f32; 2]; 4], samples: usize) -> Vec<[f32; 2]> {
    (0..=samples)
        .map(|i| {
            let t = i as f32 / samples as f32;
            let u = 1.0 - t;
            [
                u.powi(3) * points[0][0]
                    + 3.0 * u * u * t * points[1][0]
                    + 3.0 * u * t * t * points[2][0]
                    + t.powi(3) * points[3][0],
                u.powi(3) * points[0][1]
                    + 3.0 * u * u * t * points[1][1]
                    + 3.0 * u * t * t * points[2][1]
                    + t.powi(3) * points[3][1],
            ]
        })
        .collect()
}
fn append(target: &mut Vec<[f32; 2]>, mut span: Vec<[f32; 2]>) {
    if !target.is_empty() {
        span.remove(0);
    }
    target.extend(span);
}

fn bill(spec: &BillSpec) -> RawMesh {
    let l = spec.length.meters();
    let w = spec.width.meters();
    let h = spec.hook.meters();
    let r = spec.root.meters();
    let hd = spec.hook_depth.unit();
    let hc = spec.hook_curvature.unit();
    let root_l = [-r, -spec.root_length.meters()];
    let apex = [0.0, l];
    let belly = spec.belly_position.unit();
    let point = spec.point_length.unit();
    let shoulder = [w, l * 0.68];
    let crown = [w + h * 0.72, l * (0.68 + hc * 0.55)];
    let tip = [w + h, l * (0.68 - hd)];
    let inner = [w + h * 0.46, l * (0.62 + hc * 0.2)];
    let root_r = [r, -spec.root_length.meters()];
    let sh = w.min(h) * 0.16;
    let ch = h * 0.18;
    let ih = h * 0.16;
    let n = (spec.samples.0 as usize / 6).max(3);
    let mut p = Vec::new();
    for span in [
        [
            root_l,
            [-r * 0.92, l * (1.0 - belly) * 0.42],
            [-r * 0.52, l * 0.84],
            apex,
        ],
        [
            apex,
            [w * 0.1, l * (0.97 - point * 0.08)],
            [shoulder[0] - sh, shoulder[1]],
            shoulder,
        ],
        [
            shoulder,
            [shoulder[0] + sh, shoulder[1]],
            [crown[0] - ch, crown[1]],
            crown,
        ],
        [
            crown,
            [crown[0] + ch, crown[1]],
            [tip[0], l * (0.62 - hd * 0.25)],
            tip,
        ],
        [
            tip,
            [tip[0], l * (0.56 - hd)],
            [inner[0] + ih, inner[1]],
            inner,
        ],
        [
            inner,
            [inner[0] - ih, inner[1]],
            [w * 0.62, l * belly * 0.28],
            root_r,
        ],
    ] {
        append(&mut p, cubic(span, n));
    }
    prism(p, spec.thickness.meters())
}

fn glaive(spec: &GlaiveSpec) -> RawMesh {
    let l = spec.length.meters();
    let w = spec.width.meters();
    let r = spec.root.meters();
    let curve = spec.curvature.meters();
    let point = spec.point_length.unit();
    let n = spec.samples.0 as usize;
    let mut p = vec![
        [-r, -spec.root_length.meters()],
        [r, -spec.root_length.meters()],
        [r * 1.18, l * 0.025],
        [w * 0.48, l * 0.12],
    ];
    let edge_limit = 1.0 - point * 0.34;
    let edge: Vec<_> = (0..=n)
        .map(|index| {
            let t = index as f32 / n as f32 * edge_limit;
            [
                curve * 0.42
                    + (1.0 - t)
                        * w
                        * (0.54
                            + spec.edge_curvature.unit()
                                * (std::f32::consts::PI
                                    * (t / spec.belly_position.unit().max(0.1)).min(1.0))
                                .sin()),
                l * (0.08 + (0.84 - point * 0.34) * t),
            ]
        })
        .collect();
    append(&mut p, edge);
    let near_apex = *p.last().unwrap();
    let apex = [curve * 0.42, l];
    append(
        &mut p,
        cubic(
            [
                near_apex,
                [near_apex[0] * 0.72 + apex[0] * 0.28, l * 0.86],
                [apex[0] + w * 0.006, l * 0.96],
                apex,
            ],
            4,
        ),
    );
    let spine_top = [apex[0] - w * 0.1, l * (1.0 - point)];
    let spine_lower = [-w * 0.42, l * 0.18];
    let join = [-w * 0.08, -l * point * 0.12];
    append(
        &mut p,
        cubic(
            [
                apex,
                [apex[0] - w * 0.012, l * 0.96],
                [spine_top[0] - join[0], spine_top[1] - join[1]],
                spine_top,
            ],
            4,
        ),
    );
    append(
        &mut p,
        cubic(
            [
                spine_top,
                [spine_top[0] + join[0], spine_top[1] + join[1]],
                [-w * (0.34 + spec.spine_curvature.unit() * 0.04), l * 0.31],
                spine_lower,
            ],
            n,
        ),
    );
    let root_shoulder = [-r * 1.18, l * 0.025];
    append(
        &mut p,
        cubic(
            [
                spine_lower,
                [-w * 0.32, l * 0.13],
                [-r * 1.45, l * 0.05],
                root_shoulder,
            ],
            4,
        ),
    );
    prism(p, spec.thickness.meters())
}

fn fork(spec: &crate::ForkSpec) -> RawMesh {
    let l = spec.length.meters();
    let w = spec.width.meters() / 2.0;
    let root = spec.base_width.meters() / 2.0;
    let tine = spec.tine_width.meters();
    let c = spec.crotch.unit();
    let taper = spec.taper.unit();
    let shoulder = spec.shoulder_blend.unit();
    let round = spec.crotch_round.unit();
    prism(
        vec![
            [-root, 0.0],
            [-w, l * shoulder],
            [-w, l],
            [-w + tine * taper, l * 0.94],
            [-tine * 0.45, l * (c + round)],
            [0.0, l * c],
            [tine * 0.45, l * (c + round)],
            [w - tine * taper, l * 0.94],
            [w, l],
            [w, l * shoulder],
            [root, 0.0],
        ],
        spec.thickness.meters(),
    )
}
fn partisan(spec: &PartisanSpec) -> RawMesh {
    let l = spec.length.meters();
    let w = spec.width.meters();
    let lug = spec.lug_width.meters() / 2.0;
    let root = spec.root_width.meters() / 2.0;
    let belly = l * spec.belly_position.unit();
    let lug_drop = l * spec.lug_drop.unit();
    let lug_sweep = l * spec.lug_sweep.unit();
    let point_shoulder = belly + (l - belly) * (1.0 - 1.0 / (1.0 + spec.acuteness.unit()));
    prism(
        vec![
            [0.0, l],
            [-w * 0.34, point_shoulder],
            [-w / 2.0, belly],
            [-w * 0.34, l * 0.12],
            [-lug, lug_drop],
            [-lug * 0.72, 0.0],
            [-root, lug_sweep],
            [root, lug_sweep],
            [lug * 0.72, 0.0],
            [lug, lug_drop],
            [w * 0.34, l * 0.12],
            [w / 2.0, belly],
            [w * 0.34, point_shoulder],
        ],
        spec.thickness.meters(),
    )
}

fn fan_pommel(spec: &crate::FanPommelSpec) -> RawMesh {
    let width = spec.width.meters();
    let height = spec.height.meters();
    let mut points = vec![[-width * 0.18, 0.0]];
    for index in 0..12 {
        let t = index as f32 / 12.0;
        points.push([
            -width * (0.18 + 0.32 * (t * std::f32::consts::FRAC_PI_2).sin()),
            height * (0.04 + 0.36 * t),
        ]);
    }
    for index in 0..=24 {
        let angle = std::f32::consts::PI - index as f32 / 24.0 * std::f32::consts::PI;
        points.push([
            angle.cos() * width / 2.0,
            height * 0.4 + angle.sin() * height * 0.6,
        ]);
    }
    for index in (0..12).rev() {
        let t = index as f32 / 12.0;
        points.push([
            width * (0.18 + 0.32 * (t * std::f32::consts::FRAC_PI_2).sin()),
            height * (0.04 + 0.36 * t),
        ]);
    }
    points.push([width * 0.18, 0.0]);
    prism(points, spec.thickness.meters())
}

fn guard(spec: &GuardSpec) -> RawMesh {
    let path_count = spec.samples.0.max(2) as usize;
    let radial = spec.radial_segments.0.max(12) as usize;
    let radius = spec.radius.meters();
    let span = spec.span.meters();
    let sweep = spec.sweep.meters();
    let centers: Vec<_> = (0..=path_count)
        .map(|index| {
            let t = index as f32 / path_count as f32;
            let normalized = t * 2.0 - 1.0;
            [span * (t - 0.5), sweep * normalized.powi(3), 0.0]
        })
        .collect();
    let mut mesh = RawMesh::default();
    for index in 0..centers.len() {
        let previous = centers[index.saturating_sub(1)];
        let next = centers[(index + 1).min(centers.len() - 1)];
        let dx = next[0] - previous[0];
        let dy = next[1] - previous[1];
        let magnitude = dx.hypot(dy).max(f32::EPSILON);
        let normal = [-dy / magnitude, dx / magnitude];
        for segment in 0..radial {
            let angle = segment as f32 / radial as f32 * std::f32::consts::TAU;
            mesh.positions.push([
                centers[index][0] + normal[0] * angle.cos() * radius,
                centers[index][1] + normal[1] * angle.cos() * radius,
                angle.sin() * radius,
            ]);
        }
    }
    let start_center = mesh.positions.len() as u32;
    mesh.positions.push(centers[0]);
    let end_center = mesh.positions.len() as u32;
    mesh.positions.push(*centers.last().expect("guard path"));
    for index in 0..path_count {
        for segment in 0..radial {
            let next = (segment + 1) % radial;
            let a = (index * radial + segment) as u32;
            let b = (index * radial + next) as u32;
            let c = ((index + 1) * radial + next) as u32;
            let d = ((index + 1) * radial + segment) as u32;
            mesh.triangle(a, b, c);
            mesh.triangle(a, c, d);
        }
    }
    for segment in 0..radial {
        let next = (segment + 1) % radial;
        mesh.triangle(start_center, next as u32, segment as u32);
        let a = (path_count * radial + segment) as u32;
        let b = (path_count * radial + next) as u32;
        mesh.triangle(end_center, a, b);
    }
    mesh.orient_positive();
    mesh
}

fn tube_path(spec: &TubePathSpec) -> RawMesh {
    let centers: Vec<[f32; 3]> = spec.points.iter().map(|p| p.meters()).collect();
    tube_centers(
        &centers,
        spec.radius.meters(),
        spec.radial_segments.0 as usize,
        spec.closed,
    )
}
fn tube_centers(centers: &[[f32; 3]], radius: f32, radial: usize, closed: bool) -> RawMesh {
    let count = centers.len();
    let mut mesh = RawMesh::default();
    for source in 0..count {
        let prev = centers[if source == 0 {
            if closed { count - 1 } else { 0 }
        } else {
            source - 1
        }];
        let next = centers[if source + 1 == count {
            if closed { 0 } else { source }
        } else {
            source + 1
        }];
        let mut tangent = [next[0] - prev[0], next[1] - prev[1], next[2] - prev[2]];
        let magnitude = (tangent[0].powi(2) + tangent[1].powi(2) + tangent[2].powi(2))
            .sqrt()
            .max(f32::EPSILON);
        for axis in &mut tangent {
            *axis /= magnitude;
        }
        let reference = if tangent[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let mut normal = [
            tangent[1] * reference[2] - tangent[2] * reference[1],
            tangent[2] * reference[0] - tangent[0] * reference[2],
            tangent[0] * reference[1] - tangent[1] * reference[0],
        ];
        let normal_magnitude = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2))
            .sqrt()
            .max(f32::EPSILON);
        for axis in &mut normal {
            *axis /= normal_magnitude;
        }
        let binormal = [
            tangent[1] * normal[2] - tangent[2] * normal[1],
            tangent[2] * normal[0] - tangent[0] * normal[2],
            tangent[0] * normal[1] - tangent[1] * normal[0],
        ];
        for segment in 0..radial {
            let a = segment as f32 / radial as f32 * std::f32::consts::TAU;
            mesh.positions.push([
                centers[source][0] + (normal[0] * a.cos() + binormal[0] * a.sin()) * radius,
                centers[source][1] + (normal[1] * a.cos() + binormal[1] * a.sin()) * radius,
                centers[source][2] + (normal[2] * a.cos() + binormal[2] * a.sin()) * radius,
            ]);
        }
    }
    let spans = if closed { count } else { count - 1 };
    for index in 0..spans {
        let following = (index + 1) % count;
        for segment in 0..radial {
            let next = (segment + 1) % radial;
            let a = (index * radial + segment) as u32;
            let b = (index * radial + next) as u32;
            let c = (following * radial + next) as u32;
            let d = (following * radial + segment) as u32;
            mesh.triangle(a, b, c);
            mesh.triangle(a, c, d);
        }
    }
    if !closed {
        let sc = mesh.positions.len() as u32;
        mesh.positions.push(centers[0]);
        let ec = mesh.positions.len() as u32;
        mesh.positions.push(*centers.last().unwrap());
        for segment in 0..radial {
            let next = (segment + 1) % radial;
            mesh.triangle(sc, next as u32, segment as u32);
            let a = ((count - 1) * radial + segment) as u32;
            let b = ((count - 1) * radial + next) as u32;
            mesh.triangle(ec, a, b);
        }
    }
    mesh.orient_positive();
    mesh
}
fn ring(spec: &crate::RingGuardSpec) -> RawMesh {
    let n = spec.samples.0 as usize;
    let closed = (spec.arc_end.0 - spec.arc_start.0) >= 6200;
    let count = if closed { n } else { n + 1 };
    let points: Vec<_> = (0..count)
        .map(|i| {
            let t = i as f32 / n as f32;
            let a =
                (spec.arc_start.0 as f32 + (spec.arc_end.0 - spec.arc_start.0) as f32 * t) / 1000.0;
            [
                a.cos() * spec.radius.meters(),
                a.sin() * spec.radius.meters(),
                0.0,
            ]
        })
        .collect();
    tube_centers(
        &points,
        spec.bar.meters(),
        spec.radial_segments.0 as usize,
        closed,
    )
}
fn figure_eight(spec: &FigureEightSpec) -> RawMesh {
    let n = spec.samples.0 as usize;
    let points: Vec<_> = (0..n)
        .map(|index| {
            let angle = index as f32 / n as f32 * std::f32::consts::TAU;
            [
                spec.width.meters() / 2.0 * angle.sin(),
                spec.height.meters() * angle.sin() * angle.cos(),
                0.0,
            ]
        })
        .collect();
    tube_centers(
        &points,
        spec.bar.meters(),
        spec.radial_segments.0 as usize,
        true,
    )
}

fn gothic_mace(spec: &GothicMaceSpec) -> RawMesh {
    let mut result = cylinder(
        spec.length.meters() + spec.crown_length.meters(),
        spec.root_radius.meters(),
        spec.radial_segments.0,
    );
    let half = spec.length.meters() / 2.0;
    let cusp_y = -half + spec.length.meters() * spec.cusp_height.unit();
    let exponent = 1.03 + spec.concavity.unit().min(0.98) * 2.97;
    let n = spec.profile_samples.0 as usize;
    let mut outline = Vec::new();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        outline.push([
            spec.root_radius.meters()
                + (spec.cusp_radius.meters() - spec.root_radius.meters()) * t.powf(exponent),
            -half + (cusp_y + half) * t,
        ]);
    }
    for i in 1..=n {
        let t = i as f32 / n as f32;
        outline.push([
            spec.shoulder_radius.meters()
                + (spec.cusp_radius.meters() - spec.shoulder_radius.meters())
                    * (1.0 - t).powf(exponent),
            cusp_y + (half - cusp_y) * t,
        ]);
    }
    for flange in 0..spec.flanges {
        let mut part = prism(outline.clone(), spec.flange_thickness.meters());
        let angle = flange as f32 / spec.flanges as f32 * std::f32::consts::TAU;
        for p in &mut part.positions {
            let x = p[0];
            let z = p[2];
            p[0] = x * angle.cos() + z * angle.sin();
            p[2] = -x * angle.sin() + z * angle.cos();
            p[1] += half;
        }
        result.append(part);
    }
    result.orient_positive();
    result
}

fn box_mesh(center: [f32; 3], size: [f32; 3], rotation: f32) -> RawMesh {
    let mut mesh = RawMesh::default();
    for [x, y, z] in [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ] {
        let x = x * size[0] / 2.0;
        let z = z * size[2] / 2.0;
        mesh.positions.push([
            center[0] + x * rotation.cos() + z * rotation.sin(),
            center[1] + y * size[1] / 2.0,
            center[2] - x * rotation.sin() + z * rotation.cos(),
        ]);
    }
    for face in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
        [0, 1, 5, 4],
        [3, 7, 6, 2],
    ] {
        mesh.triangle(face[0], face[1], face[2]);
        mesh.triangle(face[0], face[2], face[3]);
    }
    mesh.orient_positive();
    mesh
}

fn mace(spec: &MaceSpec) -> RawMesh {
    let mut mesh = cylinder(
        spec.length.meters(),
        spec.core_radius.meters(),
        spec.segments.0,
    );
    let cusp_y = spec.length.meters() * spec.cusp_height.unit();
    for flange in 0..spec.flanges {
        mesh.append(box_mesh(
            [0.0, cusp_y, 0.0],
            [
                spec.cusp_radius.meters() * 2.0,
                spec.length.meters() * 0.72,
                spec.flange_thickness.meters(),
            ],
            flange as f32 / spec.flanges as f32 * std::f32::consts::TAU,
        ));
    }
    mesh.orient_positive();
    mesh
}

fn shaded(
    mesh: &RawMesh,
    crease_cosine: f32,
    smooth_core: Option<([f32; 3], f32)>,
) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 3]>) {
    #[derive(Clone, Copy)]
    struct Group {
        sum: [f32; 3],
        output: u32,
    }
    let mut groups = vec![Vec::<Group>::new(); mesh.positions.len()];
    let mut positions = Vec::new();
    let mut indices = Vec::with_capacity(mesh.indices.len());
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [
            mesh.positions[triangle[0] as usize],
            mesh.positions[triangle[1] as usize],
            mesh.positions[triangle[2] as usize],
        ];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let magnitude = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2))
            .sqrt()
            .max(f32::EPSILON);
        for axis in &mut normal {
            *axis /= magnitude;
        }
        for original in triangle {
            let candidates = &mut groups[*original as usize];
            let found = candidates.iter().position(|group| {
                let m = (group.sum[0].powi(2) + group.sum[1].powi(2) + group.sum[2].powi(2))
                    .sqrt()
                    .max(f32::EPSILON);
                (group.sum[0] * normal[0] + group.sum[1] * normal[1] + group.sum[2] * normal[2]) / m
                    >= crease_cosine
            });
            let output = if let Some(index) = found {
                for (sum, contribution) in candidates[index].sum.iter_mut().zip(normal) {
                    *sum += contribution;
                }
                candidates[index].output
            } else {
                let output = positions.len() as u32;
                positions.push(mesh.positions[*original as usize]);
                candidates.push(Group {
                    sum: normal,
                    output,
                });
                output
            };
            indices.push(output);
        }
    }
    let mut result = vec![[0.0; 3]; positions.len()];
    for vertex_groups in groups {
        for group in vertex_groups {
            let magnitude = (group.sum[0].powi(2) + group.sum[1].powi(2) + group.sum[2].powi(2))
                .sqrt()
                .max(f32::EPSILON);
            result[group.output as usize] = [
                group.sum[0] / magnitude,
                group.sum[1] / magnitude,
                group.sum[2] / magnitude,
            ];
        }
    }
    if let Some((origin, radius)) = smooth_core {
        for (position, normal) in positions.iter().zip(&mut result) {
            let x = position[0] - origin[0];
            let z = position[2] - origin[2];
            let radial = x.hypot(z);
            if radial <= radius * 1.05 && normal[1].abs() < 0.5 && radial > f32::EPSILON {
                *normal = [x / radial, 0.0, z / radial];
            }
        }
    }
    (positions, indices, result)
}

fn bounds(points: &[[f32; 3]]) -> Bounds {
    let mut result = Bounds {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    for point in points {
        for (axis, value) in point.iter().enumerate() {
            result.min[axis] = result.min[axis].min(*value);
            result.max[axis] = result.max[axis].max(*value);
        }
    }
    result
}

fn elliptical_loft(stations: &[([f32; 3], f32, f32)], segments: usize) -> RawMesh {
    let mut mesh = RawMesh::default();
    for (center, radius_x, radius_z) in stations {
        for segment in 0..segments {
            let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
            mesh.positions.push([
                center[0] + angle.cos() * radius_x,
                center[1],
                center[2] + angle.sin() * radius_z,
            ]);
        }
    }
    for station in 0..stations.len() - 1 {
        for segment in 0..segments {
            let next = (segment + 1) % segments;
            let a = (station * segments + segment) as u32;
            let b = (station * segments + next) as u32;
            let c = ((station + 1) * segments + next) as u32;
            let d = ((station + 1) * segments + segment) as u32;
            mesh.triangle(a, b, c);
            mesh.triangle(a, c, d);
        }
    }
    let bottom = mesh.positions.len() as u32;
    mesh.positions.push(stations[0].0);
    let top = mesh.positions.len() as u32;
    mesh.positions.push(stations[stations.len() - 1].0);
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        mesh.triangle(bottom, next as u32, segment as u32);
        let a = ((stations.len() - 1) * segments + segment) as u32;
        let b = ((stations.len() - 1) * segments + next) as u32;
        mesh.triangle(top, a, b);
    }
    mesh.orient_positive();
    mesh
}

fn holder_part(
    component_id: &str,
    material: MaterialClass,
    raw: RawMesh,
    crease_cosine: f32,
) -> MeshPart {
    let part_bounds = bounds(&raw.positions);
    let (positions, indices, normals) = shaded(&raw, crease_cosine, None);
    MeshPart {
        component_id: component_id.into(),
        material,
        positions,
        normals,
        indices,
        bounds: part_bounds,
    }
}

fn sheath_parts(
    blade: &MeshPart,
    design: &WeaponHolderDesign,
) -> Result<Vec<MeshPart>, GenerateError> {
    let mut layers = BTreeMap::<i32, Vec<[f32; 3]>>::new();
    for position in &blade.positions {
        layers
            .entry((position[1] * 1_000_000.0).round() as i32)
            .or_default()
            .push(*position);
    }
    if layers.len() < 3 {
        return Err(GenerateError::MissingHolderGeometry("blade"));
    }
    let mut stations = layers
        .into_values()
        .map(|points| {
            let layer = bounds(&points);
            let center = [
                (layer.min[0] + layer.max[0]) * 0.5,
                (layer.min[1] + layer.max[1]) * 0.5,
                (layer.min[2] + layer.max[2]) * 0.5,
            ];
            (
                center,
                ((layer.max[0] - layer.min[0]) * 0.5 + design.clearance.meters()).max(0.008),
                ((layer.max[2] - layer.min[2]) * 0.5 + design.clearance.meters() * 0.8).max(0.006),
            )
        })
        .collect::<Vec<_>>();
    stations.sort_by(|left, right| left.0[1].total_cmp(&right.0[1]));
    let mut throat = stations[0];
    throat.0[1] -= 0.004;
    stations.insert(0, throat);
    let mut tip = *stations.last().expect("blade layers");
    tip.0[1] += 0.008;
    tip.1 = (tip.1 * 0.82).max(0.007);
    tip.2 = (tip.2 * 0.82).max(0.005);
    stations.push(tip);

    let body = holder_part(
        "scabbard-body",
        design.body_material,
        elliptical_loft(&stations, 16),
        0.82,
    );
    let fitting = |name: &str, station: ([f32; 3], f32, f32), half: f32| {
        let lower = (
            [station.0[0], station.0[1] - half, station.0[2]],
            station.1 + 0.002,
            station.2 + 0.002,
        );
        let upper = (
            [station.0[0], station.0[1] + half, station.0[2]],
            station.1 + 0.002,
            station.2 + 0.002,
        );
        holder_part(
            name,
            design.fitting_material,
            elliptical_loft(&[lower, upper], 16),
            0.82,
        )
    };
    let throat_fitting = fitting(
        "scabbard-throat",
        stations[0],
        design.throat_length.meters() * 0.5,
    );
    let chape_index = stations.len().saturating_sub(2);
    let chape = fitting(
        "scabbard-chape",
        stations[chape_index],
        design.chape_length.meters() * 0.5,
    );
    let hanger_half_width = design.hanger_width.meters() * 0.5;
    let hanger_half_height = design.hanger_height.meters() * 0.5;
    let hanger_center = [
        stations[0].0[0] - stations[0].1 - hanger_half_width,
        stations[0].0[1] + hanger_half_height * 0.35,
        stations[0].0[2],
    ];
    let hanger_centers = (0..28)
        .map(|index| {
            let angle = index as f32 / 28.0 * std::f32::consts::TAU;
            [
                hanger_center[0] + angle.cos() * hanger_half_width,
                hanger_center[1] + angle.sin() * hanger_half_height,
                hanger_center[2],
            ]
        })
        .collect::<Vec<_>>();
    let suspension = holder_part(
        "scabbard-suspension",
        design.body_material,
        tube_centers(&hanger_centers, design.loop_bar_radius.meters(), 10, true),
        0.65,
    );
    Ok(vec![body, throat_fitting, chape, suspension])
}

fn haft_loop_parts(grip: &MeshPart, design: &WeaponHolderDesign) -> Vec<MeshPart> {
    let center_x = (grip.bounds.min[0] + grip.bounds.max[0]) * 0.5;
    let center_z = (grip.bounds.min[2] + grip.bounds.max[2]) * 0.5;
    let grip_length = grip.bounds.max[1] - grip.bounds.min[1];
    let y = grip.bounds.min[1] + grip_length * design.loop_position.unit();
    let radius_x = (grip.bounds.max[0] - grip.bounds.min[0]) * 0.5 + design.clearance.meters();
    let radius_z = (grip.bounds.max[2] - grip.bounds.min[2]) * 0.5 + design.clearance.meters();
    let ring_centers = (0..24)
        .map(|index| {
            let angle = index as f32 / 24.0 * std::f32::consts::TAU;
            [
                center_x + angle.cos() * radius_x,
                y,
                center_z + angle.sin() * radius_z,
            ]
        })
        .collect::<Vec<_>>();
    let hanger_half_width = design.hanger_width.meters() * 0.5;
    let hanger_half_height = design.hanger_height.meters() * 0.5;
    let hanger_center = [center_x - radius_x - hanger_half_width, y, center_z];
    let hanger_centers = (0..28)
        .map(|index| {
            let angle = index as f32 / 28.0 * std::f32::consts::TAU;
            [
                hanger_center[0] + angle.cos() * hanger_half_width,
                hanger_center[1] + angle.sin() * hanger_half_height,
                hanger_center[2],
            ]
        })
        .collect::<Vec<_>>();
    vec![
        holder_part(
            "haft-frog",
            design.body_material,
            tube_centers(&ring_centers, design.loop_bar_radius.meters(), 10, true),
            0.65,
        ),
        holder_part(
            "belt-loop",
            design.body_material,
            tube_centers(&hanger_centers, design.loop_bar_radius.meters(), 10, true),
            0.65,
        ),
    ]
}

fn resolve_origin<'a>(
    component: &'a ComponentDesign,
    by_id: &HashMap<&'a str, &'a ComponentDesign>,
    cache: &mut HashMap<&'a str, [f32; 3]>,
    visiting: &mut Vec<&'a str>,
) -> Result<[f32; 3], GenerateError> {
    if let Some(origin) = cache.get(component.id.as_str()) {
        return Ok(*origin);
    }
    if visiting.contains(&component.id.as_str()) {
        return Err(GenerateError::UnresolvedAttachment(component.id.clone()));
    }
    visiting.push(&component.id);
    let mut origin = component.offset.meters();
    if let Attachment::TopOf {
        component: parent,
        insertion,
    } = &component.attachment
    {
        let parent = by_id
            .get(parent.as_str())
            .ok_or_else(|| GenerateError::UnresolvedAttachment(component.id.clone()))?;
        let parent_origin = resolve_origin(parent, by_id, cache, visiting)?;
        origin[0] += parent_origin[0];
        origin[1] += parent_origin[1] + parent.shape.axial_length().meters() - insertion.meters();
        origin[2] += parent_origin[2];
    }
    visiting.pop();
    cache.insert(&component.id, origin);
    Ok(origin)
}

pub fn generate(design: &WeaponDesign) -> Result<GeneratedWeapon, GenerateError> {
    validate(design).map_err(GenerateError::Invalid)?;
    let derived = derive_properties(design).map_err(GenerateError::Invalid)?;
    let by_id: HashMap<_, _> = design
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut origins = HashMap::new();
    for component in &design.components {
        resolve_origin(component, &by_id, &mut origins, &mut Vec::new())?;
    }
    let mut parts = Vec::new();
    let mut anchors = Vec::new();
    let mut all_positions = Vec::new();
    for component in &design.components {
        let mut raw = match &component.shape {
            ComponentShape::Cylinder(value) => frustum(
                value.length.meters(),
                value.radius.meters() * value.bottom_scale.unit(),
                value.radius.meters() * value.top_scale.unit(),
                value.segments.0,
            ),
            ComponentShape::OvalGrip(value) => elliptical_frustum(
                value.length.meters(),
                value.width.meters() * value.bottom_scale.unit() * 0.5,
                value.width.meters() * value.top_scale.unit() * 0.5,
                value.thickness.meters() / value.width.meters(),
                value.segments.0,
            ),
            ComponentShape::Blade(value) => blade(value),
            ComponentShape::Guard(value) => guard(value),
            ComponentShape::Mace(value) => mace(value),
            ComponentShape::Socket(value) => socket(value),
            ComponentShape::Langet(value) => box_mesh(
                [0.0, value.length.meters() / 2.0, 0.0],
                [
                    value.width.meters(),
                    value.length.meters(),
                    value.thickness.meters(),
                ],
                0.0,
            ),
            ComponentShape::Axe(value) => axe(value),
            ComponentShape::HammerPoll(value) => hammer(value),
            ComponentShape::CurvedBeak(value) => curved_beak(value),
            ComponentShape::FacetedBeak(value) => faceted_beak(value),
            ComponentShape::Glaive(value) => glaive(value),
            ComponentShape::Bill(value) => bill(value),
            ComponentShape::Fork(value) => fork(value),
            ComponentShape::Partisan(value) => partisan(value),
            ComponentShape::TubePath(value) => tube_path(value),
            ComponentShape::RingGuard(value) => ring(value),
            ComponentShape::FigureEight(value) => figure_eight(value),
            ComponentShape::FanPommel(value) => fan_pommel(value),
            ComponentShape::Rondel(value) => cylinder(
                value.thickness.meters(),
                value.radius.meters(),
                value.segments.0,
            ),
            ComponentShape::GothicMace(value) => gothic_mace(value),
            ComponentShape::SlabGrip(value) => box_mesh(
                [0.0, value.length.meters() / 2.0, 0.0],
                [
                    value.width.meters(),
                    value.length.meters(),
                    value.thickness.meters() + value.scale_thickness.meters() * 2.0,
                ],
                0.0,
            ),
            ComponentShape::KnuckleBow(value) => {
                let count = value.samples.0 as usize;
                let centers: Vec<_> = (0..=count)
                    .map(|index| {
                        let t = index as f32 / count as f32;
                        let arch = (std::f32::consts::PI * t).sin();
                        [
                            value.side as f32
                                * value.width.meters()
                                * arch
                                * (1.0 + value.bulge.unit() * 0.25 * arch),
                            value.length.meters() * t,
                            0.0,
                        ]
                    })
                    .collect();
                tube_centers(
                    &centers,
                    value.bar.meters(),
                    value.radial_segments.0 as usize,
                    false,
                )
            }
            ComponentShape::Collar(value) => cylinder(
                value.width.meters(),
                value.radius.meters(),
                value.segments.0,
            ),
            ComponentShape::Sleeve(value) => socket(&SocketSpec {
                length: value.length,
                outer_radius: value.radius,
                top_radius: value.top_radius,
                wall: value.wall,
                segments: value.segments,
            }),
            ComponentShape::Boss(value) => {
                let mut mesh = cylinder(
                    value.thickness.meters(),
                    value.radius.meters(),
                    value.segments.0,
                );
                for point in &mut mesh.positions {
                    [point[1], point[2]] = [point[2], point[1]];
                }
                mesh.orient_positive();
                mesh
            }
            ComponentShape::Spear(value) => spear(value),
            ComponentShape::ProfiledPommel(value) => profiled_pommel(value),
        };
        let origin = origins[component.id.as_str()];
        raw.translate(origin);
        let part_bounds = bounds(&raw.positions);
        let crease = match &component.shape {
            ComponentShape::Cylinder(_)
            | ComponentShape::OvalGrip(_)
            | ComponentShape::Socket(_)
            | ComponentShape::Guard(_)
            | ComponentShape::TubePath(_)
            | ComponentShape::RingGuard(_)
            | ComponentShape::FigureEight(_)
            | ComponentShape::FanPommel(_)
            | ComponentShape::Rondel(_)
            | ComponentShape::KnuckleBow(_)
            | ComponentShape::Collar(_)
            | ComponentShape::Sleeve(_)
            | ComponentShape::ProfiledPommel(_) => 0.65,
            ComponentShape::Blade(_) => 0.98,
            _ => 0.9999,
        };
        let smooth_core = match &component.shape {
            ComponentShape::GothicMace(value) => Some((origin, value.root_radius.meters())),
            ComponentShape::Mace(value) => Some((origin, value.core_radius.meters())),
            _ => None,
        };
        let (positions, indices, normals) = shaded(&raw, crease, smooth_core);
        let top = [
            origin[0],
            origin[1] + component.shape.axial_length().meters(),
            origin[2],
        ];
        anchors.push(Anchor {
            name: format!("{}.base", component.id),
            position: origin,
        });
        anchors.push(Anchor {
            name: format!("{}.top", component.id),
            position: top,
        });
        if component.role == ComponentRole::Grip {
            let position = [origin[0], (origin[1] + top[1]) / 2.0, origin[2]];
            anchors.push(Anchor {
                name: "weapon.grip".into(),
                position,
            });
        }
        all_positions.extend_from_slice(&positions);
        parts.push(MeshPart {
            component_id: component.id.clone(),
            material: component.material,
            normals,
            positions,
            indices,
            bounds: part_bounds,
        });
    }
    let overall = bounds(&all_positions);
    let tip = *all_positions
        .iter()
        .max_by(|left, right| left[1].total_cmp(&right[1]))
        .expect("validated design has geometry");
    anchors.push(Anchor {
        name: "weapon.tip".into(),
        position: tip,
    });
    Ok(GeneratedWeapon {
        design_hash: design_hash(design),
        parts,
        bounds: overall,
        anchors,
        derived,
    })
}

/// Generate the body-mounted fixture recommended for this weapon chassis.
/// Holder coordinates intentionally share the weapon's grip frame so a
/// contained weapon and its parent holder align without durable transform
/// state or client-side geometric inference.
pub fn generate_holder(
    design: &WeaponHolderDesign,
) -> Result<GeneratedWeaponHolder, GenerateError> {
    validate_holder(design).map_err(GenerateError::Invalid)?;
    let weapon = generate(&design.fitted_weapon)?;
    let grip = weapon
        .anchors
        .iter()
        .find(|anchor| anchor.name == "weapon.grip")
        .ok_or(GenerateError::MissingHolderGeometry("grip"))?
        .position;
    let parts = match design.kind {
        WeaponHolderKind::BladeSheath => {
            let blade_id = design
                .fitted_weapon
                .components
                .iter()
                .find(|component| matches!(&component.shape, ComponentShape::Blade(_)))
                .map(|component| component.id.as_str())
                .ok_or(GenerateError::MissingHolderGeometry("blade"))?;
            let blade = weapon
                .parts
                .iter()
                .find(|part| part.component_id == blade_id)
                .ok_or(GenerateError::MissingHolderGeometry("blade"))?;
            sheath_parts(blade, design)?
        }
        WeaponHolderKind::HaftLoop => {
            let grip_id = design
                .fitted_weapon
                .components
                .iter()
                .find(|component| component.role == ComponentRole::Grip)
                .map(|component| component.id.as_str())
                .ok_or(GenerateError::MissingHolderGeometry("grip"))?;
            let grip_part = weapon
                .parts
                .iter()
                .find(|part| part.component_id == grip_id)
                .ok_or(GenerateError::MissingHolderGeometry("grip"))?;
            haft_loop_parts(grip_part, design)
        }
    };
    let all_positions = parts
        .iter()
        .flat_map(|part| part.positions.iter().copied())
        .collect::<Vec<_>>();
    Ok(GeneratedWeaponHolder {
        design_hash: holder_design_hash(design),
        kind: design.kind,
        grip,
        bounds: bounds(&all_positions),
        parts,
        derived: derive_holder_properties(design).map_err(GenerateError::Invalid)?,
    })
}
