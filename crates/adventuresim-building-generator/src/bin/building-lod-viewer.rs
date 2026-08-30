#[cfg(not(target_family = "wasm"))]
use adventuresim_building_generator::{
    BuildingArchetype, BuildingLodLevel, BuildingLodMaterial, BuildingProgram, LodMesh,
    WallMaterialClass, compile_building_lod, generate,
};
#[cfg(not(target_family = "wasm"))]
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat},
    window::{PresentMode, WindowResolution},
};
#[cfg(not(target_family = "wasm"))]
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
#[cfg(not(target_family = "wasm"))]
use clap::{Parser, ValueEnum};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LodChoice {
    Facade,
    Shell,
}

#[cfg(not(target_family = "wasm"))]
impl From<LodChoice> for BuildingLodLevel {
    fn from(value: LodChoice) -> Self {
        match value {
            LodChoice::Facade => Self::Facade,
            LodChoice::Shell => Self::Shell,
        }
    }
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(version, about = "Inspect joined procedural-building LOD meshes")]
struct Args {
    #[arg(long, value_enum, default_value_t = BuildingArchetype::FachwerkMerchantHouse)]
    fixture: BuildingArchetype,
    #[arg(long, value_enum, default_value_t = LodChoice::Facade)]
    lod: LodChoice,
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    let plan = generate(&BuildingProgram::fixture(args.fixture, args.seed))
        .expect("curated building fixture must generate");
    let lod = compile_building_lod(&plan, args.lod.into());
    let dimensions = plan.dimensions_metres();
    let title = format!("Fabelgeist building LOD: {:?} {:?}", args.fixture, args.lod);
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title,
                    resolution: WindowResolution::new(1440, 900),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
            PanOrbitCameraPlugin,
        ))
        .insert_resource(ClearColor(Color::srgb(0.72, 0.80, 0.86)))
        .add_systems(Startup, move |world: &mut World| {
            setup_lod_scene(world, &lod.meshes, dimensions)
        })
        .run();
}

#[cfg(not(target_family = "wasm"))]
fn setup_lod_scene(world: &mut World, batches: &[LodMesh], dimensions: Vec2) {
    let textures = create_lod_textures(world);
    let translation = Vec3::new(-dimensions.x * 0.5, 0.0, -dimensions.y * 0.5);
    let mut maximum_height = 1.0_f32;
    for batch in batches {
        maximum_height = maximum_height.max(
            batch
                .vertices
                .iter()
                .map(|vertex| vertex.position.y)
                .fold(0.0, f32::max),
        );
        let mesh = world.resource_mut::<Assets<Mesh>>().add(bevy_mesh(batch));
        let material = lod_material(world, batch.material, &textures);
        world.spawn((
            Name::new(format!("building LOD {:?}", batch.material)),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(translation),
        ));
    }

    let ground_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Plane3d::default().mesh().size(100.0, 100.0));
    let ground_material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.30, 0.21),
            perceptual_roughness: 1.0,
            ..default()
        });
    world.spawn((
        Name::new("LOD viewer ground"),
        Mesh3d(ground_mesh),
        MeshMaterial3d(ground_material),
    ));

    let focus = Vec3::new(0.0, maximum_height * 0.45, 0.0);
    let radius = dimensions.length().max(maximum_height) * 1.15;
    let camera_position = focus + Vec3::new(radius * 0.72, radius * 0.52, radius * 0.82);
    world.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(focus, Vec3::Y),
        PanOrbitCamera {
            focus,
            radius: Some(camera_position.distance(focus)),
            ..default()
        },
    ));
    world.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.85, -0.65, 0.0)),
    ));
    world.spawn((
        PointLight {
            intensity: 500_000.0,
            range: radius * 2.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(camera_position.lerp(focus, 0.25)),
    ));
}

#[cfg(not(target_family = "wasm"))]
fn bevy_mesh(batch: &LodMesh) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        batch
            .vertices
            .iter()
            .map(|vertex| vertex.position.to_array())
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

#[cfg(not(target_family = "wasm"))]
struct LodTextures {
    plaster: Handle<Image>,
    brick: Handle<Image>,
    stone: Handle<Image>,
    roof: Handle<Image>,
    details: Handle<Image>,
    crown_mask: Handle<Image>,
}

#[cfg(not(target_family = "wasm"))]
fn create_lod_textures(world: &mut World) -> LodTextures {
    let mut images = world.resource_mut::<Assets<Image>>();
    LodTextures {
        plaster: images.add(checker_texture([202, 187, 151, 255], [184, 168, 134, 255])),
        brick: images.add(checker_texture([137, 63, 43, 255], [102, 43, 31, 255])),
        stone: images.add(checker_texture([121, 122, 111, 255], [88, 91, 84, 255])),
        roof: images.add(checker_texture([102, 39, 29, 255], [66, 27, 24, 255])),
        details: images.add(facade_atlas()),
        crown_mask: images.add(crenellation_mask()),
    }
}

#[cfg(not(target_family = "wasm"))]
fn lod_material(
    world: &mut World,
    material: BuildingLodMaterial,
    textures: &LodTextures,
) -> Handle<StandardMaterial> {
    let texture = match material {
        BuildingLodMaterial::Wall(WallMaterialClass::TimberInfill) => &textures.plaster,
        BuildingLodMaterial::Wall(WallMaterialClass::CivilianMasonry) => &textures.brick,
        BuildingLodMaterial::Wall(_) | BuildingLodMaterial::CrownMasonry => &textures.stone,
        BuildingLodMaterial::Roof(_) => &textures.roof,
        BuildingLodMaterial::FacadeDetails => &textures.details,
        BuildingLodMaterial::CrownMask => &textures.crown_mask,
    };
    world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color_texture: Some(texture.clone()),
            perceptual_roughness: 0.9,
            alpha_mode: if material == BuildingLodMaterial::CrownMask {
                AlphaMode::Mask(0.5)
            } else {
                AlphaMode::Opaque
            },
            cull_mode: if matches!(
                material,
                BuildingLodMaterial::FacadeDetails | BuildingLodMaterial::CrownMask
            ) {
                None
            } else {
                Some(Face::Back)
            },
            ..default()
        })
}

#[cfg(not(target_family = "wasm"))]
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
    repeat_image(size, size, pixels)
}

#[cfg(not(target_family = "wasm"))]
fn facade_atlas() -> Image {
    let width = 256_u32;
    let height = 64_u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = match x {
                0..=63 => {
                    if (x + y / 2) % 13 < 3 {
                        [45, 24, 13, 255]
                    } else {
                        [91, 50, 25, 255]
                    }
                }
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
    clamp_image(width, height, pixels)
}

#[cfg(not(target_family = "wasm"))]
fn crenellation_mask() -> Image {
    let size = 64_u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let solid = y >= 28 || x < 38;
            pixels.extend_from_slice(&[121, 122, 111, if solid { 255 } else { 0 }]);
        }
    }
    repeat_image(size, size, pixels)
}

#[cfg(not(target_family = "wasm"))]
fn repeat_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    image_with_sampler(width, height, pixels, ImageAddressMode::Repeat)
}

#[cfg(not(target_family = "wasm"))]
fn clamp_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    image_with_sampler(width, height, pixels, ImageAddressMode::ClampToEdge)
}

#[cfg(not(target_family = "wasm"))]
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

#[cfg(target_family = "wasm")]
fn main() {
    panic!("building LOD viewer is a native-only prototype");
}
