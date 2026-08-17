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
        // Whole-tree cards face the camera, so they cannot use the model's
        // rotated basis directly. They must still inherit its horizontal and
        // vertical scale. Dropping these lengths made every level-4 card use
        // the full baked-tree dimensions, even under a small tree parent.
        let horizontal_scale = length(world_from_local[0].xyz);
        let vertical_scale = length(world_from_local[1].xyz);
        world_position = vec4<f32>(
            root_world
                + right * vertex.position.x * horizontal_scale
                + vec3<f32>(0.0, vertex.position.y * vertical_scale, 0.0),
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
#ifdef VISIBILITY_RANGE_DITHER
    // Switch aggregate tree representations at the midpoint of Bevy's
    // complementary visibility interval. A translucent crossfade reads as a
    // pale duplicate crown, while Bevy's ordered 4x4 discard reads as a
    // screen-door grid. The atlas cutout itself still uses multisample
    // coverage; only the LOD handoff is intentionally crisp.
    if in.visibility_range_dither <= -8 || in.visibility_range_dither > 8 {
        discard;
    }
#endif
    var uv = in.uv;
    if i32(round(tree.parameters.x)) == 4 {
        let direction = normalize(view.world_position.xz - in.world_position.xz + vec2<f32>(0.0001, 0.0));
        let angle = atan2(direction.y, direction.x);
        // Bake-card right axes produce view normals one quarter turn behind
        // their authored angle. Select by that normal convention rather than
        // sampling the orthographic view from the opposite crown quadrant.
        let wrapped = fract(angle / 6.2831853 + 1.0 - 0.25);
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
    // The atlas already contains the source tree's small-scale normal and
    // occlusion variation. Apply only a broad hemispherical cosine here;
    // never add the old 78% direct-light floor on top of the baked response.
    // Aggregate cards are intentionally two-sided. Their authored normal
    // describes the card plane, not a literal one-sided leaf surface, so use
    // symmetric wrap lighting and retain enough diffuse fill for an oblique
    // card not to turn an otherwise healthy crown into a black cutout.
    let card_light = abs(dot(normalize(in.world_normal), light_direction));
    let normal_light = 0.25 + card_light * 0.75;
    let ambient_irradiance = tree.ambient.rgb * tree.ambient.w;
    let direct_irradiance = normal_light
        * tree.lighting.w
        * (1.0 - tree.ambient.w);
    return vec4<f32>(baked.rgb * (ambient_irradiance + vec3<f32>(direct_irradiance)), baked.a);
}
