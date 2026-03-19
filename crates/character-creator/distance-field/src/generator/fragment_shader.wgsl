struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Uniforms {
    time: f32,
    resolution: vec2<f32>
};

const NUMBER_OF_COLORS: u32 = 16;

const COLORS: array<vec3<f32>, NUMBER_OF_COLORS> = array<vec3<f32>, NUMBER_OF_COLORS>(
    vec3<f32>(0.25, 0.0, 0.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 1.0, 1.0),
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(1.0, 0.0, 1.0),
    vec3<f32>(1.0, 1.0, 0.0),
    vec3<f32>(1.0, 1.0, 1.0),
    vec3<f32>(0.5, 0.5, 0.5),
    vec3<f32>(0.5, 0.5, 1.0),
    vec3<f32>(0.5, 1.0, 0.0),
    vec3<f32>(0.5, 1.0, 1.0),
    vec3<f32>(1.0, 0.5, 0.5),
    vec3<f32>(1.0, 0.5, 1.0),
    vec3<f32>(1.0, 1.0, 0.5),
    vec3<f32>(1.0, 1.0, 1.0),
);

@group(0) @binding(0) var my_sampler: sampler;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;
@group(0) @binding(2) var distance_: texture_3d<f32>;
@group(0) @binding(3) var bone_index: texture_3d<u32>;

fn map(p: vec3<f32>) -> f32 {
    return textureSampleLevel(distance_, my_sampler, p * 0.25, 0.0).x;
}

fn calcNormal(p: vec3<f32>) -> vec3<f32> {
    let texel = 1.0 / vec3<f32>(textureDimensions(distance_));
    let e = texel.x;

    let ex = vec3<f32>(e, 0.0, 0.0);
    let ey = vec3<f32>(0.0, e, 0.0);
    let ez = vec3<f32>(0.0, 0.0, e);

    let n = vec3<f32>(
        map(p + ex) - map(p - ex),
        map(p + ey) - map(p - ey),
        map(p + ez) - map(p - ez),
    );

    return normalize(n);
}

struct Raymarch {
    hit: bool,
    point: vec3<f32>,
    normal: vec3<f32>,
    distance: f32,
};

fn raymarch(origin: vec3<f32>, direction: vec3<f32>) -> Raymarch {
    const MAX_STEPS = 40;
    const SURF_DIST = 0.01;

    var t = 0.0;
    for (var i = 0; i < MAX_STEPS; i = i + 1) {
        var p = origin + direction * t;
        let d = map(p);
        let n = calcNormal(p);
        let cosTheta = abs(dot(direction, n));
        var step = d * (1.0 + 1.0 * (1.0 - cosTheta));
        if (d < SURF_DIST) {
            /*
            var a = t - step;
            var b = t;
			for(var i = 0; i < 0; i++) {
				t = (a + b) * 0.5;
				let d = map(origin + direction * t);
				if(d <= 0.) {
				    b = t;
                } else {
                    a = t;
                }
			}
			let p = origin + direction * t;
			*/
            return Raymarch(true, p, n, t);
        }
        t += step;
    }

    return Raymarch(false, vec3(0.0), vec3(0.0), t);
}

fn raytrace(origin: vec3<f32>, direction: vec3<f32>, bounces: i32) -> vec3<f32> {
    let light_dir = normalize(vec3(1.0, -1.0, -1.0));
    const sphere_color = vec3(1.0, 0.9, 0.8);

    var color = vec3(0.0);
    var ro = origin;
    var rd = direction;
    var t = 0.0;
    for (var i = 0; i < 3; i++) {
        var r0 = raymarch(ro, rd);    
        if (r0.hit) {
            t += r0.distance;
            // Direct lighting at first hit
            let luminance = max(dot(light_dir, r0.normal), 0.0) * 0.5;
            color += sphere_color * luminance / t * 2.0;

            // Reflection ray
            rd = normalize(reflect(rd, r0.normal));
            ro = r0.point + rd * 0.1;
        } else {
            break;
        }
    }
    return color;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv.xy * 2.0 - 1.0) * (uniforms.resolution.x / uniforms.resolution.y);
    let rd = normalize(vec3(p, 1.0)) * vec3(1.0, 1.0, 1.0);
    let ro = vec3<f32>(2.0, 2.0, 1.0);
   
    var color = vec3(0.0, 0.0, 0.0);
    let result = raymarch(ro, rd);
    if result.hit {
        let index: vec4<u32> = textureLoad(bone_index, vec3<u32>(result.point * vec3<f32>(textureDimensions(bone_index).xyz) * 0.25), 0);
        color = COLORS[index.x % NUMBER_OF_COLORS];
        color = color * (0.25 + 0.75 * dot(result.normal, normalize(vec3(1.0, -1.0, -1.0))));
    }
    
    return vec4<f32>(color, 1.0);
}
