use super::*;

#[derive(Debug, Clone)]
pub(super) struct ContinuationSpan {
    pub(super) start: LoadedClip,
    pub(super) start_time_seconds: f32,
    pub(super) contact: LoadedClip,
    pub(super) contact_time_seconds: f32,
    pub(super) end: LoadedClip,
    pub(super) end_time_seconds: f32,
    pub(super) outgoing: LoadedClip,
    pub(super) outgoing_time_seconds: f32,
    pub(super) finish: LoadedClip,
    pub(super) finish_time_seconds: f32,
    pub(super) start_coordinate: f32,
    pub(super) incoming_tangent: f32,
    pub(super) ready_phase: f32,
    pub(super) progress: f32,
    pub(super) weight: f32,
    pub(super) mirrored_weight: f32,
}

pub(super) fn append_continuation_span(
    resolver: &PoseSampleResolver<'_>,
    weighted: &mut Vec<WeightedClip>,
    spans: &mut Vec<ContinuationSpan>,
    start: &ResolvedAnchor<'_>,
    sample: PoseSample,
    sampling: PoseSampling,
    layer: ClipLayer,
) {
    let PoseSampleResolver {
        runtime,
        catalog,
        pack,
        ..
    } = *resolver;
    let PoseSampling::ContinuationSpan {
        contact,
        end,
        outgoing,
        finish,
        start_coordinate,
        incoming_tangent,
        ready_phase,
        progress,
    } = sampling
    else {
        unreachable!("continuation resolver requires continuation sampling");
    };
    let Some(contact) = resolve_anchor(runtime, catalog, pack, contact) else {
        append_weighted_anchor(weighted, start, start.anchor.frame, sample.weight, layer);
        return;
    };
    let Some(end) = resolve_anchor(runtime, catalog, pack, end) else {
        append_weighted_anchor(weighted, start, start.anchor.frame, sample.weight, layer);
        return;
    };
    let Some(outgoing) = resolve_anchor(runtime, catalog, pack, outgoing) else {
        append_weighted_anchor(weighted, &end, end.anchor.frame, sample.weight, layer);
        return;
    };
    let Some(finish) = resolve_anchor(runtime, catalog, pack, finish) else {
        append_weighted_anchor(
            weighted,
            &outgoing,
            outgoing.anchor.frame,
            sample.weight,
            layer,
        );
        return;
    };
    spans.push(ContinuationSpan {
        start: start.clip.at_anchor_layer(start.anchor.frame, layer),
        start_time_seconds: frame_seconds(start.anchor.frame),
        contact: contact.clip.at_anchor_layer(contact.anchor.frame, layer),
        contact_time_seconds: frame_seconds(contact.anchor.frame),
        end: end.clip.at_anchor_layer(end.anchor.frame, layer),
        end_time_seconds: frame_seconds(end.anchor.frame),
        outgoing: outgoing.clip.at_anchor_layer(outgoing.anchor.frame, layer),
        outgoing_time_seconds: frame_seconds(outgoing.anchor.frame),
        finish: finish.clip.at_anchor_layer(finish.anchor.frame, layer),
        finish_time_seconds: frame_seconds(finish.anchor.frame),
        start_coordinate,
        incoming_tangent: incoming_tangent.max(0.0),
        ready_phase: ready_phase.clamp(f32::EPSILON, 0.5 - f32::EPSILON),
        progress: progress.clamp(0.0, 1.0),
        weight: sample.weight,
        mirrored_weight: if start.mirrored { sample.weight } else { 0.0 },
    });
}
