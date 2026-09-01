use bevy::math::{Vec2, Vec3};
use geo::{BooleanOps, Coord, LineString, MultiPolygon, Polygon};

use super::{
    BuildingLod, BuildingLodMaterial, FacadeRun, FacadeRunPath, LodMesh, TEXTURE_REPEAT_METRES,
    append_round_wall, plan_vertex,
};

const ENVELOPE_GROUP_TOLERANCE_METRES: f32 = 0.001;

struct WallEnvelopeGroup {
    material: crate::WallMaterialClass,
    base_elevation_metres: f32,
    height_metres: f32,
    footprint: MultiPolygon<f32>,
}

impl WallEnvelopeGroup {
    fn accepts(&self, run: &FacadeRun) -> bool {
        self.material == run.material
            && (self.base_elevation_metres - run.base_elevation_metres).abs()
                <= ENVELOPE_GROUP_TOLERANCE_METRES
            && (self.height_metres - run.height_metres).abs() <= ENVELOPE_GROUP_TOLERANCE_METRES
    }

    fn insert(&mut self, polygon: Polygon<f32>) {
        self.footprint = if self.footprint.0.is_empty() {
            MultiPolygon(vec![polygon])
        } else {
            self.footprint.union(&polygon)
        };
    }
}

pub(super) fn append_wall_envelopes(lod: &mut BuildingLod) {
    let mut groups: Vec<WallEnvelopeGroup> = Vec::new();
    let runs = lod.facade_runs.clone();
    for run in &runs {
        let FacadeRunPath::Straight { .. } = run.path else {
            let material = wall_lod_material(lod.level, run.material);
            append_round_wall(lod.mesh_mut(material), run);
            continue;
        };
        let polygon = wall_footprint(run);
        if let Some(group) = groups.iter_mut().find(|group| group.accepts(run)) {
            group.insert(polygon);
        } else {
            groups.push(WallEnvelopeGroup {
                material: run.material,
                base_elevation_metres: run.base_elevation_metres,
                height_metres: run.height_metres,
                footprint: MultiPolygon(vec![polygon]),
            });
        }
    }

    for group in groups {
        let material = wall_lod_material(lod.level, group.material);
        let mesh = lod.mesh_mut(material);
        for polygon in &group.footprint.0 {
            append_extruded_polygon(
                mesh,
                polygon,
                group.base_elevation_metres,
                group.base_elevation_metres + group.height_metres,
            );
        }
    }
}

fn wall_lod_material(
    level: crate::BuildingLodLevel,
    material: crate::WallMaterialClass,
) -> BuildingLodMaterial {
    if level == crate::BuildingLodLevel::Shell && material == crate::WallMaterialClass::TimberInfill
    {
        BuildingLodMaterial::FachwerkBaked
    } else {
        BuildingLodMaterial::Wall(material)
    }
}

fn wall_footprint(run: &FacadeRun) -> Polygon<f32> {
    let FacadeRunPath::Straight {
        start,
        end,
        outward,
    } = run.path
    else {
        unreachable!("only straight runs form planar wall footprints");
    };
    let tangent = (end - start).normalize_or_zero();
    let overlap = tangent * run.thickness_metres * 0.5;
    let depth = outward.normalize_or_zero() * run.thickness_metres * 0.5;
    polygon([
        start - overlap + depth,
        end + overlap + depth,
        end + overlap - depth,
        start - overlap - depth,
    ])
}

fn polygon(points: [Vec2; 4]) -> Polygon<f32> {
    let mut coordinates = points
        .into_iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    coordinates.push(coordinates[0]);
    Polygon::new(LineString::new(coordinates), Vec::new())
}

fn append_extruded_polygon(mesh: &mut LodMesh, polygon: &Polygon<f32>, bottom: f32, top: f32) {
    append_ring(mesh, polygon.exterior(), bottom, top, false);
    for ring in polygon.interiors() {
        append_ring(mesh, ring, bottom, top, true);
    }
    append_caps(mesh, polygon, bottom, top);
}

fn append_ring(mesh: &mut LodMesh, ring: &LineString<f32>, bottom: f32, top: f32, hole: bool) {
    let points = ring_points(ring);
    let empty_side = signed_area(&points).signum() * if hole { -1.0 } else { 1.0 };
    let mut distance = 0.0;
    for index in 0..points.len() {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        let edge = end - start;
        if edge.length_squared() <= f32::EPSILON {
            continue;
        }
        let next_distance = distance + edge.length();
        let outward = Vec2::new(edge.y, -edge.x).normalize_or_zero() * empty_side;
        let positions = [
            plan_vertex(start, bottom),
            plan_vertex(end, bottom),
            plan_vertex(end, top),
            plan_vertex(start, top),
        ];
        mesh.push_quad(
            positions,
            Vec3::new(outward.x, 0.0, outward.y),
            [
                Vec2::new(distance, bottom) / TEXTURE_REPEAT_METRES,
                Vec2::new(next_distance, bottom) / TEXTURE_REPEAT_METRES,
                Vec2::new(next_distance, top) / TEXTURE_REPEAT_METRES,
                Vec2::new(distance, top) / TEXTURE_REPEAT_METRES,
            ],
        );
        distance = next_distance;
    }
}

fn append_caps(mesh: &mut LodMesh, polygon: &Polygon<f32>, bottom: f32, top: f32) {
    let mut points = ring_points(polygon.exterior());
    let mut holes = Vec::new();
    for interior in polygon.interiors() {
        holes.push(points.len() as u32);
        points.extend(ring_points(interior));
    }
    let mut triangles = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        points.iter().map(|point| [point.x, point.y]),
        &holes,
        &mut triangles,
    );
    for triangle in triangles.as_chunks::<3>().0 {
        let top_positions = triangle.map(|index| plan_vertex(points[index as usize], top));
        mesh.push_triangle(top_positions, Vec3::Y, top_positions.map(plan_uv));
        let bottom_positions = triangle.map(|index| plan_vertex(points[index as usize], bottom));
        mesh.push_triangle(bottom_positions, -Vec3::Y, bottom_positions.map(plan_uv));
    }
}

fn ring_points(ring: &LineString<f32>) -> Vec<Vec2> {
    let mut points = ring
        .0
        .iter()
        .map(|coordinate| Vec2::new(coordinate.x, coordinate.y))
        .collect::<Vec<_>>();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

fn signed_area(points: &[Vec2]) -> f32 {
    (0..points.len())
        .map(|index| points[index].perp_dot(points[(index + 1) % points.len()]))
        .sum::<f32>()
        * 0.5
}

fn plan_uv(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z) / TEXTURE_REPEAT_METRES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WallAssemblyId, WallMaterialClass, audit_triangle_mesh};

    #[test]
    fn perpendicular_wall_runs_form_one_watertight_mitered_envelope() {
        let run = |start, end, outward, id| FacadeRun {
            material: WallMaterialClass::TimberInfill,
            storey_level: 0,
            path: FacadeRunPath::Straight {
                start,
                end,
                outward,
            },
            base_elevation_metres: 0.0,
            height_metres: 3.0,
            thickness_metres: 0.2,
            source_walls: vec![WallAssemblyId(id)],
        };
        let mut lod = BuildingLod {
            level: crate::BuildingLodLevel::Facade,
            facade_runs: vec![
                run(Vec2::ZERO, Vec2::new(2.0, 0.0), -Vec2::Y, 1),
                run(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0), Vec2::X, 2),
            ],
            meshes: Vec::new(),
        };

        append_wall_envelopes(&mut lod);
        let mesh = &lod.meshes[0];
        let report = audit_triangle_mesh(
            &mesh
                .vertices
                .iter()
                .map(|vertex| vertex.position.to_array())
                .collect::<Vec<_>>(),
            &mesh.indices,
        );
        assert!(report.passes_closed_solid(), "{report:?}");
        assert!(
            mesh.vertices
                .iter()
                .any(|vertex| { vertex.position.distance(Vec3::new(2.1, 0.0, -0.1)) < 0.0001 })
        );
    }
}
