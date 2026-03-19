struct Uniforms {
    size: f32
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var distance_: texture_storage_3d<r32float, write>;
@group(0) @binding(2) var bone_index: texture_storage_3d<rgba8uint, write>;
@group(0) @binding(3) var bone_weight: texture_storage_3d<rgba32float, write>;

@compute @workgroup_size(8, 8, 4)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(distance_);
    if (global_id.x >= dims.x || global_id.y >= dims.y || global_id.z >= dims.z) {
        return;
    }
    
    let uvw = vec3<f32>(global_id) / vec3<f32>(dims);
    var o = (uvw * 2.0 - 1.0) * vec3(1.0, 1.0, 1.0) - vec3(0.0, -0.4, 0.0);
    
    let m = map(o);
    let distance = vec4<f32>(m.distance, 1.0, 1.0, 1.0);
    let bone = vec4<u32>(m.bone, 0, 0, 0);
    
    let point = vec3<i32>(i32(global_id.x), i32(global_id.y), i32(global_id.z));
    textureStore(distance_, point, distance);
    textureStore(bone_index, point, bone);
    textureStore(bone_weight, point, vec4(1.0, 0.0, 0.0, 0.0));
}
