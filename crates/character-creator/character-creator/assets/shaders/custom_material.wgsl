#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_functions
#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material_color: vec4<f32>;
// @group(#{MATERIAL_BIND_GROUP}) @binding(1) var material_color_texture: texture_2d<f32>;
// @group(#{MATERIAL_BIND_GROUP}) @binding(2) var material_color_sampler: sampler;

const LOCAL_SPHERE_RADIUS: f32 = 0.5;

@fragment
fn fragment(
    mesh: VertexOutput,
) -> @location(0) vec4<f32> {
    let world_from_local = mesh_functions::get_world_from_local(mesh.instance_index);
    let sphere_center = world_from_local[3].xyz;

    let axis_lengths = vec3<f32>(
        length(world_from_local[0].xyz),
        length(world_from_local[1].xyz),
        length(world_from_local[2].xyz),
    );
    let min_axis = min(axis_lengths.x, min(axis_lengths.y, axis_lengths.z));
    let sphere_radius = LOCAL_SPHERE_RADIUS * max(min_axis, 0.0001);

    let camera_position = view.world_position;
    let entry_point = mesh.world_position.xyz;
    let ray_dir = normalize(entry_point - camera_position);
    let ray_origin = entry_point;

    let oc = ray_origin - sphere_center;
    let half_b = dot(oc, ray_dir);
    let c = dot(oc, oc) - sphere_radius * sphere_radius;
    let discriminant = half_b * half_b - c;

    if discriminant <= 0.0 {
        discard;
    }

    let sqrt_d = sqrt(discriminant);
    var hit_distance = -half_b - sqrt_d;
    if hit_distance < 0.0 {
        hit_distance = -half_b + sqrt_d;
        if hit_distance < 0.0 {
            discard;
        }
    }

    let hit_position = ray_origin + ray_dir * hit_distance;
    let normal = normalize(hit_position - sphere_center);

    let light_dir = normalize(vec3<f32>(0.4, 0.8, 0.2));
    let diffuse = max(dot(normal, light_dir), 0.0);
    let ambient = 0.2;
    let view_dir = normalize(camera_position - hit_position);
    let specular = pow(max(dot(normal, normalize(light_dir + view_dir)), 0.0), 32.0);

    let lit_color = material_color.rgb * (ambient + diffuse) + specular * 0.25;
    return vec4<f32>(clamp(lit_color, vec3<f32>(0.0), vec3<f32>(1.0)), material_color.a);
}