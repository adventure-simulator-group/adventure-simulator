#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}

struct TacticalTreeImpostorMaterial {
    parameters: vec4<f32>,
    leaf_light: vec4<f32>,
    leaf_shadow: vec4<f32>,
    bark: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> tree: TacticalTreeImpostorMaterial;

fn rotate_2d(point: vec2<f32>, angle: f32) -> vec2<f32> {
    let cosine = cos(angle);
    let sine = sin(angle);
    return vec2<f32>(
        point.x * cosine - point.y * sine,
        point.x * sine + point.y * cosine,
    );
}

fn ellipse_mask(uv: vec2<f32>, center: vec2<f32>, radius: vec2<f32>, angle: f32) -> f32 {
    let point = rotate_2d(uv - center, angle) / radius;
    return 1.0 - dot(point, point);
}

fn segment_mask(uv: vec2<f32>, start: vec2<f32>, end: vec2<f32>, width: f32) -> f32 {
    let line = end - start;
    let along = clamp(dot(uv - start, line) / max(dot(line, line), 0.0001), 0.0, 1.0);
    return width - distance(uv, start + line * along);
}

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
    out.world_normal = normalize(mix(world_normal, vec3<f32>(0.0, 1.0, 0.0), 0.34));
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

fn individual_leaf(uv: vec2<f32>) -> vec2<f32> {
    let longitudinal = abs(uv.y * 2.0 - 1.0);
    let half_width = pow(max(1.0 - longitudinal, 0.0), 0.58)
        * (0.39 + sin(uv.y * 31.0) * 0.018);
    return vec2<f32>(half_width - abs(uv.x - 0.5), -1.0);
}

fn leafed_twig(uv: vec2<f32>) -> vec2<f32> {
    var leaf = -1.0;
    for (var index = 0; index < 7; index += 1) {
        let along = f32(index) / 7.0;
        let side = select(-1.0, 1.0, (index & 1) == 0);
        let center = vec2<f32>(0.5 + side * (0.15 + along * 0.12), 0.17 + along * 0.105);
        leaf = max(leaf, ellipse_mask(uv, center, vec2<f32>(0.16, 0.09), side * 0.68));
    }
    leaf = max(leaf, ellipse_mask(uv, vec2<f32>(0.5, 0.91), vec2<f32>(0.13, 0.1), 0.0));
    let wood = segment_mask(uv, vec2<f32>(0.5, 0.02), vec2<f32>(0.5, 0.9), 0.018);
    return vec2<f32>(max(leaf, wood), wood);
}

fn small_branch(uv: vec2<f32>) -> vec2<f32> {
    var leaves = -1.0;
    var wood = segment_mask(uv, vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.94), 0.026);
    for (var index = 0; index < 11; index += 1) {
        let along = f32(index) / 11.0;
        let side = select(-1.0, 1.0, (index & 1) == 0);
        let branch_end = vec2<f32>(0.5 + side * (0.23 + along * 0.22), 0.19 + along * 0.068);
        wood = max(wood, segment_mask(uv, vec2<f32>(0.5, 0.17 + along * 0.63), branch_end, 0.014));
        leaves = max(leaves, ellipse_mask(uv, branch_end, vec2<f32>(0.2, 0.115), side * 0.42));
    }
    leaves = max(leaves, ellipse_mask(uv, vec2<f32>(0.5, 0.91), vec2<f32>(0.22, 0.14), 0.0));
    return vec2<f32>(max(leaves, wood), wood);
}

fn crown_branch(uv: vec2<f32>, seed: f32) -> vec2<f32> {
    var canopy = ellipse_mask(uv, vec2<f32>(0.5, 0.53), vec2<f32>(0.48, 0.43), 0.0);
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.31, 0.48), vec2<f32>(0.29, 0.31), -0.24));
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.69, 0.5), vec2<f32>(0.3, 0.34), 0.2));
    canopy += sin(uv.x * 43.0 + seed * 11.0) * sin(uv.y * 37.0 - seed * 7.0) * 0.07;
    let wood = max(
        segment_mask(uv, vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.68), 0.025),
        max(
            segment_mask(uv, vec2<f32>(0.5, 0.3), vec2<f32>(0.2, 0.62), 0.014),
            segment_mask(uv, vec2<f32>(0.5, 0.38), vec2<f32>(0.82, 0.7), 0.014),
        ),
    );
    return vec2<f32>(max(canopy, wood), wood);
}

fn whole_tree(uv: vec2<f32>, seed: f32) -> vec2<f32> {
    let trunk_half_width = mix(0.045, 0.018, smoothstep(0.0, 0.47, uv.y));
    let trunk = select(-1.0, trunk_half_width - abs(uv.x - 0.5), uv.y < 0.54);
    var canopy = ellipse_mask(uv, vec2<f32>(0.5, 0.69), vec2<f32>(0.34, 0.27), 0.0);
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.31, 0.62), vec2<f32>(0.24, 0.21), -0.12));
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.69, 0.63), vec2<f32>(0.25, 0.22), 0.15));
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.5, 0.84), vec2<f32>(0.23, 0.17), 0.0));
    canopy += sin(uv.x * 47.0 + seed * 13.0) * sin(uv.y * 41.0 - seed * 9.0) * 0.055;
    return vec2<f32>(max(canopy, trunk), trunk);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let level = i32(round(tree.parameters.x));
    var shape = vec2<f32>(-1.0, -1.0);
    if level == 0 {
        shape = individual_leaf(in.uv);
    } else if level == 1 {
        shape = leafed_twig(in.uv);
    } else if level == 2 {
        shape = small_branch(in.uv);
    } else if level == 3 {
        shape = crown_branch(in.uv, tree.parameters.y);
    } else {
        shape = whole_tree(in.uv, tree.parameters.y);
    }
    if shape.x < 0.0 {
        discard;
    }
    let leaf_height = smoothstep(0.0, 1.0, in.uv.y);
    let variation = 0.86 + 0.14 * sin(
        in.world_position.x * 1.37 + in.world_position.y * 2.11 + in.world_position.z * 0.91,
    );
    var color = mix(tree.leaf_shadow.rgb, tree.leaf_light.rgb, leaf_height * 0.72 + 0.18)
        * variation;
    if shape.y >= 0.0 {
        color = tree.bark.rgb * (0.72 + leaf_height * 0.24);
    }
    if level == 0 {
        let vein = 1.0 - smoothstep(0.012, 0.035, abs(in.uv.x - 0.5));
        color *= 1.0 - vein * 0.22;
    }
    let light_direction = normalize(vec3<f32>(0.35, 0.86, 0.25));
    let soft_light = 0.7 + 0.3 * abs(dot(normalize(in.world_normal), light_direction));
    return vec4<f32>(color * soft_light, 1.0);
}
