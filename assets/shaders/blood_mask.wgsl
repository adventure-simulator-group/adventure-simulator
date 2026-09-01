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
var blood_mask: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var blood_mask_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let blood = textureSample(blood_mask, blood_mask_sampler, in.uv).r;
    let dried_blood = vec3<f32>(0.035, 0.0003, 0.0007);
    pbr_input.material.base_color = vec4<f32>(
        mix(pbr_input.material.base_color.rgb, dried_blood, blood),
        pbr_input.material.base_color.a,
    );
    pbr_input.material.perceptual_roughness = mix(
        pbr_input.material.perceptual_roughness,
        0.82,
        blood,
    );
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
