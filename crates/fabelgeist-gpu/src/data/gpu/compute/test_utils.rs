use crate::data::gpu::buffer::{Buffer, BufferDefinition};
use crate::data::gpu::compute::signature::ResourceBaseType;
use crate::data::gpu::resource::GpuResource;
use crate::data::gpu::texture::{Texture2d, Texture3d, TextureFormat};
use crate::data::vector::{Vec2, Vec3};
use crate::prelude::*;

pub struct Resource<'a, T>(&'a [T], pub TestResourceType);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestResourceType {
    Buffer,
    Texture2d(usize, usize, TextureFormat),
    Texture3d(usize, usize, usize, TextureFormat),
}

pub fn inflate_flat<S: Copy + Default>(
    data: &[S],
    element_components: usize,
    texture_components: usize,
) -> Vec<S> {
    if texture_components == element_components {
        return data.to_vec();
    }
    let num_elements = data.len() / element_components;
    let mut inflated: Vec<S> = Vec::with_capacity(num_elements * texture_components);
    for chunk in data.chunks(element_components) {
        for &val in chunk {
            inflated.push(val);
        }
        for _ in 0..(texture_components - element_components) {
            inflated.push(S::default());
        }
    }
    inflated
}

pub fn deflate_flat<S: Copy>(
    data: &[S],
    element_components: usize,
    texture_components: usize,
) -> Vec<S> {
    if texture_components == element_components {
        return data.to_vec();
    }
    let mut deflated = Vec::new();
    for chunk in data.chunks(texture_components) {
        deflated.extend(chunk.iter().take(element_components).copied());
    }
    deflated
}

pub fn get_formats(
    base_type: &ResourceBaseType,
    min_components: usize,
) -> Vec<(TextureFormat, usize)> {
    let scalar_type = match base_type {
        ResourceBaseType::F32 => ResourceBaseType::F32,
        ResourceBaseType::U32 => ResourceBaseType::U32,
        ResourceBaseType::I32 => ResourceBaseType::I32,
        ResourceBaseType::Vec2(inner) => *inner.clone(),
        ResourceBaseType::Vec4(inner) => *inner.clone(),
        _ => return vec![],
    };

    let formats = match scalar_type {
        ResourceBaseType::F32 => vec![
            (TextureFormat::R32Float, 1),
            (TextureFormat::Rg32Float, 2),
            (TextureFormat::Rgba32Float, 4),
        ],
        ResourceBaseType::I32 => vec![
            (TextureFormat::R32Sint, 1),
            (TextureFormat::Rg32Sint, 2),
            (TextureFormat::Rgba32Sint, 4),
        ],
        ResourceBaseType::U32 => vec![
            (TextureFormat::R32Uint, 1),
            (TextureFormat::Rg32Uint, 2),
            (TextureFormat::Rgba32Uint, 4),
        ],
        _ => vec![],
    };

    formats
        .into_iter()
        .filter(|&(_, comps)| comps >= min_components)
        .collect()
}

pub fn all_resource_types(
    length: usize,
    formats: &[(TextureFormat, usize)],
    element_components: usize,
) -> Vec<(TestResourceType, usize)> {
    let mut resources = vec![(TestResourceType::Buffer, element_components)];
    for &(format, components) in formats {
        resources.push((TestResourceType::Texture2d(length, 1, format), components));
        resources.push((
            TestResourceType::Texture3d(length, 1, 1, format),
            components,
        ));
    }
    resources
}

impl<'a, T: bytemuck::NoUninit> TryFrom<(&'a WgpuContext, Resource<'a, T>)> for GpuResource {
    type Error = anyhow::Error;

    fn try_from(
        (context, Resource(resource, resource_type)): (&'a WgpuContext, Resource<'a, T>),
    ) -> Result<Self, Self::Error> {
        match resource_type {
            TestResourceType::Buffer => Ok(Buffer::from_slice(
                context,
                resource,
                BufferDefinition::storage().with_copy_src(),
            )?
            .into()),
            TestResourceType::Texture2d(x, y, format) => {
                let texture_2d = Texture2d::create(context, Vec2::new(x as f32, y as f32), format)?;
                texture_2d.write(context, resource)?;
                Ok(GpuResource::Texture2d(texture_2d))
            }
            TestResourceType::Texture3d(x, y, z, format) => {
                let texture_3d =
                    Texture3d::new(context, Vec3::new(x as f32, y as f32, z as f32), format)?;
                texture_3d.write(context, resource)?;
                Ok(GpuResource::Texture3d(texture_3d))
            }
        }
    }
}

/// A generic compute test runner that iterates over all compatible resource types (Buffer, Texture2d, Texture3d).
pub async fn run_compute_test<IN, OUT, S, F>(
    input_data: &[IN],
    expected_output: &[OUT],
    input_type: ResourceBaseType,
    output_type: ResourceBaseType,
    execute_op: F,
) -> Result<()>
where
    IN: bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    OUT:
        bytemuck::NoUninit + bytemuck::AnyBitPattern + PartialEq + std::fmt::Debug + Default + Copy,
    S: bytemuck::Pod + std::fmt::Debug + Default + Copy + PartialEq,
    F: Fn(&WgpuContext, GpuResource, GpuResource) -> Result<()>,
{
    let context = WgpuContext::new().await.unwrap();

    let in_element_components = input_type.component_count();
    let out_element_components = output_type.component_count();

    let input_formats = get_formats(&input_type, in_element_components);
    let output_formats = get_formats(&output_type, out_element_components);

    let input_resource_configs =
        all_resource_types(input_data.len(), &input_formats, in_element_components);
    let output_resource_configs = all_resource_types(
        expected_output.len(),
        &output_formats,
        out_element_components,
    );

    let flat_input: &[S] = bytemuck::cast_slice(input_data);
    let flat_expected: &[S] = bytemuck::cast_slice(expected_output);

    for (in_config, in_tex_comps) in input_resource_configs {
        for (out_config, out_tex_comps) in &output_resource_configs {
            let inflated_input = inflate_flat(flat_input, in_element_components, in_tex_comps);
            let inflated_output_init =
                inflate_flat(flat_expected, out_element_components, *out_tex_comps);

            let in_res: GpuResource =
                (&context, Resource(&inflated_input, in_config)).try_into()?;
            let out_res: GpuResource =
                (&context, Resource(&inflated_output_init, *out_config)).try_into()?;

            execute_op(&context, in_res, out_res.clone())?;

            let result_flat: Vec<S> = out_res.read(&context).await?;
            let result_deflated =
                deflate_flat(&result_flat, out_element_components, *out_tex_comps);

            assert_eq!(
                result_deflated, flat_expected,
                "Failed combination: in={:?} out={:?}. Input components: {}, Output components: {}",
                in_config, out_config, in_tex_comps, out_tex_comps
            );
        }
    }

    Ok(())
}

#[allow(non_snake_case)]
pub fn vec2<T>(x: T, y: T) -> [T; 2] {
    [x, y]
}

#[allow(non_snake_case)]
pub fn vec4<T>(x: T, y: T, z: T, w: T) -> [T; 4] {
    [x, y, z, w]
}
