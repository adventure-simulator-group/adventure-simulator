//! Human review presentation.

use super::*;

pub(super) fn review_html(manifest: &AnimationCaptureManifest) -> String {
    let frame_json = serde_json::to_string(&manifest.frames).expect("review frames must serialize");
    let scenario_names_json = serde_json::to_string(
        &manifest
            .scenarios
            .iter()
            .map(|scenario| scenario.scenario.as_str())
            .collect::<Vec<_>>(),
    )
    .expect("review scenario names must serialize");
    let quality_summary = format!(
        "Quality score: {:.2}% ({}/31 weighted defect points); acceptance: {}",
        manifest.quality_score.quality_percent,
        manifest.quality_score.weighted_defect_score,
        if manifest.quality_score.acceptance_passed {
            "passed"
        } else {
            "failed"
        },
    );
    let scenario_buttons = manifest
        .scenarios
        .iter()
        .map(|scenario| {
            format!(
                "<button data-scenario=\"{}\">{}</button>",
                scenario.scenario, scenario.scenario
            )
        })
        .collect::<String>();
    let metrics = manifest
        .scenarios
        .iter()
        .map(|scenario| {
            let describe = |worst: &Option<ContinuityLocation>, unit: &str| {
                worst.as_ref().map_or("&mdash;".to_owned(), |worst| {
                    format!(
                        "{} {}&rarr;{} ({:.4}{unit})",
                        worst.bone, worst.from_frame, worst.to_frame, worst.value
                    )
                })
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{}</td><td>{}/{}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.2}</td><td>{:.3}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.4}</td></tr>",
                scenario.scenario,
                scenario.frame_count,
                describe(&scenario.worst_displacement, "m"),
                describe(&scenario.worst_rotation, "deg"),
                scenario.loop_seam_position_metres.map_or("&mdash;".into(), |value| format!("{value:.4}")),
                scenario.loop_seam_rotation_degrees.map_or("&mdash;".into(), |value| format!("{value:.2}")),
                scenario.maximum_supported_foot_slip_metres_per_frame,
                scenario.maximum_planted_foot_drift_metres,
                scenario.guard_step_liveness_required,
                scenario.visible_guard_half_step_count,
                scenario.completed_guard_half_step_count,
                scenario.minimum_guard_swing_travel_metres,
                scenario.minimum_guard_swing_clearance_gain_metres,
                scenario.minimum_signed_foot_track_metres,
                scenario.minimum_inter_foot_separation_metres,
                scenario.minimum_knee_flexion_degrees,
                scenario.minimum_knee_hemisphere_dot,
                scenario.maximum_knee_foot_yaw_offset_degrees,
                scenario.maximum_facing_motion_error_degrees,
                scenario.maximum_facing_tracking_excess_degrees,
                scenario.maximum_guard_facing_error_degrees,
                scenario.final_facing_motion_error_degrees,
                scenario.maximum_dive_axis_motion_error_degrees,
                scenario.minimum_foot_clearance_metres,
            )
        })
        .collect::<String>();
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Animation review</title><style>
body{{font:15px system-ui;background:#111820;color:#e8eef5;margin:24px}}button,select{{margin:4px;padding:8px}}img{{max-width:min(960px,100%);background:#222}}table{{border-collapse:collapse;margin-top:20px}}td,th{{border:1px solid #526171;padding:6px}}.note{{max-width:960px;color:#b9c7d5}}#contact{{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:8px;margin-top:20px}}#contact img{{width:100%}}
</style></head><body><h1>Tactical locomotion review</h1>
<p class="note">{quality_summary}</p><p class="note">This runs the shared tactical player, hills scene, gameplay camera, 64 Hz authoritative locomotion projection, authored FK, and final procedural passes. Gameplay images are raw; side/front diagnostics add the cyan skeleton and support markers. Use normal speed first, then slow motion.</p>
<div>{scenario_buttons}</div><label>View <select id="view"><option value="gameplay">gameplay (raw)</option><option value="side">side diagnostic</option><option value="front">front diagnostic</option></select></label>
<label>Playback <select id="rate"><option value="1">normal</option><option value="2">half speed</option><option value="4">quarter speed</option></select></label>
<p id="telemetry"></p><img id="player"><div id="contact"></div>
<table><thead><tr><th>scenario</th><th>frames</th><th>worst root-relative displacement</th><th>worst rotation</th><th>loop seam m</th><th>loop seam deg</th><th>supported slip m/frame</th><th>planted interval drift m</th><th>guard liveness required</th><th>visible/completed half-steps</th><th>minimum swing travel m</th><th>minimum swing clearance gain m</th><th>signed foot track m</th><th>inter-foot separation m</th><th>knee flexion deg</th><th>knee hemisphere dot</th><th>knee-foot yaw offset deg</th><th>maximum facing error deg</th><th>tracking excess deg</th><th>guard facing error deg</th><th>final facing error deg</th><th>dive axis/travel error deg</th><th>minimum terrain-relative foot clearance m</th></tr></thead><tbody>{metrics}</tbody></table>
<script>const all={frame_json},scenarioNames={scenario_names_json};let scenario=scenarioNames[0]||"",i=0,timer;const player=document.querySelector('#player'),view=document.querySelector('#view'),rate=document.querySelector('#rate'),telemetry=document.querySelector('#telemetry');
function frames(){{return all.filter(x=>x.scenario===scenario)}}function show(){{const list=frames(),f=list.length?list[i%list.length]:null;if(!f){{player.removeAttribute('src');telemetry.textContent='No completed capture frames';return}}player.src=f.screenshots[view.value];telemetry.textContent=`${{f.scenario}} frame ${{f.scenario_frame}} | guard ${{f.weapon_guard}} lead ${{f.lead_foot}} | ${{f.speed_metres_per_second.toFixed(2)}} m/s | phase ${{f.gait_phase.toFixed(3)}} | world plants L ${{f.left_support_weight.toFixed(2)}} R ${{f.right_support_weight.toFixed(2)}}`;}}
function play(){{clearInterval(timer);timer=setInterval(()=>{{i=(i+1)%frames().length;show()}},1000/64*Number(rate.value))}}function contacts(){{const f=frames(),step=Math.max(1,Math.floor(f.length/12)),box=document.querySelector('#contact');box.innerHTML='';for(let n=0;n<f.length;n+=step){{let x=document.createElement('img');x.src=f[n].screenshots[view.value];x.title=`frame ${{f[n].scenario_frame}} phase ${{f[n].gait_phase.toFixed(3)}}`;box.appendChild(x)}}}}
document.querySelectorAll('button').forEach(b=>b.onclick=()=>{{scenario=b.dataset.scenario;i=0;show();contacts();play()}});view.onchange=()=>{{show();contacts()}};rate.onchange=play;show();contacts();play();</script></body></html>"#
    )
}
