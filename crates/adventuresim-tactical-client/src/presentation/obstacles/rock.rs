use super::super::*;

pub(in crate::presentation) fn procedural_rock_mesh(seed: u64) -> Mesh {
    let mut mesh = Sphere::new(ROCK_RADIUS_METRES)
        .mesh()
        .ico(2)
        .expect("valid procedural rock seed mesh");
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions {
            let point = Vec3::from_array(*position);
            let direction = point.normalize_or_zero();
            let phase = direction.x * 4.7
                + direction.y * 6.1
                + direction.z * 5.3
                + unit_hash(seed) * core::f32::consts::TAU;
            let radius = ROCK_RADIUS_METRES * (0.82 + 0.12 * phase.sin());
            *position = Vec3::new(
                direction.x * radius,
                direction.y * radius * 0.78,
                direction.z * radius,
            )
            .to_array();
        }
    }
    mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
    mesh.with_computed_area_weighted_normals()
}

#[derive(Component)]
pub(crate) struct ProceduralRockVisual;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_rocks_remain_inside_the_authoritative_sphere() {
        for seed in [0, 1, 42, u64::MAX] {
            let mesh = procedural_rock_mesh(seed);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .unwrap();
            assert!(positions.iter().all(|position| {
                Vec3::from_array(*position).length() <= ROCK_RADIUS_METRES + 0.001
            }));
        }
    }
}
