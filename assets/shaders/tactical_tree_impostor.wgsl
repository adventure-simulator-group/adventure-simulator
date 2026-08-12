#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var baked_color: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var baked_color_sampler: sampler;

struct TacticalTreeImpostorMaterial {
    parameters: vec4<f32>,
    lighting: vec4<f32>,
    ambient: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var<uniform> tree: TacticalTreeImpostorMaterial;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    var world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    let level = i32(round(tree.parameters.x));
    if level == 4 {
        let root_world = mesh_functions::mesh_position_local_to_world(
            world_from_local,
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        ).xyz;
        let to_camera = normalize(view.world_position.xz - root_world.xz + vec2<f32>(0.0001, 0.0));
        let right = vec3<f32>(to_camera.y, 0.0, -to_camera.x);
        world_position = vec4<f32>(
            root_world + right * vertex.position.x + vec3<f32>(0.0, vertex.position.y, 0.0),
            1.0,
        );
        world_normal = vec3<f32>(to_camera.x, 0.18, to_camera.y);
    }
    let height_weight = smoothstep(0.05, 1.0, vertex.uv.y);
    let phase = globals.time * tree.parameters.w
        + world_position.x * 0.11
        + world_position.z * 0.073
        + tree.parameters.y * 6.2831853;
    let wind = (sin(phase) * 0.72 + sin(phase * 2.17 + 1.3) * 0.28)
        * tree.parameters.z
        * height_weight
        * height_weight;
    world_position.x += wind;
    world_position.z += wind * 0.38;

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = normalize(mix(world_normal, vec3<f32>(0.0, 1.0, 0.0), 0.22));
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;
    if i32(round(tree.parameters.x)) == 4 {
        let direction = normalize(view.world_position.xz - in.world_position.xz + vec2<f32>(0.0001, 0.0));
        let angle = atan2(direction.y, direction.x);
        let wrapped = fract(angle / 6.2831853 + 1.0);
        let view_index = u32(round(wrapped * 8.0)) % 8u;
        // The whole-tree atlas is laid out in three columns. Select the
        // nearest of eight real orthographic source renders.
        let column = view_index % 3u;
        let row = view_index / 3u;
        uv = vec2<f32>(
            (f32(column) + uv.x) / 3.0,
            (f32(row) + uv.y) / 3.0,
        );
    }
    let baked = textureSample(baked_color, baked_color_sampler, uv);
    if baked.a < 0.2 {
        discard;
    }
    let light_direction = normalize(tree.lighting.xyz);
    let normal_light = dot(normalize(in.world_normal), light_direction);
    let daylight = 0.78 + 0.22 * clamp(normal_light * 0.5 + 0.5, 0.0, 1.0);
    let direct_irradiance = daylight * tree.lighting.w;
    let ambient_irradiance = tree.ambient.rgb * tree.ambient.w;
    return vec4<f32>(baked.rgb * (ambient_irradiance + vec3<f32>(direct_irradiance)), baked.a);
}
