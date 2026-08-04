# Animation

The tactical animation system turns authoritative character state into a
convincing skeletal pose. Its authored assets are deliberately sparse:
animators provide important poses and contact points, while the runtime blends
between them, places feet, adjusts limbs with inverse kinematics (IK), and adds
secondary motion.

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
cooperating dimensions:

- **Posture:** upright, crouched, airborne, prone, supine, or ragdolled.
- **Locomotion:** local velocity, speed, grounded state, gait phase, and the
  currently leading or planted foot.
- **Stance:** facing, handedness, selected animation pack, aim state, and which
  foot is forward.
- **Action:** none, jump charge, jump, dodge, attack, block, hit reaction,
  prone transition, or get-up.
- **Action parameters:** direction, attack line, stay/switch footwork, phase,
  target height, and authoritative start/contact times where applicable.

This state is synchronized over the network. A client does not need to know
whether another character is an NPC or player; it needs only the replicated
physical state and presentation intent.

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

A discrete raised/lowered change is presentation-crossfaded from the currently
displayed effective pose over 0.18 seconds. This includes resolved fallback
clips and their lower- or whole-body mirror contribution, so an incomplete
guard asset set does not hard-cut from locomotion to a relaxed fallback. The
crossfade clock advances once per simulation sample in deterministic capture
tools (and by render delta in gameplay); changing direction or gait phase does
not restart it. Reversing guard during the blend starts from the pose already
on screen rather than either original endpoint.

The server owns movement, posture and action acceptance, gameplay position,
attack timing, hits, damage, and other outcomes. A client may begin an
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
6. Apply foot placement, hip correction, terrain IK, hand/weapon constraints,
   and head/torso look. Body facing is already present on the replicated root.
7. Apply optional secondary animation.

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
- airborne center/travel blend from horizontal intent (vertical velocity remains telemetry);
- attack target height;
- action phase; and
- layer weights for head look, impact reaction, and future secondary motion.

Animations blended along a shared coordinate must be phase-compatible. For
example, the contact frame of a walk must have the same gait phase as the
contact frame of a run and the crouched recipe, which reuses the ordinary gait
anchors under `crouch_idle`. Overgrowth's synchronized animation
groups also adjust playback frequency from actual ground speed, which is the
behavior we want to emulate rather than allowing feet to slide.

Hit reaction has no semantic authored pose. A client-only bounded chest,
neck, and head pulse is keyed to the replicated 64Hz locomotion sample identity;
re-evaluating one tick for multiple renders cannot advance or duplicate it.
Directional and body-region reactions remain future work.

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

Prepare each motion by validating and copying its source export exactly:

```powershell
python scripts/prepare_animation_motion.py assets_src/biped/unarmed/walk.glb assets/animations/biped/unarmed/base.glb assets/animations/biped/unarmed/walk.glb --last-frame 32
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
| `walk` | 0 `walk_contact`; 8 `walk_passing`; 16 opposite contact; 24 opposite passing; 32 loop closure |
| `run` | 0 `run_contact`; 5 `run_flight`; 10 opposite contact; 15 opposite flight; 20 loop closure |
| `prone_crawl` | 0 `prone_crawl_contact`; 8 `prone_crawl_passing`; 16 opposite contact; 24 opposite passing; 32 loop closure |
| `supine_scamper` | 0 `supine_scamper_contact`; 8 `supine_scamper_passing`; 16 opposite contact; 24 opposite passing; 32 loop closure |

Raised-guard locomotion has no authored movement semantics. It keeps the exact
static `guard_lead_<side>` upper-body pose while the client-only footwork
planner moves and plants the legs. Step targets, support handoff, pelvis
response, and speed-scaled cadence are typed code-owned behavior rather than
catalog motions.

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
between `airborne_center` and `airborne_travel` from horizontal travel intent;
there are no separate authored launch, direction, or landing samples.

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
half-cycle. When a sparse gait source contains only that half, the runtime
reflects the complete bilateral limb motion—including clavicles, arms, hands,
legs, feet, and twist bones—to construct the opposite half and closure.
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
applies its existing crouch multiplier and acceleration/deceleration. Gait phase 0 through 1 is
one complete left-right cycle rather than one step. Shared typed walk, run,
crouch, and raised-guard profiles own reference speed, step distance, support,
flight, bounce, and compression metadata used by both authoritative projection
and client presentation. This keeps cadence tied to actual post-physics ground
distance without duplicated timing formulas or double-speed footfalls.

Contact and passing/flight anchors are authoritative sparse gait inputs. The
evaluator constructs four smooth quarters: contact to passing, passing to the
character-space mirrored contact, mirrored contact to mirrored passing, and
mirrored passing back to contact. It does not traverse later exported gait
timeline data. Character-space reflection retains anatomical lateral spacing
rather than swapping bones discretely. Support narrows through the walk/run
blend and releases both feet for roughly 90-110 ms at each 5.5 m/s run flight
beat; the visual gait keeps at least 0.10 m of sole clearance during that flight.
With terrain IK explicitly enabled, high support locks the stance foot
horizontally in world space until release. A footprint is acquired only near
full contact. At the edge of leg reach, shared virtual-time rate-bounded pelvis
lowering absorbs the reach deficit first, and left and right targets remain on
separate pelvis-space tracks. The solver keeps a 20-degree soft extension
reserve and a forward, slightly outward anatomical bend hemisphere. Arm and
leg swing share the same phase reconstruction before optional terrain IK; only
the legs receive that final terrain solve. An explicit hand or weapon
constraint can then override the reconstructed arm carriage.
Locomotion bounds root, pelvis, torso, neck, and head excursions around bind
before look and final IK. Authored visual root/pelvis Y is normalized only
during active grounded locomotion so it cannot double-count procedural height;
stopping blends central bones back to the authored idle. XZ, rotations, and
authored limb silhouettes remain intact. The measured 0.033 m hierarchy-rise
compensation applies only to upright, lowered-guard `humanoid_unarmed`
locomotion; crouching, guard movement, and specialized packs receive zero
compensation until measured independently. One visual-only profile evaluation
owns height: contacts at phase 0 and 0.5 are minima, grounded gaits use smooth
compression/recovery curves, and running uses a gravity-shaped parabolic arc
across each full contact-to-contact half-step so the sole is already elevated
when shared support releases. Its 0.16 m run apex and 0.04 m walk,
0.03 m raised-guard, and 0.025 m crouch bounce are continuously blended
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
exact idle through the same deterministic 0.18-second presentation crossfade
used for guard edges instead of switching sparse clips in one frame.
A real airborne-to-grounded sequence triggers one 0.04-0.08 m, roughly 0.16
second landing compression. A landing-only analytic solve flexes the actual
hips/knees back to retained pre-compression world foot plants throughout recovery.
Its knee-flexion reserve eases toward the authored leg extension during the final
12 mm of compression release so the feet do not lift or snap on the last frame,
without translating thigh roots or enabling general terrain IK. The plants
resynchronize after tick/teleport discontinuities and clear on air, action, or
completion. At rest and while stopping, ordinary
support blends symmetrically back to full support on both feet.

Monotonic contact and landing sequences drive deduplicated client presentation
messages for future audio/VFX only. Up to eight plausible missed contacts are
reconstructed in alternating order. A backward/reset or larger delta silently
resynchronizes instead of producing a phantom burst; missed landing changes
collapse to one latest observation. `sample_tick` is the observation tick of
the replicated sequence state, not a reconstructed historical event time.

Terrain conformity defaults off while its uneven-ground behavior is being
refined. Debug clients expose `F8` as an explicit runtime opt-in. Disabling it
leaves authored FK, gait mirroring, torso stabilization, and procedural combat
foot placement intact. Ordinary walk, run, and crouch keep their authored leg
motion without the analytic terrain solve. Enabling it adds terrain height and
normal sampling, world-space plants, and terrain-derived pelvis conformity.
Debug clients also expose `F7` to toggle the connected local tactical mission
between normal and quarter-speed game time. Both client presentation and the
authoritative server clock change together, so movement, physics, combat, and
animation remain synchronized during slow-motion inspection.

The native `animation-viewer` is a deterministic gameplay-presentation fixture
for regression and visual review. It uses the gameplay player-spawn observer, character mesh,
camera, terrain presentation, authored animation evaluator, and procedural
passes rather than maintaining parallel fixture implementations. Locomotion is
projected and integrated continuously at the authoritative 64Hz fixed tick.
Most scenarios exercise default-off authored leg motion with fixed controller
Y; the explicit cross-slope probe enables seeded uneven terrain IK. A deterministic procedural clock prevents asynchronous
screenshot rendering from advancing retained IK state more than once for the
same logical tick, while the complete FK/IK pipeline still reevaluates each
view. The replay captures one raw gameplay-camera image plus side and front
diagnostic images of that exact pose. Its manifest records final world-space bones, support weights,
continuity, planted-foot drift under a stable body, signed foot tracks and separation, knee flexion
and bend hemisphere, desired body-forward alignment, bounded per-tick turning
residual (including look-facing guards), terrain-relative foot clearance,
phase-indexed height extrema and peak count, controller vertical range, run
flight duration/sole clearance, authoritative acceleration, retained lean,
landing compression, contact identity, landing identity, and fixed tick; those
signals locate suspect frames but do not replace review of the rendered mesh.
For steady height scenarios, every complete cycle after warmup must contain
exactly two prominent peaks in the phase 0.25 and 0.75 passing windows. A
0.003 m prominence threshold filters sampling jitter while still rejecting an
extra visible beat.
Typed scenario metadata distinguishes ordinary, transition, terrain,
raised-guard, and landing gates. The suite includes a speed ramp, an
apex-adjacent hard stop, real forward-input camera/controller turns through 90
and 180 degrees, airborne landing, and a two-cycle cadence/contact fixture.
Every logical sample is evaluated repeatedly across the three review views;
the gate compares bones within 0.5 mm/0.1 degrees and requires unchanged
contact/landing sequences and event counts. Success also gates lean and phase
continuity, hard-stop pelvis continuity from the moving-to-zero edge through
settling, two ordered contacts per cycle and
shared step distance, event order/count/deduplication, landing knee flex, foot
preservation within 1 cm, and landing penetration no lower than -1 cm.
The fixture supplies deterministic controller observations at the shared
server projection boundary and follows rendered terrain height only in the
cross-slope probe; it does not
exercise physics contacts, replication, interpolation, or recorded live input.

The vertical-excursion gate remains 0.20 m for ordinary flat-ground motion and
0.30 m for raised-guard scenarios. The explicit cross-slope scenario adds only
the terrain relief measured beneath its sampled feet to the ordinary 0.20 m
envelope; this separates required pelvis reach correction from authored body
bob without weakening the flat-ground check. Cumulative planted-foot drift and
per-frame supported slip are gated only where procedural plants exist: raised
guard and the explicit cross-slope terrain probe. Raised-guard scenarios
require no more than 0.01 m cumulative support drift and at least 0.16 m
inter-foot separation; cross-slope terrain IK retains the 0.035 m drift bound.
The analytic knee-flexion reserve and bend-hemisphere gates use that same
procedural scope; they are not asserted against default-off authored FK.
Ordinary default-off FK is reviewed through continuity, clearance, phase-height,
and visual gates rather than being mislabeled as world-planted.
Start/stop, guard-entry, and crouch-state transitions permit at most 0.04 m of
pelvis-height change per 64 Hz sample; the pre-existing guard entry itself uses
about 0.033 m of that budget.
Loop-seam gates apply only to repeatable cycles. Start/stop, facing-turn, and
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
lateral travel. Raised upright grounded movement freezes the current lead and
samples only its static guard pose. A client-only procedural lower-body pass
alternates one swing foot with exactly one world-space support foot. Each
compact step projects authoritative local velocity from the step origin,
retains the authored guard's separated stance tracks, interpolates horizontally
with a smooth curve, and adds a sine clearance arc. Step reach scales with
analogue speed and is bounded to combat-shuffle distances.

Cadence follows current authoritative speed throughout the first step, so a
small acceleration sample cannot slow a complete cycle. Ordinary turns are
accepted at the next foot handoff. A material opposite-direction reversal
performs an immediate safe semantic handoff; releasing movement finishes only
the active half-step rather than freezing a foot in the air or completing an
entire two-step pulse. The guard lead never doubles as swing-side state and
remains fixed across forward, backward, lateral, and diagonal motion. Authored
guard walk and strafe files remain available for comparison but are not sampled
by continuous raised locomotion.

Semantic intent carries a wrapping step sequence and a swing side separate
from guard lead. The sequence increments at every handoff, allowing a client
that receives coalesced updates to reset safely even when normalized phase
returns to the same parity after a skipped full cycle. World-space targets
remain client-only.

Procedural guard plants and targets stay entirely client-side. Replicated
`SkeletonState` carries only semantic direction, speed, phase, swing side, and
step identity; it never
carries bones or world foot positions. Flat-ground placement works with
terrain IK disabled. When terrain conformity is explicitly enabled, the same
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
the requested counterpart file is absent. Forward/downward dodge currently
uses the existing `crouch_idle` recipe basis rather than inventing another
authored semantic; richer procedural displacement remains future work. Direction still
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

A stay attack ends with the same foot forward. A switch attack takes one step
and ends with the opposite foot forward.

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
`lead_right`, it is constructed from `guard_lead_right`. Stay returns to that
guard; switch selects the opposite guard as the recovery endpoint.

The implemented visual sequence is:

```text
start guard -> contact at authoritative phase 0.5 -> selected stay/switch end guard
```

The guard is already positioned to attack, so there is no authored commit,
wind-up, follow-through, stay, or switch pose. The typed evaluator times the
single family/lead contact to the server-owned event and selects the ending
guard from footwork. More sophisticated bounded post-contact continuation is
a future procedural layer, not an authored vocabulary requirement.

The runtime accepts two lead requests per strike family. The complete unarmed
root authors the left-lead contact and satisfies the right-lead request by
reflection; an exact optional right-lead file wins when handedness requires
one. The contact marker synchronizes presentation with gameplay but does not
itself decide whether anything was hit.

Overgrowth likewise stores attack height, direction, stance swapping,
mobility, reactions, and animation paths as data rather than deriving them
from filenames; see
[`attacks.h`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.h#L52-L78)
and
[`attacks.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.cpp#L74-L145).
Overgrowth samples four authored keyframes with cubic interpolation while
clamping the interpolation coordinate to the authored interval; our current
sparse guard/contact/guard recipe is intentionally smaller and does not claim
equivalent procedural continuation.
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
pose. Supine may still result from a hit or physical fall. Supine get-up is
currently unsupported: requesting it holds `supine_idle` rather than silently
reusing the prone transition.

## Initial complete-pack size

The humanoid unarmed root must satisfy every required semantic pose because it
has no parent pack. Exact files and the presence-based mirrored counterparts
both satisfy that requirement. Its tentative authored size is:

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
root supplies one punch contact and one swipe contact, with reflection covering
their optional right-lead requests.

## Secondary animation

Clients may eventually use joint motors or other procedural dynamics to give
bones inertia and react to movement, collisions, weapons, clothing, and
equipment. Overgrowth can mix authored animation with active-ragdoll physics;
its animation output carries per-bone physics weights through
[`animation.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/animation.cpp#L1269-L1425)
and `RiggedObject` applies them to joint strength.

Secondary animation is not in scope for the MVP. The base evaluator should
nevertheless preserve a clean final-pose stage so it can be added without
changing skeleton state or authored pack semantics.

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
