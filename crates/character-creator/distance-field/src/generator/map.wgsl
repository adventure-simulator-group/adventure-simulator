
// Map

const UNKNOWN_INDEX: u32 = 0;
const LEFT_ARM_INDEX: u32 = 3;
const RIGHT_ARM_INDEX: u32 = 33;
const LEFT_LEG_INDEX: u32 = 56;
const RIGHT_LEG_INDEX: u32 = 61;
const TORSO_INDEX: u32 = 1;
const HEAD_INDEX: u32 = 6;

fn foot(p: vec3<f32>, size: vec3<f32>, bone_index: u32) -> Map {
    // separate it in three parts: hindfoot, midfoot and forefoot
    let pos = vec3(-p.z, p.y, p.x);
    let len = size.x / 2.0;
    let height = size.y / 2.0;
    let width = size.z / 2.0;
    let start_size = vec2(height, width);
    let end_size = vec2(height * 0.5, width);
    let a = vec3(0., start_size.x, 0.0);
    let b = vec3(len, end_size.x, 0.0);
    let pil = pointInLine(pos, a, b);
    let distance = sdABRoundBox(pos, a, b, start_size, end_size, start_size.x, end_size.x, pil);
    return Map(distance, bone_index);
}

fn leg(pos: vec3<f32>, size: vec2<f32>, leg_index: u32) -> Map {
    let start_radius = size.x;
    let end_radius = size.x;
    let distance = sdRoundCone(pos - vec3(0.0, start_radius, 0.0), vec3(0.0, 0.0, 0.0), vec3(0.0, size.y, 0.0), start_radius, end_radius);
    return Map(distance, leg_index);
}

fn lower_limb(pos: vec3<f32>, leg_size: vec2<f32>, foot_size: vec3<f32>, bone_index: u32) -> Map {
    var m = foot(pos, foot_size, bone_index);
    return MapMin(m, leg(pos, leg_size, bone_index));
}

fn is_left(pos: vec3<f32>) -> bool {
    return pos.x < 0.0;
}

fn lower_limbs(pos: vec3<f32>, leg_size: vec2<f32>, foot_size: vec3<f32>, body_width: f32) -> Map {
    let distance = body_width / 2.0 - leg_size.x;
    let bone_index = select(RIGHT_LEG_INDEX, LEFT_LEG_INDEX, is_left(pos));
    return lower_limb(vec3(abs(pos.x) - distance, pos.yz), leg_size, foot_size, bone_index);
}

fn torso(pos: vec3<f32>, size: vec3<f32>) -> Map {
    let distance = sdRoundBox(pos, size / 2.0, size.z * 0.25);
    return Map(distance, TORSO_INDEX);
}

fn arm(pos: vec3<f32>, size: vec2<f32>, bone_index: u32) -> Map {
    let start_radius = size.x;
    let end_radius = size.x;
    let distance = sdRoundCone(pos - vec3(0.0, start_radius, 0.0), vec3(0.0, 0.0, 0.0), vec3(0.0, size.y, 0.0), start_radius, end_radius);
    return Map(distance, bone_index);
}

fn hand(p: vec3<f32>, size: vec3<f32>, bone_index: u32) -> Map {
    let distance = sdRoundBox(p, size, size.z * 0.5);
    return Map(distance, bone_index);
}

const PI: f32 = acos(-1.0);

fn upper_limb(p: vec3<f32>, arm_size: vec2<f32>, hand_size: vec3<f32>, bone_index: u32) -> Map {
    let shoulder = vec3(0.0, arm_size.y, 0.0);

    let rot = rotationZ(-PI / 8.0);
    let pos = transpose(rot) * (p - shoulder) + shoulder;

    var m = hand(pos, hand_size, bone_index);
    m = MapMin(m, arm(pos, arm_size, bone_index));
    return m;
}

fn upper_limbs(pos: vec3<f32>, arm_size: vec2<f32>, hand_size: vec3<f32>, body_width: f32) -> Map {
    let limbs_distance = body_width / 2.0 + arm_size.x;
    let bone_index = select(RIGHT_ARM_INDEX, LEFT_ARM_INDEX, is_left(pos));
    let m = upper_limb(vec3(abs(pos.x) - limbs_distance, pos.yz), arm_size, hand_size, bone_index);
    return m;
}

fn neck(pos: vec3<f32>, size: vec2<f32>) -> Map {
    let distance = sdRoundCone(pos, vec3(0.0), vec3(0.0, size.y, 0.0), size.x, size.x);
    return Map(distance, UNKNOWN_INDEX);
}

fn head(pos: vec3<f32>, size: vec2<f32>) -> Map {
    let distance = sdRoundCone(pos, vec3(0.0), vec3(0.0, size.y, 0.0), size.x, size.x);
    return Map(distance, UNKNOWN_INDEX);
}

fn character(pos: vec3<f32>, height: f32) -> Map {
    let relative = height * 0.1;
    let leg_radius = 0.25 * relative;
    let leg_length = 3.0 * relative;
    let leg = vec2(leg_radius, leg_length);

    let foot_length = 1.5 * relative;
    let foot_height = 0.5 * relative;
    let foot_width = 0.5 * relative;
    let foot = vec3(foot_length, foot_height, foot_width);

    let torso_width = 1.5 * relative;
    let torso_height = 3.0 * relative;
    let torso_depth = leg_radius * 2.0;
    let torso_size = vec3(torso_width, torso_height, torso_depth);
    
    let arm_radius = 0.25 * relative;
    let arm_length = 2.5 * relative;
    let arm = vec2(arm_radius, arm_length);
    
    let hand_length = 0.3 * relative;
    let hand_height = 0.3 * relative;
    let hand_width = 0.3 * relative;
    let hand = vec3(hand_length, hand_height, hand_width);
    
    let neck_radius = 0.25 * relative;
    let neck_length = 0.25 * relative;
    let neck_size = vec2(neck_radius, neck_length);
    
    let head_radius = 0.5 * relative;
    let head_length = 0.3 * relative;
    let head_size = vec2(head_radius, head_length);
    
    var m = lower_limbs(pos, leg, foot, torso_width);
    m = MapMin(m, torso(pos - vec3(0.0, leg_length + leg_radius + torso_height * 0.5, 0.0), torso_size));
    m = MapMin(m, upper_limbs(pos - vec3(0.0, leg_length + torso_height - arm.y - arm.x, 0.0), arm, hand, torso_width));
    m = MapMin(m, neck(pos - vec3(0.0, leg_length + torso_height + neck_size.x, 0.0), neck_size));
    m = MapMin(m, head(pos - vec3(0.0, leg_length + torso_height + neck_size.x + neck_size.y + head_size.x, 0.0), head_size));
    return m;
}

fn map(p: vec3<f32>) -> Map {
    return character(p, 1.0);
}
