use crate::data::gpu::compute::{Map, MapDefinition};
use crate::data::gpu::parameters::PassParameters;
use crate::data::gpu::resource::GpuResource;
use crate::data::gpu::texture::{Texture2d, Texture3d, TextureFormat};
use crate::globals::WgpuContext;
use crate::prelude::*;

pub struct DistanceFieldJfa;

impl DistanceFieldJfa {
    pub fn distance_field_2d(
        context: &WgpuContext,
        input: &Texture2d,
        output: &Texture2d,
    ) -> Result<()> {
        let size = input.size();
        let max_dim = size.x.max(size.y);

        let temp_a = Texture2d::create(context, size, TextureFormat::Rg32Float)?;
        let temp_b = Texture2d::create(context, size, TextureFormat::Rg32Float)?;

        let init_code = r#"
        fn map(index: vec2<u32>, input_val: f32) -> vec2<f32> {
            if (input_val >= 0.5) {
                return vec2<f32>(index);
            } else {
                return vec2<f32>(-99999.0, -99999.0);
            }
        }
        "#
        .to_string();
        let init_def = MapDefinition::new(init_code)?;
        Map::execute(
            context,
            &init_def,
            Some(&GpuResource::Texture2d(input.clone())),
            &GpuResource::Texture2d(temp_a.clone()),
        )?;

        let step_code = r#"
        fn map(index: vec2<u32>, size: vec2<u32>, in_val: vec2<f32>, step: f32) -> vec2<f32> {
            var best_seed = in_val;
            var best_dist = 999999.0;
            let current_pos = vec2<f32>(index);
            if (best_seed.x >= -10000.0) {
                best_dist = distance(current_pos, best_seed);
            }

            let s = i32(step);
            let size_i = vec2<i32>(size);
            let index_i = vec2<i32>(index);

            for (var dy = -1; dy <= 1; dy++) {
                for (var dx = -1; dx <= 1; dx++) {
                    if (dx == 0 && dy == 0) {
                        continue;
                    }
                    let sample_pos = index_i + vec2<i32>(dx, dy) * s;
                    if (sample_pos.x >= 0 && sample_pos.x < size_i.x && sample_pos.y >= 0 && sample_pos.y < size_i.y) {
                        let seed = textureLoad(input, sample_pos, 0).xy;
                        if (seed.x >= -10000.0) {
                            let d = distance(current_pos, seed);
                            if (d < best_dist) {
                                best_dist = d;
                                best_seed = seed;
                            }
                        }
                    }
                }
            }
            return best_seed;
        }
        "#.to_string();
        let step_def = MapDefinition::new(step_code)?;

        let mut step = (max_dim / 2.0).ceil() as i32;
        let mut ping = true;

        while step >= 1 {
            let (src, dst) = if ping {
                (&temp_a, &temp_b)
            } else {
                (&temp_b, &temp_a)
            };

            let mut params = PassParameters::new();
            params.insert("step", step as f32);

            Map::execute_with_parameters(
                context,
                &step_def,
                Some(&GpuResource::Texture2d(src.clone())),
                &GpuResource::Texture2d(dst.clone()),
                Some(params),
            )?;

            step /= 2;
            ping = !ping;
        }

        let dist_code = r#"
        fn map(index: vec2<u32>, in_val: vec2<f32>) -> f32 {
            if (in_val.x < -10000.0) {
                return 999999.0;
            }
            return distance(vec2<f32>(index), in_val);
        }
        "#
        .to_string();
        let dist_def = MapDefinition::new(dist_code)?;

        let final_src = if ping { &temp_a } else { &temp_b };
        Map::execute(
            context,
            &dist_def,
            Some(&GpuResource::Texture2d(final_src.clone())),
            &GpuResource::Texture2d(output.clone()),
        )?;

        Ok(())
    }

    pub fn distance_field_3d(
        context: &WgpuContext,
        input: &Texture3d,
        output: &Texture3d,
    ) -> Result<()> {
        let size = input.size;
        let max_dim = (size.0.max(size.1).max(size.2)) as f32;
        let size_vec3 = crate::data::vector::Vec3::new(size.0 as f32, size.1 as f32, size.2 as f32);

        let temp_a = Texture3d::new(context, size_vec3, TextureFormat::Rgba32Float)?;
        let temp_b = Texture3d::new(context, size_vec3, TextureFormat::Rgba32Float)?;

        let init_code = r#"
        fn map(index: vec3<u32>, input_val: f32) -> vec4<f32> {
            if (input_val >= 0.5) {
                return vec4<f32>(vec3<f32>(index), 1.0);
            } else {
                return vec4<f32>(-99999.0, -99999.0, -99999.0, 0.0);
            }
        }
        "#
        .to_string();
        let init_def = MapDefinition::new(init_code)?;
        Map::execute(
            context,
            &init_def,
            Some(&GpuResource::Texture3d(input.clone())),
            &GpuResource::Texture3d(temp_a.clone()),
        )?;

        let step_code = r#"
        fn map(index: vec3<u32>, size: vec3<u32>, in_val: vec4<f32>, step: f32) -> vec4<f32> {
            var best_seed = in_val;
            var best_dist = 999999.0;
            let current_pos = vec3<f32>(index);
            if (best_seed.w > 0.5) {
                best_dist = distance(current_pos, best_seed.xyz);
            }

            let s = i32(step);
            let size_i = vec3<i32>(size);
            let index_i = vec3<i32>(index);

            for (var dz = -1; dz <= 1; dz++) {
                for (var dy = -1; dy <= 1; dy++) {
                    for (var dx = -1; dx <= 1; dx++) {
                        if (dx == 0 && dy == 0 && dz == 0) {
                            continue;
                        }
                        let sample_pos = index_i + vec3<i32>(dx, dy, dz) * s;
                        if (sample_pos.x >= 0 && sample_pos.x < size_i.x &&
                            sample_pos.y >= 0 && sample_pos.y < size_i.y &&
                            sample_pos.z >= 0 && sample_pos.z < size_i.z) {
                            let seed = textureLoad(input, sample_pos, 0);
                            if (seed.w > 0.5) {
                                let d = distance(current_pos, seed.xyz);
                                if (d < best_dist) {
                                    best_dist = d;
                                    best_seed = seed;
                                }
                            }
                        }
                    }
                }
            }
            return best_seed;
        }
        "#
        .to_string();
        let step_def = MapDefinition::new(step_code)?;

        let mut step = (max_dim / 2.0).ceil() as i32;
        let mut ping = true;

        while step >= 1 {
            let (src, dst) = if ping {
                (&temp_a, &temp_b)
            } else {
                (&temp_b, &temp_a)
            };

            let mut params = PassParameters::new();
            params.insert("step", step as f32);

            Map::execute_with_parameters(
                context,
                &step_def,
                Some(&GpuResource::Texture3d(src.clone())),
                &GpuResource::Texture3d(dst.clone()),
                Some(params),
            )?;

            step /= 2;
            ping = !ping;
        }

        let dist_code = r#"
        fn map(index: vec3<u32>, in_val: vec4<f32>) -> f32 {
            if (in_val.w <= 0.5) {
                return 999999.0;
            }
            return distance(vec3<f32>(index), in_val.xyz);
        }
        "#
        .to_string();
        let dist_def = MapDefinition::new(dist_code)?;

        let final_src = if ping { &temp_a } else { &temp_b };
        Map::execute(
            context,
            &dist_def,
            Some(&GpuResource::Texture3d(final_src.clone())),
            &GpuResource::Texture3d(output.clone()),
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_distance_field_2d() -> Result<()> {
        let context = WgpuContext::new().await.expect("Failed to init WGPU");

        // 5x5 grid
        let size = crate::data::Vec2::new(5.0, 5.0);
        let input_tex = Texture2d::create(&context, size, TextureFormat::R32Float)?;
        let output_tex = Texture2d::create(&context, size, TextureFormat::R32Float)?;

        // Center pixel (2, 2) is solid (1.0), others empty (0.0)
        let mut input_data = vec![0.0f32; 25];
        input_data[2 * 5 + 2] = 1.0;
        input_tex.write(&context, &input_data)?;

        DistanceFieldJfa::distance_field_2d(&context, &input_tex, &output_tex)?;

        let result = output_tex.read::<f32>(&context).await?;

        // Center is 0.0
        assert!((result[2 * 5 + 2] - 0.0).abs() < 1e-4);
        // Orthogonal neighbors are 1.0
        assert!((result[2 * 5 + 1] - 1.0).abs() < 1e-4);
        assert!((result[2 * 5 + 3] - 1.0).abs() < 1e-4);
        assert!((result[5 + 2] - 1.0).abs() < 1e-4);
        assert!((result[3 * 5 + 2] - 1.0).abs() < 1e-4);
        // Diagonal neighbors are sqrt(2) ~ 1.4142
        assert!((result[5 + 1] - std::f32::consts::SQRT_2).abs() < 1e-3);

        Ok(())
    }

    #[tokio::test]
    async fn test_distance_field_3d() -> Result<()> {
        let context = WgpuContext::new().await.expect("Failed to init WGPU");

        // 3x3x3 grid
        let size = crate::data::vector::Vec3::new(3.0, 3.0, 3.0);
        let input_tex = Texture3d::new(&context, size, TextureFormat::R32Float)?;
        let output_tex = Texture3d::new(&context, size, TextureFormat::R32Float)?;

        // Center voxel (1, 1, 1) is solid (1.0)
        let mut input_data = vec![0.0f32; 27];
        input_data[9 + 3 + 1] = 1.0;
        input_tex.write(&context, &input_data)?;

        DistanceFieldJfa::distance_field_3d(&context, &input_tex, &output_tex)?;

        let result = output_tex.read::<f32>(&context).await?;

        // Center is 0.0
        assert!((result[9 + 3 + 1] - 0.0).abs() < 1e-4);
        // Orthogonal neighbors are 1.0
        assert!((result[9 + 3] - 1.0).abs() < 1e-4);
        assert!((result[9 + 3 + 2] - 1.0).abs() < 1e-4);

        Ok(())
    }
}
