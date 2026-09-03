//! Localized cohesive slump head: an arcuate depletion scarp and lowered,
//! backtilted bench. This is a static morphology, not an observed failure,
//! stability calculation or mass-conserving reconstruction of a whole slide.

const PLAN_CURVATURE_PER_METRE: f32 = 0.025;
const VERTICAL_HEAD_FRACTION: f32 = 0.45;
const LOWER_FACE_RUN_RELIEF_FRACTION: f32 = 0.6;
const BENCH_HEAD_HEIGHT_METRES: f32 = 0.7;
const BENCH_BACKTILT_GRADE: f32 = 0.18;
const BENCH_RISE_END_METRES: f32 = 4.0;
const BENCH_TOE_METRES: f32 = 7.0;

pub(super) fn front(along: f32, depth_fraction: f32, relief: f32) -> f32 {
    let lower =
        ((depth_fraction - VERTICAL_HEAD_FRACTION) / (1.0 - VERTICAL_HEAD_FRACTION)).max(0.0);
    PLAN_CURVATURE_PER_METRE * along * along
        + lower * lower * relief * LOWER_FACE_RUN_RELIEF_FRACTION
}

pub(super) fn bench(along: f32, across: f32) -> f32 {
    let distance = across - PLAN_CURVATURE_PER_METRE * along * along;
    if !(0.0..BENCH_TOE_METRES).contains(&distance) {
        return 0.0;
    }
    let top = BENCH_HEAD_HEIGHT_METRES + BENCH_BACKTILT_GRADE * distance.min(BENCH_RISE_END_METRES);
    let head_blend = distance.min(1.0);
    let toe_blend =
        ((BENCH_TOE_METRES - distance) / (BENCH_TOE_METRES - BENCH_RISE_END_METRES)).min(1.0);
    top * head_blend * toe_blend
}
