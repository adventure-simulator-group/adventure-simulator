# Fabelgeist tactical client

The tactical client renders transient server-authoritative combat state with
Bevy. Skeletal animation is presentation-only: the server replicates compact
`SkeletonState` posture, locomotion, stance, action, and timing coordinates;
the client selects and blends authored poses, then applies procedural look and
terrain leg IK.

EGUI renders the centered incapacitation wheel without taking pointer input.
The segmented arc surrounds the retained Bevy UI crosshair, starts at 12
o'clock, and uses the strategic condition colors for live pain and blood loss
plus enrolled fear, fatigue, hunger, thirst, and temperature; tactical
imbalance is white. Zero incapacitation draws nothing and a full revolution is
the visible maximum.

The gameplay camera is likewise client presentation. A single retained rig
blends from centered lowered-guard exploration to raised-guard right-shoulder
aiming without smoothing manual yaw or pitch. Focus translation uses bounded
anisotropic critical damping and a screen-space sweet spot. A sphere sweep
retracts the boom around hard geometry, with hysteretic recovery and
tight-space shoulder recentering. Raised aiming resolves the center-screen
camera target and the subsequent muzzle path separately. Debug builds use
`F6` to show rig, collision, smoothing, occlusion classification, and aim-ray
telemetry.

The tactical workspace targets Bevy 0.19, Avian 0.7, Ahoy 0.2, Replicon
0.41, Aeronet 0.21, Enhanced Input 0.26, and Flair 0.8. The engine upgrade does
not move animation or movement authority: `SkeletonState` and controller state
remain server-owned, while authored pose evaluation, lighting, and the Bevy
world-asset scene attachment are client presentation. Native and Wasm builds
share those semantics through their existing explicit feature sets.

## Client-generated geological geometry

Collider-bearing rocks replicate compact recipes containing a seed, silhouette
archetype, lithology, dimensions, and conservative collision radius. The
tactical server creates only the bounded sphere collider and transform. The
client samples each recipe on a small uniform grid and extracts its render
surface with the private Surface Nets implementation in
`src/presentation/volumetric.rs`; it rejects non-finite fields and tests the
result for deterministic, outward, closed topology. Loose-stone ground cover
reuses four shared client-generated variants without collision or foliage wind.

This extractor is infrastructure for later bounded cliffs, overhangs, and cave
patches, not an implemented cave system. Future scene authority must send a
compact deterministic field recipe, never a server-generated render mesh.
Authored clips are sampled by the client-owned pose buffer. The semantic router
reads presentation state, resolves weighted clip samples through the animation
pack catalog, and applies fixed-rate FK with per-joint inertialization before
procedural terrain, body-response, and weapon-constraint passes.
## Animation export contract

The humanoid base rig is independent from authored motions:

```text
assets/animations/biped/unarmed/base.glb
assets/animations/biped/unarmed/walk.glb
assets/animations/biped/unarmed/swing.glb
assets/animations/biped/unarmed/thrust.glb
assets/animations/biped/unarmed/offhand.glb
```

Only `base.glb` supplies a spawnable scene. Its default scene must retain the
skinned character mesh; `prepare_rig_base.py` strips only authoring helpers such
as the placeholder weapon cylinder. The client attaches this authored scene to
both the client-controlled character and replicated remote characters. Each
other file contains exactly one coherent motion, named or unnamed, and never
has its scene attached. The
30fps `AnimationPackCatalog` explicitly owns every semantic pose through a
file/frame anchor and includes unnamed endpoint/closure frames. Source motion
files belong under `assets_src/biped/unarmed/`; `assets_src/base.*` remains the
rig-source special case until `assets_src/biped/unarmed/base.casc` has a matching
base GLB export.

Publish and verify every currently available runtime animation without changing
source exports:

```powershell
python scripts/prepare_rig_base.py assets_src/biped/unarmed/base.glb assets/animations/biped/unarmed/base.glb
python scripts/prepare_animation_assets.py
python scripts/prepare_animation_assets.py --check
```

Motion publication validates the one-animation, duration, and canonical
bone-path contracts. Runtime motion GLBs preserve only the canonical hierarchy,
bind transforms, and animation data; mesh, skin, material, texture, and image
payloads remain solely in the spawnable runtime `base.glb`. The publisher keeps
only catalog-addressable frames for ordinary motions, removes tracks that equal
the bind transform, and collapses other constant tracks to one key. Walk and run
store five cubic anchor keys rather than Cascadeur's exported in-betweens.
The `.casc` projects are tracked authoring sources. Their reproducible
`assets_src/**/*.glb` exports are ignored; export them locally before publishing
the tracked runtime GLBs.

Use these conventions:

- glTF coordinates and meters: +Y up, -Z forward, +X anatomical left;
- the scene root stays at the origin and gameplay movement is not baked into it;
- the armature bind pose is a T-pose, which is the final runtime fallback;
- each motion GLB contains exactly one animation and preserves its documented
  semantic anchors;
- locomotion uses only its contact and passing/flight anchor frames. The
  runtime constructs contact -> passing -> mirrored contact -> mirrored
  passing -> contact by sampling the two exact catalog frames on distinct Bevy
  graph nodes, selecting pre-mirrored endpoint clips, and blending complete
  poses with linear quarter-cycle weights;
  every exported in-between key and every later exporter key is ignored; and
- packs in one fallback chain use identical bone names and hierarchy.

Lower-body reflection is evaluated in character space so anatomical left and
right retain their lateral spacing while exchanging gait roles. Authored
upper-body carriage remains intact; explicit hand targets and weapon
constraints apply only when gameplay supplies them. Root, pelvis, spine, neck,
and head translations are clamped around the authored bind pose before look
and final IK, while authored joint rotations remain intact. During active
locomotion, sparse anchor blending advances linearly with phase so the pose
cannot ease to a hold and then accelerate between anchors. Gait parity is
binary per endpoint and is applied before blending; the runtime never
fractionally reflects an already blended skeleton, which would collapse the
forward/back separation of bilateral limbs.
Authored root/pelvis Y is normalized, then the
shared gait profile supplies grounded bounce or a gravity-shaped run flight
arc with two phase-aligned peaks per cycle without moving the gameplay
controller. A contact-edge calibration translates the complete visual rig so
the supported sole meets the rig floor, then retains that baseline through the
stride without reconstructing either leg. Idle poses blend back
to their authored central-bone transforms. The 33mm hierarchy compensation is
measured for upright, lowered-guard `humanoid_unarmed` locomotion only;
crouching, guard movement, and specialized packs receive no inferred
compensation.

## Deterministic animation capture

Republish after changing any motion with
`python scripts/prepare_animation_assets.py`; CI-style verification uses the
same command with `--check`. The publisher requires Python 3 and NumPy.

The native `animation-viewer` binary is a deterministic gameplay-presentation
fixture rather than a separate pose renderer. It installs the gameplay player,
camera, scene presentation, authored FK, pre-mirrored gait endpoints,
whole-body fallback mirroring, look, and
terrain-IK plugins,
then advances the shared authoritative locomotion projector at its real 64Hz
fixed tick. Default-off scenarios retain authored ordinary leg motion with a
vertically fixed gameplay root; the explicit cross-slope scenario opts into
the seeded terrain-IK pass. Coverage includes two-cycle 2.0m/s walk, 3.75m/s
blend, 5.5m/s run, crouch, raised-guard full/half-speed movement, and
start/stop, guard-entry, guard-release, and crouch-enter/exit transitions. Every logical tick is captured first from the raw
gameplay third-person camera, then from side and front diagnostic cameras with
a skeleton overlay and yellow supported-foot / pink swing-foot markers. The
simulation is frozen while those three views are rendered, so they describe
one pose. The output includes per-view PNG sequences, `manifest.json` bone and
support telemetry, and an `index.html` normal/half/quarter-speed reviewer with
representative contact sheets. A missing rig or unresolved locomotion clip
times out with `failure.txt` rather than hanging.

Run it from the repository root:

```powershell
cargo run -p adventuresim-tactical-client --bin animation-viewer -- --output target/animation-captures/locomotion-review
```

Use `--asset-root` when invoking it outside the repository root,
`--scenario steady-walk-2.0` for a focused iteration, and
`--frames-per-sample` to change the render settle interval (not the simulated
64Hz sample interval). Open `index.html` after capture and review each scenario
at normal speed before using slow motion. The manifest tracks pelvis, chest,
head, shoulders, elbows, hands, hips, knees, and feet; finite transforms; loop
seams; per-frame displacement and rotation spikes; knee direction;
terrain-relative foot clearance/support/slip; contact-sole grounding;
controller-height stability;
phase-indexed contact/pass height; visual peak count; run flight duration and
sole clearance; and pelvis/head stability. These
values are regression signals and do not establish biomechanical correctness
without visual review.
Capture fails for teleport-scale continuity, ground penetration, duplicate
front/side/third-person image output, missing artifacts, or excessive
supported-foot per-frame slip and planted-interval drift, and records the
responsible frames.

The fixture synthesizes deterministic controller observations at the shared
server projection boundary; it does not run the transport or physics character
controller. Only the cross-slope probe follows rendered
terrain height; flat scenarios verify that animation never changes controller Y.

## Real-client animation diagnostics

Use the supervised diagnostic profile when a problem appears in gameplay but
not in `animation-viewer`:

```powershell
just tactical-play diagnostic
```

This launches the ordinary native client, server, transport, replicated
physics controller, and rendering stack. Once the controlled character is
available, the client turns 90 degrees right, holds forward at 0.5 analogue
input for two seconds, raises its guard for half a second, starts a real
preferred attack, captures a PNG during the attack, exercises full-speed
movement and posture transitions, stops, and exits. The supervisor then stops
its isolated server and database and returns successfully. The profile run
directory contains the generated
`animation-input-script.json`, the per-render-frame `animation-state.jsonl`,
the attack PNG, and the ordinary client/server logs. `just tactical-status`
prints that run directory.

The JSONL record includes the requested command and input, controller
transform, replicated authoritative `SkeletonState`, client-predicted
presentation shadow, semantic `AnimationEvaluation`, resolved clip weights
and sample times, endpoint parity, whole-body mirror coordinates,
phase prediction/correction deltas,
authoritative phase measurements, pending drift correction, any presentation
crossfade, wall-clock time, and the latest render-schedule completion counter.
After final pose evaluation and transform propagation, each record also
contains the global translation, rotation, and scale of every authored
animation target. PresentMon remains the independent authority for actual
swapchain presentation. This is the diagnostic boundary at the pose actually
submitted for rendering; it does not replace the real network or animation
path.

Only the bounded `diagnostic` profile enables the per-frame JSONL log by
default. Interactive `animation` and `combat` sessions avoid an unbounded log;
launch the native client with an explicit `--animation-log PATH` when a manual
session needs one.

On Windows the bounded diagnostic launcher also starts PresentMon when it is
available and writes `presentmon-<session>.csv` beside the JSONL log. This
records ETW display/presentation timing independently of Bevy's update loop.
Pass `presentation_trace=off` to disable it or
`presentation_trace=required` to fail startup when PresentMon cannot run (or
to force it for an interactive profile). `PRESENTMON_PATH` overrides PATH and
standard-location discovery.

For presentation A/B tests, pass `present_mode=auto-vsync`,
`auto-no-vsync`, `fifo`, `fifo-relaxed`, `mailbox`, or `immediate` to
`just tactical-play`. The default remains `auto-vsync`; unsupported explicit
modes are reported by the graphics backend rather than silently selected by
the launcher.

The native client also accepts custom files through `--input-script PATH` and
`--animation-log PATH`. A script has this shape:

```json
{
  "commands": [
    { "type": "rotate", "degrees_right": 90.0 },
    { "type": "move", "direction": "forward", "input_speed": 0.5, "duration_seconds": 2.0 },
    { "type": "guard", "raised": true },
    { "type": "wait", "duration_seconds": 0.5 },
    { "type": "attack", "duration_seconds": 0.25 },
    { "type": "screenshot", "path": "C:/capture/attack.png" },
    { "type": "wait_for_signal", "path": "C:/capture/ready.json" },
    { "type": "wait", "duration_seconds": 0.5 }
  ]
}
```

Movement directions are `forward`, `backward`, `left`, and `right`.
`guard` changes the persistent aiming state. `attack` presses the ordinary
preferred-attack control once and then observes neutral input for
`duration_seconds`; it requires raised guard under normal gameplay rules.
`screenshot` captures the gameplay window directly through Bevy, without OBS.
`wait_for_signal` holds neutral input until its file exists, which lets a
capture supervisor release movement only after recording is ready. Add
`--exit-after-script` for bounded unattended captures.

The gameplay camera already runs with MSAA disabled. For matched performance
diagnostics, pass `graphics_preset=no-shadows`, `no-bloom`, or
`no-atmosphere` to disable one cost independently. `minimal` omits all four:

```powershell
just tactical-play diagnostic 24920 no-shadows
just tactical-play diagnostic 24920 minimal
```

The normal client uses a 64×64 generated atmosphere environment map.
`no-environment-light` keeps the rendered atmosphere but omits that lighting.

The same presets are available on the native client through
`--graphics-preset`.

Walk support telemetry remains continuous through its cycle. As locomotion
blends toward run, support narrows around each foot contact. At 5.5m/s the
quarter-cycle run flight unloads both legs for roughly 90-110ms and presents
0.10-0.30m of sole clearance. Contact-phase sole clearance must remain between
-0.02m and 0.04m. When terrain IK is explicitly enabled, high
support retains the foot's world-space horizontal plant until release. Only a
meaningfully supported leg enters the analytic terrain solve; the swing leg
keeps its authored FK and action poses opt out until they expose explicit foot
contact semantics. Idle continues to support both feet.

The replicated skeleton also carries the shared 64 Hz locomotion sample tick,
observed world velocity/acceleration, alternating contact sequence, and landing
sequence/impact. The client transforms acceleration through the current body
frame and advances retained lean only once per authoritative tick, including
bounded coalesced gaps. A hard stop retains the effective authored locomotion
pose and releases it to exact idle over a fixed-tick 0.18-second crossfade,
preventing the sparse run/idle clips from switching in one frame. Landing
response compresses once on a real airborne
landing, retains both pre-compression world foot plants through recovery, and
solves the actual hip/knee chains back to them; it never translates or stretches thigh roots. Stationary and
stopping ordinary locomotion blends both feet back to full support.

Rendering uses a client-only shadow of `SkeletonState`. Between authoritative
samples it advances gait phase from the most recently measured physical speed
and smooths the displayed local/world velocity. Minor authoritative phase
differences are treated as packet-timing jitter. Larger persistent drift is
low-pass filtered before a slow bounded circular correction is applied, while
posture, actions, contacts, landings, and large discontinuities snap to
authority. This removes packet-cadence pose holds and packet-by-packet speed
modulation without predicting gameplay events or changing the replicated component.

Contact and landing messages are deduplicated presentation hooks for future
audio/VFX. Plausible contact gaps reconstruct at most eight ordered alternating
contacts; resets, backward sequences, and larger gaps resynchronize silently.
Missed landing updates collapse to the latest observation rather than bursting.
An event's sample tick is the tick where its replicated sequence was observed,
not an invented historical contact timestamp.

Terrain conformity starts off. In debug builds, press `F8` to opt into its
height, slope, and pelvis corrections. The HUD reports whether it is on or off;
authored FK, gait endpoint blending, torso stabilization, and procedural guard stepping
remain active. Ordinary flat-ground locomotion does not run the terrain leg
solver while the toggle is off.

In debug builds, `F7` switches both peers between normal and quarter-speed game
time. The server retains the latest validated analogue movement request across
missing unreliable input packets and restores Ahoy's fixed-loop input from it
before each movement step. That intent drives the controller only. Current
post-physics planar speed selects the idle/walk/run blend and determines stride
cadence, while acceleration is reserved for procedural body response. The
clock toggle therefore cannot directly change walk/run selection.

The procedural humanoid pass recognizes these case-sensitive bone names:

```text
body_world           root                 c_spine0 / c_spine1
c_spine2 / c_spine3  c_neck               c_head / c_camera
l_clavicle / r_clavicle                   l_uparm / r_uparm
l_lowarm / r_lowarm  l_wrist / r_wrist    l_weapon / r_weapon
l_upleg / r_upleg    l_lowleg / r_lowleg  l_foot / r_foot
l_ball / r_ball
```

Finger, face, foot-articulation, and distributed twist bones retain authored FK.
They still participate in full-pose mirroring. `l_weapon`, `r_weapon`, and
`c_camera` are export-added attachment joints on the canonical MHR hierarchy.
Equipment removes each target's rolled authored bind frame before following its
live deformation; worn placeholders derive their +Y axis from semantic joint
pairs, while held weapons retain the character-space +Y tip convention.

The final client-only pose pass distributes bounded look across the actual
spine/neck chain, converts bounded pelvis compensation through its real parent,
and solves legs and optional hand targets across the MHR hierarchy without
overwriting authored twist locals. Foot slope alignment uses the authored bind
transform to derive its sole-up axis rather than assuming an MHR joint-local
cardinal axis. A primary hand socket drives a held weapon, then an optional
weapon-local secondary grip drives the off hand. These targets and constraints
are client-only and never extend replicated `SkeletonState`.

## Missing assets

Ordinary pose lookup first follows the pack's single fallback chain, then its
deterministic similar-pose chain. Attack availability is stricter because it is
a gameplay capability. A pack that defines `swing` or `thrust` owns that
complete main-hand set, and an absent family stays unavailable; only a pack
with neither motion inherits its parent's main-hand attacks. `offhand` resolves
independently through the parent chain. Missing, unloaded, zero-animation,
multiple-animation, or short motion files are unavailable.
Every local or remote character also gets a generated T-pose safety net until
the base scene is available. Bind locals are reset before every animation
evaluation so partial clips cannot accumulate stale or procedural transforms.
