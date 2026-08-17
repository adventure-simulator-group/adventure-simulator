use super::super::*;

const BOULDER_GRID_SAMPLES: usize = 18;

fn rock_field(recipe: RockRecipe, point: Vec3) -> f32 {
    let dimensions = Vec3::from_array(recipe.dimensions_metres());
    let half_extents = dimensions * 0.5;
    let normalized = point / half_extents;
    let phase = unit_hash(recipe.seed) * core::f32::consts::TAU;
    let (sin_phase, cos_phase) = phase.sin_cos();
    let oriented = Vec3::new(
        normalized.x * cos_phase - normalized.z * sin_phase,
        normalized.y,
        normalized.x * sin_phase + normalized.z * cos_phase,
    );
    let broad = (normalized.dot(Vec3::new(2.73, 1.91, 2.27)) + phase).sin();
    let cross = (normalized.dot(Vec3::new(-1.37, 3.11, 1.73)) - phase * 0.61).sin();
    let radius = 0.84 + broad * 0.055 + cross * 0.035;
    let shape = match recipe.archetype {
        RockArchetype::Rounded => {
            let softly_faceted = oriented.abs().dot(Vec3::new(0.48, 0.58, 0.46)) / 0.88;
            normalized.length().lerp(softly_faceted, 0.17)
        }
        RockArchetype::Angular => {
            let primary = oriented.abs().max_element() * 0.72 + normalized.length() * 0.28;
            let fracture = oriented.dot(Vec3::new(0.61, 0.27, -0.74)).abs() / 0.94;
            primary.max(fracture * 0.92)
        }
        RockArchetype::Slab => {
            let bedded = (oriented.x.abs().powi(6)
                + (oriented.y * 1.13).abs().powi(4)
                + oriented.z.abs().powi(6))
            .powf(1.0 / 6.0);
            let cleaved = oriented.dot(Vec3::new(-0.72, 0.18, 0.66)).abs() / 0.96;
            bedded.max(cleaved * 0.9)
        }
    };
    let ground_contact = -normalized.y
        - match recipe.archetype {
            RockArchetype::Rounded => 0.63,
            RockArchetype::Angular => 0.69,
            RockArchetype::Slab => 0.74,
        };
    let side_chip = match recipe.archetype {
        RockArchetype::Rounded => oriented.dot(Vec3::new(0.78, 0.31, 0.54)) - 0.73,
        RockArchetype::Angular => oriented.dot(Vec3::new(-0.55, 0.43, 0.71)) - 0.57,
        RockArchetype::Slab => oriented.dot(Vec3::new(0.64, 0.12, -0.76)) - 0.66,
    };
    // The procedural render surface is always contained by the server's
    // conservative sphere proxy, irrespective of archetype or field detail.
    let proxy_bound = point.length() / recipe.collision_radius_metres() - 0.94;
    (shape - radius)
        .max(ground_contact)
        .max(side_chip)
        .max(proxy_bound)
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

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalRockExtension {
    #[texture(100)]
    #[sampler(101)]
    diffuse: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    normal_gl: Handle<Image>,
    #[texture(104)]
    #[sampler(105)]
    arm: Handle<Image>,
    /// Linear lithology multiplier and perceptual roughness bias.
    #[uniform(106)]
    surface: Vec4,
    /// Seed phase, tiles per metre, macro strength, and normal strength.
    #[uniform(106)]
    geology: Vec4,
}

impl MaterialExtension for TacticalRockExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/tactical_rock.wgsl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/tactical_rock.wgsl".into()
    }
}

pub(in crate::presentation) type TacticalRockMaterial =
    ExtendedMaterial<StandardMaterial, TacticalRockExtension>;

pub(in crate::presentation) fn rock_material(
    recipe: RockRecipe,
    assets: &ProceduralEnvironmentAssets,
) -> TacticalRockMaterial {
    rock_material_with_textures(
        recipe,
        assets.rock.albedo.clone(),
        assets.rock.normal_gl.clone(),
        assets.rock.arm.clone(),
    )
}

fn rock_material_with_textures(
    recipe: RockRecipe,
    diffuse: Handle<Image>,
    normal_gl: Handle<Image>,
    arm: Handle<Image>,
) -> TacticalRockMaterial {
    let (tint, roughness_bias, macro_strength, normal_strength) = match recipe.lithology {
        RockLithology::Granite => (Vec3::new(0.78, 0.82, 0.87), 0.0, 0.08, 0.42),
        RockLithology::Limestone => (Vec3::new(1.08, 1.05, 0.82), 0.06, 0.06, 0.36),
        RockLithology::Sandstone => (Vec3::new(1.15, 0.77, 0.55), 0.09, 0.09, 0.32),
    };
    TacticalRockMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalRockExtension {
            diffuse,
            normal_gl,
            arm,
            surface: tint.extend(roughness_bias),
            geology: Vec4::new(
                unit_hash(recipe.seed) * core::f32::consts::TAU,
                0.5,
                macro_strength,
                normal_strength,
            ),
        },
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

    #[test]
    fn rock_fields_have_asymmetric_ground_contact_and_archetype_silhouettes() {
        for archetype in [
            RockArchetype::Rounded,
            RockArchetype::Angular,
            RockArchetype::Slab,
        ] {
            let recipe = recipe(17, archetype);
            let radius = recipe.collision_radius_metres();
            assert!(rock_field(recipe, Vec3::new(0.0, -radius, 0.0)) > 0.0);
            assert!(rock_field(recipe, Vec3::ZERO) < 0.0);
        }
        let point = Vec3::new(0.45, 0.28, -0.51);
        let values = [
            RockArchetype::Rounded,
            RockArchetype::Angular,
            RockArchetype::Slab,
        ]
        .map(|archetype| rock_field(recipe(17, archetype), point).to_bits());
        assert_ne!(values[0], values[1]);
        assert_ne!(values[1], values[2]);
    }

    #[test]
    fn geological_material_is_dielectric_bounded_and_lithology_specific() {
        let mut surfaces = BTreeSet::new();
        for lithology in [
            RockLithology::Granite,
            RockLithology::Limestone,
            RockLithology::Sandstone,
        ] {
            let mut recipe = recipe(42, RockArchetype::Angular);
            recipe.lithology = lithology;
            let material = rock_material_with_textures(
                recipe,
                Handle::default(),
                Handle::default(),
                Handle::default(),
            );
            assert_eq!(material.base.base_color, Color::WHITE);
            assert_eq!(material.base.metallic, 0.0);
            assert_eq!(material.base.perceptual_roughness, 1.0);
            assert!((0.0..=0.1).contains(&material.extension.surface.w));
            assert!((0.3..=0.45).contains(&material.extension.geology.w));
            surfaces.insert(material.extension.surface.to_array().map(f32::to_bits));
        }
        assert_eq!(surfaces.len(), 3);
    }

    #[test]
    fn rock_shader_keeps_matched_bounded_pbr_detail() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_rock.wgsl"
        ));
        assert!(shader.contains("pbr_input.world_normal = composed_normal"));
        assert!(shader.contains("pbr_input.N = composed_normal"));
        assert!(shader.contains("perceptual_roughness = clamp"));
        assert!(shader.contains("triplanar_weights"));
        assert!(shader.contains("textureSample(rock_diffuse"));
        assert!(shader.contains("textureSample(rock_normal_gl"));
        assert!(shader.contains("textureSample(rock_arm"));
        assert!(!shader.contains("emissive"));
    }
}
