use adventuresim_building_generator::{
    BuildingLodLevel, BuildingLodMaterial, BuildingProgram, LodMesh, RoofMaterial,
    WallMaterialClass, compile_building_collision, compile_building_detail, compile_building_lod,
    generate,
};
use bevy::{
    ecs::hierarchy::ChildSpawnerCommands,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat},
};

use super::*;

const DETAIL_LOD_END_START_METRES: f32 = 55.0;
const DETAIL_LOD_END_END_METRES: f32 = 70.0;
const FACADE_LOD_END_START_METRES: f32 = 150.0;
const FACADE_LOD_END_END_METRES: f32 = 175.0;
const SHELL_LOD_END_START_METRES: f32 = 300.0;
const SHELL_LOD_END_END_METRES: f32 = 340.0;

#[derive(Component)]
pub(crate) struct DistantCityBuildingPresentation;

#[derive(Component)]
pub(crate) struct PresentedBuildingMesh {
    pub(crate) scope: BuildingPresentationScope,
    pub(crate) level: BuildingRenderLevel,
    pub(crate) material: BuildingLodMaterial,
    pub(crate) triangles: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildingRenderLevel {
    Lod0,
    Lod1,
    Lod2,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildingPresentationScope {
    Playable,
    DistantCity,
}

#[derive(Clone)]
struct CompiledBuildingBatch {
    material: BuildingLodMaterial,
    mesh: Handle<Mesh>,
    triangles: usize,
}

#[derive(Clone)]
struct CompiledBuildingLevels {
    program: BuildingProgram,
    floor_offset_metres: f32,
    lod0: Vec<CompiledBuildingBatch>,
    lod1: Vec<CompiledBuildingBatch>,
    lod2: Vec<CompiledBuildingBatch>,
}

#[derive(Default, Resource)]
pub(in crate::presentation) struct TacticalBuildingMeshCache(Vec<CompiledBuildingLevels>);

#[derive(Resource)]
pub(crate) struct TacticalBuildingMaterials {
    plaster: Handle<StandardMaterial>,
    brick: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    tile: Handle<StandardMaterial>,
    slate: Handle<StandardMaterial>,
    timber_roof: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    fachwerk_baked: Handle<StandardMaterial>,
    details: Handle<StandardMaterial>,
    crown_mask: Handle<StandardMaterial>,
}

impl TacticalBuildingMaterials {
    fn get(&self, material: BuildingLodMaterial) -> Handle<StandardMaterial> {
        match material {
            BuildingLodMaterial::Wall(WallMaterialClass::TimberInfill) => self.plaster.clone(),
            BuildingLodMaterial::Wall(WallMaterialClass::CivilianMasonry) => self.brick.clone(),
            BuildingLodMaterial::Wall(WallMaterialClass::InternalTimber) => self.timber.clone(),
            BuildingLodMaterial::Wall(_) | BuildingLodMaterial::CrownMasonry => self.stone.clone(),
            BuildingLodMaterial::Roof(RoofMaterial::ClayTile) => self.tile.clone(),
            BuildingLodMaterial::Roof(RoofMaterial::Slate | RoofMaterial::Lead) => {
                self.slate.clone()
            }
            BuildingLodMaterial::Roof(_) => self.timber_roof.clone(),
            BuildingLodMaterial::FachwerkBaked => self.fachwerk_baked.clone(),
            BuildingLodMaterial::Timber => self.timber.clone(),
            BuildingLodMaterial::Floor => self.floor.clone(),
            BuildingLodMaterial::Glass => self.glass.clone(),
            BuildingLodMaterial::FacadeDetails => self.details.clone(),
            BuildingLodMaterial::CrownMask => self.crown_mask.clone(),
        }
    }
}

pub(in crate::presentation) fn setup_tactical_building_materials(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let plaster = images.add(checker_texture([202, 187, 151, 255], [184, 168, 134, 255]));
    let brick = images.add(checker_texture([137, 63, 43, 255], [102, 43, 31, 255]));
    let stone = images.add(checker_texture([121, 122, 111, 255], [88, 91, 84, 255]));
    let tile = images.add(checker_texture([102, 39, 29, 255], [66, 27, 24, 255]));
    let slate = images.add(checker_texture([61, 67, 73, 255], [40, 45, 51, 255]));
    let timber_roof = images.add(checker_texture([91, 57, 31, 255], [61, 38, 24, 255]));
    let timber = images.add(checker_texture([79, 39, 21, 255], [48, 24, 14, 255]));
    let floor = images.add(checker_texture([109, 94, 72, 255], [83, 72, 57, 255]));
    let fachwerk_baked = images.add(fachwerk_baked_texture());
    let details = images.add(facade_atlas());
    let crown_mask = images.add(crenellation_mask());
    commands.insert_resource(TacticalBuildingMaterials {
        plaster: materials.add(opaque_material(plaster)),
        brick: materials.add(opaque_material(brick)),
        stone: materials.add(opaque_material(stone.clone())),
        tile: materials.add(opaque_material(tile)),
        slate: materials.add(opaque_material(slate)),
        timber_roof: materials.add(opaque_material(timber_roof)),
        timber: materials.add(opaque_material(timber)),
        floor: materials.add(opaque_material(floor)),
        fachwerk_baked: materials.add(opaque_material(fachwerk_baked)),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgba(0.06, 0.22, 0.31, 0.38),
            perceptual_roughness: 0.18,
            metallic: 0.05,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }),
        details: materials.add(StandardMaterial {
            base_color_texture: Some(details),
            perceptual_roughness: 0.9,
            cull_mode: None,
            ..default()
        }),
        crown_mask: materials.add(StandardMaterial {
            base_color_texture: Some(crown_mask),
            perceptual_roughness: 0.95,
            alpha_mode: AlphaMode::Mask(0.5),
            cull_mode: None,
            ..default()
        }),
    });
    commands.insert_resource(TacticalBuildingMeshCache::default());
}

fn opaque_material(texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        cull_mode: Some(Face::Back),
        ..default()
    }
}

pub(in crate::presentation) fn on_scene_building_added(
    event: On<Add, SceneBuilding>,
    mut commands: Commands,
    buildings: Query<&SceneBuilding>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
    mut cache: ResMut<TacticalBuildingMeshCache>,
) -> Result {
    let building = buildings.get(event.entity)?;
    let compiled = cached_building_levels(&mut cache, &building.program, &mut meshes)?;
    commands
        .entity(event.entity)
        .insert(Visibility::default())
        .with_children(|parent| {
            spawn_building_levels(
                parent,
                &compiled,
                BuildingPresentationScope::Playable,
                &materials,
            );
        });
    Ok(())
}

pub(in crate::presentation) fn on_scene_vista_buildings(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    existing: Query<Entity, With<DistantCityBuildingPresentation>>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
    mut cache: ResMut<TacticalBuildingMeshCache>,
) -> Result {
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for placement in &bundle.distant_buildings {
        let compiled = cached_building_levels(&mut cache, &placement.program(), &mut meshes)?;
        commands
            .spawn((
                Name::new(format!("Distant city building {}", placement.id)),
                DistantCityBuildingPresentation,
                Visibility::default(),
                Transform::from_xyz(
                    placement.centre_metres.x,
                    placement.base_elevation_metres + compiled.floor_offset_metres,
                    placement.centre_metres.y,
                )
                .with_rotation(Quat::from_rotation_y(
                    f32::from(placement.quarter_turns) * core::f32::consts::FRAC_PI_2,
                )),
            ))
            .with_children(|parent| {
                spawn_building_levels(
                    parent,
                    &compiled,
                    BuildingPresentationScope::DistantCity,
                    &materials,
                );
            });
    }
    Ok(())
}

fn cached_building_levels(
    cache: &mut TacticalBuildingMeshCache,
    program: &BuildingProgram,
    meshes: &mut Assets<Mesh>,
) -> Result<CompiledBuildingLevels> {
    if let Some(compiled) = cache.0.iter().find(|compiled| compiled.program == *program) {
        return Ok(compiled.clone());
    }

    let plan = generate(program)?;
    let collision = compile_building_collision(&plan);
    let local_origin = collision.bounds.centre();
    let floor_offset_metres = local_origin.y - collision.bounds.min.y;
    let detail = compile_building_detail(&plan);
    let facade = compile_building_lod(&plan, BuildingLodLevel::Facade);
    let shell = compile_building_lod(&plan, BuildingLodLevel::Shell);
    let compile_batches = |source: &[LodMesh], meshes: &mut Assets<Mesh>| {
        source
            .iter()
            .map(|batch| CompiledBuildingBatch {
                material: batch.material,
                mesh: meshes.add(building_mesh(batch, local_origin)),
                triangles: batch.indices.len() / 3,
            })
            .collect()
    };
    let compiled = CompiledBuildingLevels {
        program: program.clone(),
        floor_offset_metres,
        lod0: compile_batches(&detail.meshes, meshes),
        lod1: compile_batches(&facade.meshes, meshes),
        lod2: compile_batches(&shell.meshes, meshes),
    };
    cache.0.push(compiled.clone());
    Ok(compiled)
}

fn spawn_building_levels(
    parent: &mut ChildSpawnerCommands,
    compiled: &CompiledBuildingLevels,
    scope: BuildingPresentationScope,
    materials: &TacticalBuildingMaterials,
) {
    for (level, batches) in [
        (BuildingRenderLevel::Lod0, &compiled.lod0),
        (BuildingRenderLevel::Lod1, &compiled.lod1),
        (BuildingRenderLevel::Lod2, &compiled.lod2),
    ] {
        for batch in batches {
            parent.spawn((
                Name::new(format!("Building {:?} {:?}", level, batch.material)),
                PresentedBuildingMesh {
                    scope,
                    level,
                    material: batch.material,
                    triangles: batch.triangles,
                },
                Mesh3d(batch.mesh.clone()),
                MeshMaterial3d(materials.get(batch.material)),
                building_lod_visibility(level),
            ));
        }
    }
}

fn building_lod_visibility(level: BuildingRenderLevel) -> VisibilityRange {
    match level {
        BuildingRenderLevel::Lod0 => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: DETAIL_LOD_END_START_METRES..DETAIL_LOD_END_END_METRES,
            use_aabb: false,
        },
        BuildingRenderLevel::Lod1 => VisibilityRange {
            start_margin: DETAIL_LOD_END_START_METRES..DETAIL_LOD_END_END_METRES,
            end_margin: FACADE_LOD_END_START_METRES..FACADE_LOD_END_END_METRES,
            use_aabb: false,
        },
        BuildingRenderLevel::Lod2 => VisibilityRange {
            start_margin: FACADE_LOD_END_START_METRES..FACADE_LOD_END_END_METRES,
            end_margin: SHELL_LOD_END_START_METRES..SHELL_LOD_END_END_METRES,
            use_aabb: false,
        },
    }
}

fn building_mesh(batch: &LodMesh, local_origin: Vec3) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        batch
            .vertices
            .iter()
            .map(|vertex| (vertex.position - local_origin).to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        batch
            .vertices
            .iter()
            .map(|vertex| vertex.normal.to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        batch
            .vertices
            .iter()
            .map(|vertex| vertex.uv.to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(batch.indices.clone()));
    mesh
}

fn checker_texture(first: [u8; 4], second: [u8; 4]) -> Image {
    let size = 64_u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            pixels.extend_from_slice(if (x / 8 + y / 8) % 2 == 0 {
                &first
            } else {
                &second
            });
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
}

fn fachwerk_baked_texture() -> Image {
    let size = 128_u32;
    let bay = 64_i32;
    let timber_half_width = 4_i32;
    let plaster = [194, 181, 148, 255];
    let plaster_shadow = [175, 162, 132, 255];
    let timber = [55, 29, 17, 255];
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let bay_x = x as i32 % bay;
            let bay_y = y as i32 % bay;
            let on_post = bay_x <= timber_half_width || bay_x >= bay - timber_half_width;
            let on_rail = bay_y <= timber_half_width || bay_y >= bay - timber_half_width;
            let on_brace = (bay_x - bay_y).abs() <= timber_half_width
                || (bay_x + bay_y - bay).abs() <= timber_half_width;
            let color = if on_post || on_rail || on_brace {
                timber
            } else if (x / 8 + y / 8) % 2 == 0 {
                plaster
            } else {
                plaster_shadow
            };
            pixels.extend_from_slice(&color);
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
}

fn facade_atlas() -> Image {
    let width = 256_u32;
    let height = 64_u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = match x {
                0..=63 if (x + y / 2) % 13 < 3 => [45, 24, 13, 255],
                0..=63 => [91, 50, 25, 255],
                64..=95 => [53, 102, 123, 255],
                96..=127 => [94, 48, 23, 255],
                128..=159 => [70, 38, 22, 255],
                160..=191 => [30, 28, 24, 255],
                192..=223 => [24, 22, 20, 255],
                _ => [64, 50, 35, 255],
            };
            pixels.extend_from_slice(&color);
        }
    }
    image_with_sampler(width, height, pixels, ImageAddressMode::ClampToEdge)
}

fn crenellation_mask() -> Image {
    let size = 64_u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let alpha = if y >= 28 || x < 38 { 255 } else { 0 };
            pixels.extend_from_slice(&[121, 122, 111, alpha]);
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
}

fn image_with_sampler(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    address_mode: ImageAddressMode,
) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
