#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
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

struct TacticalTreeAggregateBarkExtension {
    surface: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> bark: TacticalTreeAggregateBarkExtension;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // Keep the tree mesh's large-scale normal and regular PBR light/fog
    // response. Aggregate branches intentionally omit material relief and the
    // trunk-only root-contact treatment.
    let macro_normal = normalize(select(-in.world_normal, in.world_normal, is_front));
    let micro_roughness = 0.0125
        * sin(dot(in.world_position.xyz, vec3<f32>(37.0, 53.0, 29.0)));
    pbr_input.world_normal = macro_normal;
    pbr_input.N = macro_normal;
    pbr_input.material.base_color = vec4<f32>(bark.surface.rgb, 1.0);
    pbr_input.material.perceptual_roughness = clamp(
        bark.surface.w + micro_roughness,
        0.62,
        0.94,
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
