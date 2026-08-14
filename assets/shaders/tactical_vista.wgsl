#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> vista_weather: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let terrain_normal = normalize(in.world_normal);
    let sward_coverage = clamp(pbr_input.material.base_color.a, 0.0, 1.0);
    var color = pbr_input.material.base_color.rgb;
    let slope = smoothstep(0.18, 0.62, 1.0 - abs(terrain_normal.y));

    // Distant blades become one hard-selected aggregate color instead of
    // subpixel geometry or textured surface detail.
    let sward = sward_coverage * (1.0 - slope * 0.82);
    let sward_color = color * vec3<f32>(0.80, 1.055, 0.70);
    color = select(color, sward_color, sward >= 0.5);

    // Exposed regional slopes use the same kind of solid molded-material
    // palette region as playable terrain, without triplanar albedo sampling.
    let molded_rock = vec3<f32>(0.31, 0.30, 0.275);
    let rock_amount = slope * (1.0 - sward_coverage * 0.48);
    color = select(color, molded_rock * 0.90, rock_amount >= 0.5);

    // The same authoritative weather snapshot drives near and vista terrain.
    // Snow remains slope-aware at regional scale and mutes the residual sward
    // instead of ending at the playable-mesh boundary.
    let snow_mask = vista_weather.y * smoothstep(0.28, 0.86, terrain_normal.y);
    color = select(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask >= 0.5);

    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    pbr_input.material.perceptual_roughness = mix(0.94, 0.82, rock_amount);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
