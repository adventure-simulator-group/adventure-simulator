use super::super::*;

const BOULDER_GRID_SAMPLES: usize = 18;

fn rock_field(recipe: RockRecipe, point: Vec3) -> f32 {
    let dimensions = Vec3::from_array(recipe.dimensions_metres());
    let half_extents = dimensions * 0.5;
    let normalized = point / half_extents;
    let phase = unit_hash(recipe.seed) * core::f32::consts::TAU;
    let broad = (normalized.dot(Vec3::new(2.73, 1.91, 2.27)) + phase).sin();
    let cross = (normalized.dot(Vec3::new(-1.37, 3.11, 1.73)) - phase * 0.61).sin();
    let radius = 0.84 + broad * 0.055 + cross * 0.035;
    let shape = match recipe.archetype {
        RockArchetype::Rounded => normalized.length(),
        RockArchetype::Angular => {
            normalized.abs().max_element() * 0.58 + normalized.length() * 0.42
        }
        RockArchetype::Slab => {
            (normalized.x.abs().powi(4) + normalized.y.abs().powi(4) + normalized.z.abs().powi(4))
                .sqrt()
                .sqrt()
        }
    };
    // The procedural render surface is always contained by the server's
    // conservative sphere proxy, irrespective of archetype or field detail.
    let proxy_bound = point.length() / recipe.collision_radius_metres() - 0.94;
    (shape - radius).max(proxy_bound)
}

pub(in crate::presentation) fn procedural_rock_mesh(recipe: RockRecipe) -> Mesh {
    let radius = recipe.collision_radius_metres();
    extract_surface_nets(
        SurfaceNetsGrid {
            sample_counts: [BOULDER_GRID_SAMPLES; 3],
            minimum: Vec3::splat(-radius),
            maximum: Vec3::splat(radius),
        },
        |point| rock_field(recipe, point),
    )
    .expect("bounded rock recipe produces a finite client field")
    .into_mesh()
}

pub(in crate::presentation) fn rock_color(lithology: RockLithology) -> Color {
    match lithology {
        RockLithology::Granite => Color::srgb_u8(112, 108, 104),
        RockLithology::Limestone => Color::srgb_u8(151, 146, 126),
        RockLithology::Sandstone => Color::srgb_u8(151, 112, 78),
    }
}

#[derive(Component)]
pub(crate) struct ProceduralRockVisual;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn recipe(seed: u64, archetype: RockArchetype) -> RockRecipe {
        RockRecipe {
            seed,
            archetype,
            lithology: RockLithology::Granite,
            dimensions_cm: match archetype {
                RockArchetype::Rounded => [128, 104, 120],
                RockArchetype::Angular => [136, 112, 124],
                RockArchetype::Slab => [142, 72, 132],
            },
            collision_radius_cm: 75,
        }
    }

    #[test]
    fn procedural_rocks_are_deterministic_distinct_and_inside_the_proxy() {
        let mut signatures = BTreeSet::new();
        for (seed, archetype) in [
            (0, RockArchetype::Rounded),
            (1, RockArchetype::Angular),
            (42, RockArchetype::Slab),
        ] {
            let recipe = recipe(seed, archetype);
            let mesh = procedural_rock_mesh(recipe);
            let repeated = procedural_rock_mesh(recipe);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .unwrap();
            let repeated_positions = repeated
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .unwrap();
            assert_eq!(positions, repeated_positions);
            assert!(positions.iter().all(|position| {
                Vec3::from_array(*position).length() <= recipe.collision_radius_metres() + 0.001
            }));
            signatures.insert((positions.len(), mesh.indices().unwrap().len()));
        }
        assert!(signatures.len() >= 2);
    }
}
