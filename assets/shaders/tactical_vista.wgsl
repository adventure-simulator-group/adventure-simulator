#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var rock_diffuse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var rock_diffuse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> vista_weather: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let position = in.world_position.xyz;
    let terrain_normal = normalize(in.world_normal);
    let sward_coverage = clamp(pbr_input.material.base_color.a, 0.0, 1.0);
    var color = pbr_input.material.base_color.rgb;
    let slope = smoothstep(0.18, 0.62, 1.0 - abs(terrain_normal.y));

    // Distant blades become a band-limited aggregate reflectance and normal
    // response instead of subpixel geometry. Frequencies are deliberately
    // non-harmonic with the vista grid, preventing cell-sized repetition.
    let sward = sward_coverage * (1.0 - slope * 0.82);
    let sward_color = color * vec3<f32>(0.80, 1.055, 0.70);
    color = select(color, sward_color, sward >= 0.5);

    // Reuse the generated production rock surface on exposed regional slopes. Triplanar
    // sampling keeps mountain texture scale independent of coarse vista UVs.
    let weights = abs(terrain_normal) / max(dot(abs(terrain_normal), vec3<f32>(1.0)), 0.001);
    let scale = 0.035;
    let rock_yz = textureSample(rock_diffuse, rock_diffuse_sampler, position.yz * scale).rgb;
    let rock_xz = textureSample(rock_diffuse, rock_diffuse_sampler, position.xz * scale).rgb;
    let rock_xy = textureSample(rock_diffuse, rock_diffuse_sampler, position.xy * scale).rgb;
    let molded_rock = rock_yz * weights.x + rock_xz * weights.y + rock_xy * weights.z;
    let rock_amount = slope * (1.0 - sward_coverage * 0.48);
    color = select(color, molded_rock * 0.90, rock_amount >= 0.5);

    // The same authoritative weather snapshot drives near and vista terrain.
    // Snow remains slope-aware at regional scale and mutes the residual sward
    // instead of ending at the playable-mesh boundary.
    let snow_mask = vista_weather.y * smoothstep(0.28, 0.86, terrain_normal.y);
    color = select(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask >= 0.5);

    let micro = vec3<f32>(
        cos(position.x * 0.071 + position.z * 0.029),
        0.0,
        sin(position.z * 0.067 - position.x * 0.031),
    );
    let composed_normal = normalize(
        terrain_normal + micro * (0.014 * sward + 0.022 * rock_amount) * (1.0 - snow_mask)
    );
    pbr_input.world_normal = composed_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    pbr_input.material.perceptual_roughness = mix(0.94, 0.82, rock_amount);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
