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
- each motion GLB contains exactly one animation and preserves all authored
  in-betweens between its documented frame anchors;
- cyclic locomotion exports include their complete opposite-foot half and loop
  closure; runtime gait phase traverses the loaded clip's actual duration while
  catalog frame numbers continue to identify semantic anchors; and
- packs in one fallback chain use identical bone names and hierarchy.

## Deterministic animation capture

The native `animation-viewer` binary exercises the same authored FK,
procedural mirroring, and terrain IK plugin as the tactical client without a
server or player input. It holds eight evenly spaced walk phases under a fixed
camera, writes one PNG per phase plus `manifest.json`, validates that foot lead
changes twice across the captured cycle, and exits. A missing rig, unresolved
walk clip, or unbound foot times out with `failure.txt` rather than hanging.

Run it from the repository root:

```powershell
cargo run -p adventuresim-tactical-client --bin animation-viewer -- --output target/animation-captures/walk
```

Use `--asset-root` when invoking it outside the repository root and
`--frames-per-sample` to change the regular interval after the initial render
warmup. The manifest records gait phase, lower-body mirror weight, and both
knee/foot world positions so procedural regressions can be diagnosed without
visual guesswork.

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
