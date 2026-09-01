//! Batched presentation meshes for city streets and developed block interiors.

use super::*;

const SAMPLE_SPACING_METRES: f32 = 4.0;
const SURFACE_LIFT_METRES: f32 = 0.035;
const SURFACE_PRIORITY_LIFT_METRES: f32 = 0.004;
const YARD_SURFACE_LIFT_METRES: f32 = 0.024;

#[derive(Clone, Copy)]
pub(super) struct UrbanGround<'a> {
    streets: &'a [CityStreetPatch],
    yards: &'a [CityYardPatch],
}

impl<'a> UrbanGround<'a> {
    pub(super) const fn new(streets: &'a [CityStreetPatch], yards: &'a [CityYardPatch]) -> Self {
        Self { streets, yards }
    }

    pub(super) fn suppresses_grass(self, point: Vec2) -> bool {
        self.streets.iter().any(|street| street.contains(point))
            || self.yards.iter().any(|yard| yard.contains(point))
    }
}

#[derive(Component)]
pub(crate) struct CityStreetPresentation;

#[derive(Component)]
pub(crate) struct CityYardPresentation;

#[derive(Default)]
struct CitySurfaceMeshBuilder {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl CitySurfaceMeshBuilder {
    fn append(
        &mut self,
        street: CityStreetPatch,
        vista: &ActiveVistaSurface,
        scene_digest: &str,
        terrain: &SceneTerrain,
    ) -> Option<()> {
        let vertex_offset = self.positions.len() as u32;
        let surface_lift = SURFACE_LIFT_METRES
            + f32::from(street.surface().priority()) * SURFACE_PRIORITY_LIFT_METRES;
        let height = |point: Vec2| {
            vista
                .presented_height_at(scene_digest, terrain, point)
                .map(|height| height + surface_lift)
        };
        match street {
            CityStreetPatch::Corridor {
                start_metres,
                end_metres,
                half_width_metres,
                ..
            } => {
                let displacement = end_metres - start_metres;
                let length = displacement.length();
                let tangent = displacement / length;
                let normal = Vec2::new(-tangent.y, tangent.x) * half_width_metres;
                let steps = (length / SAMPLE_SPACING_METRES).ceil() as usize;
                for step in 0..=steps {
                    let fraction = step as f32 / steps as f32;
                    let centre = start_metres + displacement * fraction;
                    for (side, point) in [centre - normal, centre + normal].into_iter().enumerate()
                    {
                        self.positions.push([point.x, height(point)?, point.y]);
                        self.uvs.push([side as f32, length * fraction / 2.0]);
                    }
                }
                for step in 0..steps as u32 {
                    let first = vertex_offset + step * 2;
                    self.indices.extend_from_slice(&[
                        first,
                        first + 2,
                        first + 1,
                        first + 1,
                        first + 2,
                        first + 3,
                    ]);
                }
            }
            CityStreetPatch::Market { corners_metres, .. } => {
                for point in corners_metres {
                    self.positions.push([point.x, height(point)?, point.y]);
                }
                self.uvs
                    .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
                self.indices.extend_from_slice(&[
                    vertex_offset,
                    vertex_offset + 2,
                    vertex_offset + 1,
                    vertex_offset,
                    vertex_offset + 3,
                    vertex_offset + 2,
                ]);
            }
        }
        Some(())
    }

    fn append_yard(
        &mut self,
        yard: CityYardPatch,
        vista: &ActiveVistaSurface,
        scene_digest: &str,
        terrain: &SceneTerrain,
    ) -> Option<()> {
        let vertex_offset = self.positions.len() as u32;
        for point in yard.corners_metres {
            let height =
                vista.presented_height_at(scene_digest, terrain, point)? + YARD_SURFACE_LIFT_METRES;
            self.positions.push([point.x, height, point.y]);
        }
        self.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 2,
            vertex_offset + 1,
            vertex_offset,
            vertex_offset + 3,
            vertex_offset + 2,
        ]);
        Some(())
    }

    fn build(self) -> Option<Mesh> {
        if self.indices.is_empty() {
            return None;
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone());
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![[0.0, 1.0, 0.0]; self.positions.len()],
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        Some(mesh)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "street presentation consumes the shared city geometry and active terrain surfaces"
)]
pub(super) fn spawn(
    commands: &mut Commands,
    streets: &[CityStreetPatch],
    yards: &[CityYardPatch],
    vista: &ActiveVistaSurface,
    scene_digest: &str,
    terrain: &SceneTerrain,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut builders = [
        CitySurfaceMeshBuilder::default(),
        CitySurfaceMeshBuilder::default(),
        CitySurfaceMeshBuilder::default(),
    ];
    for street in streets.iter().copied() {
        let _ = builders[usize::from(street.surface().priority())].append(
            street,
            vista,
            scene_digest,
            terrain,
        );
    }
    let mut yard_builders = [
        CitySurfaceMeshBuilder::default(),
        CitySurfaceMeshBuilder::default(),
    ];
    for yard in yards.iter().copied() {
        let index = match yard.surface {
            CityYardSurface::PackedEarth => 0,
            CityYardSurface::KitchenGarden => 1,
        };
        let _ = yard_builders[index].append_yard(yard, vista, scene_digest, terrain);
    }
    for (index, (builder, color)) in yard_builders
        .into_iter()
        .zip([Color::srgb_u8(91, 69, 45), Color::srgb_u8(77, 75, 43)])
        .enumerate()
    {
        let Some(mesh) = builder.build() else {
            continue;
        };
        commands.spawn((
            Name::new(format!("Batched city yard surface {index}")),
            VistaTerrain(0),
            CityYardPresentation,
            NotShadowCaster,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.98,
                reflectance: 0.05,
                ..default()
            })),
            Transform::default(),
        ));
    }
    for (index, (builder, color)) in builders
        .into_iter()
        .zip([
            Color::srgb_u8(85, 68, 48),
            Color::srgb_u8(105, 101, 88),
            Color::srgb_u8(92, 91, 84),
        ])
        .enumerate()
    {
        let Some(mesh) = builder.build() else {
            continue;
        };
        let material = materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.96,
            reflectance: 0.08,
            ..default()
        });
        commands.spawn((
            Name::new(format!("Batched city street surface {index}")),
            VistaTerrain(0),
            CityStreetPresentation,
            NotShadowCaster,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::default(),
        ));
    }
}
