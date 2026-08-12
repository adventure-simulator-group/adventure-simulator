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

struct TacticalRockMaterial {
    surface: vec4<f32>,
    geology: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> rock: TacticalRockMaterial;

fn geological_field(position: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    let phase = rock.geology.x;
    let macro_point = position * rock.geology.y;
    let grain_point = position * rock.geology.z;
    let macro_value = sin(dot(macro_point, vec3<f32>(1.13, 0.71, 0.93)) + phase)
        * cos(dot(macro_point, vec3<f32>(-0.47, 1.29, 0.63)) - phase * 0.37);
    let grain_value = sin(dot(grain_point, vec3<f32>(0.83, 1.37, -0.59)) - phase * 1.17)
        * cos(dot(grain_point, vec3<f32>(1.41, -0.43, 0.77)) + phase * 0.61);
    let inclusion_a = dot(grain_point, vec3<f32>(1.91, 0.37, 1.43)) + phase * 0.83;
    let inclusion_b = dot(grain_point, vec3<f32>(-0.29, 2.07, 0.61)) - phase * 0.49;
    let inclusion_value = sin(inclusion_a) * cos(inclusion_b);
    let field = macro_value * 0.54 + grain_value * 0.29 + inclusion_value * 0.17;
    // Analytic derivatives keep albedo and normal response tied to the same
    // bounded field without texture fetches or screen-space noise.
    let macro_a = dot(macro_point, vec3<f32>(1.13, 0.71, 0.93)) + phase;
    let macro_b = dot(macro_point, vec3<f32>(-0.47, 1.29, 0.63)) - phase * 0.37;
    let grain_a = dot(grain_point, vec3<f32>(0.83, 1.37, -0.59)) - phase * 1.17;
    let grain_b = dot(grain_point, vec3<f32>(1.41, -0.43, 0.77)) + phase * 0.61;
    let gradient = (
        cos(macro_a) * cos(macro_b) * vec3<f32>(1.13, 0.71, 0.93)
        - sin(macro_a) * sin(macro_b) * vec3<f32>(-0.47, 1.29, 0.63)
    ) * rock.geology.y * 0.54 + (
        cos(grain_a) * cos(grain_b) * vec3<f32>(0.83, 1.37, -0.59)
        - sin(grain_a) * sin(grain_b) * vec3<f32>(1.41, -0.43, 0.77)
    ) * rock.geology.z * 0.29 + (
        cos(inclusion_a) * cos(inclusion_b) * vec3<f32>(1.91, 0.37, 1.43)
        - sin(inclusion_a) * sin(inclusion_b) * vec3<f32>(-0.29, 2.07, 0.61)
    ) * rock.geology.z * 0.17;
    let tangent_gradient = gradient - normal * dot(gradient, normal);
    return vec4<f32>(tangent_gradient, field);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let base_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let geology = geological_field(in.world_position.xyz, base_normal);
    let cavity = 1.0 - smoothstep(-0.78, -0.15, geology.w);
    let mineral = smoothstep(0.38, 0.82, geology.w);
    let color_variation = 0.91 + geology.w * 0.11 + mineral * 0.055 - cavity * 0.055;
    let composed_normal = normalize(base_normal - geology.xyz * rock.geology.w);
    pbr_input.world_normal = composed_normal;
    pbr_input.N = composed_normal;
    pbr_input.material.base_color = vec4<f32>(rock.surface.rgb * color_variation, 1.0);
    pbr_input.material.perceptual_roughness = clamp(
        rock.surface.w + geology.w * 0.035 + cavity * 0.055,
        0.58,
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
