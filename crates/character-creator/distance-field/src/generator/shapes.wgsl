struct Map {
    distance: f32,
    bone: u32
}

fn MapMin(a: Map, b: Map) -> Map {
    if a.distance < b.distance {
        return a;
    } else {
        return b;
    }
}

// Shapes

fn sdBox(p: vec3<f32>, b: vec3<f32>) -> f32 {
  let q = abs(p) - b;
  return length(max(q, vec3(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdCappedCone(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, ra: f32, rb: f32) -> f32 {
    let rba  = rb-ra;
    let baba = dot(b-a,b-a);
    let papa = dot(p-a,p-a);
    let paba = dot(p-a,b-a)/baba;
    let x = sqrt( papa - paba*paba*baba );
    var cax = max(0.0, x - select(rb, ra, paba < 0.5));
    let cay = abs(paba-0.5)-0.5;
    let k = rba*rba + baba;
    let f = clamp( (rba*(x-ra)+paba*baba)/k, 0.0, 1.0 );
    let cbx = x-ra - f*rba;
    let cby = paba - f;
    let s = select(1.0, -1.0, cbx < 0.0 && cay < 0.0);
    return s * sqrt( min(cax*cax + cay*cay*baba,
                     cbx*cbx + cby*cby*baba) );
}

fn sdCapsule(
    p: vec3<f32>,
    a: vec3<f32>,
    b: vec3<f32>,
    r0: f32,
    r1: f32
) -> f32 {
    let d = b - a;
    let h = length(d);
    let dn = d / h;
    let t = clamp(dot(p - a, dn), 0.0, h);
    return distance(a + t * dn, p) - mix(r0, r1, t / h);
}

fn sdCapsuleF(
    p: vec3<f32>,
    a: vec3<f32>,
    b: vec3<f32>,
    r0: f32,
    r1: f32,
    f: f32
) -> f32 {
    let d = b - a;
    let h = length(d);
    let dn = d / h;
    let t = dot(p - a, dn);
    let th = t / h;
    let rr = mix(r0, r1, th)
           * max(0.0, 1.0 + f - f * 4.0 * abs(th - 0.5) * abs(th - 0.5));
    return distance(a + clamp(t, 0.0, h) * dn, p) - rr;
}

fn sdSphere(p: vec3<f32>, s: f32) -> f32 {
    return length(p) - s;
}

fn sdCylinder(p: vec3<f32>, h: vec2<f32>) -> f32 {
    let d = abs(vec2<f32>(length(p.xz), p.y)) - h;
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdRoundBox(p: vec3<f32>, b: vec3<f32>, r: f32) -> f32
{
  let q = abs(p) - b + r;
  return length(max(q, vec3(0.0))) + min(max(q.x,max(q.y,q.z)), 0.0) - r;
}

fn sdRoundCone(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r1: f32, r2: f32) -> f32
{
    let ba = b - a;
    let l2 = dot(ba,ba);
    let rr = r1 - r2;
    let a2 = l2 - rr*rr;
    let il2 = 1.0/l2;
    
    let pa = p - a;
    let y = dot(pa,ba);
    let z = y - l2;
    let x2 = dot( pa*l2 - ba*y, pa*l2 - ba*y );
    let y2 = y*y*l2;
    let z2 = z*z*l2;

    // single square root!
    let k = sign(rr)*rr*rr*x2;
    if( sign(z)*a2*z2>k ) {
        return  sqrt(x2 + z2) * il2 - r2;
    }
    if( sign(y)*a2*y2<k ) {
        return  sqrt(x2 + y2) * il2 - r1;
    }
    return (sqrt(x2*a2*il2)+y*rr)*il2 - r1;
}

fn sdABRoundBox(pos: vec3<f32>, a: vec3<f32>, b: vec3<f32>, a_size: vec2<f32>, b_size: vec2<f32>, a_radius: f32, b_radius: f32, x: f32) -> f32 {
    let center = (a + b) * 0.5;
    let diff = b - a;
    let extension = abs(diff) * 0.5;
    let x_offset = center.x;
    let y_offset = mix(a.y, b.y, x);
    let z_offset = mix(a.z, b.z, x);
    let offset = vec3(x_offset, y_offset, z_offset);
    let size_x = extension.x;
    let size_y = mix(a_size.x, b_size.x, x);
    let size_z = mix(a_size.y, b_size.y, x);
    let size = vec3(size_x, size_y, size_z);
    let radius = mix(a_radius, b_radius, x);
    return sdRoundBox(pos - offset, size, radius);
}


// Helpers

fn pointInLine(pos: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> f32 {
    let ab = b - a;
    let ap = pos - a;
    let t = dot(ap, ab) / dot(ab, ab);
    return t;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn opRep(p: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return p - c * floor(p / c + 0.5);
}

fn rotationZ(angle: f32) -> mat3x3<f32> {
    let c = cos(angle);
    let s = sin(angle);

    return mat3x3<f32>(
        vec3( c, -s, 0.0),
        vec3( s,  c, 0.0),
        vec3(0.0, 0.0, 1.0)
    );
}