use crate::data::gpu::compute::{Map, MapDefinition};
use crate::data::gpu::parameters::PassParameters;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;

pub struct RenderSimplex;

impl RenderSimplex {
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
            fn mod289(x: vec4<f32>) -> vec4<f32> {{
                return x - floor(x * (1.0 / 289.0)) * 289.0;
            }}

            fn mod289_3(x: vec3<f32>) -> vec3<f32> {{
                return x - floor(x * (1.0 / 289.0)) * 289.0;
            }}

            fn permute(x: vec4<f32>) -> vec4<f32> {{
                return mod289(((x * 34.0) + 1.0) * x);
            }}

            fn taylor_inv_sqrt(r: vec4<f32>) -> vec4<f32> {{
                return 1.79284291400159 - 0.85373472095314 * r;
            }}

            fn simplex_noise(v: vec3<f32>) -> f32 {{
                let C = vec2<f32>(1.0/6.0, 1.0/3.0);
                let D = vec4<f32>(0.0, 0.5, 1.0, 2.0);

                // First corner
                var i  = floor(v + dot(v, C.yyy) );
                let x0 = v - i + dot(i, C.xxx) ;

                // Other corners
                var g = step(x0.yzx, x0.xyz);
                var l = 1.0 - g;
                var i1 = min( g.xyz, l.zxy );
                var i2 = max( g.xyz, l.zxy );

                let x1 = x0 - i1 + C.xxx;
                let x2 = x0 - i2 + C.yyy;
                let x3 = x0 - D.yyy;

                // Permutations
                i = mod289_3(i);
                var p = permute( permute( permute(
                            i.z + vec4<f32>(0.0, i1.z, i2.z, 1.0 ))
                          + i.y + vec4<f32>(0.0, i1.y, i2.y, 1.0 ))
                          + i.x + vec4<f32>(0.0, i1.x, i2.x, 1.0 ));

                // Gradients
                let n_ = 0.142857142857; // 1.0/7.0
                let ns = n_ * D.wyz - D.xzx;

                let j = p - 49.0 * floor(p * ns.z * ns.z);

                let x_ = floor(j * ns.z);
                let y_ = floor(j - 7.0 * x_ );

                let x = x_ * ns.x + ns.yyyy;
                let y = y_ * ns.x + ns.yyyy;
                let h = 1.0 - abs(x) - abs(y);

                let b0 = vec4<f32>( x.xy, y.xy );
                let b1 = vec4<f32>( x.zw, y.zw );

                let s0 = floor(b0)*2.0 + 1.0;
                let s1 = floor(b1)*2.0 + 1.0;
                let sh = -step(h, vec4<f32>(0.0));

                let a0 = b0.xzyw + s0.xzyw*sh.xxyy ;
                let a1 = b1.xzyw + s1.xzyw*sh.zzww ;

                var p0 = vec3<f32>(a0.xy, h.x);
                var p1 = vec3<f32>(a0.zw, h.y);
                var p2 = vec3<f32>(a1.xy, h.z);
                var p3 = vec3<f32>(a1.zw, h.w);

                // Normalise gradients
                let norm = taylor_inv_sqrt(vec4<f32>(dot(p0,p0), dot(p1,p1), dot(p2, p2), dot(p3,p3)));
                p0 = p0 * norm.x;
                p1 = p1 * norm.y;
                p2 = p2 * norm.z;
                p3 = p3 * norm.w;

                // Mix final noise value
                var m = max(0.5 - vec4<f32>(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), vec4<f32>(0.0));
                m = m * m;
                return 105.0 * dot( m*m, vec4<f32>( dot(p0,x0), dot(p1,x1),
                                                    dot(p2,x2), dot(p3,x3) ) );
            }}

            fn fbm(p: vec3<f32>, octaves: u32, lacunarity: f32, gain: f32) -> f32 {{
                var total = 0.0;
                var amplitude = 1.0;
                var frequency = 1.0;
                var max_amplitude = 0.0;

                var pos = p;
                for (var i = 0u; i < octaves; i = i + 1u) {{
                    total = total + simplex_noise(pos * frequency) * amplitude;
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
