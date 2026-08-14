#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct TacticalTerrainMaterial {
    base_color: vec4<f32>,
    grass_color: vec4<f32>,
    cover: vec4<f32>,
    weather: vec4<f32>,
    far_sward: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> terrain: TacticalTerrainMaterial;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var ground_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var ground_map_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let position = in.world_position.xyz;
    let normal = normalize(in.world_normal);
    let canopy = terrain.cover.x;
    let wetland = terrain.cover.y;
    let cultivation = terrain.cover.z;
    let water = terrain.cover.w;
    let wetness = terrain.weather.x;
    let snow = terrain.weather.y;
    let hilly = terrain.weather.z;
    let slope = smoothstep(0.16, 0.68, 1.0 - abs(normal.y));
    let ground_sample = textureSample(ground_map, ground_map_sampler, in.uv);
    let cover_kind = round(ground_sample.r * 255.0);
    let substrate_kind = round(ground_sample.g * 255.0);
    let tall_grass = 1.0 - step(0.5, abs(cover_kind - 1.0));
    let leaf_litter = 1.0 - step(0.5, abs(cover_kind - 2.0));

    // Ground uses only solid molded-material colors selected at hard gameplay
    // boundaries. There is no sampled albedo or normal-map surface detail.
    var color = terrain.base_color.rgb;
    if canopy >= 0.5 { color = vec3<f32>(0.105, 0.175, 0.072); }
    if cultivation >= 0.4 { color = vec3<f32>(0.33, 0.285, 0.105); }
    if wetland >= 0.4 || substrate_kind == 3.0 { color = vec3<f32>(0.18, 0.16, 0.105); }

    // Tall-grass ground and blades share one albedo pigment. Applying it
    // before exposed rock, litter, and water preserves those authoritative
    // surface types while hiding the final blade-geometry fade.
    if tall_grass > 0.5 {
        color = terrain.grass_color.rgb;
    }
    if substrate_kind == 1.0 || substrate_kind == 2.0 || slope >= 0.5 {
        color = vec3<f32>(0.31, 0.30, 0.275);
    }
    if leaf_litter > 0.5 { color = vec3<f32>(0.255, 0.16, 0.065); }
    if water >= 0.5 || substrate_kind == 5.0 { color = vec3<f32>(0.09, 0.18, 0.22); }

    // Use the same true camera distance that drives mesh visibility. Horizontal
    // distance alone left an overhead camera with culled blades but no
    // aggregate sward, revealing the playable terrain as a bare square.
    let camera_distance = distance(position, view.lod_view_world_position.xyz);

    // Past the geometric LOD, represent the aggregate colour and normal
    // response of a sward directly on the terrain. Frequencies stay low enough
    // to remain stable when minified in a WebGPU browser canvas.
    let sward_fade = smoothstep(terrain.far_sward.x, terrain.far_sward.y, camera_distance);
    let sward_amount = sward_fade
        * terrain.far_sward.z
        * tall_grass
        * (1.0 - water)
        * (1.0 - slope * 0.72);
    let sward_color = terrain.grass_color.rgb;
    // A stable world-space screen-door transition preserves discrete molded
    // colors while avoiding a circular hard band around the camera.
    let sward_cell = floor(position.xz * 2.0);
    let sward_dither = fract(sin(dot(sward_cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    color = select(color, sward_color, sward_dither < sward_amount);

    let snow_mask = snow * smoothstep(0.3, 0.86, normal.y);
    color = select(color, vec3<f32>(0.79, 0.84, 0.86), snow_mask >= 0.5);

    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    let base_roughness = 0.9 + wetness * 0.07 - water * 0.19;
    pbr_input.material.perceptual_roughness = clamp(base_roughness, 0.55, 1.0);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
