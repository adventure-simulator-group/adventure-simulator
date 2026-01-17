#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::forward_io::VertexOutput

struct SphereTracingMaterial {
    color: vec4<f32>,
    sphere_params: vec4<f32>,
};

@group(2) @binding(0) var<storage, read> material: SphereTracingMaterial;

fn sphere_sdf(p: vec3<f32>, c: vec3<f32>, r: f32) -> f32 {
    return length(p - c) - r;
}

fn ray_march(ro: vec3<f32>, rd: vec3<f32>) -> f32 {
    var depth = 0.0;
    let sphere_center = material.sphere_params.xyz;
    let sphere_radius = material.sphere_params.w;
    
    for (var i = 0; i < 100; i++) {
        let p = ro + rd * depth;
        let dist = sphere_sdf(p, sphere_center, sphere_radius);
        if (dist < 0.001) {
            return depth;
        }
        depth += dist;
        if (depth > 20.0) {
            break;
        }
    }
    return -1.0;
}

@fragment
fn fragment(
    mesh: VertexOutput,
) -> @location(0) vec4<f32> {
    let ro = view.world_position;
    let rd = normalize(mesh.world_position.xyz - ro);

    let t = ray_march(ro, rd);

    if (t < 0.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 0.1); 
    }

    let p = ro + rd * t;
    let eps = 0.001;
    let sphere_center = material.sphere_params.xyz;
    let sphere_radius = material.sphere_params.w;
    let n = normalize(vec3<f32>(
        sphere_sdf(p + vec3<f32>(eps, 0.0, 0.0), sphere_center, sphere_radius) - sphere_sdf(p - vec3<f32>(eps, 0.0, 0.0), sphere_center, sphere_radius),
        sphere_sdf(p + vec3<f32>(0.0, eps, 0.0), sphere_center, sphere_radius) - sphere_sdf(p - vec3<f32>(0.0, eps, 0.0), sphere_center, sphere_radius),
        sphere_sdf(p + vec3<f32>(0.0, 0.0, eps), sphere_center, sphere_radius) - sphere_sdf(p - vec3<f32>(0.0, 0.0, eps), sphere_center, sphere_radius)
    ));

    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let diff = max(dot(n, light_dir), 0.0);
    
    return vec4<f32>(vec3<f32>(1.0) * (diff + 0.1), 1.0);
}
