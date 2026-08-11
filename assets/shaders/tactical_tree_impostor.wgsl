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

fn hash21(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn leaf_mask(uv: vec2<f32>, center: vec2<f32>, radius: vec2<f32>, angle: f32) -> f32 {
    let point = rotate_2d(uv - center, angle) / radius;
    let along = clamp(point.y * 0.5 + 0.5, 0.0, 1.0);
    let profile = pow(max(sin(along * 3.14159265), 0.0), 0.62);
    let asymmetry = 1.0 + point.y * 0.12;
    return profile * asymmetry - abs(point.x) + sin(along * 43.0) * 0.018;
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
    let centered = uv - vec2<f32>(0.5 + sin(uv.y * 3.14159265) * 0.025, 0.5);
    let along = clamp(centered.y + 0.5, 0.0, 1.0);
    let half_width = pow(max(sin(along * 3.14159265), 0.0), 0.64)
        * (0.76 + sin(along * 47.0) * 0.018);
    let blade = half_width - abs(centered.x * 2.0);
    let petiole = segment_mask(uv, vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.13), 0.018);
    return vec2<f32>(max(blade, petiole), petiole);
}

fn leafed_twig(uv: vec2<f32>) -> vec2<f32> {
    var leaf = -1.0;
    for (var index = 0; index < 15; index += 1) {
        let along = f32(index) / 15.0;
        let side = select(-1.0, 1.0, (index & 1) == 0);
        let center = vec2<f32>(0.5 + side * (0.11 + along * 0.1), 0.12 + along * 0.7);
        leaf = max(leaf, leaf_mask(uv, center, vec2<f32>(0.18, 0.09), side * 0.62));
    }
    leaf = max(leaf, leaf_mask(uv, vec2<f32>(0.5, 0.87), vec2<f32>(0.14, 0.1), 0.0));
    let wood = segment_mask(uv, vec2<f32>(0.5, 0.04), vec2<f32>(0.5, 0.87), 0.012);
    let visible_wood = select(wood, -1.0, leaf >= 0.0);
    return vec2<f32>(max(leaf, wood), visible_wood);
}

fn small_branch(uv: vec2<f32>) -> vec2<f32> {
    var leaves = ellipse_mask(uv, vec2<f32>(0.5, 0.56), vec2<f32>(0.3, 0.42), 0.0);
    var wood = segment_mask(uv, vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.83), 0.019);
    for (var index = 0; index < 9; index += 1) {
        let fi = f32(index);
        let angle = fi * 2.3999631 + 0.37;
        let radial = vec2<f32>(cos(angle), sin(angle));
        let center = vec2<f32>(0.5, 0.58) + radial * vec2<f32>(0.31, 0.3) * (0.45 + fi / 16.0);
        leaves = max(leaves, ellipse_mask(uv, center, vec2<f32>(0.22, 0.18), angle * 0.13));
        if index < 5 {
            wood = max(wood, segment_mask(uv, vec2<f32>(0.5, 0.33 + fi * 0.075), center, 0.009));
        }
    }
    leaves += sin(uv.x * 53.0) * sin(uv.y * 47.0) * 0.027;
    let visible_wood = select(wood, -1.0, leaves >= 0.0);
    return vec2<f32>(max(leaves, wood), visible_wood);
}

fn crown_branch(uv: vec2<f32>, seed: f32) -> vec2<f32> {
    var canopy = -1.0;
    for (var index = 0; index < 9; index += 1) {
        let fi = f32(index);
        let random = hash21(vec2<f32>(fi + seed * 17.0, seed * 31.0));
        let angle = fi * 2.3999631 + seed * 2.7;
        let center = vec2<f32>(0.5, 0.53)
            + vec2<f32>(cos(angle), sin(angle)) * vec2<f32>(0.22, 0.16) * (0.45 + random * 0.55);
        canopy = max(
            canopy,
            ellipse_mask(uv, center, vec2<f32>(0.27 + random * 0.07, 0.23 + random * 0.08), angle * 0.13),
        );
    }
    canopy += sin(uv.x * 61.0 + seed * 11.0) * sin(uv.y * 53.0 - seed * 7.0) * 0.035;
    let wood = max(
        segment_mask(uv, vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.68), 0.025),
        max(
            segment_mask(uv, vec2<f32>(0.5, 0.3), vec2<f32>(0.2, 0.62), 0.014),
            segment_mask(uv, vec2<f32>(0.5, 0.38), vec2<f32>(0.82, 0.7), 0.014),
        ),
    );
    let visible_wood = select(wood, -1.0, canopy >= 0.0);
    return vec2<f32>(max(canopy, wood), visible_wood);
}

fn whole_tree(uv: vec2<f32>, seed: f32) -> vec2<f32> {
    let bend = (seed - 0.5) * 0.055 * smoothstep(0.05, 0.58, uv.y);
    let trunk_center = 0.5 + bend;
    let trunk_half_width = mix(0.058, 0.014, smoothstep(0.03, 0.64, uv.y));
    var wood = select(-1.0, trunk_half_width - abs(uv.x - trunk_center), uv.y < 0.67);
    wood = max(wood, segment_mask(uv, vec2<f32>(trunk_center, 0.45), vec2<f32>(0.24, 0.72), 0.018));
    wood = max(wood, segment_mask(uv, vec2<f32>(trunk_center, 0.5), vec2<f32>(0.76, 0.76), 0.018));
    var canopy = -1.0;
    for (var index = 0; index < 13; index += 1) {
        let fi = f32(index);
        let random = hash21(vec2<f32>(fi * 1.7 + seed * 19.0, seed * 43.0));
        let angle = fi * 2.3999631 + seed * 3.1;
        let radial = vec2<f32>(cos(angle), sin(angle));
        let center = vec2<f32>(0.5 + bend, 0.68)
            + radial * vec2<f32>(0.25, 0.14) * (0.34 + random * 0.66);
        let radius = vec2<f32>(0.19 + random * 0.06, 0.13 + random * 0.05);
        canopy = max(canopy, ellipse_mask(uv, center, radius, angle * 0.11));
    }
    canopy = max(canopy, ellipse_mask(uv, vec2<f32>(0.5 + bend, 0.7), vec2<f32>(0.32, 0.24), 0.0));
    canopy += sin(uv.x * 73.0 + seed * 13.0) * sin(uv.y * 61.0 - seed * 9.0) * 0.032;
    let visible_wood = select(wood, -1.0, canopy >= 0.0);
    return vec2<f32>(max(canopy, wood), visible_wood);
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
    var fine_variation = 0.5 + 0.5
        * sin(in.world_position.x * 4.7 + in.world_position.y * 2.3)
        * sin(in.world_position.z * 4.1 - in.world_position.y * 3.7);
    if level >= 3 {
        fine_variation = 0.5 + 0.3
            * sin(in.uv.x * 21.0 + tree.parameters.y * 7.0)
            * sin(in.uv.y * 17.0 - tree.parameters.y * 11.0);
    }
    let crown_breakup = sin(in.uv.x * 17.0 + tree.parameters.y * 9.0)
        * sin(in.uv.y * 19.0 - tree.parameters.y * 13.0);
    let interior = smoothstep(0.0, 0.12, shape.x);
    let tonal = 0.78 + fine_variation * 0.18 + crown_breakup * select(0.035, 0.105, level >= 2);
    var color = mix(tree.leaf_shadow.rgb, tree.leaf_light.rgb, leaf_height * 0.56 + 0.25)
        * tonal
        * mix(0.8, 1.0, interior);
    if shape.y >= 0.0 {
        let bark_band = sin(in.uv.y * 87.0 + in.world_position.y * 9.0) * 0.055;
        color = tree.bark.rgb * (0.74 + leaf_height * 0.18 + bark_band);
    }
    if level == 0 {
        let vein = 1.0 - smoothstep(0.012, 0.035, abs(in.uv.x - 0.5));
        let side_veins = pow(abs(sin(in.uv.y * 31.4159 + abs(in.uv.x - 0.5) * 8.0)), 18.0);
        color *= 1.0 - vein * 0.18 - side_veins * 0.045;
    }
    let light_direction = normalize(vec3<f32>(0.35, 0.86, 0.25));
    let normal_light = dot(normalize(in.world_normal), light_direction);
    let wrapped_light = 0.62 + 0.3 * clamp(normal_light * 0.6 + 0.55, 0.0, 1.0);
    let transmission = pow(max(-normal_light, 0.0), 2.0) * 0.12;
    return vec4<f32>(color * wrapped_light + tree.leaf_light.rgb * transmission, 1.0);
}
