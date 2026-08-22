# Animation

The tactical renderer uses Bevy 0.19. Its `WorldAsset` scene terminology,
atmosphere entity, and linear-space bloom are engine presentation details;
they do not change the replicated skeleton contract, semantic pack fallback,
root-motion prohibition, or authoritative attack timing described below.

The tactical animation system turns authoritative character state into a
convincing skeletal pose. One-shot actions may use sparse semantic poses.
Ordinary idle, walk, and run instead evaluate complete runtime motions; the
runtime does not plan their steps or synthesize their swing trajectories.

The system takes heavy inspiration from Wolfire Games' *Overgrowth*, while
keeping Fabelgeist's gameplay authority separate from client-side
presentation. The useful model to borrow is its layered pipeline: script-level
state selects animations and blend coordinates; synchronized animation groups
share a normalized phase; the animation client performs blending, layers,
mirroring, and event playback; and a final procedural pass modifies the bones.
See Overgrowth's
[`aschar.as`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Data/Scripts/aschar.as),
[`syncedanimation.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/syncedanimation.cpp),
[`animationclient.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Graphics/animationclient.cpp),
and
[`riggedobject.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Objects/riggedobject.cpp).

## State and authority

There are four conceptual layers to the state of a character:

1. Input or NPC AI: what action is the character attempting?
2. Authoritative skeleton state: what is the character actually doing?
3. Animation evaluation: which authored poses, blends, and IK adjustments are
   producing the current bone transforms?
4. Secondary animation: how do inertia, impacts, and joint motors modify the
   evaluated pose?

### Input and NPC AI

When a player presses a movement key, input determines that the player is
trying to move. When an NPC has a goal ahead of it, pathfinding reaches the
same conclusion. Both sources submit the same kind of intent to the tactical
simulation. The animation system does not need to know whether that intent
came from a human or AI.

### Skeleton state

Skeleton state contains the minimum authoritative information needed to
reproduce what a character is doing. It is not a single mutually exclusive
enum, because a character may be moving, crouching, looking at an opponent,
and starting an attack at the same time. It is instead composed from several
cooperating, typed dimensions. The body mode owns ground contact, and each
active action owns its own timeline and parameters. State-changing methods
preserve these invariants rather than exposing a public property bag:

- **Body:** grounded upright/crouched, airborne, prone, supine, or ragdolled;
  this tagged value owns the ground-contact/posture relationship.
- **Locomotion:** local/world velocity, acceleration, gait phase, and the
  currently leading or planted foot.
- **Stance:** lowered, or raised with validated planted/moving footwork intent.
- **Selection and facing:** body-facing rotation remains on the character root;
  lead foot and animation-pack selection are independent skeleton fields.
- **Action:** opaque idle, dodge, attack, or block state constructed through
  typed transitions. Additional actions require an authoritative producer.
- **Action payload:** dodge direction, block line, or selected attack contact,
  target height, and a saturating authoritative timeline, carried only by the
  corresponding action variant.

This state is synchronized over the network. A client does not need to know
whether another character is an NPC or player; it needs only the replicated
physical state and presentation intent.

The client copies each received skeleton into a presentation-only shadow.
While grounded locomotion remains semantically continuous, that shadow advances
gait phase every render frame from the latest measured speed and exponentially
smooths displayed velocity. A new authoritative sample corrects the predicted
phase along the shortest circular path. Posture/action changes, backward or
large tick gaps, and large phase errors resynchronize immediately. Contact and
landing sequences remain authoritative and are never predicted.

Weapon guard is a two-state authoritative stance intent: `lowered` is the
default and `raised` is the direct-control Aim state. Every unreliable input
request carries the client's complete desired state rather than an edge event.
The server validates guard, movement, and look together, stores the accepted
guard only in transient tactical `SkeletonState`, and replicates it with the
rest of that state. It is never persisted to SpacetimeDB. Incapacitation
forces the authoritative state back to `lowered`.

Raised upright locomotion also carries transient procedural step intent and
cadence. Speed follows the controller throughout a step. Ordinary turns are
accepted at the next foot handoff, while a material opposite-direction reversal
performs an immediate safe semantic handoff so the support foot agrees with the
already-reversed gameplay root. Releasing movement completes only the active
half-step. Lowering, incapacitation, crouching, or leaving the ground clears the
intent.

The replicated stance is a tagged `lowered` or `raised` value; only the raised
variant can contain locomotion intent. Raised intent is itself tagged as
`planted` or `moving`. Both retain the wrapping step sequence needed to detect
coalesced handoffs, while only `moving` can contain normalized finite direction,
positive speed, and a swing foot. Construction and deserialization validate
those payloads. Its externally tagged representation is deliberately compatible
with Replicon's non-self-describing `postcard` wire codec. Lowering discards the
intent entirely. Repeated writes of
the already-current guard state preserve live footwork and gait phase.

A discrete raised/lowered change is smoothed from the currently displayed
effective pose. The pose buffer captures each joint's displayed local pose and
velocity relative to the new target and critically damps that offset toward
zero without retaining the outgoing clip. Resolved fallback clips and their
whole-body mirror contribution remain part of the effective pose, so an
incomplete guard asset set does not hard-cut from locomotion to a relaxed
fallback. Changing direction or authoritative gait phase does not restart
transition smoothing, and an interruption begins from the pose already on
screen rather than either original endpoint.

The server owns movement, body mode, authoritative action timing, gameplay
position, attack timing, hits, damage, and other outcomes. Typed action starts
return an explicit admission result. Attacks and blocks require idle, a valid
swing follow may replace a contacted swing, and defensive dodge or quickstep
explicitly preempts an attack or block. Repeated evasion cannot restart its own
timeline. Defensive dodge and quickstep are separate variants; only a quickstep
can contain a finite, non-zero normalized travel direction. Downed bodies and
active posture transitions reject action starts. Entering a downed body mode
atomically lowers guard and cancels the presentation action. A client may begin
an animation immediately in response to local input, then reconcile it with the
server's accepted skeleton state. Bone transforms, terrain-adjusted foot
positions, and secondary motion are presentation and are not authoritative. This
follows the tactical trust boundary described in
[Networking](../engineering/networking.md#tactical-experience).

For remote characters, an action start tick and a gait/lead-foot anchor are
enough to advance the animation locally. Individual bone transforms should not
normally be replicated.

## Animation evaluation

The animation evaluator consumes skeleton state in this order:

1. Resolve the active animation pack and its fallback chain.
2. Select semantic poses for the posture, locomotion, stance, and action.
3. Advance normalized gait or action phase from authoritative timing and
   motion.
4. Interpolate authored poses using continuous blend coordinates.
5. Apply masked or additive layers such as directional ducking and impact
   flinches.
6. For ordinary locomotion, apply contact-weighted terrain correction, one
   shared hip correction, and one two-bone solve per leg. Raised-guard foot
   planning, hand/weapon constraints, and head/torso look follow. Body facing is
   already present on the replicated root.
7. Apply optional secondary animation.

The semantic router reads a read-only snapshot of `PresentedSkeleton` and its
pure `AnimationEvaluation`: speed, local direction, gait/action phase,
crouch/airborne state, attack height, lead and support feet, contact sequence,
effective pack, and the selected attack contact. Movement remains live input;
the action contains no captured movement or foot-step plan.

Attack clips are deliberately sampled as whole-body rotation sources: twisting
the pelvis may yaw the thighs, shins, ankles, and feet. Their authoring contract
does not translate either foot. The procedural raised-footwork pass remains the
sole owner of rendered foot positions and terrain contact, including when an
attack begins midway through a step. Authored attack yaw and procedural foot
translation therefore compose without capturing, replacing, or restarting the
step.

The pose buffer flattens the resolved clips onto a 30 Hz grid keyed by skeleton
family. Each tactical presentation tick interpolates baked keyframes at the
current shared semantic phase and retains previous/current per-character poses
for velocity and interruption capture. Quaternion interpolation and angular
velocity differences select the nearest quaternion hemisphere. Missing tracks
and non-finite samples fall back to the canonical bind transform, while imported
`body_world` translation is replaced by bind translation so authored root motion
cannot move the gameplay entity. The anatomical `root` joint is the pelvis, so
its authored translation remains part of the pose. State and clip-set changes
use interruption-safe per-joint inertial offsets. Characters beyond 100 metres
or outside the camera frustum freeze their buffered pose and discard sampling
debt when they return.

Authored FK remains ordered before bind restoration, whole-body mirroring, body
response, terrain IK or raised-guard foot planning, and weapon constraints. The
router and pose buffer cannot choose actions or contacts, advance authoritative
phase, emit gameplay events, displace the controller, or mutate server state.
Action authoring may stay sparse. Walk and run authoring supplies contact and
passing/flight poses; `scripts/build_locomotion_cycles.py` combines them with
their character-space mirrors into closed runtime motions. The graph samples
those complete cycles at authoritative gait phase. Record semantic frames in the
code-owned catalog, use full-body masks only where the resolver requires them,
and keep phase markers aligned with authoritative gait or action phase. Graphs
and masks are presentation assets. They cannot choose actions or contacts,
advance phase, emit authoritative gameplay events, apply root motion to the
controller, or move server state. The editor feature and its large UI/preview
dependencies are native-only and disabled in shipping Wasm.

Overgrowth similarly supplies named blend coordinates such as movement speed,
ground speed, crouch height, and attack height from AngelScript, then evaluates
nested synchronized animation groups against the same normalized time in
[`aschar.as`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Data/Scripts/aschar.as#L11834-L12077)
and
[`syncedanimation.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/syncedanimation.cpp#L127-L264).

### Blend coordinates

The initial evaluator should support at least:

- gait phase;
- movement speed, including continuous walk-to-run blending;
- crouch amount;
- local movement or dodge direction;
- airborne phase derived primarily from vertical velocity;
- attack target height;
- action phase; and
- layer weights for head look, impact reaction, and future secondary motion.

Animations blended along a shared coordinate must be phase-compatible. For
example, the contact frame of a walk must have the same gait phase as the
contact frame of a run and crouch-walk. Overgrowth's synchronized animation
groups also adjust playback frequency from actual ground speed, which is the
behavior we want to emulate rather than allowing feet to slide.

## Animation packs and fallback

An animation pack maps semantic names to authored pose data and may declare one
compatible parent pack. Ordinary posture, locomotion, duck, dive, block, and
downed poses resolve independently through that parent chain, then through the
deterministic similar-pose chain. Missing content ultimately leaves the rig in
its authored bind pose rather than crashing or hiding the actor.

Attack poses use a stricter rule because their presence changes gameplay. The
three-member set is `swing`, `swing_follow`, and `thrust`. Resolution walks up
the parent chain only until it finds the first pack containing any member of
that set. That pack owns all three results: present members are usable and
absent members remain unavailable. If no pack in the chain contains an attack,
the character has no melee attack animation capability.

This means a weapon pack may inherit every attack by defining none, or replace
only the attacks it actually supports and intentionally disable the rest. A
missing `swing` cannot be substituted with a parent's swing or with `thrust`;
the input layer may instead choose the available alternate strike family.
Likewise, a missing `swing_follow` forces recovery before another ordinary
`swing` may begin.

Fallback graphs are deterministic and validated when assets load or build:
cycles and missing parents are invalid, every pack in a chain must use a
compatible skeleton, and each skeleton family ends in a root pack. The root's
required non-attack semantics must resolve for a complete release build.
Attacks remain optional capabilities rather than completeness requirements.

An unarmed `thrust` may be a punch and an unarmed `swing` may be a swipe. The
state machine still requests the semantic family, so gameplay does not need a
special case for fists, claws, or weapons.
## Authored pose contract

All authored poses in a compatible pack chain must follow the same conventions:

- stable bone names and hierarchy;
- a documented forward axis, up axis, scale, and neutral root transform;
- consistent weapon and hand attachment points;
- explicit left/right foot contact metadata where relevant;
- phase-compatible locomotion poses;
- an action phase or event marker for attack contact and other important
  moments; and
- no baked assumption that visual root movement grants gameplay movement.

### Animation file convention

Every authored motion has its own Cascadeur source file and matching binary
glTF export. Files are grouped first by skeleton family. The initial humanoid
family uses `biped/`:

```text
biped/unarmed/idle_relaxed.casc
biped/unarmed/idle_relaxed.glb
biped/unarmed/walk.casc
biped/unarmed/walk.glb
```

The `.casc` and `.glb` files always share a basename. `.casc` is the editable
Cascadeur scene and `.glb` is its runtime export. Do not place several
unrelated motions at undocumented ranges of one long timeline, and do not use
Cascadeur Animation Tracks as animation names: tracks organize parts of a rig,
not runtime clips.

Editable unarmed motion sources live under `assets_src/biped/unarmed/`; runtime
exports live under `assets/animations/biped/unarmed/`. The semantic pack ID is
still `humanoid_unarmed`; `unarmed` is its ergonomic on-disk directory. The
character creator generates `assets_src/biped/unarmed/base.glb` from the
canonical John Fabelgeist recipe using the MHR v1.0.1 hierarchy. The animation
contract contains MHR's 127 joints plus `l_weapon` under `l_wrist`, `r_weapon`
under `r_wrist`, and the first-person reference `c_camera` under `c_head`.
These three zero-weight joints are part of the exported animation skeleton and
must remain present in every Cascadeur motion export.
The legacy top-level `assets_src/base.*` files describe only the replaced
placeholder rig. Prepare the MHR runtime scene deterministically with:

```powershell
python scripts/prepare_rig_base.py assets_src/biped/unarmed/base.glb assets/animations/biped/unarmed/base.glb
```

`base.glb` supplies only the spawnable skinned scene and contains zero
animations. Every other `.glb` is a non-spawnable motion source and must contain
exactly one animation, named or unnamed. The 30fps catalog, not the animation's
glTF name, assigns semantic anchors to file/frame pairs. A missing, malformed,
or short motion invalidates only that motion so pack and similar-pose fallback
can continue.

Publish every currently authored motion with one command:

```powershell
python scripts/prepare_animation_assets.py
python scripts/prepare_animation_assets.py --check
```

The publisher validates ordinary motions against the MHR runtime base, removes
their meshes and skins, retains only catalog-addressable ordinary-motion frames,
removes bind-default tracks, collapses constant tracks, builds closed walk/run
cycles, and emits every available bind-relative mirrored counterpart. Each
locomotion cycle stores five cubic anchor keys; Cascadeur's exported in-betweens
are not copied into runtime assets. Missing source motions are reported and
skipped so John can be republished incrementally while animation work continues.
The runtime `base.glb` remains the sole spawnable skinned mesh.

Git tracks the `.casc` authoring projects and ignores their reproducible
`assets_src/**/*.glb` exports. Export the applicable motion GLBs locally before
running the publisher; only the compact runtime GLBs under `assets/animations`
belong in a commit.

The equivalent Just recipes are `just prepare-animation-assets` and
`just check-animation-assets`. The individual generators remain available for
focused development:

```powershell
python scripts/build_locomotion_cycles.py
python scripts/mirror_gait_assets.py
python scripts/build_locomotion_cycles.py --check
python scripts/mirror_gait_assets.py --check
```

The same commands with `--check` verify committed output. Runtime motion GLBs
retain the canonical `Skeleton` node hierarchy, bind transforms, and animation
accessors, but contain no mesh, skin, material, texture, or image payload.

At runtime, the zero-animation base is given one canonical animation player on
its `Skeleton` scene root. Target IDs are rebuilt from the full stable bone-name
paths, matching Bevy's glTF loader convention. Each independently loaded motion
must animate a non-empty subset of those exact targets; a motion containing a
foreign target is rejected without invalidating the base or any other motion.

A file may contain more than one required semantic pose when those poses are
phases of the same coherent motion. For example, `biped/unarmed/walk.casc` contains a
complete walk cycle, with particular keyframes designated as `walk_contact`
and `walk_passing`. The corresponding `biped/unarmed/walk.glb` preserves the full
motion. The animation catalog maps each semantic pose to a file and frame; the
semantic poses are not separate glTF clips. Frames not named by the catalog are
ordinary in-betweens or endpoint references.

Author at 30 frames per second. Frame numbers in the tables below are part of
the asset contract. A cyclic file repeats its initial pose on the last frame so
that it previews as a closed loop; the repeated last frame is not a second
semantic sample. Cascadeur labels may repeat the semantic names for animator
convenience, but labels are not relied upon to survive glTF export.

Coordinates are glTF-native meters with +Y up, -Z forward, and +X anatomical
left. The bind pose is a T-pose. Each exported `.glb` uses the same skeleton,
bone names, hierarchy, bind pose, and neutral root transform as the rest of its
compatible pack chain. A root-family file may include the skinned mesh;
specialized overrides may contain only the compatible skeleton and animation.

#### File and keyframe assignments

Single-pose files place their named semantic at frame 0:

| File basename under `biped/unarmed/` | Semantic pose at frame 0 |
|---|---|
| `idle_relaxed` | `idle_relaxed` |
| `crouch_idle` | `crouch_idle` |
| `guard` | `guard` |
| `airborne_center` | `airborne_center` |
| `airborne_travel` | `airborne_travel` |
| `swing` | `swing` |
| `swing_follow` | `swing_follow` |
| `thrust` | `thrust` |
| `prone_idle` | `prone_idle` |
| `supine_idle` | `supine_idle` |
Gaits are complete cycles. Their second half is the opposite-foot counterpart
of the named first-half samples and is available for interpolation and
preview, but does not introduce additional semantic names:

| File basename | Frame assignments |
|---|---|
| `walk` runtime | 0 `walk_contact`; 16 `walk_passing`; 32 opposite contact; 48 opposite passing; 64 loop closure. The source file begins on contact and ends on the exact passing pose. |
| `run` runtime | 0 `run_contact`; 16 `run_flight`; 32 opposite contact; 48 opposite flight; 64 loop closure. The source file begins on contact and ends on the exact flight pose; moderate its silhouette in the authored asset rather than the cycle builder. |
| `prone_crawl` | 0 `prone_crawl_contact` |
| `supine_scamper` | 0 `supine_scamper_contact` |

Prone and supine locomotion interpolate directly between the authored contact
and its character-space mirror. They do not require authored passing poses or
baked cycle files; the compact mid-cycle blend is close enough for these low,
constrained gaits. The publication mirror step emits exact
`prone_crawl_mirrored` and `supine_scamper_mirrored` runtime clips, so both
endpoints remain present throughout the blend, and the root animation catalog
loads both generated files as ordinary motion endpoints. Character-space baking
reflects the center-line pelvis, torso, neck, and head transforms and exchanges
every matching `.L`/`.R` bone pair, including the complete palm, thumb, and
finger hierarchies. Exchanging only the major limbs tears asymmetric downed
poses away from their authored torso support and leaves their hands behind.

Raised-guard movement has no authored locomotion files. `guard` remains the
whole-body FK reference while the client procedurally plans the same live feet
for stationary, forward, backward, lateral, and diagonal movement.

Directional ducks are independent of stance lead:

| File basename | Frame assignment |
|---|---|
| `duck_forward` | 0 `duck_forward` |
| `duck_backward` | 0 `duck_backward` |
| `duck_left` | 0 `duck_left` |
| `duck_right` | 0 `duck_right` |
Every direction shares one stance-independent upper-body dive pose:

| File basename | Frame assignment |
|---|---|
| `dive` | Frame 0 supplies `dive_forward`, `dive_backward`, `dive_left`, and `dive_right` above the pelvis |

The runtime samples the direction-neutral frame-0 dive pose above the pelvis. It
ignores that file's pelvis and leg tracks, even when an older asset still
contains them. A missing `dive` uses `guard` for the upper-body layer. The
lower body holds the selected directional duck/load through takeoff, while a
procedural pelvis override removes the load or guard's hip yaw, faces the hips
forward, and supplies the dive direction with a 40-degree tilt. The override
remains fixed for terrain-dependent airtime, then unwinds as the load blends
into the ground-contact pose after authoritative contact. A forward dive
recovers to `prone_idle`, a backward dive recovers to
`supine_idle`, and lateral dives recover directly to their matching
`prone_supine_roll_<left|right>` side-supported midpoint. The latter seeds the
continuous downed-roll coordinate at that midpoint, so held camera-following
continues without passing through prone idle; without held aim it settles back
to prone normally. The transition root remains fixed through takeoff and flight
while the procedural pelvis tilt owns direction. During terrain-contact
recovery, the server progressively transfers the contact pose's directional
yaw to the character root at the same rate that the pelvis returns to its
canonical contact coordinates. This handoff preserves one world-space landing
heading instead of visibly turning toward camera-forward and snapping back at
the endpoint. Backward recovery chooses the positive-pi root branch against the
contact pose's negative-pi half turn, avoiding an otherwise equivalent endpoint
reached through a visible full flip. Forward and lateral
airborne-to-contact recoveries own the rendered skeleton for 20 fixed ticks
(0.3125 seconds at 64 Hz); the 180-degree backward-to-supine recovery uses 32
ticks (0.5 seconds) to retain the same continuity bound. During either span,
ordinary-locomotion IK, terrain IK, landing leg compression, locomotion body
response, and upright height normalization must remain disabled until that
transition completes. The dive file contains neither an impact pose nor arrival
at prone or supine idle. These are standalone single-pose files; the older
frame-5 convention belonged to the discarded combined duck/dive layout and is
not part of the runtime contract. A missing `dive.glb` uses `guard` above the
pelvis.

The shared supine contact convention keeps the head toward local +Z so rolls
remain coherent with prone and both side-supported poses. Relative to canonical
upright coordinates, that convention contributes an implicit positive-pi turn
inside the midpoint-to-upright half of `supine_transition`. The server leaves
the root fixed throughout the supine-to-midpoint half, then progressively
applies the equivalent negative-pi turn during the midpoint-to-upright half.
These two turns cancel in world space: the rendered character does not change
heading, while the authoritative root finishes in the correct upright
orientation. This counter-yaw applies only to `SupineToUpright`;
`prone_transition` and all rolls are unchanged. Authored transitions own body
facing only while active. At the upright endpoint, a continuously held aim input
immediately resumes ordinary camera-facing; it does not require a release and
second press. The authoritative input state represents camera-facing ownership
with one enum: free/settling, aim-driven downed roll, or modifier-driven downed
body alignment. Those modes cannot overlap, and no persistent facing-suspension
flag can survive a transition into upright movement.

Ordinary jump airborne motion uses the two generic single-pose files listed
above. A quickstep instead retains the guard pose for its complete low hop. Its
direction and action timeline are authoritative, while presentation leans into
travel procedurally. The authored guard supplies the bent-knee load without an
additional pelvis drop. Each takeoff ankle remains planted in world space until
the leg reaches its solve limit or the hip-to-ankle line
reaches a 45-degree arch. The leg solver then releases that foot and moves it
toward the authored guard position so both feet have returned by landing. At
contact, the dodge action and its procedural IK end immediately: there is no
separate landing pose or compression. Ordinary raised guard presentation
resumes on that contact frame and follows the character's remaining physical
momentum while authoritative horizontal drag slows it. This separation allows
raised guard locomotion to present externally imparted motion without requiring
a dodge action, which is also the intended seam for later knockback. Guard
pelvis correction is otherwise limited to the minimum required to keep a
retained foot target within leg reach. The shared dive pose remains the
upper-body airborne exception described above.

Attacks use the optional `swing`, `swing_follow`, and `thrust` frame-0 contact
poses. Runtime timing supplies the guard-to-contact and contact-to-guard spans.
No attack file names a lead, step, switch, stay, wind-up, or recovery.

Each block file uses its semantic name as its basename. Frame 0 reproduces
`guard`, frame 6 is the named block contact, and frame 14 returns toward
`guard`:

```text
block_cut_left
block_cut_right
block_thrust
```
The remaining ground transitions are single midpoint poses. Runtime blends
between their separately authored contact endpoints:

| File basename | Frame assignments |
|---|---|
| `prone_transition` | 0 `prone_transition` |
| `prone_supine_roll_left` | 0 `prone_supine_roll_left`. Runtime mirrors this canonical leftward roll when `prone_supine_roll_right` is absent. Supine-to-prone reverses the opposite-side motion so the player's requested travel direction remains left or right in world space. |
| `prone_supine_roll_right` (optional counterpart) | 0 `prone_supine_roll_right` |
| `supine_transition` | 0 `supine_transition` |

The catalog entries above designate exactly one authoritative file and frame
for every required semantic pose; endpoint interpolation is runtime-owned.
The publication mirror step also emits an exact `prone_supine_roll_right`
runtime clip from its leftward source. This keeps interpolation between a
mirrored midpoint and an unmirrored contact pose from becoming a fractional
post-blend reflection. Dive files aren't mirrored; each missing direction uses
`guard` above the pelvis.

The canonical procedural rig is the Meta MHR hierarchy. Its central semantic
chain is `body_world`, anatomical `root`, `c_spine0` through `c_spine3`,
`c_neck`, and `c_head`. Paired major joints use the `l_` and `r_` prefixes:
`*_clavicle`, `*_uparm`, `*_lowarm`, `*_wrist`, `*_upleg`, `*_lowleg`,
`*_foot`, and `*_ball`. Runtime-added `l_weapon` and `r_weapon` joints are
parented to their respective wrists, while `c_camera` is parented to
`c_head`. The full MHR set of distributed twist, hand, foot, face, and center
joints participates in whole-pose mirroring. Offline mirror baking reflects
each joint's deformation relative to its bind transform, preserving MHR's
rolled local bind frames. Procedural IK directly solves only the semantic
major-joint chains and preserves the intervening MHR locals. See the
tactical-client README for the concise exporter checklist.

Procedural IK rotates major thigh/shin and upper-arm/forearm joints through the
real twist intermediates while preserving authored twist locals. Stable bend
poles are stored in owner space; foot planting uses an authored bind-derived
sole axis, bounded terrain normals, toe-coherent slope tilt, and smooth gait
weights. Optional hand and held-weapon constraints are client presentation
only: the primary socket places the weapon before its secondary grip targets
the off hand. None of these targets are replicated in `SkeletonState`.

Every procedural leg solve constrains the effective knee pole to within
plus-or-minus pi/8 radians of the rendered foot direction. This remains true
while attacking: authored torso and leg rotations may change the requested
joint pose, but the attack introduces no separate foot target or solver state.
Native captures compare attacks against the same live raised-guard foot plans
used by equivalent movement without an attack.

The server-owned character transform remains authoritative. Authored root
motion may shape a lean but cannot move the controller, extend hit range, or
change the terrain-conformed contact targets.
### Animator-facing conventions

The pose descriptions below are intended to be sufficient for constructing
the assets without understanding the runtime evaluator. Until a final rig and
export axis are selected, **forward**, **backward**, **left**, and **right**
always mean the character's own local anatomical directions. Left and right
never mean screen direction.

Unless a pose says otherwise:

- place the character on a flat floor in the pack's standard scale;
- keep the world/root control at the reference origin and express the pose
  through the pelvis and skeleton rather than translating the character
  through the scene;
- keep the head in a plausible neutral gaze generally forward;
- keep joints slightly unlocked and anatomically plausible;
- preserve enough clearance between limbs for clothing, armor, and held items;
- use the pack's normal hand carriage: fists and open guard for unarmed combat,
  or the appropriate grip and weapon carriage for a weapon pack;
- pose the whole body, even when the pose will usually be used as a masked or
  additive layer; and
- mark every hand, foot, knee, elbow, forearm, torso, or other body surface
  intended to be planted against the floor or an opponent-side contact plane.

`Contact` in a pose name means a support or impact relationship, not a game
hit. A locomotion contact identifies a planted foot. An attack or block contact
identifies the instant at which the weapon, hand, or claw crosses its canonical
opponent-side contact plane. The engine remains responsible for authoritative
collision and damage.

Locomotion semantic anchors use the **left** side as their canonical first
half-cycle. Generated mirrored clips reflect the complete bilateral motion to
construct the opposite half and closure before runtime FK blending.

`guard`, all three attack contacts, and all four directional ducks are exact
single poses without lead variants. Attack-set parent fallback follows the
pack-local capability rule described above, while ordinary poses retain the
normal per-semantic fallback chain.
## Required semantic poses

The following inventory defines the complete initial humanoid unarmed pack.
Other packs may omit any of these names and inherit them through their single
fallback chain.

### Standing and locomotion

The opposite half of each sparse unarmed gait cycle is produced by mirroring
both the arm swing and leg motion. Packs with handed upper-body carriage use an
explicit weapon or hand constraint after gait reconstruction.

| Pose | Animator brief |
|---|---|
| `idle_relaxed` | Stand upright with the feet approximately hip- to shoulder-width apart and the weight balanced between them. Keep one foot only slightly ahead if perfect symmetry looks artificial. The knees and elbows remain unlocked, the shoulders hang without slumping, and the pelvis and ribcage are stacked comfortably. This is a non-threatening rest pose, not a combat guard. |
| `walk_contact` | Pose the instant of left-foot initial contact. The left leg reaches forward with the heel touching or about to touch the floor; the right leg trails with only its forefoot/toes still supporting. The legs are at their greatest useful separation. Weight is transferring forward rather than already resting fully on the left foot. The pelvis counter-rotates naturally and, where the hand carriage permits it, the right arm is forward and the left arm back. Both designated foot contacts must lie on the same floor plane. |
| `walk_passing` | Keep the left foot planted beneath or just behind the pelvis while the right swing foot passes beside it on the way forward. The right knee is flexed enough for toe clearance and the right foot is not planted. The body is near its tallest point in the walk cycle, but the left knee remains soft. Pelvis and shoulder rotation are between their contact extremes. |
| `run_contact` | Pose the instant the left foot accepts a running landing beneath or only modestly ahead of the center of mass. Flex the left ankle, knee, and hip to absorb load. The right leg trails and is leaving the prior flight phase. The torso inclines forward from the ankles without folding at the waist. Only the left foot is marked planted; avoid a long over-stride. |
| `run_flight` | Pose the airborne crossover after left support, with neither foot planted. The right knee travels forward, the left leg trails, and both knees remain flexed rather than forming a split. Keep the pelvis level enough to interpolate cleanly, the body traveling forward, and the upper-body carriage compatible with the pack's held item. This must read as a run flight phase, not a walking passing pose. |
| `crouch_idle` | Lower the pelvis by flexing hips, knees, and ankles while keeping both whole feet stably planted about shoulder-width apart. Keep the chest sufficiently upright to look forward and use the hands. Do not obtain the height by bending only the spine. The pose must be able to load into a jump and serve as the center of the directional duck blend. |

Walk and run use the same normalized gait phase, and speed blends continuously
between them. A run is not merely an exaggerated walk: it retains an airborne
phase, while walking retains ground contact. Until a dedicated crouch gait is
authored, crouched locomotion reuses the ordinary leg motion with its 0.025 m
phase bob. It does not add a flat-ground pelvis drop or leg solve, because
moving the pelvis alone would drive the authored feet through the floor.

Server-authoritative tactical movement tops out at 5.5 metres per second with
the guard lowered and 2.0 metres per second with the guard raised. Analogue
input preserves its radial magnitude, so half stick deflection requests 2.75 m/s
lowered or 1.0 m/s raised. Radial clamping prevents diagonal overspeed, generic
controllers without skeleton state use the lowered cap, and Ahoy still applies
its existing crouch multiplier and deceleration. Raised guard scales Ahoy's
acceleration frequency from 8 Hz to 22 Hz, so its 2.0 m/s cap and the ordinary
5.5 m/s run cap both produce the same 44 m/s² full-input ground acceleration.
Gait phase 0 through 1 is one complete left-right cycle rather than one step.
Shared typed walk, run, crouch, and raised-guard profiles own reference speed,
step distance, support, flight, bounce, and compression metadata used by both
authoritative projection and client presentation. This keeps cadence tied to
actual post-physics ground distance without duplicated timing formulas or
double-speed footfalls. The server retains each player's latest validated
analogue movement request until a later request explicitly replaces or clears
it. Before every movement step, the server restores Ahoy's disposable fixed-loop
accumulator from that state. Missing packets on the unreliable input channel
therefore cannot erase movement intent for one fixed tick. Intent drives the
controller but does not select an authored gait. Measured post-physics planar
velocity owns idle/walk/run selection and stride cadence. Presentation also
uses body-relative velocity for a sustained travel lean and measured
acceleration for a stronger transient inertial response. The authored
locomotion backs remain straight; the procedural response rotates the pelvis
and torso as one rigid hierarchy in the stable authored pelvis reference frame,
while exactly compensating the legs before their IK pass. This keeps gait-phase
pelvis twist from steering forward lean sideways and supplies forward, backward,
and lateral inclination without bending the spine or turning lean into an
additional leg animation. Turn-driven lateral inertia is deliberately gentler
than the forward acceleration response. Braking counter-lean scales with
current planar speed and fades to zero at rest, while acceleration into motion
retains the full response. The run flight curve contributes about nine
centimetres of visible rise after authored passing-height normalization. The
debug game-clock switch therefore cannot directly select a different gait.

Ordinary idle, walk, and run now follow one compact ownership contract:

1. The semantic evaluator returns idle/walk/run weights at the shared predicted
   authoritative phase.
2. Walk and run sample their closed 64-frame runtime cycles continuously.
3. The same semantic samples provide left/right IK weights. Walk retains
   support; run uses a narrow contact lobe and publishes real zero-weight
   flight intervals; idle loads both feet. Simulation support timing remains a
   separate authoritative locomotion concern.
4. Terrain IK keeps each animated ankle's XZ, adjusts only toward sampled
   terrain height and normal by that foot's weight, computes one bounded shared
   pelvis drop, and solves each leg once.
5. Ordinary locomotion has no world-space plants, planned contacts, procedural
   swing arc, stop capture, support-acquisition latch, or post-propagation
   ownership correction. Starts and stops use the single presentation
   inertial transition between pose-buffer targets.

This deliberately follows Overgrowth's division: authored locomotion owns the
performance and IK only conforms the final FK pose to terrain. Raised-guard
foot planning remains a specialized presentation system and continues
unchanged during attacks.


Monotonic contact and landing sequences drive deduplicated client presentation
messages for future audio/VFX only. Up to eight plausible missed contacts are
reconstructed in alternating order. A backward/reset or larger delta silently
resynchronizes instead of producing a phantom burst; missed landing changes
collapse to one latest observation. `sample_tick` is the observation tick of
the replicated sequence state, not a reconstructed historical event time.

Terrain conformity defaults on. Debug clients expose `F8` as a runtime A/B
toggle. Disabling it leaves graph-evaluated authored FK, locomotion height, and
procedural combat foot placement intact. The enabled ordinary path adds only
weighted terrain height/normal correction, one shared pelvis drop, and one
two-bone solve per leg; it never creates a world-space plant.
Debug clients also expose `F7` to toggle the connected local tactical mission
between normal and quarter-speed game time. Both client presentation and the
authoritative server clock change together, so movement, physics, combat, and
animation remain synchronized during slow-motion inspection.

The preferred end-to-end animation diagnostic is `just tactical-play diagnostic`.
It executes a structured JSON input script through the ordinary native client,
network transport, authoritative server, prediction, reconciliation, semantic
evaluation, and final procedural pose. Its default scenario walks, aims,
attacks, captures the gameplay view directly to PNG, and then exercises posture
transitions. Every `animation-state.jsonl` record includes the final propagated
global transform of every authored animation target. The standalone
`scripts/analyze_animation_bone_trace.py` parser accepts this rich trace and
checks attack hand excursion without loading the trace into an interactive
review.

The native `animation-viewer` remains a deterministic gameplay-presentation fixture
for regression and visual review. It uses the gameplay player-spawn observer,
character mesh, camera, terrain presentation, authored animation evaluator, and
procedural passes rather than maintaining parallel fixture implementations.
Every capture also writes `global-bone-transforms.jsonl`. Each line contains
the global translation, rotation, and scale of every animation target for one
logical frame, together with the character-root transform. Run
`python scripts/analyze_animation_bone_trace.py <trace>` to remove root travel
and rotation and enforce the five-centimetre minimum forward hand excursion for
an attack without loading the raw trace into an interactive review.
Locomotion is projected and integrated continuously at the authoritative 64Hz
fixed tick. Non-terrain scenarios can still exercise authored leg motion with
fixed controller Y. The terrain suite enables seeded uneven-ground IK for
cross-slope, uphill, downhill, diagonal, crouched, mid-stride toggle, hard-stop,
small tap-stop, flight-phase run-stop, tap/restart crossfade, speed-threshold
chatter, steady 5.5 m/s run, raised-guard tap-stop, gradual 90-degree turn, and
exact 180-degree reversal probes. A deterministic procedural clock prevents
asynchronous screenshot rendering from advancing retained IK state more than
once for the same logical tick, while the complete FK/IK pipeline still
reevaluates each view. The replay captures one raw gameplay-camera image plus
side and front diagnostic images of that exact pose. Its manifest records final
world-space bones, semantic IK weights, continuity, signed foot tracks and
separation, knee flexion and bend hemisphere, desired body-forward alignment,
bounded per-tick turning residual (including look-facing guards),
terrain-relative foot clearance, and authored/solved foot targets, phase-indexed
height extrema and peak count, contact-phase sole clearance, controller vertical
range, run flight duration/sole clearance, authoritative acceleration, retained
lean, landing compression, contact identity, landing identity, and fixed tick;
those signals locate suspect frames but do not replace review of the rendered
mesh. The manifest records baked clip count and bytes, sampled-pose count, and
the final culled character count so representative CPU and memory measurements
use the same fixture. For steady height scenarios, every complete cycle after
warmup must contain exactly two prominent peaks in the phase 0.25 and 0.75
passing windows. A 0.003 m prominence threshold filters sampling jitter while
still rejecting an extra visible beat. The steady terrain run additionally
requires alternating contact weights, 80-200 ms unsupported intervals, bounded
contact clearance, and a 2 cm maximum pelvis-height step. Ordinary feet are not
tested as stationary world plants. The flight-phase stop and tap/restart probes
apply their +1 cm transient toe floor only after locomotion begins or while
stop-settle owns a foot. Their zero-speed, no-settle pre-roll remains covered by
the general -1 cm terrain penetration tolerance and is not mislabeled as a Run
flight sample. Typed scenario metadata distinguishes ordinary, transition,
terrain, raised-guard, and landing gates. The suite includes a speed ramp, an
apex-adjacent hard stop, real forward-input camera/controller turns through 90
and 180 degrees, airborne landing, and a two-cycle cadence/contact fixture.
Every logical sample is evaluated repeatedly across the three review views; the
gate compares bones within 0.5 mm/0.05 degrees and requires unchanged
contact/landing sequences and event counts. The first fixed-tick evaluation owns
IK state advancement and the complete cached local pose; later views restore
that pose without re-entering or mutating support/release state. Success also
gates lean and phase continuity, hard-stop pelvis continuity from the
moving-to-zero edge through settling, two ordered contacts per cycle and shared
step distance, straight-run chest/head lateral excursion, event
order/count/deduplication, contact soles from -0.02 m to 0.04 m, run flight
soles from 0.05 m to 0.20 m, landing knee flex, foot preservation within 1 cm,
and landing penetration no lower than -1 cm. The
fixture supplies deterministic controller observations at the shared server
projection boundary and follows rendered terrain height only in the cross-slope
probe. Its replication-presentation probe withholds three of every four
projected skeleton samples while accelerating and turning, so render-side phase
prediction and resynchronization are exercised. It still does not run physics
contacts, the network transport itself, or recorded live input.

The individually selectable `flat-grid-walk-2.0` and `flat-grid-run-5.5`
scenarios replace the seeded hills with a mathematically flat surface and draw
quarter-metre grid lines, with stronger whole-metre lines and highlighted world
axes. They retain the live terrain-conformity and locomotion-height passes. Use
their gameplay, side, and front sequences to judge vertical cadence against an
unchanging horizon and to expose foot sliding against fixed world-space marks.

The vertical-excursion gate remains 0.20 m for ordinary flat-ground motion and
0.30 m for raised-guard scenarios. Each explicit terrain scenario adds only the
terrain relief measured beneath its sampled feet to the ordinary 0.20 m
envelope; this separates required pelvis reach correction from authored body bob
without weakening the flat-ground check. Cumulative planted-foot drift and
per-frame supported slip are gated only where world-space procedural plants
exist: raised guard, including while an attack pose is active. Raised-guard
scenarios require no more than 0.01 m cumulative support drift and at least 0.16
m inter-foot separation. Ordinary terrain locomotion is instead gated by pose
continuity, penetration, reach, knee flexion, and slope alignment. Explicit 5.5
m/s Run segments use a 0.15 m foot budget for complete authored cycle motion in
addition to 0.086 m of owner travel per 64 Hz sample, while the knee uses a 0.13
m budget. The exact authored flight pose measures 0.143 m of foot motion and
0.125 m of knee motion per sample; lower-speed non-run probes retain the 0.055 m
foot and 0.10 m knee budgets. Strict terrain-Run probes permit a 0.16 m knee
step for slope-aligned contact acquisition (measured at 0.152 m). Terrain-run
slope alignment permits the direct contact-weighted rotation without adding a
temporal foot-orientation cache. The analytic knee-flexion reserve and
bend-hemisphere gates use that same procedural scope; they are not asserted
against authored FK-only motion. Ordinary FK-only motion is reviewed through
continuity, clearance, phase-height, and visual gates rather than being
mislabeled as world-planted. Ordinary terrain probes require unloaded swing feet
to remain free of terrain correction and near-full graph contact weights to
converge on terrain. Stops are graph crossfades and do not require a
capture-point or planned-footprint diagnostic. Start/stop, guard-entry, and
crouch-state transitions permit at most 0.04 m of pelvis-height change per 64 Hz
sample; the pre-existing guard entry itself uses about 0.033 m of that budget.
Loop-seam gates apply only to repeatable cycles. The complete authored Run cycle
permits a 0.03 m sampled positional seam (measured at 0.029 m); other repeatable
cycles retain the 0.015 m gate. Start/stop, facing-turn, and raised-guard
release-at-peak scenarios are transition probes whose final simulation state
intentionally differs from the state that initiated them; their continuity
remains covered by the per-frame displacement and rotation gates instead.

During ordinary lowered-guard travel the server advances the replicated body's
authored +Z axis toward authoritative horizontal velocity at a bounded turn rate
that can complete a 180-degree reversal in 0.25 seconds. Camera pitch is removed
before planar gait projection. Camera yaw intentionally maps raw movement input
into world movement, but it is not applied again by either the client root or
authored-rig child. At idle, the last body yaw is retained; an exact reversal
uses a deterministic turn side. Raised guard, attack, and block retain
controller-yaw look facing while moving. This root is shared by local players,
remote players, bots, fallback bodies, authored rigs, and the viewer, with the
authored +Z/controller -Z half-turn represented once. The viewer additionally
replays gradual turns, an exact reversal, planted guard rotation, camera pitch,
cross-slope terrain, every raised cardinal and diagonal direction, release
during a step, and a mid-step lateral reversal.

For the locally controlled character, the client advances that same bounded
facing rule every presentation frame. Raised guard reads live camera yaw;
ordinary travel reads replicated authoritative velocity. Incoming server
transforms remain authoritative, but their sparser rotation samples cannot
reduce local aiming or movement-facing to network-cadence steps. Procedural
neck and head aim uses only the residual between camera direction and the
current-frame root rotation before transform propagation; it never reapplies
the root's yaw from the previous frame.

During lowered travel, forward walk and run continue to serve diagonal and
lateral travel. Ordinary raised upright grounded movement retains its current
support cadence and samples only the static `guard` pose. A client-only
procedural lower-body pass alternates one swing foot with exactly one
world-space support foot. Each compact step projects authoritative local
velocity from the step origin, retains the authored guard's separated stance
tracks, interpolates horizontally with a smooth curve, and adds a sine clearance
arc. Step reach scales with analogue speed and is bounded to combat-shuffle
distances. Raised swings use a high continuity ceiling rather than the old low
IK velocity limit: the ceiling is above the measured worst ordinary 2 m/s guard
step, so replacing the support foot can still meet its semantic contact
deadline. Unusually long recovery steps remain bounded and converge over
subsequent procedural steps instead of snapping in one frame.

Cadence follows current authoritative speed throughout the first step, so a
small acceleration sample cannot slow a complete cycle. The replicated raised
intent continues to own semantic step direction, cadence, swing side, and
handoff identity;
deriving a second direction from smoothed client velocity would mix controller
and authored-rig frames and can invalidate a reversal. A material
opposite-direction reversal
performs an immediate safe semantic handoff; releasing movement finishes only
the active half-step rather than freezing a foot in the air or completing an
entire two-step pulse. Support-foot identity is procedural cadence state, not a
second guard stance. The same `guard` pose is sampled for every direction.

Raised sprint input is the sole exception. Its gameplay speed remains the
character's endurance-neutral jog, but presentation layers the static guard on
the upper body over the ordinary walk/run interpolation on the lower body. The
clip resolver masks locomotion off `c_spine0` and every descendant upper-body
target while masking guard off `body_world`, anatomical `root`, and every MHR
leg target. Ordinary locomotion terrain IK owns this composite's legs;
procedural combat stepping owns every non-sprint raised movement.

Semantic intent carries a wrapping step sequence and a swing side. The
sequence increments at every handoff, allowing a client
that receives coalesced updates to reset safely even when normalized phase
returns to the same parity after a skipped full cycle. World-space targets
remain client-only.

Procedural guard plants and targets stay entirely client-side. Replicated
`SkeletonState` carries a tagged planted/moving intent whose moving payload has
semantic direction, speed, swing side, and step identity; it never carries bones
or world foot positions. Flat-ground placement works with terrain IK disabled
through `F8`. Raised planning and terrain conformity intentionally share one
ordered solver pass so pole, plant, and pelvis memory are sampled once per
frame. When terrain conformity is enabled, the same targets additionally follow
height and slope without replacing their planted XZ positions. Raised grounded
idle keeps the static authored guard while the procedural solver retains the
feet in world space. If rotating the guard moves an authored foot more than 4 cm
from its plant, the client replants one foot at a time over 0.16 seconds along a
short lifted arc around the body. The other foot remains the support, and the
moving target is constrained to the live guard corridor and minimum stance
separation. Knee bend directions are parallel-transported with each leg and
corrected continuously toward their anatomical hemisphere so a turn cannot
abruptly flip a pole target. The final raised-guard and attack knee pole has its
ground-plane yaw constrained to within plus or minus pi/8 radians of the
rendered foot-to-toe direction, then receives the vertical component required by
the leg's valid bend plane. This pivot state, like all procedural foot targets,
is presentation-only and is never replicated. Raised crouched and airborne
characters retain the existing crouch and airborne posture rules; specialized
raised variants can be added later.

### Combat guards

Every complete combat pack provides or inherits one `guard` pose. It is the
stable endpoint for attacks, blocks, and directional ducks. Place the feet on
two useful tracks with neither knee locked, distribute weight so either foot
can move, and carry the upper body according to the pack's fighting method.

There is no alternate lead guard and no switch animation. The procedural
raised-guard planner may put either foot forward as movement unfolds, and the
same authored guard remains interchangeable with those live foot targets.
### Crouching and directional ducking

Directional ducking begins from `guard` and keeps its live foot targets. The
four optional frame-0 contact poses are `duck_forward`, `duck_backward`,
`duck_left`, and `duck_right`. Direction describes the character's local body
or head displacement, not a stance lead or the attacker's bearing.

| Pose | Animator brief |
|---|---|
| `duck_forward` | Lower and incline the body forward behind the hands or weapon while preserving a controlled base. |
| `duck_backward` | Withdraw the head and upper torso while sitting the pelvis down and back between the feet. |
| `duck_left` | Shift the pelvis, ribcage, and head toward anatomical left without crossing the legs or moving the feet. |
| `duck_right` | Make the corresponding anatomical-right displacement while retaining the same foot contacts. |

The standalone `dive` file is a stance-independent upper-body airborne pose at
frame 0. It contains no impact or arrival pose; direction, contact timing, and
the subsequent prone, supine, or side-roll blend are runtime-owned.
### Jumping and dodging

Jumping and dodging share two sustainable airborne poses. Charge changes
height, distance, and timing rather than selecting another authored sequence:

| Pose | Animator brief |
|---|---|
| `airborne_center` | Hold both knees moderately flexed beneath the hips with the feet separated enough for balance. Keep the pelvis and torso generally upright and avoid implying horizontal travel. Neither foot or other body surface is marked planted. The arrangement must remain plausible when held through an unusually long vertical airtime. |
| `airborne_travel` | Arrange a compact traveling leap with the canonical left knee and foot leading and the right leg trailing without forming a split. Incline the body slightly along travel without folding the spine, keep both knees flexed, preserve forward awareness and controlled equipment carriage, and mark no floor contacts. The runtime can mirror the lower-body delta and orient its travel contribution from horizontal velocity. |

The sequence is:

```text
directional duck/load -> airborne blend -> directional duck/load -> guard
```

The load and landing both reuse the crouch/duck blend, with the runtime choosing
the pushing and receiving legs from horizontal velocity and planted-foot state.
It synthesizes the short leg extension after takeoff, reaches a receiving foot
beneath the projected center of mass before landing, and uses foot IK at ground
contact. The center-to-travel blend is driven primarily by horizontal speed;
air phase is driven primarily by vertical velocity so the airborne pose can
extend for different airtimes without slowing takeoff or landing. Overgrowth
uses the same general idea by deriving an `up_coord` from vertical velocity in
[`aircontrols.as`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Data/Scripts/aircontrols.as#L114-L180).

### Attacking

A pack may contain at most three attack contact poses:

| Pose | Purpose |
|---|---|
| `swing` | Initial swing contact. Without it, the character cannot swing. |
| `swing_follow` | A second swing that begins after the first swing reaches contact. Without it, another swing waits for full recovery. |
| `thrust` | Initial thrust contact. Without it, the character cannot thrust. |

These are single poses, not clips with authored wind-up, footwork, continuation,
or recovery. The evaluator blends `guard -> contact -> guard` on the
server-owned action timeline. `swing_follow` may replace a post-contact `swing`,
but it cannot chain directly into another follow.

The owning client keeps one buffered melee request, replacing the previous
buffered request when a newer input arrives. A second swing branches at contact
when `swing_follow` is available. Any request that cannot branch waits until
the current action recovers, then starts as an ordinary `swing` or `thrust` if
that initial pose remains available.

The equipped weapon declares a preferred family. Preferred and alternate input
request swing or thrust, but either input falls back to the other family when
only that initial pose is available. If neither initial pose is available, the
character cannot begin a melee attack.

Attack availability is resolved as one pack-local set. If a pack defines any
of the three attack poses, only that pack's three presence bits count; a
missing member is not borrowed from a parent. A pack that defines no attack
poses inherits the nearest parent's complete attack set.

Movement remains fully live throughout an attack. The attack pose may rotate
the pelvis, knees, ankles, or feet and may pivot a foot on its ball, but it does
not request, capture, replace, or recover any foot target. The ordinary
raised-guard locomotion and terrain-IK planner continues exactly as though the
character had not attacked. Authored root motion never moves the gameplay
controller or changes reach.

An attack start is transactional: the client creates its hit-timing state and
sends the network request only after the typed animation transition accepts the
action. An input received while another admissible melee action is recovering
is buffered instead of creating a gameplay attack with no visual action. The
server uses the weapon's complete authored windup for the visual preparation
and equal-length recovery. Its packet-jitter tolerance applies only to the
minimum gameplay authorization time and never shortens animation playback.

At contact, align the striking structure from the feet through the hips and
torso to the fist, claw, point, or edge without locking a knee or elbow. A
`thrust` travels generally forward along a direct line. A `swing` crosses the
target line with coordinated pelvis and ribcage rotation; `swing_follow` is the
natural continuation available after that first contact.
### Blocking

The initial design has one guard and three incoming attack lines: a cut from
left, a cut from right, and a thrust. It therefore uses three block contacts:

| Pose | Animator brief |
|---|---|
| `block_cut_left` | Interpose the weapon, shield, forearms, or other blocking structure on anatomical left while keeping the face and centerline protected. |
| `block_cut_right` | Make the corresponding structurally supported interposition on anatomical right. |
| `block_thrust` | Meet or deflect a forward thrust near the centerline without locking the elbows or abandoning the face. |

Each block file reproduces `guard` at frame 0, reaches its named contact at
frame 6, and returns toward `guard` at frame 14. Procedural height adjustment
may move the contact toward the predicted line without creating more authored
stance variants.
### Prone and supine

The initial complete set contains:

| Pose | Animator brief |
|---|---|
| `prone_idle` | Lie face-down with the chest and pelvis close to the floor. Support the upper body lightly on the forearms or hands so the head can look forward without an extreme neck extension. Keep the legs extended or modestly bent and separated enough for stability. Mark the torso/pelvis and supporting forearms as floor contacts as appropriate. Do not trap held equipment beneath the chest when a neutral alternative exists. |
| `supine_idle` | Lie on the back with the head and shoulders slightly raised enough to see forward. Flex the knees enough to keep the feet available for movement, with one or both soles planted, and keep the arms in a protective usable position rather than flat in a rigid anatomical display pose. |
| `prone_crawl_contact` | Show a contralateral crawling support: left forearm/hand reaches or plants forward while the right knee/inside leg advances, with the right arm and left leg contributing rearward support. Keep hips and chest low and mark the current supporting surfaces. This is the maximum useful extension of the crawl, not a long military split. |
| `supine_scamper_contact` | On the back, plant the left heel/foot and the opposite hand or forearm as the canonical extended support, with the pelvis slightly lifted or unloaded enough to move. The right leg is advancing toward its next plant. Protect the head and keep the neck from bearing body weight. |
| `prone_transition` | Pose the stable intermediate between standing/crouching and prone: both hands or forearms and at least one knee contact the floor, the head remains protected and able to look forward, and the pelvis is low enough to continue down without dropping the chest through the ground. It must work in both directions. |
| `prone_supine_roll_left` | Pose the stable side-supported midpoint of a leftward roll between prone and supine. Keep the head clear of the floor, draw the near arm and held equipment out of the torso's path, and use the shoulder, flank, hip, and bent legs to distribute contact without balancing on the neck or spine. The silhouette must remain valid when mirrored for `prone_supine_roll_right` and when traversed backward from supine. |
| `prone_supine_roll_right` | Optional authored counterpart to the canonical leftward roll for equipment or anatomy that cannot be mirrored cleanly. Preserve the same contacts and endpoint compatibility on the opposite side. |
| `supine_transition` | Pose the protected midpoint between supine and upright/crouched support. Turn partly onto one side, post with a hand or forearm, bring at least one foot beneath the body, and keep the head protected while the pelvis changes level. Avoid a symmetric sit-up that leaves both hands and feet unavailable. It must work in both directions, even though direct controls currently use only the get-up direction. |

Backward crawling may initially reverse the forward cycle, and getting up may
reverse the upright-to-prone transition. Pressing the jump/roll modifier with
a lateral direction rolls from prone to supine; the same motion plays backward
from supine to prone, with the opposite authored side
selected so the requested travel direction remains unchanged. Releasing an
armed posture control begins the applicable get-up.
While aim/block is held downed, camera yaw selects one of four discrete sectors
around the character's head-to-feet axis: prone, right side-supported roll,
supine, or left side-supported roll. Their nominal boundaries lie halfway
between the 90-degree sector centers. A selected sector remains committed for
10 degrees beyond its nominal edge, producing a 20-degree hysteresis deadband
when the camera reverses near a boundary. The pose is static while the camera
remains inside that sticky sector and interpolates through the authored roll
only after another sector is committed. Releasing aim settles to whichever
prone/supine contact endpoint is nearer to the camera angle. This path reuses
the same two roll midpoints and requires no additional animation.
Supine may also result from a hit or physical fall and uses the direct
`supine_transition` motion when recovery does not first require a ragdoll handoff.
The initial controls do not include prone strafing.

Prone travel uses the ordinary pace controls with crawl-specific speeds: walking
is fixed at 0.45 m/s, jogging is one third of the character's neutral upright
jog speed, and sprinting reaches 2.0 m/s. Crawl effort is assessed at three
times its physical speed, making the middle pace breath-neutral and the maximum
pace exhausting at every endurance rank. Prone WASD input uses tank controls in
the body's orientation rather than the camera orientation, with lateral input
limited to three-eighths of longitudinal speed. Downed postures retain Ahoy's
shortened crouch collision shape without inheriting its upright one-third crouch
speed penalty. Authored prone contacts follow authoritative velocity directly.
Their 0.60 m contact step balances the authored hand and knee reach so neither
support point accumulates the much larger slide produced by a full metre of
controller travel. Supine scampering remains capped at 2.4 m/s with its
deliberately reduced contact cadence. All authored posture transitions keep the
gameplay root facing fixed: the directional dive and get-up poses encode their
own direction relative to that root and must not be rotated a second time toward
residual velocity.

## Initial complete-pack size

The humanoid unarmed root has 28 required non-attack semantics. Two mirrored
pairs and one dive file shared by four directional semantics let it satisfy
those with 23 authored poses. Attack poses are optional capabilities and
therefore do not participate in complete-pack validation.
The current authored size is:

| Family | Authored poses |
|---|---:|
| Standing and locomotion | 6 |
| Directional ducking | 3 |
| Jumping, dodging, and diving | 3 |
| Prone and supine | 7 |
| Combat guard | 1 |
| Blocks | 3 |
| **Required unarmed root** | **23** |
| Optional attacks | **0-3** |

A specialized pack may inherit all ordinary poses. If it supplies no attacks,
it also inherits the nearest parent's attack set. Once it supplies any attack,
its own missing attack members deliberately remain unavailable.
## Secondary animation

Secondary procedural dynamics may later add bone inertia and reactions to
movement, collisions, weapons, clothing, and equipment. Those effects remain
client presentation and must not move the replicated player root, gameplay
collider, hitboxes, or persistent strategic state.
## Stylistic principles

Animations should remain realistic in accordance with the
meta-level heuristics. Melee attacks should generally be inspired
by historical European martial arts. Trained characters therefore use much
less exaggerated anticipation than typical action-game or film choreography.
Future animation packs may vary by skill or species: an untrained character
or goblin may telegraph attacks much more strongly, making them easier to
dodge, while using the same semantic attack graph.

Because the game is online, an animation may have different apparent timing
from different perspectives. The local character should react immediately to
input, while authoritative action times ensure that the consequential moment
agrees for all participants. Network delay should be absorbed preferentially
in preparation or recovery segments rather than by uniformly slowing the
entire action.
