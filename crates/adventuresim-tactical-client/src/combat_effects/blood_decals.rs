use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, VertexAttributeValues},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
};

const BLOOD_MASK_SHADER: &str = "shaders/blood_mask.wgsl";
const BLOOD_MASK_SIZE: u32 = 512;
const BLOOD_STAIN_RADIUS_TEXELS: f32 = 23.0;
const TERRAIN_BLOOD_STAIN_RADIUS_METRES: f32 = 0.11;

pub(super) struct BloodDecalPlugin;

impl Plugin for BloodDecalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<BloodMaskMaterial>::default());
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(super) struct BloodMaskExtension {
    #[texture(100)]
    #[sampler(101)]
    mask: Handle<Image>,
}

impl MaterialExtension for BloodMaskExtension {
    fn fragment_shader() -> ShaderRef {
        BLOOD_MASK_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        BLOOD_MASK_SHADER.into()
    }
}

pub(super) type BloodMaskMaterial = ExtendedMaterial<StandardMaterial, BloodMaskExtension>;

#[derive(Component)]
pub(super) struct BloodMaskSurface {
    mask: Handle<Image>,
}

pub(super) type BloodSurfaceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Mesh3d,
        &'static GlobalTransform,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
        Option<&'static BloodMaskSurface>,
    ),
>;

pub(super) struct BloodMaterialAssets<'a> {
    pub(super) meshes: &'a Assets<Mesh>,
    pub(super) images: &'a mut Assets<Image>,
    pub(super) standard: &'a Assets<StandardMaterial>,
    pub(super) blood: &'a mut Assets<BloodMaskMaterial>,
}

pub(super) fn stamp_character_blood(
    commands: &mut Commands,
    target: Entity,
    world_point: Vec3,
    sequence: u64,
    parents: &Query<&ChildOf>,
    surfaces: &BloodSurfaceQuery,
    assets: &mut BloodMaterialAssets,
) {
    let Some(hit) = closest_surface_uv(target, world_point, parents, surfaces, assets.meshes)
    else {
        return;
    };

    let mask = if let Some(surface) = hit.surface {
        surface.mask.clone()
    } else {
        let Some(material_handle) = hit.material else {
            return;
        };
        let Some(base) = assets.standard.get(material_handle) else {
            return;
        };
        let mask = assets.images.add(empty_blood_mask());
        let material = assets.blood.add(ExtendedMaterial {
            base: base.clone(),
            extension: BloodMaskExtension { mask: mask.clone() },
        });
        commands
            .entity(hit.entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((
                MeshMaterial3d(material),
                BloodMaskSurface { mask: mask.clone() },
            ));
        mask
    };

    if let Some(mut image) = assets.images.get_mut(&mask) {
        stamp_mask(&mut image, hit.uv, sequence);
    }
}

struct SurfaceUvHit<'a> {
    entity: Entity,
    uv: Vec2,
    distance_squared: f32,
    material: Option<&'a Handle<StandardMaterial>>,
    surface: Option<&'a BloodMaskSurface>,
}

fn closest_surface_uv<'a>(
    target: Entity,
    world_point: Vec3,
    parents: &Query<&ChildOf>,
    surfaces: &'a BloodSurfaceQuery,
    meshes: &Assets<Mesh>,
) -> Option<SurfaceUvHit<'a>> {
    surfaces
        .iter()
        .filter(|(entity, _, _, material, surface)| {
            (material.is_some() || surface.is_some()) && is_descendant_of(*entity, target, parents)
        })
        .filter_map(|(entity, mesh_handle, global, material, surface)| {
            let mesh = meshes.get(&mesh_handle.0)?;
            let local_point = global.affine().inverse().transform_point3(world_point);
            let (uv, distance_squared) = closest_mesh_uv(mesh, local_point)?;
            Some(SurfaceUvHit {
                entity,
                uv,
                distance_squared,
                material: material.map(|handle| &handle.0),
                surface,
            })
        })
        .min_by(|left, right| left.distance_squared.total_cmp(&right.distance_squared))
}

fn is_descendant_of(mut entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    while let Ok(parent) = parents.get(entity) {
        entity = parent.parent();
        if entity == ancestor {
            return true;
        }
    }
    false
}

fn closest_mesh_uv(mesh: &Mesh, point: Vec3) -> Option<(Vec2, f32)> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)? else {
        return None;
    };
    let indices: Vec<usize> = match mesh.indices() {
        Some(Indices::U16(values)) => values.iter().map(|value| usize::from(*value)).collect(),
        Some(Indices::U32(values)) => values.iter().map(|value| *value as usize).collect(),
        None => (0..positions.len()).collect(),
    };

    indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|triangle| {
            let a = Vec3::from(*positions.get(triangle[0])?);
            let b = Vec3::from(*positions.get(triangle[1])?);
            let c = Vec3::from(*positions.get(triangle[2])?);
            let barycentric = closest_triangle_barycentric(point, a, b, c);
            let closest = a * barycentric.x + b * barycentric.y + c * barycentric.z;
            let uv_a = Vec2::from(*uvs.get(triangle[0])?);
            let uv_b = Vec2::from(*uvs.get(triangle[1])?);
            let uv_c = Vec2::from(*uvs.get(triangle[2])?);
            Some((
                uv_a * barycentric.x + uv_b * barycentric.y + uv_c * barycentric.z,
                closest.distance_squared(point),
            ))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

// Real-Time Collision Detection, Christer Ericson, section 5.1.5.
fn closest_triangle_barycentric(point: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return Vec3::X;
    }
    let bp = point - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return Vec3::Y;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return Vec3::new(1.0 - v, v, 0.0);
    }
    let cp = point - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return Vec3::Z;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return Vec3::new(1.0 - w, 0.0, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return Vec3::new(0.0, 1.0 - w, w);
    }
    let denominator = 1.0 / (va + vb + vc);
    Vec3::new(va, vb, vc) * denominator
}

fn empty_blood_mask() -> Image {
    let pixels = vec![0; (BLOOD_MASK_SIZE * BLOOD_MASK_SIZE) as usize];
    let mut image = Image::new(
        Extent3d {
            width: BLOOD_MASK_SIZE,
            height: BLOOD_MASK_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

fn stamp_mask(image: &mut Image, uv: Vec2, sequence: u64) {
    stamp_mask_with_radius(image, uv, Vec2::splat(BLOOD_STAIN_RADIUS_TEXELS), sequence);
}

pub(super) fn stamp_terrain_blood(
    image: &mut Image,
    world_point: Vec3,
    terrain_size: Vec2,
    sequence: u64,
) {
    let dimensions = Vec2::new(
        image.texture_descriptor.size.width as f32,
        image.texture_descriptor.size.height as f32,
    );
    let uv = world_point.xz() / terrain_size + Vec2::splat(0.5);
    let radius = Vec2::splat(TERRAIN_BLOOD_STAIN_RADIUS_METRES) / terrain_size * dimensions;
    stamp_mask_with_radius(image, uv, radius, sequence);
}

fn stamp_mask_with_radius(image: &mut Image, uv: Vec2, radius: Vec2, sequence: u64) {
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    let Some(pixels) = image.data.as_mut() else {
        return;
    };
    let centre = Vec2::new(
        uv.x.clamp(0.0, 1.0) * (width - 1) as f32,
        (1.0 - uv.y.clamp(0.0, 1.0)) * (height - 1) as f32,
    );
    let angle = (sequence as f32 * 2.399_963_1).rem_euclid(std::f32::consts::TAU);
    let minimum = (centre - radius - Vec2::splat(2.0)).max(Vec2::ZERO);
    let maximum = (centre + radius + Vec2::splat(2.0))
        .min(Vec2::new((width - 1) as f32, (height - 1) as f32));
    for y in minimum.y as u32..=maximum.y as u32 {
        for x in minimum.x as u32..=maximum.x as u32 {
            let delta = Vec2::new(x as f32, y as f32) - centre;
            let polar = delta.y.atan2(delta.x) + angle;
            let normalized_distance = (delta / radius.max(Vec2::ONE)).length();
            let edge = 0.78 + 0.12 * (polar * 5.0).sin() + 0.07 * (polar * 9.0 + 0.8).sin();
            let alpha =
                ((edge - normalized_distance) * radius.min_element() * 0.45).clamp(0.0, 1.0);
            let index = (y * width + x) as usize;
            pixels[index] = pixels[index].max((alpha * 235.0) as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_triangle_uv_uses_barycentric_coordinates() {
        let mut mesh = Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );

        let (uv, distance) = closest_mesh_uv(&mesh, Vec3::new(0.25, 0.5, 0.2)).unwrap();
        assert!(uv.abs_diff_eq(Vec2::new(0.25, 0.5), 0.0001));
        assert!((distance - 0.04).abs() < 0.0001);
    }

    #[test]
    fn stamping_accumulates_into_mask() {
        let mut image = empty_blood_mask();
        stamp_mask(&mut image, Vec2::splat(0.5), 3);
        assert!(image.data.as_ref().unwrap().iter().any(|alpha| *alpha > 0));
    }
}
