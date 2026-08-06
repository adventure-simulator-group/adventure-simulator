# Adventure Simulator tactical client

The tactical client renders transient server-authoritative combat state with
Bevy. Skeletal animation is presentation-only: the server replicates compact
`SkeletonState` posture, locomotion, stance, action, and timing coordinates;
the client selects and blends authored poses, then applies procedural look and
terrain leg IK.

## Animation export contract

The humanoid base rig is independent from authored motions:

```text
assets/animations/biped/unarmed/base.glb
assets/animations/biped/unarmed/walk.glb
assets/animations/biped/unarmed/attack_thrust_lead_left_stay.glb
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

Prepare and verify runtime files without changing source exports:

```powershell
python scripts/prepare_rig_base.py assets_src/base.glb assets/animations/biped/unarmed/base.glb
python scripts/prepare_animation_motion.py assets_src/biped/unarmed/walk.glb assets/animations/biped/unarmed/base.glb assets/animations/biped/unarmed/walk.glb --last-frame 32
python scripts/prepare_animation_motion.py assets_src/biped/unarmed/walk.glb assets/animations/biped/unarmed/base.glb assets/animations/biped/unarmed/walk.glb --last-frame 32 --check
```

Motion preparation validates the one-animation, duration, and canonical
bone-path contracts, then copies the GLB byte-for-byte. Scenes and meshes in a
motion export are harmless because the client loads only its animation asset.

Use these conventions:

- glTF coordinates and meters: +Y up, -Z forward, +X anatomical left;
- the scene root stays at the origin and gameplay movement is not baked into it;
- the armature bind pose is a T-pose, which is the final runtime fallback;
- each motion GLB contains exactly one animation and preserves its documented
  semantic anchors;
- locomotion uses only its contact and passing/flight anchor frames. The
  runtime constructs contact -> passing -> mirrored contact -> mirrored
  passing -> contact with smooth quarter-cycle interpolation; later exporter
  keys cannot become accidental runtime gait poses; and
- packs in one fallback chain use identical bone names and hierarchy.

Lower-body reflection is evaluated in character space so anatomical left and
right retain their lateral spacing while exchanging gait roles. Authored
upper-body carriage remains intact; explicit hand targets and weapon
constraints apply only when gameplay supplies them. Root, pelvis, spine, neck,
and head motion is clamped around the authored bind pose before look and final
IK. During active locomotion, authored root/pelvis Y is normalized, then one
phase-owned `sin²` curve supplies two contact minima and two passing/flight
peaks per cycle without moving the gameplay controller. Idle poses blend back
to their authored central-bone transforms. The 33mm hierarchy compensation is
measured for upright, lowered-guard `humanoid_unarmed` locomotion only;
crouching, guard movement, and specialized packs receive no inferred
compensation.

## Deterministic animation capture

The native `animation-viewer` binary is a deterministic gameplay-presentation
fixture rather than a separate pose renderer. It installs the gameplay player,
camera, scene presentation, authored FK, procedural mirroring, look, and
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
terrain-relative foot clearance/support/slip; controller-height stability;
phase-indexed contact/pass height; visual peak count; run flight duration and
sole clearance; and pelvis/head stability. These
values are regression signals and do not establish biomechanical correctness
without visual review.
Capture fails for teleport-scale continuity, ground penetration, duplicate
front/side/third-person image output, missing artifacts, or excessive
supported-foot per-frame slip and planted-interval drift, and records the
responsible frames.

The fixture synthesizes deterministic controller observations at the shared
server projection boundary; it does not replay network packets or run the
physics character controller. Only the cross-slope probe follows rendered
terrain height; flat scenarios verify that animation never changes controller Y.

Walk support telemetry remains continuous through its cycle. As locomotion
blends toward run, support narrows around each foot contact. At 5.5m/s the
quarter-cycle run flight unloads both legs for roughly 90-110ms and presents
at least 0.10m of sole clearance. When terrain IK is explicitly enabled, high
support retains the foot's world-space horizontal plant until release.

Terrain conformity starts off. In debug builds, press `F8` to opt into its
height, slope, and pelvis corrections. The HUD reports whether it is on or off;
authored FK, gait mirroring, torso stabilization, and procedural guard stepping
remain active. Ordinary flat-ground locomotion does not run the terrain leg
solver while the toggle is off.

The procedural humanoid pass recognizes these case-sensitive bone names:

```text
root                 pelvis               stomach_01 / stomach_02
chest                neck_01 / neck_02    head
clavicle.L / .R      upper_arm.L / .R     upper_arm_twist.L / .R
forearm.L / .R       forearm_twist.L / .R hand.L / .R
weapon.L / .R        thigh.L / .R         thigh_twist.L / .R
shin.L / .R          shin_twist.L / .R    foot.L / .R, toe.L / .R
```

Finger and breast bones remain under authored FK. Twist, toe, and weapon socket
bones are canonical parts of the base hierarchy and are available to later
procedural constraints.

The final client-only pose pass distributes bounded look across the actual
spine/neck chain, converts bounded pelvis compensation through its real parent,
and solves legs and optional hand targets through the twist-intermediate
hierarchy without overwriting authored twist locals. Foot slope alignment uses
the authored bind transform to derive its sole-up axis; local +Y is
ankle-to-toe on this rig and is not a sole normal. A primary hand socket drives
a held weapon, then an optional weapon-local secondary grip drives the off hand.
These targets and constraints are client-only and never extend replicated
`SkeletonState`.

## Missing assets

Pack lookup first follows the pack's single fallback chain. If the requested
semantic pose is still absent, lookup follows the deterministic similar-pose
chain (for example run to walk and thrust to slash), restarting pack lookup for
each candidate. Missing, unloaded, zero-animation, multiple-animation, or short
motion files affect only that motion. Every local or remote character also gets
a generated T-pose safety net until the base scene is available. If no pose
candidate resolves, the client uses the complete authored `base.glb` bind
T-pose. The generated mannequin appears only when the compatible base rig
itself is unavailable. Bind locals are reset before every animation evaluation
so partial clips cannot accumulate stale or procedural transforms. Incomplete
in-progress art does not panic.
