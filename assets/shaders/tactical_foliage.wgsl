#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::{globals, view},
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{
        apply_pbr_lighting,
        main_pass_post_lighting_processing,
        visibility_range_dither,
    },
    pbr_types::{
        STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT,
        STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT,
    },
    view_transformations::position_world_to_clip,
}

struct TacticalFoliageMaterial {
    wind: vec4<f32>,
    interaction: vec4<f32>,
    interaction_motion: vec4<f32>,
    shading: vec4<f32>,
    shape: vec4<f32>,
    quality: vec4<f32>,
    lighting: vec4<f32>,
    ambient: vec4<f32>,
    ground_mask_transform: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> foliage: TacticalFoliageMaterial;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var ground_mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var ground_mask_sampler: sampler;

fn rotate_between(
    vector: vec3<f32>,
    source_direction: vec3<f32>,
    target_direction: vec3<f32>,
) -> vec3<f32> {
    let axis = cross(source_direction, target_direction);
    let cosine = clamp(dot(source_direction, target_direction), -1.0, 1.0);
    return vector
        + cross(axis, vector)
        + cross(axis, cross(axis, vector)) / max(1.0 + cosine, 0.001);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var position = vertex.position;
    var root_local = vec3<f32>(0.0, 0.0, 0.0);
    var blade_threshold = 0.0;
    var blade_visibility = 1.0;
#ifdef VERTEX_UVS_B
    root_local = vec3<f32>(vertex.uv_b.x, 0.0, vertex.uv_b.y);
    blade_threshold = vertex.color.a;
#endif

    let root_world = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(root_local, 1.0),
    ).xyz;
    // Far and vista grass share the mesh/material with close grass, but a
    // uniform branch selects this deliberately small path before the costly
    // interactive curved-ribbon reconstruction below. Every draw has one
    // value, so this does not diverge grass instances within a draw.
    if foliage.quality.x > 0.5 {
        var position = vertex.position;
        var blade_visibility = 1.0;
        // `quality.y` is uniform per material/draw. Interior patches have a
        // conservatively verified authoritative TallGrass footprint, so they
        // skip both the mask read and boundary-only blade collapse.
        if foliage.quality.y < 0.5 {
            let mask_uv = root_world.xz * foliage.ground_mask_transform.xy
                + foliage.ground_mask_transform.zw;
            let ground_coverage = textureSampleLevel(
                ground_mask_texture,
                ground_mask_sampler,
                mask_uv,
                0.0,
            ).r;
            if blade_threshold > ground_coverage {
                blade_visibility = 0.0;
            }
        }
        let width_compensation = max(foliage.shape.w, 1.0);
        let adjusted_xz = root_local.xz
            + (position.xz - root_local.xz) * width_compensation;
        position = vec3<f32>(adjusted_xz.x, position.y, adjusted_xz.y);
        var world_position = mesh_functions::mesh_position_local_to_world(
            world_from_local,
            vec4<f32>(position, 1.0),
        );
        let height_fraction = clamp(vertex.uv.y, 0.0, 1.0);
        let wind_direction = normalize(foliage.wind.xy);
        let wind_phase = globals.time * foliage.wind.w
            + dot(root_world.xz, wind_direction) * 0.16;
        let broad_wind = sin(wind_phase) * foliage.wind.z * height_fraction * height_fraction;
        let wind_offset = wind_direction * broad_wind * blade_visibility;
        world_position = vec4<f32>(
            world_position.x + wind_offset.x,
            world_position.y,
            world_position.z + wind_offset.y,
            world_position.w,
        );

        out.world_position = world_position;
        out.position = position_world_to_clip(world_position.xyz);
        var cheap_world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
            vertex.normal,
            vertex.instance_index,
        ));
        cheap_world_normal.y = abs(cheap_world_normal.y);
        out.world_normal = normalize(mix(
            cheap_world_normal,
            vec3<f32>(0.0, 1.0, 0.0),
            foliage.shading.z,
        ));
        out.uv = vertex.uv;
#ifdef VERTEX_UVS_B
        out.uv_b = vertex.uv_b;
#endif
        out.color = vertex.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
        out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
        out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
            vertex.instance_index,
            vec4<f32>(root_world, 1.0),
        );
#endif
        return out;
    }
    let spatial_noise = sin(root_world.x * 0.071 + root_world.z * 0.113)
        * sin(root_world.x * 0.037 - root_world.z * 0.053);
    var ground_coverage = 1.0;
    if foliage.shape.x > 0.5 {
        if foliage.quality.y < 0.5 {
            let mask_uv = root_world.xz * foliage.ground_mask_transform.xy
                + foliage.ground_mask_transform.zw;
            ground_coverage = textureSampleLevel(
                ground_mask_texture,
                ground_mask_sampler,
                mask_uv,
                0.0,
            ).r;
            // Preserve the four-times-density close field, then open it into
            // irregular meadow clumps as it approaches the original-density LOD.
            // Boundary patches retain the authored mask's exact collapse.
            let meadow_zone = smoothstep(-0.36, 0.52, spatial_noise);
            let camera_distance = distance(root_world.xyz, view.lod_view_world_position.xyz);
            let distance_opening = smoothstep(7.0, 24.0, camera_distance);
            let clump_coverage = mix(1.0, mix(0.54, 1.0, meadow_zone), distance_opening);
            let effective_coverage = ground_coverage * clump_coverage;
            if blade_threshold > effective_coverage {
                blade_visibility = 0.0;
            }
        }
        position = root_local + (position - root_local) * blade_visibility;
    }
    // Density is represented by genuinely smaller LOD meshes. The previous
    // distance threshold collapsed rejected blades here, after their vertex
    // invocations had already begun, so it did not save vertex work.
    let width_compensation = max(foliage.shape.w, 1.0);
    // Near-LOD seed heads use negative V. Unlike a blade ribbon, their U is
    // the authored height where the rigid branch cluster attaches to its
    // stalk; the magnitude of V is that attachment's normalized height.
    let is_inflorescence = vertex.uv.y < 0.0;
    let bend = select(
        clamp(vertex.uv.y, 0.0, 1.0),
        clamp(-vertex.uv.y, 0.0, 1.0),
        is_inflorescence,
    );
    let wind_direction = normalize(foliage.wind.xy);
    let wind_cross = vec2<f32>(-wind_direction.y, wind_direction.x);
    // World-space vigor breaks the repeated shared-mesh silhouette without
    // adding blades or per-instance materials. The range matches ordinary
    // mixed-age meadow growth rather than scaling whole macro patches.
    let blade_variation = fract(
        blade_threshold * 1.73 + spatial_noise * 0.31
            + root_world.x * 0.013 - root_world.z * 0.017
    );
    let age_signal = fract(blade_threshold * 2.37 + 0.13);
    let mature_age = smoothstep(0.68, 0.94, age_signal);
    // A dense near field needs a broad juvenile-to-mature height mix or it
    // becomes an opaque vertical curtain despite varied roots.
    let juvenile_vigor = 0.28 + 0.78 * blade_variation * blade_variation;
    let mature_vigor = 0.48 + 0.58 * blade_variation;
    let meadow_zone = smoothstep(-0.36, 0.52, spatial_noise);
    // Short juvenile pockets expose ground between mature clumps. At an
    // authoritative dirt/grass boundary, surviving blades also grow shorter
    // rather than ending in a same-height density wall.
    let edge_growth = mix(0.26, 1.0, smoothstep(0.04, 0.92, ground_coverage));
    let blade_vigor = mix(juvenile_vigor, mature_vigor, meadow_zone)
        * edge_growth
        * mix(1.0, 0.94, mature_age);
    let wave_position = dot(root_world.xz, wind_direction) * 0.22;
    let wind_time = globals.time * foliage.wind.w;
    let gust = 0.68 + 0.32 * sin(wind_time * 0.29 + spatial_noise * 3.7);
    let primary_wave = sin(wind_time + wave_position + spatial_noise * 1.9);
    let flutter = sin(wind_time * 2.73 - wave_position * 1.61 + spatial_noise * 5.3);
    // Algebraically decorrelate lean magnitude from the shared vigor signal;
    // this keeps per-blade variety without another trigonometric evaluation.
    let lean_variation = fract(blade_variation * 1.618 + blade_threshold * 0.73);
    let lean_direction = normalize(vec2<f32>(
        fract(blade_threshold * 1.37 + 0.17) - 0.5,
        fract(blade_threshold * 2.11 + 0.63) - 0.5,
    ) + vec2<f32>(0.0001, 0.0));
    let lean_amount = (0.008 + 0.010 * lean_variation) * foliage.shading.w;
    let natural_lean = lean_direction * (lean_amount + 0.004 * mature_age);
    let wind_offset = (
        wind_direction * primary_wave * gust
        + wind_cross * flutter * 0.18
    ) * foliage.wind.z;
    var interaction_offset = vec2<f32>(0.0, 0.0);
    var interaction_droop = 0.0;
    if foliage.interaction.w > 0.0 && foliage.shading.w > 0.5 {
        let from_player = root_world.xz - foliage.interaction.xz;
        let player_distance = length(from_player);
        let velocity_xz = foliage.interaction_motion.xz;
        let fallback_direction = normalize(velocity_xz + vec2<f32>(0.0001, 0.0));
        let push_direction = select(
            fallback_direction,
            normalize(from_player),
            player_distance > 0.001,
        );
        let player_push = (1.0 - smoothstep(0.18, foliage.interaction.w, player_distance))
            * foliage.interaction_motion.w;
        let motion_direction = normalize(velocity_xz + push_direction * 0.15);
        interaction_offset = (
            push_direction * 0.62 + motion_direction * min(length(velocity_xz) * 0.035, 0.28)
        ) * player_push;
        interaction_droop = player_push * 0.34;
    }

    let original_world_normal = normalize(mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    ));
    var world_position: vec4<f32>;
    var shaped_world_normal = original_world_normal;
    if foliage.shape.x > 0.5 {
        // Each grass blade is a single longitudinal ribbon. Its sampled rows
        // follow one cubic curve, rather than translating a rigid card.
        let t = bend;
        let one_minus_t = 1.0 - t;
        let curve_profile = 3.0 * one_minus_t * one_minus_t * t * 0.06
            + 3.0 * one_minus_t * t * t * 0.5
            + t * t * t;
        let curve_derivative = 3.0 * one_minus_t * one_minus_t * 0.06
            + 6.0 * one_minus_t * t * (0.5 - 0.06)
            + 3.0 * t * t * (1.0 - 0.5);
        let authored_facing = normalize(original_world_normal.xz + vec2<f32>(0.0001, 0.0));
        let authored_amount = 0.65 + 0.30 * fract(blade_threshold * 1.91 + blade_variation * 0.47);
        let total_curve = authored_facing * foliage.shape.z * authored_amount
            + natural_lean
            + wind_offset
            + interaction_offset;

        let curve_offset = total_curve * curve_profile * blade_visibility;
        let world_up = normalize(
            (world_from_local * vec4<f32>(0.0, 1.0, 0.0, 0.0)).xyz
        );
        let tangent = normalize(vec3<f32>(
            world_up.x * blade_vigor + total_curve.x * curve_derivative,
            world_up.y * blade_vigor - interaction_droop * 2.0 * t,
            world_up.z * blade_vigor + total_curve.y * curve_derivative,
        ));

        if is_inflorescence {
            let authored_anchor_y = vertex.uv.x * blade_visibility;
            let anchor_local = vec3<f32>(
                root_local.x,
                authored_anchor_y * blade_vigor,
                root_local.z,
            );
            let anchor_world = mesh_functions::mesh_position_local_to_world(
                world_from_local,
                vec4<f32>(anchor_local, 1.0),
            );
            let authored_offset_local = vec3<f32>(
                position.x - root_local.x,
                (position.y - authored_anchor_y) * blade_vigor,
                position.z - root_local.z,
            ) * blade_visibility;
            let authored_offset_world = (
                world_from_local * vec4<f32>(authored_offset_local, 0.0)
            ).xyz;
            let bent_offset = rotate_between(authored_offset_world, world_up, tangent);
            world_position = vec4<f32>(
                anchor_world.x + curve_offset.x + bent_offset.x,
                anchor_world.y - interaction_droop * t * t * blade_visibility + bent_offset.y,
                anchor_world.z + curve_offset.y + bent_offset.z,
                anchor_world.w,
            );
            shaped_world_normal = normalize(rotate_between(
                original_world_normal,
                world_up,
                tangent,
            ));
        } else {
            let centre_local = vec3<f32>(root_local.x, position.y * blade_vigor, root_local.z);
            let centre_world = mesh_functions::mesh_position_local_to_world(
                world_from_local,
                vec4<f32>(centre_local, 1.0),
            );

            // Rotate a ribbon toward the camera only as it becomes edge-on.
            // Seed heads bypass this reconstruction and retain their authored
            // multi-quad silhouette in the branch above.
            let local_side = normalize(vec2<f32>(-vertex.normal.z, vertex.normal.x));
            let transformed_side = (world_from_local
                * vec4<f32>(local_side.x, 0.0, local_side.y, 0.0)).xz;
            let side_scale = length(transformed_side);
            let original_side = normalize(transformed_side + vec2<f32>(0.0001, 0.0));
            let to_camera = normalize(view.lod_view_world_position.xz - root_world.xz
                + vec2<f32>(0.0001, 0.0));
            var camera_side = vec2<f32>(-to_camera.y, to_camera.x);
            camera_side = select(camera_side, -camera_side, dot(camera_side, original_side) < 0.0);
            let edge_on = 1.0 - smoothstep(
                0.08,
                0.38,
                abs(dot(original_world_normal.xz, to_camera)),
            );
            let visible_side = normalize(mix(
                original_side,
                camera_side,
                edge_on * foliage.shape.y,
            ));
            // Recover only authored displacement along the ribbon side. The
            // perpendicular component is centreline lean, not blade width.
            // Shared centre vertices use u=0.5 and must reconstruct to zero
            // width so the terminal triangle cannot collapse onto a shoulder.
            let authored_half_width = abs(dot(
                position.xz - root_local.xz,
                local_side,
            ));
            let is_centre_vertex = abs(vertex.uv.x - 0.5) < 0.001;
            let half_width = select(
                authored_half_width * side_scale * width_compensation,
                0.0,
                is_centre_vertex,
            );
            let signed_side = select(-1.0, 1.0, vertex.uv.x >= 0.5);
            let side_offset = visible_side * half_width * signed_side;
            world_position = vec4<f32>(
                centre_world.x + curve_offset.x + side_offset.x,
                centre_world.y - interaction_droop * t * t * blade_visibility,
                centre_world.z + curve_offset.y + side_offset.y,
                centre_world.w,
            );

            shaped_world_normal = normalize(cross(
                tangent,
                vec3<f32>(visible_side.x, 0.0, visible_side.y),
            ));
        }
    } else {
        let adjusted_xz = root_local.xz
            + (position.xz - root_local.xz) * width_compensation;
        position = vec3<f32>(
            adjusted_xz.x,
            position.y,
            adjusted_xz.y,
        );
        world_position = mesh_functions::mesh_position_local_to_world(
            world_from_local,
            vec4<f32>(position, 1.0),
        );
        let card_bend = (natural_lean + wind_offset + interaction_offset) * bend * bend;
        world_position = vec4<f32>(
            world_position.x + card_bend.x,
            world_position.y - interaction_droop * bend * bend,
            world_position.z + card_bend.y,
            world_position.w,
        );
    }

    out.world_position = world_position;
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = normalize(mix(
        shaped_world_normal,
        vec3<f32>(0.0, 1.0, 0.0),
        foliage.shading.z,
    ));
    out.uv = vertex.uv;
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
    out.color = vertex.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        vec4<f32>(root_world, 1.0),
    );
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var lod_coverage = 1.0;
#ifdef VISIBILITY_RANGE_DITHER
    // Convert Bevy's sixteen-step visibility value into continuous coverage.
    // With AlphaToCoverage this becomes four hardware sample levels instead
    // of a conspicuous 4x4 checkerboard of discarded grass fragments.
    lod_coverage = clamp(
        1.0 - abs(f32(in.visibility_range_dither)) / 16.0,
        0.0,
        1.0,
    );
    if lod_coverage <= 0.0 {
        discard;
    }
#endif
    if foliage.quality.z > 0.5 {
        // A two-sided Lambert response is adequate once individual blades are
        // below close-reading distance. It replaces diffuse transmission,
        // specular BRDF evaluation, and the full clustered PBR lighting path.
        var normal = select(-in.world_normal, in.world_normal, is_front);
        normal.y = abs(normal.y);
        normal = normalize(normal);
        let light_response = clamp(
            (dot(normal, normalize(foliage.lighting.xyz)) + 0.24) / 1.24,
            0.0,
            1.0,
        );
        let direct = (0.30 + 0.70 * light_response)
            * foliage.lighting.w
            * (1.0 - foliage.ambient.w);
        let irradiance = foliage.ambient.rgb * foliage.ambient.w + vec3<f32>(direct);
        let height_fraction = clamp(in.uv.y, 0.0, 1.0);
        let root_self_shadow = mix(
            foliage.shading.x,
            1.0,
            pow(height_fraction, 0.72),
        );
        let centre_distance = abs(in.uv.x - 0.5) * 2.0;
        let centre_rib = mix(0.84, 1.0, smoothstep(0.12, 0.72, centre_distance));
        var pbr_input = pbr_input_from_vertex_output(in, is_front, true);
        pbr_input.material.flags = STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
        var out: FragmentOutput;
        out.color = main_pass_post_lighting_processing(
            pbr_input,
            vec4<f32>(
                in.color.rgb
                    * irradiance
                    * root_self_shadow
                    * centre_rib
                    * foliage.quality.w,
                lod_coverage,
            ),
        );
        return out;
    }
    let height_fraction = clamp(in.uv.y, 0.0, 1.0);
    let root_self_shadow = mix(
        foliage.shading.x,
        1.0,
        pow(height_fraction, 0.72),
    );
    let centre_distance = abs(in.uv.x - 0.5) * 2.0;
    let centre_rib = mix(0.84, 1.0, smoothstep(0.12, 0.72, centre_distance));
    var base_normal = select(-in.world_normal, in.world_normal, is_front);
    // The vertex stage deliberately bends both sides of a blade normal toward
    // the sky. Flipping the entire vector for the back face would point that
    // bias underground and blacken half the ribbons. Preserve the opposing
    // horizontal component while keeping both thin faces in the upper
    // hemisphere, analogous to foliage normal smoothing.
    base_normal.y = abs(base_normal.y);
    base_normal = normalize(base_normal);
    var pbr_input = pbr_input_from_vertex_output(in, is_front, true);
    pbr_input.material.flags = STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT
        | STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    pbr_input.material.base_color = vec4<f32>(
        in.color.rgb,
        lod_coverage,
    );
    // Ground foliage is grass. Its waxy cuticle produces a tighter highlight
    // than the shared dry-twig and woodland-plant response, allowing the
    // differently oriented blade normals to separate without albedo noise.
    pbr_input.material.perceptual_roughness = select(
        0.86,
        0.60,
        foliage.shape.x > 0.5,
    );
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.16);
    // Living blades are thin enough for broad diffuse transmission. This is
    // evaluated by the same shadow-aware PBR path as other foliage.
    pbr_input.material.diffuse_transmission = 0.36;
    pbr_input.material.thickness = 0.001;
    pbr_input.world_normal = base_normal;
    pbr_input.N = base_normal;
    pbr_input.diffuse_occlusion = vec3<f32>(root_self_shadow * centre_rib);
    pbr_input.specular_occlusion = root_self_shadow * centre_rib;

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
