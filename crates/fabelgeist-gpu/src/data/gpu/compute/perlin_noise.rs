use crate::data::gpu::compute::{Map, MapDefinition};
use crate::data::gpu::parameters::PassParameters;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;

pub struct RenderPerlin;

impl RenderPerlin {
    pub fn execute(
        context: &WgpuContext,
        output: &GpuResource,
        scale: crate::data::vector::Vec3,
        offset: crate::data::vector::Vec3,
        octaves: u32,
        lacunarity: f32,
        gain: f32,
    ) -> Result<()> {
        let is_buffer = matches!(output, GpuResource::Buffer(_));
        let index_type = match output {
            GpuResource::Buffer(_) => "u32",
            GpuResource::Texture2d(_) => "vec2<u32>",
            GpuResource::Texture3d(_) => "vec3<u32>",
        };

        let coord_mapping = match output {
            GpuResource::Buffer(_) => "vec3<f32>(f32(index), 0.0, 0.0)",
            GpuResource::Texture2d(_) => "vec3<f32>(vec2<f32>(index), 0.0)",
            GpuResource::Texture3d(_) => "vec3<f32>(index)",
        };
        let return_type = if is_buffer { "f32" } else { "vec4<f32>" };
        let return_statement = if is_buffer {
            "return val;"
        } else {
            "return vec4<f32>(val, val, val, 1.0);"
        };

        let shader_code = format!(
            r#"
            fn hash3(p: vec3<f32>) -> vec3<f32> {{
                var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
                p3 = p3 + dot(p3, p3.yxz + 33.33);
                return fract((p3.xxy + p3.yzz) * p3.zyx) * 2.0 - 1.0;
            }}

            fn perlin_noise(p: vec3<f32>) -> f32 {{
                let pi = floor(p);
                let pf = fract(p);
                
                let w = pf * pf * pf * (pf * (pf * 6.0 - 15.0) + 10.0);
                
                let g000 = hash3(pi + vec3<f32>(0.0, 0.0, 0.0));
                let g100 = hash3(pi + vec3<f32>(1.0, 0.0, 0.0));
                let g010 = hash3(pi + vec3<f32>(0.0, 1.0, 0.0));
                let g110 = hash3(pi + vec3<f32>(1.0, 1.0, 0.0));
                let g001 = hash3(pi + vec3<f32>(0.0, 0.0, 1.0));
                let g101 = hash3(pi + vec3<f32>(1.0, 0.0, 1.0));
                let g011 = hash3(pi + vec3<f32>(0.0, 1.0, 1.0));
                let g111 = hash3(pi + vec3<f32>(1.0, 1.0, 1.0));
                
                let v000 = dot(g000, pf - vec3<f32>(0.0, 0.0, 0.0));
                let v100 = dot(g100, pf - vec3<f32>(1.0, 0.0, 0.0));
                let v010 = dot(g010, pf - vec3<f32>(0.0, 1.0, 0.0));
                let v110 = dot(g110, pf - vec3<f32>(1.0, 1.0, 0.0));
                let v001 = dot(g001, pf - vec3<f32>(0.0, 0.0, 1.0));
                let v101 = dot(g101, pf - vec3<f32>(1.0, 0.0, 1.0));
                let v011 = dot(g011, pf - vec3<f32>(0.0, 1.0, 1.0));
                let v111 = dot(g111, pf - vec3<f32>(1.0, 1.0, 1.0));
                
                let a = mix(v000, v100, w.x);
                let b = mix(v010, v110, w.x);
                let c = mix(v001, v101, w.x);
                let d = mix(v011, v111, w.x);
                
                let e = mix(a, b, w.y);
                let f = mix(c, d, w.y);
                
                return mix(e, f, w.z);
            }}

            fn fbm(p: vec3<f32>, octaves: u32, lacunarity: f32, gain: f32) -> f32 {{
                var total = 0.0;
                var amplitude = 1.0;
                var frequency = 1.0;
                var max_amplitude = 0.0;
                
                var pos = p;
                for (var i = 0u; i < octaves; i = i + 1u) {{
                    total = total + perlin_noise(pos * frequency) * amplitude;
                    max_amplitude = max_amplitude + amplitude;
                    amplitude = amplitude * gain;
                    frequency = frequency * lacunarity;
                }}
                
                if (max_amplitude > 0.0) {{
                    return (total / max_amplitude) * 0.5 + 0.5;
                }} else {{
                    return 0.5;
                }}
            }}

            fn map(
                index: {0}, 
                scale: vec3<f32>, 
                offset: vec3<f32>, 
                octaves: u32, 
                lacunarity: f32, 
                gain: f32
            ) -> {1} {{
                let p = {2} * scale + offset;
                let val = fbm(p, octaves, lacunarity, gain);
                {3}
            }}
            "#,
            index_type, return_type, coord_mapping, return_statement
        );

        let map_def = MapDefinition::new(shader_code)?;

        let mut params = PassParameters::new();
        params.insert("scale", scale);
        params.insert("offset", offset);
        params.insert("octaves", octaves);
        params.insert("lacunarity", lacunarity);
        params.insert("gain", gain);

        Map::execute_with_parameters(context, &map_def, None, output, Some(params))?;
        Ok(())
    }
}
