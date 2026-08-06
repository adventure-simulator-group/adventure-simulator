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
- all locomotion cycles use the documented normalized phase convention; and
- packs in one fallback chain use identical bone names and hierarchy.

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

## Missing assets

Pack lookup first follows the pack's single fallback chain. If the requested
semantic pose is still absent, lookup follows the deterministic similar-pose
chain (for example run to walk and thrust to slash), restarting pack lookup for
each candidate. Missing, unloaded, zero-animation, multiple-animation, or short
motion files affect only that motion. Every local or remote character also gets
a generated T-pose safety net until the base scene is available. If no pose
candidate resolves, the client uses the authored bind pose, or that generated
T-pose when the base scene itself is unavailable; incomplete in-progress art
does not panic.
