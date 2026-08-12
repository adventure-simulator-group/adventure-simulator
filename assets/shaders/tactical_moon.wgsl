#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> moon_light: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> moon_appearance: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var moon_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var moon_albedo_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let direct = max(dot(normal, normalize(moon_light.xyz)), 0.0);
    let terminator = smoothstep(0.0, 0.035, direct);
    let earthshine = moon_appearance.x;
    // The source is an sRGB base-colour map. Bevy decodes it to linear here;
    // normalizing by representative lunar reflectance preserves the existing
    // shared disc-radiance scale without adding an asset-specific light.
    let lunar_reflectance = textureSample(moon_albedo, moon_albedo_sampler, in.uv).rgb;
    let relative_albedo = lunar_reflectance / vec3<f32>(0.18);
    let to_viewer = normalize(view.world_position - in.world_position.xyz);
    let emission_cosine = clamp(dot(normal, to_viewer), 0.0, 1.0);
    // A restrained Lommel-Seeliger response avoids the plastic Lambert-sphere
    // limb while remaining finite on the terminator.
    let lunar_scattering = direct / max(direct + emission_cosine, 0.025);
    let radiance = moon_appearance.y
        * moon_light.w
        * mix(earthshine, max(lunar_scattering * 2.0, earthshine), terminator);
    return vec4<f32>(relative_albedo * radiance, 1.0);
}
