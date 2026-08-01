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
6. Apply procedural facing, foot placement, hip correction, terrain IK,
   hand/weapon constraints, and head/torso look.
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

Locomotion poses use the **left** side as their canonical first half-cycle.
The runtime may construct the opposite half by mirroring lower-body motion.
Attack and guard poses are not assumed to be whole-body mirrorable because a
weapon may remain in the same hand.

## Required semantic poses

The following inventory defines the complete initial humanoid unarmed pack.
Other packs may omit any of these names and inherit them through their single
fallback chain.

### Standing and locomotion

The opposite half of each gait cycle can be produced by mirroring lower-body
motion while preserving handed upper-body carriage.

| Pose | Animator brief |
|---|---|
| `idle_relaxed` | Stand upright with the feet approximately hip- to shoulder-width apart and the weight balanced between them. Keep one foot only slightly ahead if perfect symmetry looks artificial. The knees and elbows remain unlocked, the shoulders hang without slumping, and the pelvis and ribcage are stacked comfortably. This is a non-threatening rest pose, not a combat guard. |
| `walk_contact` | Pose the instant of left-foot initial contact. The left leg reaches forward with the heel touching or about to touch the floor; the right leg trails with only its forefoot/toes still supporting. The legs are at their greatest useful separation. Weight is transferring forward rather than already resting fully on the left foot. The pelvis counter-rotates naturally and, where the hand carriage permits it, the right arm is forward and the left arm back. Both designated foot contacts must lie on the same floor plane. |
| `walk_passing` | Keep the left foot planted beneath or just behind the pelvis while the right swing foot passes beside it on the way forward. The right knee is flexed enough for toe clearance and the right foot is not planted. The body is near its tallest point in the walk cycle, but the left knee remains soft. Pelvis and shoulder rotation are between their contact extremes. |
| `run_contact` | Pose the instant the left foot accepts a running landing beneath or only modestly ahead of the center of mass. Flex the left ankle, knee, and hip to absorb load. The right leg trails and is leaving the prior flight phase. The torso inclines forward from the ankles without folding at the waist. Only the left foot is marked planted; avoid a long over-stride. |
| `run_flight` | Pose the airborne crossover after left support, with neither foot planted. The right knee travels forward, the left leg trails, and both knees remain flexed rather than forming a split. Keep the pelvis level enough to interpolate cleanly, the body traveling forward, and the upper-body carriage compatible with the pack's held item. This must read as a run flight phase, not a walking passing pose. |
| `crouch_idle` | Lower the pelvis by flexing hips, knees, and ankles while keeping both whole feet stably planted about shoulder-width apart. Keep the chest sufficiently upright to look forward and use the hands. Do not obtain the height by bending only the spine. The pose must be able to load into a jump and serve as the center of the directional duck blend. |
| `crouch_walk_contact` | Use the same left-forward/right-rear contact relationship as `walk_contact`, but keep the pelvis at crouch height and the stride shorter. The left foot is accepting weight and the right forefoot is leaving. Maintain head clearance and a usable hand carriage; do not let the knees collapse inward. |
| `crouch_walk_passing` | Keep the left foot planted while the right foot passes low beside it with enough toe clearance. The pelvis stays near crouch height instead of rising to full standing height. The left knee remains flexed and stable, and the torso does not bob upright. |

Walk, run, and crouch-walk use the same normalized gait phase. Speed blends
walk continuously into run; crouch amount blends upright locomotion into its
crouched counterpart. A run is not merely an exaggerated walk: it must retain
an airborne phase, while walking retains ground contact.

During ordinary travel the body can turn toward its velocity, so the first
version does not require separate forward, backward, and lateral gait cycles.
During combat, procedural stance stepping keeps the body oriented toward the
opponent. Overgrowth takes a similar approach by maintaining planted-foot
targets and moving one foot at a time in
[`HandleFootStance`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Data/Scripts/aschar.as#L11726-L11833).

### Combat guards

Every complete combat pack provides, or inherits:

| Pose | Animator brief |
|---|---|
| `guard_lead_left` | Place the left foot forward and the right foot back on two stable tracks rather than one tightrope line. Distribute weight so either foot can move without a preliminary shuffle; neither knee is locked. Turn the pelvis and torso only as required by the pack's fighting method. In the unarmed root pack, raise both hands to protect the head and torso with the dominant hand free to punch. In a weapon pack, use its normal ready grip and keep the point or striking portion controlled. |
| `guard_lead_right` | Construct the corresponding stable guard with the right foot forward and left foot back. Preserve the same handedness and held-item hand as `guard_lead_left`; this is a change of foot lead, not a whole-body mirror. Match stance width, guard height, and overall readiness closely enough that attacks can end in either guard without a visible change of style. |

These are stable endpoints for attacks and blocks. Which foot is forward is an
explicit part of skeleton state. Complete one-handed guards should be authored
for both lead feet rather than obtained by mirroring the entire body, because
whole-body mirroring also changes the weapon hand.

### Crouching and directional ducking

`crouch_idle` is the center of a two-dimensional directional duck blend. Four
additional poses define its extrema. Begin each pose from `crouch_idle`, keep
the same planted foot locations, and preserve the normal hand carriage.

| Pose | Animator brief |
|---|---|
| `duck_forward` | Move the head, ribcage, and pelvis forward and lower them slightly by increasing ankle, knee, and hip flexion. Keep the center of mass inside the feet and the heels controlled; this should look like slipping under and toward an incoming path, not falling or diving. Tuck the chin enough to protect the face. |
| `duck_backward` | Withdraw the head and upper torso backward while sitting the pelvis down and back between the feet. Increase knee flexion to preserve balance and avoid creating the motion solely by arching the lower back. Keep the gaze generally forward and do not lift both toes or heels. |
| `duck_left` | Shift the pelvis, ribcage, and especially the head toward anatomical left. Load the left leg more heavily, flex it, and allow the right leg to lengthen without moving either foot. Incline or rotate the torso only enough to keep balance and protect the head. Do not cross the legs. |
| `duck_right` | Shift toward anatomical right as the structural counterpart of `duck_left`: load and flex the right leg, allow the left leg to lengthen, keep both feet planted, and move the head clearly out of the central line without collapsing the torso. |

The direction describes the defender's desired body or head displacement in
local space, not merely the attacker's bearing. The incoming weapon path and
targeted body region determine which displacement is useful. Diagonal ducks
are produced by blending adjacent extrema.

Directional ducks should preferably be authored as pose deltas or masked
overrides so that they can be applied over the current guard without requiring
four versions for every weapon and lead foot.

### Jumping and dodging

Jumping and dodging share one directional pose family. Charge changes the
height, distance, and timing rather than selecting an unrelated animation.
There are five directional samples: center, forward, backward, left, and
right. Each has a launch, flight, and landing pose. Use the following phase
rules for every direction:

- **Launch:** show the first instant after the final floor contact. The pushing
  leg or legs are near extension but not hyperextended; no foot is marked
  planted. The pose should clearly continue from a loaded crouch.
- **Flight:** show a sustainable mid-air arrangement with no floor contacts.
  Avoid an extreme tuck or split that would look frozen when held for a long
  airtime. The head remains able to track forward and held weapons remain
  controlled.
- **Landing:** show the instant of first anticipated floor contact. Mark the
  named receiving foot or feet as planted, flex the receiving joints, keep the
  knee aligned over the foot, and arrange the torso so it can continue into
  `crouch_idle` or a guard without snapping.

The direction-specific briefs are:

| Direction | Launch | Flight | Landing |
|---|---|---|---|
| `center` | Push evenly through both legs with the pelvis rising vertically and both feet leaving together or nearly together. Keep lateral symmetry except for the pack's hand carriage. | Hold both knees moderately flexed beneath the hips with the feet separated enough for balance. Keep the torso upright and avoid implying forward travel. | Receive on both forefeet/feet at approximately shoulder width, with symmetrical ankle, knee, and hip flexion and the pelvis descending between them. |
| `forward` | Finish the push with the rear leg extending behind the traveling pelvis while the forward knee begins to advance. Incline the whole body slightly forward without folding the spine. | Carry the forward knee ahead and the opposite leg behind in a compact, controllable leap. The pelvis and chest face generally forward. | Reach the forward foot toward first contact beneath, not far ahead of, the center of mass; keep the rear foot ready to follow and flex the lead leg to absorb forward momentum. |
| `backward` | Push the body backward while keeping the chest and gaze sufficiently forward to see the threat. Let the knees and feet travel slightly forward relative to the pelvis; do not throw the head backward. | Keep both legs somewhat in front of or beneath the pelvis, one prepared to reach backward for the ground. Maintain a guarded torso rather than an uncontrolled back arch. | Reach one foot backward under the traveling pelvis for first contact, flex it to absorb motion, and keep the other foot ready to widen the base. The character must not land on a locked rear knee. |
| `left` | Push primarily from the right leg, allowing the left leg and pelvis to travel left. Keep the right leg extended toward the takeoff point and the torso balanced over the lateral motion. | Lead with the left knee/foot and let the right leg trail to the right without crossing behind it. Keep the pelvis facing generally forward. | Receive first on the left foot with the left knee flexed and aligned, the right foot ready to establish width, and the torso resisting excessive leftward collapse. |
| `right` | Push primarily from the left leg, allowing the right leg and pelvis to travel right. Keep the left leg extended toward the takeoff point and the torso balanced. | Lead with the right knee/foot and let the left leg trail without crossing. Preserve forward awareness and controlled hand carriage. | Receive first on the right foot with aligned flexion, the left foot ready to establish width, and the torso balanced against rightward momentum. |

The resulting authored names are
`jump_center_launch`, `jump_center_flight`, `jump_center_landing`, and the
equivalent three names for `forward`, `backward`, `left`, and `right`.

The sequence is:

```text
directional duck/load -> launch -> flight -> landing -> crouch or guard
```

The load reuses the directional duck blend, so it does not require another
five poses. The center sample supports a stationary vertical jump. Air phase
is driven primarily by vertical velocity so the flight pose can extend for
different airtimes without slowing takeoff or landing. Overgrowth uses the
same general idea by deriving an `up_coord` from vertical velocity in
[`aircontrols.as`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Data/Scripts/aircontrols.as#L114-L180).

### Attacking

An attack recipe declares:

- its semantic strike family: `thrust` or `slash`;
- its starting and ending lead foot;
- whether its footwork is `stay` or `switch`;
- its target-height behavior;
- its commit, contact, recovery, and cancellation timing; and
- its hand/weapon constraints.

A stay attack ends with the same foot forward. A switch attack takes one step
and ends with the opposite foot forward.

Each attack motion has three attack-specific poses. Names use
`attack_<family>_lead_<left|right>_<stay|switch>_<phase>`; for example,
`attack_thrust_lead_left_switch_contact`.

The phases have these animator-facing meanings:

| Phase | Animator brief |
|---|---|
| `commit` | Begin from the named guard and show the last controllable preparation before acceleration toward the target. Preserve balance and defense. The pose may visibly load the legs and torso, but a trained attack should not use a theatrical wind-up. In a stay attack, both feet remain in their guard locations. In a switch attack, unload the rear foot that will become the new lead while the original lead still supports the body. |
| `contact` | Pose the instant the fist, claw, point, or edge crosses a canonical target plane at approximately upper-torso height. Align the striking structure from the floor through hips and torso to the striking limb without locking the elbow or knee. A stay attack retains both original foot contacts. In a switch attack, the formerly rear foot is arriving or planted as the new lead and must be capable of supporting the strike. |
| `follow_through` | Continue the real line of force beyond contact rather than stopping unnaturally at the target plane, but retain enough structure to recover. A stay attack still uses the original foot locations and prepares to return to the same guard. A switch attack settles weight into the new lead and arranges the body to finish in the opposite-lead guard. Do not add a full spin unless the particular pack explicitly calls for one. |

The strike-family construction rules are:

| Family | Animator brief |
|---|---|
| `thrust` | Move the primary striking point generally forward along a direct line. For the unarmed root this is a punch: the primary fist travels toward the target while the other hand protects, the shoulder does not rise into the neck, and the fist/wrist/forearm align at contact. For a weapon, the point and grip align with the thrust while the off hand follows the weapon's normal use. Retract or chamber only enough to create a plausible commit pose. |
| `slash` | Move the primary striking edge, claw, or hand from the pack's primary side across the target line toward the opposite side. For the unarmed root this is a swipe, particularly suitable for claws. At contact, preserve edge or claw alignment and support the motion with coordinated pelvis and ribcage rotation rather than an isolated arm swing. Continue onto the opposite side in follow-through without turning it into an uncontrolled baseball swing. A pack that later supports the reverse slash direction should define it as a distinct recipe. |

For `lead_left`, begin from `guard_lead_left`; for `lead_right`, begin from
`guard_lead_right`. For `stay`, the feet occupy those same floor positions in
all three poses. For `switch`, the initially rear foot passes or steps beyond
the initial lead so that the motion can finish in the opposite guard. The new
lead should be usable by contact or immediately afterward; the animator should
not depict both feet airborne at the contact pose unless that recipe is
explicitly an airborne attack.

The full visual sequence is:

```text
start guard -> commit -> contact -> follow-through -> end guard
```

The commit may be subtle for a trained character, but it gives immediate local
feedback and provides a plausible segment in which to absorb network latency.
The contact marker synchronizes presentation with the server-owned attack
event; it does not itself decide whether anything was hit.

For each strike family a pack defines, the minimum graph has four motions:

- left-foot lead, stay;
- left-foot lead, switch;
- right-foot lead, stay; and
- right-foot lead, switch.

At three poses per motion, overriding only thrust or only slash requires twelve
attack-specific poses. Defining both requires twenty-four. The complete
unarmed root defines both so that every descendant can resolve either semantic
strike. Slash direction may initially be the biomechanically appropriate
direction for the starting stance. Supporting both slash directions
independently from either lead foot would add more recipes, but is not required
by gameplay yet.

Overgrowth likewise stores attack height, direction, stance swapping,
mobility, reactions, and animation paths as data rather than deriving them
from filenames; see
[`attacks.h`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.h#L52-L78)
and
[`attacks.cpp`](https://github.com/WolfireGames/overgrowth/blob/245fe4828631c84c0023d29d1525f5716ccb6106/Source/Asset/Asset/attacks.cpp#L74-L145).

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
lead.

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
| `prone_strafe_contact` | Author the canonical leftward strafe extreme. Plant the left forearm/hand to pull or brace toward the left while the right forearm and right knee/foot push the torso laterally. Keep the body facing forward rather than rolling fully onto its side, and separate the limbs enough to prevent intersection. The rightward extreme may be produced from the lower/body-safe mirror. |
| `prone_strafe_passing` | Compact the limbs beneath and beside the torso halfway through the lateral crawl. The body remains low and forward-facing, with support transferring between forearms and knees without a visible forward surge. |
| `supine_scamper_contact` | On the back, plant the left heel/foot and the opposite hand or forearm as the canonical extended support, with the pelvis slightly lifted or unloaded enough to move. The right leg is advancing toward its next plant. Protect the head and keep the neck from bearing body weight. |
| `supine_scamper_passing` | Bring the advancing foot under the knee and move the torso through the support provided by heels and arms. Keep the pelvis near the floor but visibly mobile, and preserve a guarded upper body. This is the midpoint used between mirrored scamper contacts. |
| `upright_prone_transition` | Pose the stable intermediate between standing/crouching and prone: both hands or forearms and at least one knee contact the floor, the head remains protected and able to look forward, and the pelvis is low enough to continue down without dropping the chest through the ground. It must also work in reverse as the main get-up intermediate. |
| `dive_impact` | Pose first controlled contact after a forward dive. Use forearms and/or hands to absorb impact with bent elbows, turn or raise the head clear of the floor, keep the chest just above contact, and let knees/hips flex behind the body. Do not land on locked wrists, straight elbows, the face, or the weapon. |
| `prone_supine_roll_left` | Pose the midpoint of rolling over the anatomical left side between prone and supine. The body rests chiefly on the left side of torso/hip, the left arm is placed where it will not be trapped beneath the body, the right arm protects or guides the roll, and the knees are modestly flexed to clear each other. |
| `prone_supine_roll_right` | Construct the corresponding safe midpoint over the anatomical right side, with the right arm clear of entrapment and the left arm free to protect or guide. Preserve the same handedness and item grip rather than blindly mirroring equipment into the opposite hand. |

Backward crawling may initially reverse the forward cycle, and getting up may
reverse the upright-to-prone transition. The side-roll poses support the
camera-driven change between prone and supine and can later contribute to
ground dodges.

## Initial complete-pack size

The humanoid unarmed root must define every required semantic pose because it
has no fallback. Its tentative size is:

| Family | Authored poses |
|---|---:|
| Standing and locomotion | 8 |
| Directional ducking | 4 |
| Jumping and dodging | 15 |
| Prone and supine | 12 |
| Combat guards | 2 |
| Thrust/punch attacks | 12 |
| Slash/swipe attacks | 12 |
| Blocks | 6 |
| **Complete unarmed root** | **71** |

Most specialized packs should be substantially smaller because they inherit
unchanged poses. A pack that overrides just one strike family needs twelve
attack poses, not another complete set of seventy-one. The unarmed root
supplies punch poses for unresolved thrust semantics and swipe poses for
unresolved slash semantics.

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
