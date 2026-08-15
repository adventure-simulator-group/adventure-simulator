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
var rock_diffuse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var rock_diffuse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var rock_normal_gl: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var rock_normal_gl_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var rock_arm: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var rock_arm_sampler: sampler;

struct TacticalRockMaterial {
    surface: vec4<f32>,
    geology: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(106)
var<uniform> rock: TacticalRockMaterial;

fn triplanar_weights(normal: vec3<f32>) -> vec3<f32> {
    let softened = pow(abs(normal), vec3<f32>(2.5));
    return softened / max(dot(softened, vec3<f32>(1.0)), 0.0001);
}

fn axis_uvs(position: vec3<f32>) -> mat3x2<f32> {
    let phase = rock.geology.x;
    let point = position * rock.geology.y;
    let x_uv = point.zy + vec2<f32>(phase * 0.071, phase * 0.113);
    let y_uv = point.xz + vec2<f32>(phase * 0.127, phase * 0.053);
    let z_uv = point.xy + vec2<f32>(phase * 0.089, phase * 0.137);
    return mat3x2<f32>(
        vec2<f32>(0.819 * x_uv.x - 0.574 * x_uv.y, 0.574 * x_uv.x + 0.819 * x_uv.y),
        vec2<f32>(0.906 * y_uv.x + 0.423 * y_uv.y, -0.423 * y_uv.x + 0.906 * y_uv.y),
        vec2<f32>(0.766 * z_uv.x - 0.643 * z_uv.y, 0.643 * z_uv.x + 0.766 * z_uv.y),
    );
}

fn triplanar_color(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec3<f32> {
    return textureSample(rock_diffuse, rock_diffuse_sampler, uvs[0]).rgb * weights.x
        + textureSample(rock_diffuse, rock_diffuse_sampler, uvs[1]).rgb * weights.y
        + textureSample(rock_diffuse, rock_diffuse_sampler, uvs[2]).rgb * weights.z;
}

fn triplanar_arm(uvs: mat3x2<f32>, weights: vec3<f32>) -> vec3<f32> {
    return textureSample(rock_arm, rock_arm_sampler, uvs[0]).rgb * weights.x
        + textureSample(rock_arm, rock_arm_sampler, uvs[1]).rgb * weights.y
        + textureSample(rock_arm, rock_arm_sampler, uvs[2]).rgb * weights.z;
}

fn triplanar_normal(
    uvs: mat3x2<f32>,
    weights: vec3<f32>,
    macro_normal: vec3<f32>,
) -> vec3<f32> {
    let nx = textureSample(rock_normal_gl, rock_normal_gl_sampler, uvs[0]).xyz * 2.0 - 1.0;
    let ny = textureSample(rock_normal_gl, rock_normal_gl_sampler, uvs[1]).xyz * 2.0 - 1.0;
    let nz = textureSample(rock_normal_gl, rock_normal_gl_sampler, uvs[2]).xyz * 2.0 - 1.0;
    let signs = select(vec3<f32>(-1.0), vec3<f32>(1.0), macro_normal >= vec3<f32>(0.0));
    let world_x = vec3<f32>(signs.x * nx.z, nx.y, nx.x);
    let world_y = vec3<f32>(ny.x, signs.y * ny.z, ny.y);
    let world_z = vec3<f32>(nz.x, nz.y, signs.z * nz.z);
    let mapped = normalize(world_x * weights.x + world_y * weights.y + world_z * weights.z);
    return normalize(mix(macro_normal, mapped, rock.geology.w));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let macro_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let weights = triplanar_weights(macro_normal);
    let uvs = axis_uvs(in.world_position.xyz);
    let sampled_color = triplanar_color(uvs, weights);
    let sampled_arm = triplanar_arm(uvs, weights);
    let composed_normal = triplanar_normal(uvs, weights, macro_normal);
    let lithology = rock.surface.rgb;

    pbr_input.world_normal = composed_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(sampled_color * lithology, 1.0);
    pbr_input.material.perceptual_roughness = clamp(sampled_arm.g + rock.surface.w, 0.54, 1.0);
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
