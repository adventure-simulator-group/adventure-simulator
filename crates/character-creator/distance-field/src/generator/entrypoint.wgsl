struct Uniforms {
    size: f32
};

@group(0) @binding(0) var output_tex: texture_storage_3d<r32float, write>;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;

@compute @workgroup_size(8, 8, 4)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(output_tex);
    if (global_id.x >= dims.x || global_id.y >= dims.y || global_id.z >= dims.z) {
        return;
    }
    
    let uvw = vec3<f32>(global_id) / vec3<f32>(dims);
    var o = (uvw * 2.0 - 1.0) * vec3(1.0, -1.0, 1.0);
    
    let distance = map(o);
    let color = vec4<f32>(distance, 1.0, 1.0, 1.0);
    
    let point = vec3<i32>(i32(global_id.x), i32(dims.y) - i32(global_id.y), i32(global_id.z));
    textureStore(output_tex, point, color);
}
