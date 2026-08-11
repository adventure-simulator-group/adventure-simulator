#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> moon_light: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> moon_appearance: vec4<f32>;

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
    let maria = 0.9
        + 0.08 * sin(in.uv.x * 41.0 + sin(in.uv.y * 17.0) * 2.1)
        + 0.04 * sin(in.uv.y * 73.0 - in.uv.x * 11.0);
    let limb = pow(clamp(abs(dot(normal, normalize(-in.world_position.xyz))), 0.0, 1.0), 0.12);
    let radiance = moon_appearance.y
        * moon_light.w
        * maria
        * limb
        * mix(earthshine, 1.0, terminator);
    let lunar_color = vec3<f32>(1.0, 0.94, 0.80);
    return vec4<f32>(lunar_color * radiance, clamp(earthshine + terminator, 0.0, 1.0));
}
