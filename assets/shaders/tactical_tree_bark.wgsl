#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
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

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var bark_diffuse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var bark_diffuse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var bark_normal_gl: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var bark_normal_gl_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var bark_arm: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var bark_arm_sampler: sampler;

struct TacticalTreeBarkMaterial {
    projection: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(106)
var<uniform> bark: TacticalTreeBarkMaterial;

fn projection_weights(normal: vec3<f32>) -> vec3<f32> {
    let weights = pow(abs(normal), vec3<f32>(bark.projection.w));
    return weights / max(dot(weights, vec3<f32>(1.0)), 0.0001);
}

fn projection_uvs(position: vec3<f32>) -> mat3x2<f32> {
    let horizontal = bark.projection.x;
    let vertical = bark.projection.y;
    return mat3x2<f32>(
        vec2<f32>(position.z * horizontal, position.y * vertical),
        position.xz * horizontal,
        vec2<f32>(position.x * horizontal, position.y * vertical),
    );
}

fn projected_color(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec3<f32> {
    return textureSample(bark_diffuse, bark_diffuse_sampler, uvs[0]).rgb * weights.x
        + textureSample(bark_diffuse, bark_diffuse_sampler, uvs[1]).rgb * weights.y
        + textureSample(bark_diffuse, bark_diffuse_sampler, uvs[2]).rgb * weights.z;
}

fn projected_arm(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec3<f32> {
    return textureSample(bark_arm, bark_arm_sampler, uvs[0]).rgb * weights.x
        + textureSample(bark_arm, bark_arm_sampler, uvs[1]).rgb * weights.y
        + textureSample(bark_arm, bark_arm_sampler, uvs[2]).rgb * weights.z;
}

fn projected_normal(
    uvs: mat3x2<f32>,
    weights: vec3<f32>,
    macro_normal: vec3<f32>,
) -> vec3<f32> {
    let nx = textureSample(bark_normal_gl, bark_normal_gl_sampler, uvs[0]).xyz * 2.0 - 1.0;
    let ny = textureSample(bark_normal_gl, bark_normal_gl_sampler, uvs[1]).xyz * 2.0 - 1.0;
    let nz = textureSample(bark_normal_gl, bark_normal_gl_sampler, uvs[2]).xyz * 2.0 - 1.0;
    let signs = select(vec3<f32>(-1.0), vec3<f32>(1.0), macro_normal >= vec3<f32>(0.0));
    let world_x = vec3<f32>(signs.x * nx.z, nx.y, nx.x);
    let world_y = vec3<f32>(ny.x, signs.y * ny.z, ny.y);
    let world_z = vec3<f32>(nz.x, nz.y, signs.z * nz.z);
    let mapped = normalize(world_x * weights.x + world_y * weights.y + world_z * weights.z);
    return normalize(mix(macro_normal, mapped, bark.projection.z));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let macro_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let weights = projection_weights(macro_normal);
    let uvs = projection_uvs(in.world_position.xyz);
    let sampled_color = projected_color(uvs, weights);
    let sampled_arm = projected_arm(uvs, weights);
    let composed_normal = projected_normal(uvs, weights, macro_normal);

    pbr_input.world_normal = composed_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(sampled_color, 1.0);
    pbr_input.material.perceptual_roughness = clamp(sampled_arm.g, 0.62, 1.0);
    pbr_input.diffuse_occlusion = clamp(
        pbr_input.diffuse_occlusion * sampled_arm.r,
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    pbr_input.specular_occlusion = clamp(
        pbr_input.specular_occlusion * sampled_arm.r,
        0.0,
        1.0,
    );
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
