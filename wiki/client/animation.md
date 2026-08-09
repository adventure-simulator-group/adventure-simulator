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
keeping Adventure Simulator's gameplay authority separate from client-side
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
- **Action payload:** dodge direction, block line, or attack family,
  stay/switch footwork, target height, and a saturating authoritative timeline,
  carried only by the corresponding action variant.

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

A discrete raised/lowered change is presentation-crossfaded from the currently
displayed effective pose over 0.18 seconds. This includes resolved fallback
clips and their whole-body mirror contribution, so an incomplete
guard asset set does not hard-cut from locomotion to a relaxed fallback. The
crossfade clock advances once per simulation sample in deterministic capture
tools (and by render delta in gameplay); changing direction or gait phase does
not restart it. Reversing guard during the blend starts from the pose already
on screen rather than either original endpoint.

The server owns movement, body mode, authoritative action timing, gameplay
position, attack timing, hits, damage, and other outcomes. Typed action starts
currently preserve the established last-writer-wins replacement behavior;
there is no invented action-vs-action rejection table. Entering a downed body
mode atomically lowers guard and cancels the presentation action. A client may begin an
animation immediately in response to local input, then reconcile it with the
server's accepted skeleton state. Bone transforms, terrain-adjusted foot
positions, and secondary motion are presentation and are not authoritative.
This follows the tactical trust boundary described in
[Networking](../networking.md#tactical-experience).

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
6. For ordinary locomotion, apply graph-weighted terrain correction, one shared
   hip correction, and one two-bone solve per leg. Specialized combat footwork,
   hand/weapon constraints, and head/torso look follow. Body facing is already
   present on the replicated root.
7. Apply optional secondary animation.

The dependency-backed semantic bridge routes ordinary grounded locomotion and
raised-guard/attack evaluation through real dependency `AnimationGraph`
queries. Its inputs are a read-only snapshot of
`PresentedSkeleton` plus `AnimationEvaluation`: speed, local direction,
gait/action phase, crouch/airborne state, attack height, lead and support feet,
contact sequence, effective pack, and the attack's captured step
direction/speed. The bridge flattens anchor samples directly and span samples
into separate start/end contributions. A chain of dependency pose-blend nodes
composes those weights; the graph-returned start/end weights atomically
reconstruct each `PoseSample` total and span progress before driving the existing
effective-pack resolver. Persistent contexts are keyed by player and route and
seek meaningful gait/action phase; despawned-player contexts are pruned. A
missing, malformed, non-finite, out-of-range, or non-normalized graph output
discards the entire temporary decode and selects the untouched legacy
evaluation. Resolution
therefore remains exact pose, same-pack mirrored counterpart, then parent
fallback, independently for each requested semantic. Specialized packs can
continue overriding only a subset.

The existing authored FK player remains ordered before bind restoration,
whole-body mirroring, body response, terrain IK/attack footwork, and weapon
constraints. Ordinary locomotion samples complete continuous cycles;
guard/action fallback still chooses one coherent whole-body mirror. The bridge
does not consume root motion, emit gameplay events, choose actions or contacts,
advance authoritative phases, displace the controller, add a second
inertializer, or add another IK pass. The existing 0.18-second presentation
crossfade remains the sole transition smoothing.

### Graph authoring capability map

The pinned dependency supplies generic semantic anchor nodes, 1D and 2D sparse
blend spaces, nested reusable graphs, bone-masked linear layers,
additive/difference layers, semantic mirroring, marker synchronization, and
presentation-only transitions/inertialization. Adventure Simulator's initial
bridge intentionally uses only the smallest subset: a registered custom sparse
semantic blend node, a fixed dependency pose-blend chain, and dependency pose
evaluation for ordinary locomotion and raised guard/attack. Existing code
continues to own sparse 1D speed/gait-phase
and directional selection, nested effective-pack fallback, binary gait endpoint
mirroring, coherent whole-body guard mirroring, and the single 0.18-second
presentation crossfade. Bone masks remain in the authored resolver. The bridge
does not yet claim to use the dependency's 2D blend-space, nested-graph,
additive/difference, marker-sync, or inertializer nodes.

Launch the project-compatible native editor with `just animation-graph-editor
assets`. It registers the custom sparse blend node, reports optional missing
motion files, validates anchor frame bounds and deterministic catalog fallback,
then validates and queries the same centralized runtime graphs for a
representative ordinary stride and right-lead attack before opening the UI.
Graph load, schema, or query failures are fatal. Use `just animation-graph-preview`
for deterministic viewer/capture evidence; editor clip preview does not replace
the viewer manifest and failure gates.

Action authoring may stay sparse. Walk and run authoring supplies contact and
passing/flight poses; `scripts/build_locomotion_cycles.py` combines them with
their character-space mirrors into closed runtime motions. The graph samples
those complete cycles at authoritative gait phase. Record semantic frames in
the code-owned catalog, use full-body masks only where the resolver requires
them, and keep phase markers aligned with authoritative gait or action phase.
Graphs and masks are presentation assets. They cannot choose
actions or contacts, advance phase, emit authoritative gameplay events, apply
root motion to the controller, or move server state. The editor feature and its
large UI/preview dependencies are native-only and disabled in shipping Wasm.

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

An animation pack maps semantic animation names to authored pose data. A pack
may define any subset of the semantic names. This lets a weapon, creature,
skill level, injury, or stylistic variant override only the motions that are
actually distinctive.

Each pack may declare exactly one fallback pack. The declaration belongs to
the pack as a whole: there are no per-pose or per-category fallback settings.
When a requested semantic animation is absent, lookup continues through that
single fallback, and the fallback may itself have another fallback. For
example:

```text
expert_rapier
    -> rapier
        -> one_handed_sword
            -> humanoid_unarmed
```

`expert_rapier` might override only its attacks. `rapier` might add its guards,
blocks, and thrusts. `one_handed_sword` might provide generic one-handed
carriage while moving. All remaining poses would resolve from
`humanoid_unarmed`.

Lookup is performed independently for each requested semantic name, but every
miss follows the same pack-level chain. A child cannot choose one fallback for
locomotion and another for attacks.

Fallback resolution must be deterministic and validated when assets are
loaded or built:

- fallback cycles are invalid;
- every referenced fallback must exist;
- every pack in a chain must use a compatible skeleton or an explicitly
  supported retargeting relationship;
- the chain must end in a complete unarmed-combat pack; and
- the resolved table must contain every semantic animation required by that
  skeleton family.

The complete unarmed pack is the ultimate fallback for a skeleton family.
Different skeleton families may need different authored unarmed roots; a
humanoid pose cannot automatically animate a quadruped. They nevertheless
implement the same semantic action vocabulary wherever their anatomy permits
it.

At the semantic level, a **punch is the unarmed implementation of a thrust**,
and a **swipe is the unarmed implementation of a swing/slash**. This is
particularly useful for creatures with claws. Gameplay and the state machine
request `thrust` or `slash`; the resolved unarmed pack supplies a punch or
swipe pose sequence. Code should not need special cases for `punch` versus
`weapon thrust` or `claw swipe` versus `weapon slash`.

An equipped weapon still selects the character's effective animation pack.
The fallback mechanism means that this selection need not duplicate identical
walking, prone, or airborne poses. A weapon pack may override those poses when
the weapon genuinely changes how the character carries their body.

During asset production, runtime lookup is deliberately more tolerant than
final content validation. After the selected pack's single fallback chain
misses a semantic name, the client follows a deterministic similar-pose chain
and restarts pack lookup for each candidate. Examples include `run_contact` to
`walk_contact`, `run_flight` to `walk_passing`, `airborne_travel` to
`airborne_center`, same-lead thrust contacts to slash contacts, and then attack
or block poses to the appropriate guard. If neither chain resolves, the client
displays the skeleton's authored bind pose, which is a T-pose for the humanoid
convention. Release validation still requires the complete unarmed root
described above; this graceful runtime behavior exists so incomplete or
temporarily unavailable art never crashes the tactical client or makes an
actor disappear.

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
existing `assets_src/base.*` files remain the special-case rig source until the
new `assets_src/biped/unarmed/base.casc` has a matching GLB. Prepare its
runtime-only scene deterministically with:

```powershell
python scripts/prepare_rig_base.py assets_src/base.glb assets/animations/biped/unarmed/base.glb
```

`base.glb` supplies only the spawnable skinned scene and may contain zero
animations. Every other `.glb` is a non-spawnable motion source and must contain
exactly one animation, named or unnamed. The 30fps catalog, not the animation's
glTF name, assigns semantic anchors to file/frame pairs. A missing, malformed,
or short motion invalidates only that motion so pack and similar-pose fallback
can continue.

Prepare non-locomotion motions by validating and copying their source export
exactly:

```powershell
python scripts/prepare_animation_motion.py assets_src/biped/unarmed/idle_relaxed.glb assets/animations/biped/unarmed/base.glb assets/animations/biped/unarmed/idle_relaxed.glb --last-frame 0
```

Walk and run are the exception. Their source exports contain the authored
contact and passing/flight poses, while the committed runtime files are closed
cycles baked from those poses and their character-space mirrors:

```powershell
python scripts/build_locomotion_cycles.py
python scripts/mirror_gait_assets.py
python scripts/build_locomotion_cycles.py --check
python scripts/mirror_gait_assets.py --check
```

The same command with `--check` verifies committed output. Motion scenes and
meshes need not be stripped because only the animation asset is loaded; keeping
the raw bytes makes source-to-runtime generation deterministic and auditable.

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

Single-pose files place their named semantic pose at frame 0:

| File basename under `biped/unarmed/` | Semantic pose at frame 0 |
|---|---|
| `idle_relaxed` | `idle_relaxed` |
| `crouch_idle` | `crouch_idle` |
| `guard_lead_left` | `guard_lead_left` |
| `guard_lead_right` (optional counterpart) | `guard_lead_right` |
| `airborne_center` | `airborne_center` |
| `airborne_travel` | `airborne_travel` |
| `attack_thrust_lead_left_contact` | `attack_thrust_lead_left_contact` |
| `attack_thrust_lead_right_contact` (optional counterpart) | `attack_thrust_lead_right_contact` |
| `attack_slash_lead_left_contact` | `attack_slash_lead_left_contact` |
| `attack_slash_lead_right_contact` (optional counterpart) | `attack_slash_lead_right_contact` |
| `prone_idle` | `prone_idle` |
| `supine_idle` | `supine_idle` |

Gaits are complete cycles. Their second half is the opposite-foot counterpart
of the named first-half samples and is available for interpolation and
preview, but does not introduce additional semantic names:

| File basename | Frame assignments |
|---|---|
| `walk` runtime | 0 `walk_contact`; 16 `walk_passing`; 32 opposite contact; 48 opposite passing; 64 loop closure. The source passing pose is frame 8. |
| `run` runtime | 0 `run_contact`; 16 `run_flight`; 32 opposite contact; 48 opposite flight; 64 loop closure. The runtime flight key is the exact source frame-5 pose; moderate its silhouette in the authored asset rather than the cycle builder. |
| `prone_crawl` | 0 `prone_crawl_contact`; 8 `prone_crawl_passing`; 16 opposite contact; 24 opposite passing; 32 loop closure |
| `supine_scamper` | 0 `supine_scamper_contact`; 8 `supine_scamper_passing`; 16 opposite contact; 24 opposite passing; 32 loop closure |

Raised-guard locomotion files each contain one directional movement pose at
frame 0. They are not gait cycles and do not contain or imply an opposite-foot
half. The other runtime endpoint is the separate same-lead static guard:

| File basename | Frame assignments |
|---|---|
| `guard_walk_lead_left` | 0 `guard_walk_lead_left` movement extreme |
| `guard_walk_lead_right` (optional counterpart) | 0 `guard_walk_lead_right` movement extreme |
| `guard_strafe_lead_left_left` | 0 `guard_strafe_lead_left_left` movement extreme |
| `guard_strafe_lead_left_right` | 0 `guard_strafe_lead_left_right` movement extreme |
| `guard_strafe_lead_right_left` (optional counterpart) | 0 `guard_strafe_lead_right_left` movement extreme |
| `guard_strafe_lead_right_right` (optional counterpart) | 0 `guard_strafe_lead_right_right` movement extreme |

Each guard-relative duck extreme is a single pose in its own file at frame 0.
The runtime blends from the current guard to the extreme and back. The lead
and final direction components are both semantic: a left-lead dodge toward
anatomical left is not interchangeable with a left-lead dodge toward right.

| File basename | Semantic pose at frame 0 |
|---|---|
| `duck_lead_left_backward` | `duck_lead_left_backward` |
| `duck_lead_left_left` | `duck_lead_left_left` |
| `duck_lead_left_right` | `duck_lead_left_right` |
| `duck_lead_right_backward` (optional counterpart) | `duck_lead_right_backward` |
| `duck_lead_right_left` (optional counterpart) | `duck_lead_right_left` |
| `duck_lead_right_right` (optional counterpart) | `duck_lead_right_right` |

An exact file always wins. If one side is absent, the runtime mirrors the
opposite-side pose from the same pack before consulting its parent pack. A
whole-body mirror swaps both the stance lead and anatomical direction, so the
pairs are `left_backward`/`right_backward`, `left_left`/`right_right`, and
`left_right`/`right_left`.

Airborne motion uses the two single-pose files listed above. The runtime blends
from a directional crouch/load into `airborne_center` or `airborne_travel`,
modifies the traveling pose from horizontal velocity, and returns through a
directional crouch/load on landing. There are no separate authored launch,
direction, or landing samples in the complete pack.

Attacks likewise use the single-pose contact files listed above. There are no
required commit or follow-through poses, and `stay` versus `switch` is not part
of the asset name. Runtime footwork, continuation, and recovery turn the
contact pose into a complete attack. A pack may export both lead files when
handedness makes them distinct, or export only one and let the missing lead use
the mirrored same-pack counterpart.

Each block contact has its own file using the semantic pose name as its
basename. Frame 0 reproduces the applicable guard, frame 6 is the named block
contact, and frame 14 returns toward that guard. Instantiate this layout as:

```text
block_cut_left_lead_left
block_cut_left_lead_right
block_cut_right_lead_left
block_cut_right_lead_right
block_thrust_lead_left
block_thrust_lead_right
```

The remaining ground transitions each use one motion-coherent file:

| File basename | Frame assignments |
|---|---|
| `upright_prone_transition` | 0 upright/crouch reference; 12 `upright_prone_transition`; 24 `prone_idle` reference |
| `dive` | 0 launch reference; 10 `dive_impact`; 18 `prone_idle` reference |

Endpoint references make each file understandable when previewed and provide
useful interpolation, but they do not redefine semantic poses owned by another
file. The catalog entries above designate exactly one authoritative file and
frame for every required semantic pose.

The canonical procedural rig uses `root`, `pelvis`, `stomach_01`,
`stomach_02`, `chest`, `neck_01`, `neck_02`, and `head`; paired clavicle,
major arm, arm-twist, hand, major leg, leg-twist, foot, and toe bones use the
`.L`/`.R` suffix. `weapon.L` and `weapon.R` are hand attachment sockets. The
scene-root animation cylinder is authoring-only. See the tactical-client README
for the concise exporter checklist.

Procedural IK rotates major thigh/shin and upper-arm/forearm joints through the
real twist intermediates while preserving authored twist locals. Stable bend
poles are stored in owner space; foot planting uses an authored bind-derived
sole axis, bounded terrain normals, toe-coherent slope tilt, and smooth gait
weights. Optional hand and held-weapon constraints are client presentation
only: the primary socket places the weapon before its secondary grip targets
the off hand. None of these targets are replicated in `SkeletonState`.

The server-owned character transform remains authoritative. Authored root
offsets may shape a step or lean visually, but the evaluator reconciles them
with actual movement. Terrain alignment, final foot height, and exact contact
placement should not be baked into the poses.

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
half-cycle. When a sparse gait source contains only that half, generated
mirrored clips reflect the complete bilateral limb motion—including clavicles,
arms, hands, legs, feet, and twist bones—to construct the opposite half and
closure before runtime FK blending.
Guard, attack, and guard-relative duck counterparts use presence-based
mirroring. The runtime prefers an exact pose in the requested pack, then a
whole-body mirrored opposite-side pose from that same pack, and only then the
parent pack and ordinary semantic fallback chain. Symmetric styles such as
unarmed combat can therefore author one side. Handed weapon styles author both
sides whenever reflection would move the weapon to the wrong hand or otherwise
change the technique.

Guard locomotion follows the same exact-first rule. Its mirror pairs are walk
left/right, strafe left-lead-left/right-lead-right, and strafe
left-lead-right/right-lead-left. If a strafe is still unavailable it falls
back to the same-lead guard walk and then the same-lead static guard. A
diagonal may mix walk and strafe contributions; the resolver selects one
coherent whole-body mirror orientation for that blend. It scores both parity
candidates by how many requested movement semantics and guard endpoints they
preserve before ordinary fallback. Thus a complete opposite-side walk+strafe
set beats an exact walk plus a collapsed strafe, while an exact cardinal pose
wins a complete tie. Partial asset sets cannot create a fractionally mirrored
body.

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
input preserves its radial magnitude, so half stick deflection requests 2.75
m/s lowered or 1.0 m/s raised. Radial clamping prevents diagonal overspeed,
generic controllers without skeleton state use the lowered cap, and Ahoy still
applies its existing crouch multiplier and deceleration. Raised guard scales
Ahoy's acceleration frequency from 8 Hz to 22 Hz, so its 2.0 m/s cap and the
ordinary 5.5 m/s run cap both produce the same 44 m/s² full-input ground
acceleration. Gait phase 0 through 1 is
one complete left-right cycle rather than one step. Shared typed walk, run,
crouch, and raised-guard profiles own reference speed, step distance, support,
flight, bounce, and compression metadata used by both authoritative projection
and client presentation. This keeps cadence tied to actual post-physics ground
distance without duplicated timing formulas or double-speed footfalls.
The server retains each player's latest validated analogue movement request
until a later request explicitly replaces or clears it. Before every movement
step, the server restores Ahoy's disposable fixed-loop accumulator from that
state. Missing packets on the unreliable input channel therefore cannot erase
movement intent for one fixed tick. Intent drives the controller but does not
select an authored gait. Measured post-physics planar velocity owns idle/walk/run
selection and stride cadence, while measured acceleration is used only for
procedural body response. The debug game-clock switch therefore cannot directly
select a different gait.

Ordinary idle, walk, and run now follow one compact ownership contract:

1. The dependency animation graph returns idle/walk/run weights at the shared
   predicted authoritative phase.
2. Walk and run sample their closed 64-frame runtime cycles continuously.
3. The same graph-returned samples provide left/right IK weights. Walk retains
   support; run uses a narrow contact lobe and publishes real zero-weight
   flight intervals; idle loads both feet. Simulation support timing remains a
   separate authoritative locomotion concern.
4. Terrain IK keeps each animated ankle's XZ, adjusts only toward sampled
   terrain height and normal by that foot's weight, computes one bounded shared
   pelvis drop, and solves each leg once.
5. Ordinary locomotion has no world-space plants, planned contacts, procedural
   swing arc, stop capture, support-acquisition latch, or post-propagation
   ownership correction. Starts and stops use the single presentation
   crossfade between graph poses.

This deliberately follows Overgrowth's division: authored locomotion owns the
performance and IK only conforms the final FK pose to terrain. Combat guard and
attack footwork remain specialized systems outside this ordinary-locomotion
contract.

<!-- Historical ordinary-locomotion implementation notes retained temporarily
for archaeology; they describe the state machine bypassed by the current path.

Contact and passing/flight anchors are authoritative sparse gait inputs. The
evaluator constructs four smooth quarters: contact to passing, passing to the
character-space mirrored contact, mirrored contact to mirrored passing, and
mirrored passing back to contact. It does not traverse later exported gait
timeline data or any baked in-between frames between the two anchors. The
client pauses distinct Bevy graph nodes at the exact catalog frames and blends
their bone transforms using a monotone cubic Hermite quarter-cycle weight.
The semantic anchors remain exact, while zero endpoint velocity prevents the
knee and ankle velocity discontinuity produced by linear sparse-pose blending.
Each graph contribution is a complete unmirrored or pre-mirrored endpoint.
Parity is never averaged into a fractional post-FK reflection: at the middle
of passing-to-opposite-contact, Bevy blends half of each complete pose rather
than pulling both legs toward their reflected counterparts. Character-space reflection retains anatomical lateral spacing
rather than swapping bones discretely. Support narrows through the walk/run
blend and releases both feet for roughly 90-110 ms at each 5.5 m/s run flight
beat; the visual gait keeps at least 0.05 m of sole clearance during that flight.
With terrain IK enabled, only the current contact leg is ground-constrained by
the analytic terrain solver. On support loss, the unsupported swing leg releases
in owner space from its last solve target toward authored FK at no more than
3.1 m/s (about 4.8 cm per 64 Hz sample), without a world plant, terrain floor,
or terrain projection. Each release converges on a frozen snapshot of the
authored swing target before refreshing that goal, so a rapidly moving authored
pose cannot make the validation mistake bounded following for foot sticking.
If an advancing hip would require more than 1.5 cm of reach correction, support
is released before the retained footprint can skate. After that bounded
airborne release, and during the
latter part of its swing,
it may instead approach a phase-predicted touchdown ahead of the projected
center of mass. That target converges vertically on sampled terrain only as
contact approaches, so uneven ground cannot pin the foot during clearance.
Touchdown acquisition begins during the run's preceding swing, follows a
smooth clearance arc, and evaluates its frozen-start world trajectory directly
from 0.75 phase before contact. Target continuity and the shared nine-degree
world-foot orientation limit bound the 5.5 m/s reference run; the analytic leg
solve is not delayed by a second joint follower. The swing's world start is
frozen with the contact plan, and a bounded-slope deterministic Hermite path reaches
touchdown 0.15 phase before the profile's support-entry radius rather than
recursively easing from the current foot until the center of contact. When a
new plan begins during an unfinished release, its start is the prior visible
solve target transported by the current owner displacement rather than held
for one frame in world space or replaced by the newly restored authored FK
foot. If sampled
terrain makes the early plan vertically unreachable, the solve follows its
nearest reachable point without replacing the eventual touchdown. Before
freezing a run plan, its terrain-sampled XZ is projected into the shared reach
region of the predicted upper-leg root at support entry, center, and exit,
including the permitted 0.25 m pelvis reach allowance; reachable flat contacts remain
unchanged. The frozen
prediction survives the swing-to-support handoff instead of
being recomputed from a later body position. An unacquired plan remains frozen
through its nominal support lobe so a reach-limited ankle can finish the last
bounded samples into contact; it expires only if that lobe ends without contact
or an explicit reach failure invalidates it. Expiry clears the endpoint, visible
swing start, and start phase atomically so a replacement cannot inherit timing
from the failed plan. Once the rendered sole reaches contact,
high support solves directly to the frozen world plant. For walk and stop,
nominal phase requests the next step but the current acquired plant remains the
logical support until the opposite rendered sole has actually acquired its
replacement contact or an explicit reach check releases it. Run instead releases
at the signed outer edge of its raw post-contact support lobe and raises the sole
directly onto its 5 cm clearance floor, so its authored aerial interval remains
real while the rising landing shoulder can still acquire. That first Run release
sample transports the ankle by the controller's owner-local root displacement
instead of holding the old world plant, so root travel plus lift cannot become
one oversized visible step; walk and stop retain world-hold behavior. Toe-off marks the
remainder of that raw support lobe exhausted, preventing
the release-created next plan from re-entering support and losing its frozen
start metadata on the following sample; divergence of
the newly restored authored FK swing is not a plant-discontinuity signal. A reach-released foot marks that
support lobe exhausted until the raw gait enters true flight, so residual nominal
weight cannot reacquire or replan the same contact before the next step. The
unsuppressed raw cadence clears that latch in flight independently of reported
contact or retained plan state; the next rising raw support shoulder can then
lower a reachable frozen endpoint from the 5 cm flight floor and acquire it.
Running overlays a bounded sagittal foot-roll curve on the authored/slope-aligned
orientation: modest heel presentation approaches contact, the sole flattens
early in stance, and a modest toe-off releases back to neutral swing. The
shared angular limiter permits at most 9 degrees per 64 Hz sample, including
the first terrain-alignment frame; Run holds the prior foot orientation on the
first toe-off sample before returning to that shared bound.
Support diagnostics require the rendered sole to remain within 0.01 m of the
sampled terrain contact. This narrow allowance covers the measured residual
introduced by the complete analytic and scene-hierarchy solve; a foot outside
that same shared tolerance is always reported as unsupported. This truthful
post-propagation report is separate from solver transition ownership, so a
single rendered miss cannot make the next tick forget that it must release
from the preceding planted chain.
Both legs are supported at stable idle; action poses opt out of ordinary terrain
IK because they do not yet publish explicit foot-contact semantics. A footprint
is acquired only near full contact. At the edge of leg reach, shared
virtual-time rate-bounded pelvis
lowering absorbs the reach deficit first, and left and right targets remain on
separate pelvis-space tracks. The complete airborne Run target is limited in
owner-local 3D to 9 cm per sample only after terrain height and clearance are
applied. Raw-flight plans retain the 5 cm sole floor even after horizontal
progress reaches the endpoint, descending only once nominal support is eligible
and the frozen contact is within one bounded follower step. That same
follower remains active through nominal support until the propagated sole
actually acquires the frozen endpoint; only an acquired world plant bypasses
it. If controller travel plus the final 5 cm descent would exceed that budget,
the still-unacquired last footprint transports once with the owner's current
displacement, is re-sampled and reach-checked there, and freezes in world space
as soon as the sole acquires it. Uphill advancement projects XZ and clearance jointly so continuity never
trades for penetration. Upright motion keeps a 20-degree soft
extension reserve. Crouch and terrain swing keep at least 12 degrees of flexion
to conform without dragging a compact stance. A run also anticipates
its frozen planned contact during late approach, blending at most 0.25 m of
shared visual-rig-root drop so the pelvis and both thigh roots remain coherent
and support entry never collapses an already visible target under the hip. This
contact-coupled correction reinforces the existing phase
minima and retains the two-peak run-height contract. The remembered knee bend
is parallel-transported by the shortest hip-target direction rotation before
each length solve, avoiding a new analytic pole choice near full extension.
Both use a forward, slightly outward anatomical bend hemisphere. Arm and
leg swing share the same phase reconstruction before optional terrain IK; only
the legs receive that final terrain solve. An explicit hand or weapon
constraint can then override the reconstructed arm carriage.
Locomotion bounds root, pelvis, torso, neck, and head translations around bind
before look and final IK while preserving authored rotations. Authored visual
root/pelvis Y is normalized only
during active grounded locomotion so it cannot double-count procedural height;
stopping blends central bones back to the authored idle. XZ, rotations, and
authored limb silhouettes remain intact. At each authoritative contact edge, a
visual-only whole-rig translation calibrates the supported sole to the rig
floor and retains that baseline through the stride. This does not rotate or
solve either leg and does not move the gameplay controller. The measured 0.033 m hierarchy-rise
compensation applies only to upright, lowered-guard `humanoid_unarmed`
locomotion; crouching, guard movement, and specialized packs receive zero
compensation until measured independently. One visual-only profile evaluation
owns height: contacts at phase 0 and 0.5 are minima, grounded gaits use smooth
compression/recovery curves, and running uses a smooth sine-squared arc
across each full contact-to-contact half-step so the sole is already elevated
when shared support releases. Its 0.09 m raw run apex compensates for the
authored passing rise to display about 0.06 m; the 0.04 m walk,
0.03 m raised-guard, and 0.025 m crouch bounce profiles are continuously blended
across speed and state changes. The curve never changes authoritative owner Y,
grounded state, or posture. Guard's separate reach correction remains an
additive baseline concern rather than a gait wave.
When guard, crouch, grounded, or action state changes, a decaying visual offset
preserves the previously displayed height across the edge; the new phase curve
then resumes without resetting or delaying authoritative gait phase.

The authoritative projector derives world velocity/acceleration, alternating
contact identity, landing identity/impact, and the shared 64 Hz sample tick from
consecutive post-physics observations. The client transforms acceleration into
the current body frame and advances retained response only once per logical
sample: acceleration leans forward, braking leans back, lateral acceleration
rolls inward, and steady motion decays to neutral. Bounded skipped-tick gaps use
their authoritative tick duration; repeated renders of one tick do not advance.
A hard stop retains the effective authored locomotion pose, then releases it to
exact idle through a deterministic presentation crossfade instead of switching
sparse clips in one frame. The crossfade applies on both locomotion activation
and release, with separate start/stop speed thresholds to prevent threshold
chatter. If the projected center of mass is outside the current support region,
one foot remains the sole support while the other follows a short clearance arc
  to a bounded capture point beyond the projected center of mass. The capture
  seeds both chains from their prior post-propagation ankle snapshots rather
  than the newly restored idle FK or a potentially unreachable solve goal, then
  retains the selected support at that exact visible footprint while discarding
  any run contact plan left over from the preceding stride. The swing
  uses a 0.28-second nominal arc but does not settle into idle until the swing
sole reaches the frozen planned XZ within 2 cm and terrain contact within 1 cm;
  a bounded timeout may finish only from an already grounded, supportable stance.
  It never converts both feet into sliding
supports or strands the body behind the new contact.
If locomotion restarts before capture contact, the frozen stop target is
discarded immediately so ordinary phase-owned contact acquisition cannot be
  starved by a cancelled settle. The visible foot still uses the same bounded
  airborne release back to authored locomotion; it is not snapped to either the
  stale plant or new gait. A completed settle promotes both final solve targets
  to a stable dual-plant idle stance and clears only transient plans/releases;
  it neither freezes a raised sparse-run swing nor snaps the wide settled step
  under the body in one frame. New movement releases those plants through the
  ordinary bounded gait handoff.
A real airborne-to-grounded sequence triggers one 0.04-0.08 m, roughly 0.16
second landing compression. A landing-only analytic solve flexes the actual
hips/knees back to retained pre-compression world foot plants throughout recovery.
Its knee-flexion reserve eases toward the authored leg extension during the final
12 mm of compression release so the feet do not lift or snap on the last frame,
without translating thigh roots or enabling general terrain IK. The plants
resynchronize after tick/teleport discontinuities and clear on air, action, or
completion. At stable rest, ordinary support blends symmetrically back to both
feet. During a stop-settle capture, support stays exclusive to the selected
contact foot until the moving foot reaches its planned contact.

-->

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

The native `animation-viewer` is a deterministic gameplay-presentation fixture
for regression and visual review. It uses the gameplay player-spawn observer, character mesh,
camera, terrain presentation, authored animation evaluator, and procedural
passes rather than maintaining parallel fixture implementations. Locomotion is
projected and integrated continuously at the authoritative 64Hz fixed tick.
Non-terrain scenarios can still exercise authored leg motion with fixed
controller Y. The terrain suite enables seeded uneven-ground IK for cross-slope,
uphill, downhill, diagonal, crouched, mid-stride toggle, hard-stop, small
tap-stop, flight-phase run-stop, tap/restart crossfade, speed-threshold chatter,
steady 5.5 m/s run, raised-guard tap-stop, gradual 90-degree turn, and exact
180-degree reversal probes. A deterministic procedural clock prevents asynchronous
screenshot rendering from advancing retained IK state more than once for the
same logical tick, while the complete FK/IK pipeline still reevaluates each
view. The replay captures one raw gameplay-camera image plus side and front
diagnostic images of that exact pose. Its manifest records final world-space
bones, graph IK weights, continuity, signed foot tracks and separation, knee
flexion and bend hemisphere, desired body-forward alignment, bounded per-tick
turning residual (including look-facing guards), terrain-relative foot
clearance, and authored/solved foot targets,
phase-indexed height extrema and peak count, contact-phase sole clearance,
controller vertical range, run flight duration/sole clearance, authoritative acceleration, retained lean,
landing compression, contact identity, landing identity, and fixed tick; those
signals locate suspect frames but do not replace review of the rendered mesh.
For steady height scenarios, every complete cycle after warmup must contain
exactly two prominent peaks in the phase 0.25 and 0.75 passing windows. A
0.003 m prominence threshold filters sampling jitter while still rejecting an
extra visible beat.
The steady terrain run additionally requires alternating graph contact weights,
80-200 ms unsupported intervals, bounded contact clearance, and a 2 cm maximum
pelvis-height step. Ordinary feet are not tested as stationary world plants.
The flight-phase stop and tap/restart probes apply their +1 cm transient toe
floor only after locomotion begins or while stop-settle owns a foot. Their
zero-speed, no-settle pre-roll remains covered by the general -1 cm terrain
penetration tolerance and is not mislabeled as a Run flight sample.
Typed scenario metadata distinguishes ordinary, transition, terrain,
raised-guard, and landing gates. The suite includes a speed ramp, an
apex-adjacent hard stop, real forward-input camera/controller turns through 90
and 180 degrees, airborne landing, and a two-cycle cadence/contact fixture.
Every logical sample is evaluated repeatedly across the three review views;
the gate compares bones within 0.5 mm/0.05 degrees and requires unchanged
contact/landing sequences and event counts. The first fixed-tick evaluation
owns IK state advancement and the complete cached local pose; later views
restore that pose without re-entering or mutating support/release state.
Success also gates lean and phase
continuity, hard-stop pelvis continuity from the moving-to-zero edge through
settling, two ordered contacts per cycle and
shared step distance, event order/count/deduplication, contact soles from
-0.02 m to 0.04 m, run flight soles from 0.05 m to 0.20 m, landing knee flex, foot
preservation within 1 cm, and landing penetration no lower than -1 cm.
The fixture supplies deterministic controller observations at the shared
server projection boundary and follows rendered terrain height only in the
cross-slope probe. Its replication-presentation probe withholds three of every
four projected skeleton samples while accelerating and turning, so render-side
phase prediction and resynchronization are exercised. It still does not run
physics contacts, the network transport itself, or recorded live input.

The vertical-excursion gate remains 0.20 m for ordinary flat-ground motion and
0.30 m for raised-guard scenarios. Each explicit terrain scenario adds only
the terrain relief measured beneath its sampled feet to the ordinary 0.20 m
envelope; this separates required pelvis reach correction from authored body
bob without weakening the flat-ground check. Cumulative planted-foot drift and
per-frame supported slip are gated only where world-space procedural plants
exist: raised guard and attack footwork. Raised-guard scenarios
require no more than 0.01 m cumulative support drift and at least 0.16 m
inter-foot separation. Ordinary terrain locomotion is instead gated by pose
continuity, penetration, reach, knee flexion, and slope alignment. Explicit
5.5 m/s Run segments use a 0.15 m foot budget for complete authored cycle
motion in addition to 0.086 m of owner travel per 64 Hz sample, while the knee
uses a 0.13 m budget. The exact authored flight pose measures 0.143 m of foot
motion and 0.125 m of knee motion per sample; lower-speed non-run probes retain
the 0.055 m foot and 0.10 m knee budgets. Strict terrain-Run probes permit a
0.16 m knee step for slope-aligned contact acquisition (measured at 0.152 m).
Terrain-run slope alignment permits the direct graph-weighted contact rotation
without adding a temporal foot-orientation cache.
The analytic knee-flexion reserve and bend-hemisphere gates use that same
procedural scope; they are not asserted against authored FK-only motion.
Ordinary FK-only motion is reviewed through continuity, clearance, phase-height,
and visual gates rather than being mislabeled as world-planted. Ordinary terrain
probes require unloaded swing feet to remain free of terrain correction and
near-full graph contact weights to converge on terrain. Stops are graph
crossfades and do not require a capture-point or planned-footprint diagnostic.
Start/stop, guard-entry, and crouch-state transitions permit at most 0.04 m of
pelvis-height change per 64 Hz sample; the pre-existing guard entry itself uses
about 0.033 m of that budget.
Loop-seam gates apply only to repeatable cycles. The complete authored Run
cycle permits a 0.03 m sampled positional seam (measured at 0.029 m); other
repeatable cycles retain the 0.015 m gate. Start/stop, facing-turn, and
raised-guard release-at-peak scenarios are transition probes whose final
simulation state intentionally differs from the state that initiated them;
their continuity remains covered by the per-frame displacement and rotation
gates instead.

During ordinary lowered-guard travel the server advances the replicated body's authored +Z
axis toward authoritative horizontal velocity at a bounded turn rate that can
complete a 180-degree reversal in 0.25 seconds. Camera
pitch is removed before planar gait projection. Camera yaw intentionally maps
raw movement input into world movement, but it is not applied again by either
the client root or authored-rig child. At idle, the last body yaw is retained;
an exact reversal uses a deterministic turn side. Raised guard, attack, and
block retain controller-yaw look facing while moving. This root is
shared by local players, remote players, bots, fallback bodies, authored rigs,
and the viewer, with the authored +Z/controller -Z half-turn represented once.
The viewer additionally replays gradual turns, an exact reversal, planted
guard rotation, camera pitch, cross-slope terrain, every raised cardinal and
diagonal direction, release during a step, and a mid-step lateral
reversal.

During lowered travel, forward walk and run continue to serve diagonal and
lateral travel. Ordinary raised upright grounded movement freezes the current
lead and samples only its static guard pose. A client-only procedural lower-body pass
alternates one swing foot with exactly one world-space support foot. Each
compact step projects authoritative local velocity from the step origin,
retains the authored guard's separated stance tracks, interpolates horizontally
with a smooth curve, and adds a sine clearance arc. Step reach scales with
analogue speed and is bounded to combat-shuffle distances. Raised swings use a
high continuity ceiling rather than the old low IK velocity limit: the ceiling
is above the measured worst ordinary 2 m/s guard step, so replacing the support
foot can still meet its semantic contact deadline. Unusually long recovery
steps remain bounded and converge over subsequent procedural steps instead of
snapping in one frame.

Cadence follows current authoritative speed throughout the first step, so a
small acceleration sample cannot slow a complete cycle. Ordinary turns are
accepted at the next foot handoff. A material opposite-direction reversal
performs an immediate safe semantic handoff; releasing movement finishes only
the active half-step rather than freezing a foot in the air or completing an
entire two-step pulse. The guard lead never doubles as swing-side state and
remains fixed across forward, backward, lateral, and diagonal motion. Authored
guard walk and strafe files remain available for comparison but are not sampled
by continuous raised locomotion.

Raised sprint input is the sole exception. Its gameplay speed remains the
character's endurance-neutral jog, but presentation layers the static guard on
the upper body over the ordinary walk/run interpolation on the lower body. The
animation graph masks the locomotion clip off `stomach_01` and every descendant
upper-body target while masking the guard clip off `root`, `pelvis`, and every
leg target. Ordinary locomotion terrain IK owns this composite's legs;
procedural combat stepping owns every non-sprint raised movement.

Semantic intent carries a wrapping step sequence and a swing side separate
from guard lead. The sequence increments at every handoff, allowing a client
that receives coalesced updates to reset safely even when normalized phase
returns to the same parity after a skipped full cycle. World-space targets
remain client-only.

Procedural guard plants and targets stay entirely client-side. Replicated
`SkeletonState` carries a tagged planted/moving intent whose moving payload has
semantic direction, speed, swing side, and step identity; it never
carries bones or world foot positions. Flat-ground placement works with
terrain IK disabled through `F8`. Raised planning and terrain conformity intentionally
share one ordered solver pass so pole, plant, and pelvis memory are sampled
once per frame. When terrain conformity is enabled, the same
targets additionally follow height and slope without replacing their planted
XZ positions. Raised grounded idle uses the static guard. Raised crouched and airborne
characters retain the existing crouch and airborne posture rules; specialized
raised variants can be added later.

### Combat guards

Every complete combat pack provides, or inherits:

| Pose | Animator brief |
|---|---|
| `guard_lead_left` | Place the left foot forward and the right foot back on two stable tracks rather than one tightrope line. Distribute weight so either foot can move without a preliminary shuffle; neither knee is locked. Turn the pelvis and torso only as required by the pack's fighting method. In the unarmed root pack, raise both hands to protect the head and torso with the dominant hand free to punch. In a weapon pack, use its normal ready grip and keep the point or striking portion controlled. |
| `guard_lead_right` | Construct the corresponding stable guard with the right foot forward and left foot back. When this pose is authored, preserve the same handedness and held-item hand as `guard_lead_left`; this is a change of foot lead, not a reflection. Match stance width, guard height, and overall readiness closely enough that attacks can end in either guard without a visible change of style. |

These are stable endpoints for attacks and blocks. Which foot is forward is an
explicit part of skeleton state. If only one guard exists in a pack, the other
lead mirrors it. Complete one-handed guards should therefore author both lead
feet whenever whole-body mirroring would change the weapon hand incorrectly.

### Crouching and directional ducking

Directional ducking begins from the active guard rather than a neutral crouch.
Each lead has backward, anatomical-left, and anatomical-right semantics. A
symmetrical pack may author the three extremes for one lead; the opposite lead
then comes from the mirrored counterpart. Both lateral extremes for a single
lead remain distinct and cannot be constructed by mirroring each other,
because that would also exchange the lead feet. Keep the guard's planted foot
locations and normal hand carriage.

| Pose | Animator brief |
|---|---|
| `duck_lead_<left\|right>_backward` | From the named guard lead, withdraw the head and upper torso backward while sitting the pelvis down and back between the feet. Increase knee flexion to preserve balance and avoid creating the motion solely by arching the lower back. Keep the gaze generally forward and do not lift both toes or heels. |
| `duck_lead_<left\|right>_left` | From the named guard lead, shift the pelvis, ribcage, and especially the head toward anatomical left. Load the left leg more heavily, flex it, and allow the right leg to lengthen without moving either foot. Incline or rotate the torso only enough to keep balance and protect the head. Do not cross the legs. |
| `duck_lead_<left\|right>_right` | From the named guard lead, make the corresponding anatomical-right displacement while retaining that same lead. This is a separately authored extreme when the pack supplies both lateral directions for the lead; do not derive it by reflecting the other lateral pose without also changing lead. |

The same-pack counterpart rule mirrors an entire lead/direction pair only when
the requested counterpart file is absent. Forward/downward ducking remains
procedural: it applies a bounded forward head, ribcage, and pelvis displacement
with planted-foot IK. Diagonal ducks blend these components. Direction still
describes the defender's desired body or head displacement in local space, not
merely the attacker's bearing.

Directional ducks should preferably be authored as pose deltas or masked
overrides over their named guard. Packs need only omit counterparts that remain
valid under whole-body reflection; asymmetric weapons and techniques should
export the exact opposite-lead files.

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

An attack recipe declares:

- its semantic strike family: `thrust` or `slash`;
- its starting and ending lead foot;
- whether its footwork is `stay` or `switch`;
- its target-height behavior;
- its contact, continuation, recovery, and cancellation timing; and
- its hand/weapon constraints.

A stay attack keeps the same guard orientation. When it begins while moving
with the feet close together, the rear foot plants and the forward foot takes
one step without passing it. When stationary, stay plants both feet. A switch
attack plants the foot that is forward along the captured movement direction,
steps the rear foot past it, and ends in the opposite guard.

For melee attacks, the tactical authority snapshots the controller-local
physical velocity when the attack starts. Attack steps follow that captured
movement direction, including lateral and backward movement. Later input,
velocity reversal, or camera yaw cannot repick this semantic.
Ranged attacks remain `stay`. For immediate contact synchronization the owning
client predicts the same typed choice from the last physical velocity already
stored in its replicated `SkeletonState`; it does not send or trust a fresh
movement-input vector. The server observation remains authoritative and the
presentation reconciles to it.

At attack start, the visible feet are projected onto the guard's fore-aft axis.
They count as close when their separation is no greater than half the authored
guard separation (equivalently, a foot has crossed halfway from its guard
position toward the hips). A moving close stance selects `stay`; a moving
separated stance selects `switch`. A stationary thrust selects a planted
`stay`, while every slash selects a step and therefore uses a forward `switch`
when stationary. Maximum extension and the moving foot's ground contact
coincide with the server-owned contact tick.
The procedural targets are client-only world-space data: they do not translate
the controller, extend hit range, or enter persistent state. The attack begins
immediately. If its selected moving foot is airborne, presentation first lowers
that foot vertically and temporarily ignores the authored lateral foot target;
the remaining preparation samples compress the interpolation so contact still
arrives on the authoritative hit tick. The other foot's ball remains an exact
world plant through the strike while its ankle may rotate or pivot around it.

After the action, both feet are compared with the terrain-conformed destination
guard at the final owner position and facing. Error inside the small recovery
threshold is accepted. Otherwise the farther foot takes a bounded procedural
step; recovery may alternate through additional steps so neither foot is
starved while a moving root advances the goal. Once each necessary correction
has landed, raised locomotion retains those exact plants until the next
replicated step sequence instead of replaying an already-consumed gait phase.
The server carries the captured direction and speed through the complete
switching action. Releasing or reversing movement cannot stop the controller
underneath the attack step; current movement input resumes only after the end
guard commits. Multiple attack-timed steps and an explicit attack-turning
policy are intentionally deferred; this iteration always times one attack step
to contact. Capture telemetry
records requested and reach-constrained targets separately so any analytic
reach yield is measured rather than misreported as a perfect plant.

The equipped weapon declares a preferred family and separate swing and stab
precision terms. Normal attack input selects the preferred family; alternate
input selects the other family. Unarmed fists prefer thrusts. These semantic
families travel with the attack request and select both the contact pose and
the combat precision term.

Each strike family has one contact pose for each starting lead. Names use
`attack_<family>_lead_<left|right>_contact`; for example,
`attack_thrust_lead_left_contact`. `stay` and `switch` remain gameplay and
footwork parameters, not separate authored motions.

At contact, pose the instant the fist, claw, point, or edge crosses a canonical
target plane at approximately upper-torso height. Align the striking structure
from the feet through hips and torso to the striking limb without locking an
elbow or knee. Begin with the feet in the named starting guard and make the
whole body mechanically coherent, even though the evaluator will combine the
upper-body and torso action with its separately evaluated footwork.

The strike-family construction rules are:

| Family | Animator brief |
|---|---|
| `thrust` | Move the primary striking point generally forward along a direct line. For the unarmed root this is a punch: the primary fist reaches the target while the other hand protects, the shoulder does not rise into the neck, and the fist, wrist, and forearm align at contact. For a weapon, the point and grip align with the thrust while the off hand follows the weapon's normal use. Do not add an anticipatory chamber beyond what already exists in the guard. |
| `slash` | Move the primary striking edge, claw, or hand from the pack's primary side across the target line toward the opposite side. For the unarmed root this is a swipe, particularly suitable for claws. At contact, preserve edge or claw alignment and support the motion with coordinated pelvis and ribcage rotation rather than an isolated arm swing. Arrange the body so that continuing along the same line briefly after contact remains anatomically safe. A pack that later supports the reverse slash direction should define it as a distinct recipe. |

For `lead_left`, the contact is constructed from `guard_lead_left`; for
`lead_right`, it is constructed from `guard_lead_right`. The runtime preserves
those planted targets for a stay attack. The switch-step direction and moving
foot follow the authoritative rules above, and the moving foot reaches maximum
extension at contact before both legs recover toward the opposite guard.

The full visual sequence is:

```text
start guard -> immediate acceleration -> contact -> bounded continuation -> end guard
```

The guard is already positioned to attack, so there is no required commit or
wind-up pose. The evaluator accelerates away from the guard immediately and
times contact to the server-owned attack event. After contact it estimates the
selected bones' incoming linear and angular velocities, continues them for a
short recipe-defined interval, clamps weapon-tip travel and joint rotation,
then uses a critically damped recovery toward the ending guard. A thrust uses
little continuation and usually retracts promptly; a slash continues farther
along its striking line. Network timing differences are absorbed by adjusting
early acceleration and recovery, not by adding a visible telegraph.

Unusual attacks whose path cannot be represented safely by bounded
continuation, such as a full spin or a weapon passing around the body, may add
an optional authored recovery anchor. It is not part of the complete-pack
contract. The contact marker synchronizes presentation with gameplay but does
not itself decide whether anything was hit.

Each strike family therefore requires two poses, one for each starting lead.
The complete unarmed root defines two thrust/punch contacts and two slash/swipe
contacts so that every descendant can resolve either semantic strike. Slash
direction may initially be the biomechanically appropriate direction for the
starting stance. Supporting both slash directions independently from either
lead foot would add more recipes, but is not required by gameplay yet.

Overgrowth likewise stores attack height, direction, stance swapping,
mobility, reactions, and animation paths as data rather than deriving them
from filenames; see
[`attacks.h`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.h#L52-L78)
and
[`attacks.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.cpp#L74-L145).
Overgrowth samples four authored keyframes with cubic interpolation while
clamping the interpolation coordinate to the authored interval; our bounded
post-contact continuation is therefore an intentional extension of the
inspiration rather than a claim that Overgrowth extrapolates indefinitely.
See
[`animation.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/animation.cpp#L1285-L1316).

### Blocking

The initial design uses two lead-foot states and three incoming attack lines:

1. cut arriving from defender-left;
2. cut arriving from defender-right; and
3. thrust.

This produces six block-contact poses. Each starts from the named guard, keeps
both feet in that guard's floor positions, and depicts a canonical contact at
approximately upper-torso height:

| Pose pattern | Animator brief |
|---|---|
| `block_cut_left_lead_<foot>` | Interpose the weapon, shield, forearms, or other blocking structure on the defender's anatomical left against a cut arriving from that side. Keep elbows slightly flexed, shoulders connected to the torso, the face protected behind the structure, and the body capable of accepting force through the legs. Do not reach so far left that the right side and centerline are abandoned. |
| `block_cut_right_lead_<foot>` | Make the corresponding structurally supported interposition on the defender's anatomical right. Keep the head behind protection, avoid crossing or locking the arms unless the pack's weapon method requires it, and retain stable foot contacts. |
| `block_thrust_lead_<foot>` | Meet or deflect a forward thrust near the centerline using the pack's weapon, shield, or unarmed parry structure. Move the line just far enough off the torso rather than posing a strength contest directly against the point. Keep the face protected, elbows flexed, and the stance able to redirect force. |

Instantiate each pattern once with `foot` equal to `left` and once with it
equal to `right`, preserving the corresponding guard's handedness and foot
lead. These lead variants are required rather than procedural lower-body
overlays: shield position, torso presentation, reachable blocking structure,
and the path by which force reaches the stance can change substantially with
the forward foot.

The evaluator interpolates from the current guard into the appropriate contact
pose, procedurally adjusts its height toward the predicted contact point, and
adds an impact or flinch layer afterward. Without procedural height adjustment,
head, torso, and leg variants would increase this set from six to eighteen.

### Prone and supine

The initial complete set contains:

| Pose | Animator brief |
|---|---|
| `prone_idle` | Lie face-down with the chest and pelvis close to the floor. Support the upper body lightly on the forearms or hands so the head can look forward without an extreme neck extension. Keep the legs extended or modestly bent and separated enough for stability. Mark the torso/pelvis and supporting forearms as floor contacts as appropriate. Do not trap held equipment beneath the chest when a neutral alternative exists. |
| `supine_idle` | Lie on the back with the head and shoulders slightly raised enough to see forward. Flex the knees enough to keep the feet available for movement, with one or both soles planted, and keep the arms in a protective usable position rather than flat in a rigid anatomical display pose. |
| `prone_crawl_contact` | Show a contralateral crawling support: left forearm/hand reaches or plants forward while the right knee/inside leg advances, with the right arm and left leg contributing rearward support. Keep hips and chest low and mark the current supporting surfaces. This is the maximum useful extension of the crawl, not a long military split. |
| `prone_crawl_passing` | Bring the advancing right knee and left arm back beneath the body as the torso passes over the support polygon. Limbs are compact and changing roles; avoid a moment where the entire body appears unsupported. This is the neutral midpoint between mirrored crawl contacts. |
| `supine_scamper_contact` | On the back, plant the left heel/foot and the opposite hand or forearm as the canonical extended support, with the pelvis slightly lifted or unloaded enough to move. The right leg is advancing toward its next plant. Protect the head and keep the neck from bearing body weight. |
| `supine_scamper_passing` | Bring the advancing foot under the knee and move the torso through the support provided by heels and arms. Keep the pelvis near the floor but visibly mobile, and preserve a guarded upper body. This is the midpoint used between mirrored scamper contacts. |
| `upright_prone_transition` | Pose the stable intermediate between standing/crouching and prone: both hands or forearms and at least one knee contact the floor, the head remains protected and able to look forward, and the pelvis is low enough to continue down without dropping the chest through the ground. It must also work in reverse as the main get-up intermediate. |
| `dive_impact` | Pose first controlled contact after a forward dive. Use forearms and/or hands to absorb impact with bent elbows, turn or raise the head clear of the floor, keep the chest just above contact, and let knees/hips flex behind the body. Do not land on locked wrists, straight elbows, the face, or the weapon. |

Backward crawling may initially reverse the forward cycle, and getting up may
reverse the upright-to-prone transition. The planned controls do not include
prone strafing or a deliberate prone-to-supine roll, so neither has an authored
pose. Supine may still result from a hit or physical fall; recovery from it is
an automatic get-up or ragdoll transition rather than a player-controlled roll.

## Initial complete-pack size

The humanoid unarmed root must satisfy every required semantic pose because it
has no parent pack. Exact files and the presence-based mirrored counterparts
both satisfy that requirement. The contract contains 34 resolvable semantics;
six mirrored pairs allow the root to satisfy it with 28 authored files. Its
tentative authored size is:

| Family | Authored poses |
|---|---:|
| Standing and locomotion | 6 |
| Directional ducking | 3 |
| Jumping and dodging | 2 |
| Prone and supine | 8 |
| Combat guards | 1 |
| Thrust/punch attacks | 1 |
| Slash/swipe attacks | 1 |
| Blocks | 6 |
| **Complete unarmed root** | **28** |

Most specialized packs should be substantially smaller because they inherit
unchanged poses. A symmetric pack that overrides one strike family needs one
attack pose; an asymmetric pack exports the opposite lead as well. The unarmed
root supplies punch
poses for unresolved thrust semantics and swipe poses for unresolved slash
semantics.

## Secondary animation

The native `ragdoll-viewer` now provides a deliberately isolated passive
ragdoll for the existing Cascadeur humanoid. It maps a conservative set of
major bones through reusable `bevy_animation_graph` ragdoll definitions and
runs a complete Avian solver without changing the live client's query-only
physics setup. Twist bones, toes, clavicles, neck intermediates, and weapon
sockets remain excluded from the rigid-body topology. The ragdoll owns only
rendered bone transforms while active; it never moves the replicated player
root, gameplay collider, hitboxes, or persistent strategic state.

Clients may also use joint motors or other procedural dynamics to give bones
inertia and react to movement, collisions, weapons, clothing, and equipment.
Overgrowth can mix authored animation with active-ragdoll physics;
its animation output carries per-bone physics weights through
[`animation.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/animation.cpp#L1269-L1425)
and `RiggedObject` applies them to joint strength.

The focused fixture is an engineering/review capability rather than an MVP
combat or death mechanic. Integrating ragdoll state with authoritative combat,
network replication, recovery, or get-up behavior remains future work. The
base evaluator preserves a clean final-pose stage so those decisions do not
change authored pack semantics.

The active fixture mode uses the fork's validated Avian adapter to drive the
existing revolute knee and elbow joints. Strength ramps at the fixed physics
rate; switching to passive ramps toward an explicit zero-torque, disabled
motor rather than retaining an implicit solver default. Target, velocity,
frequency, damping, and torque inputs are finite-checked and clamped by the
dependency. This is hinge-only: Avian's spherical joints have limits but no
corresponding angular motor API, so the hips, shoulders, spine, and neck are
not claimed as actively driven.

`just ragdoll-capture` advances animated, active, and passive modes for exact
fixed-solver tick counts independent of render cadence, then records screenshots plus
bounded telemetry in `manifest.json`. It gates finite metrics, driven hinge
count, active error convergence, and passive zero strength, writing
`failure.txt` on failure. The fixture terrain is also a real static physics
collider restricted to the ragdoll layer, and capture rejects active or passive
poses whose pelvis falls through the terrain or remains in high-speed motion.
Settling is evaluated across the final half-second of fixed physics samples,
not from one potentially misleading instant at the end of a bounce.
These numeric gates catch wiring, collision, and solver regressions; the images
remain the authority for presentation quality.

## Stylistic principles

Animations should remain realistic in accordance with the
[meta-level heuristics](../meta.md). Melee attacks should generally be inspired
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
