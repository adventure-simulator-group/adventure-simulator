# Combat

Strategic autoresolve uses shared bestiary combat profiles. Skeleton bone is a
full-coverage innate protection layer with substantial resistance and no
padding, making blunt attacks substantially more effective than cutting attacks
through the normal force, resistance, padding, coverage, and penetration model.
There is no species-level post-hoc damage multiplier. The tactical
server does not yet receive canonical bestiary identity, so not every profile
field changes real-time behavior; tactical enemy state remains transient.
**Combat** is the solemn duty of any good knight or mercenary, and until we have a working [fashion](../client/models.md#fifth-version-clothing-and-armor) module, it'll be what players spend most of their time doing. So let's get it right!

## Attacking
When the player clicks the [Attack button](../client/controls.md#direct-controls), initiating an attack animation, we run a shapecast in front of the player character. If there is an intersection between the attacker's hitreg and some other actor's hitbox, we calculate [input precision](../client/controls.md#direct-controls). Then comes the skill check algorithm.

Client melee requests and server-controlled melee AI feed one internal server
attack-intent path. Each melee weapon declares a preferred slash or stab and
separate swing and stab precision terms; the selected animation family selects
the matching combat term. Unarmed fists are a stab-preferring melee fallback.
Client-reported input precision is preserved: reproducing
full animation and secondary physics on the headless server is not an intended
authority boundary, while character and equipment statistics still bound the
combat calculation.

Client windup start and completion are variants of one mapped ordered melee
action protocol, so completion cannot overtake start on a separate event
stream. An observed windup expires one second after it becomes ready, bounding
delayed or replayed completions.

The variants carry only valid data: `Start` has no completion sentinels, melee
`Complete` always carries a target, body part, and reported precision, while
ranged completion is either `CompleteMiss` or `CompleteHit`. Finite precision
is converted to a trusted boundary type and timing to duration-backed authority
types before combat mutation.

### Current offensive AI

Each AI melee combatant has an explicit Party or Enemy allegiance and chooses
the nearest opposing combatant, breaking exact distance ties by stable Bevy
entity identity. It faces that target, walks directly toward it using the
normal character-controller input, stops once the target is within the shared
body-and-arms plus equipped-weapon interaction range, and attacks after a
server-owned windup followed by a cooldown.
Its provisional deterministic attack aims at the chest with full input
precision. Targeted AI windups notify only their intended defender; because
the existing client windup message does not yet identify a target, that legacy
path offers a defensive reaction only to the nearest opposing AI instead of
every frontal AI.

Before resolution, the headless server rejects self or friendly attacks,
missing allegiance or combat state, incapacitated participants, non-finite
precision, invalid weapon state, attacks beyond shared interaction range plus a
small 0.25-meter network-motion allowance, requests outside a fresh
server-observed windup/cooldown, and blocked authoritative physics line of
sight. Cheap state, timing, and range checks run before the physics cast. Finite
client-reported precision remains trusted rather than reconstructed.

Accepted contacts clamp damage against the targeted limb's remaining health
and accumulate blood loss and imbalance. Incapacitation is the shared
autoresolve sum of projected strategic starting condition, pain, blood loss,
and temporary imbalance. Tactical enrollment carries authoritative body weight,
blood volume, and strategic condition inputs; pain and blood are recomputed in
combat instead of double-counted. Balance skill recovers imbalance continuously.
An actor over the threshold stops moving, attacking, defending, and being
selected by offensive AI; an imbalance-only incapacitation can recover and
return the actor to combat. Limb and live combat-state replication provide
basic client feedback, but all of this remains transient server memory.
Every consumer derives readiness from the one numeric incapacitation value;
there is no parallel boolean or server marker for AI and authority to observe
differently.

An enemy's first transition into incapacitation counts as its defeat, even if
temporary imbalance later lets it recover. The server immediately ends the
mission as `Defeated` after all required enemies have been defeated, or as
`Failed` after all loaded Party combatants are incapacitated. Simultaneous
defeat is a failure. Strategic authority binds the expected living Party size;
the decision waits until every expected adventurer has loaded at least once,
no player is still loading, and all required enemies are loaded. After that
enrollment begins, an empty Party has a ten-second reconnection grace before the
mission fails as abandoned, even if everyone disconnects before the seal. A
timeout-disabled development server where nobody ever joins stays available.
Terminal submission freezes the decided result and retries synchronous failures
at one-second intervals before reevaluating combat, commits only after successful
submission, and does not depend on the optional mission timeout; a configured
timeout is only a bounded failure fallback.

This remains an iteration harness without terrain/obstacle pathfinding, but
offensive AI can pursue and attack with melee or ranged equipment. Accepted
attacks accumulate clamped Party injury, blood, and ammunition consequences.
The tactical consequence receipt introduced in stacked branch 4 also supplies
durable equipment-wear provenance: presentation never infers contact from
animations. Instead, the server records bounded contact stress against the
actual attacker weapon, parry shield, or contacted armor inventory item. The
frozen receipt is strictly bounded and validated by strategic authority before
injuries, blood, capability, filth, ammunition, equipment wear, and defeat
morale commit transactionally; invalid receipts remain retryable.

### Skill check algorithm
Broadly speaking, the flow goes like this:
1. Calculate accuracy based on:
	1. The attacker's weighted weapon [skill check](../shared/stats.md#skills). Each weapon distributes its check across Polearm, Axe, Bludgeon, Sword, Knife, Bow, Crossbow, Firearm, and Throw; hybrid tags are normalized.
		1. pass in LimbWeights configured for whatever limb(s) they are attacking with
		2. If they are two handing, 0.75 for main and 0.25 for off-hand
	2. Multiply by weapon term (small knife: 2.0, long hammer: 0.5)
	3. Multiply final value by [input precision](../client/controls.md)

For melee weapons, the weapon term is selected from separate **swing precision** and **stab precision** values. Edges and impact faces generally have swing precision at or below 0.5, while points can be substantially more exact. The catalog's war hammer is the unusual high-swing-precision example because its compact four-sided beak concentrates a swung attack on a small target. Ranged weapons retain their single accuracy term. Damage type is not a recruitment role.
2. calculate `dodge_defense`:
	1. Calculate `armor_dodge_term` from their armor.
		1. This isn't actually the weight of the armor; it's based on articulations on joints.
		2. Full-plate gives 0.6, full-body chainmail is 0.8, and unobstructed joints is 1.0.
	2. Calculate [encumbrance_term](../shared/encumbrance.md) from total weight versus leg-strength
	3. Multiply a dodge [skill_check](../shared/stats.md#skills) by `armor_dodge_term` and `encumbrance_term`
		1. LimbWeights should be something like 0.4 for each leg and 0.1 for each arm
3. calculate `block_defense`:
   
   ```
   let side = // set to whatever side is holding shield
   block = defender.skill_check(block, Some(LimbWeights { la: 1.0, .. }.flip(side))
   shield = defender.shield_bonus()
   ```
	`shield_bonus()` = 0 for weapon; 1–2 for a small shield; 2–4 for normal; 5 for pavise

$$
\mathrm{defense}(\mathrm{shield},\mathrm{block}) = 5 \cdot \left(1 - e^{-\tfrac{\mathrm{shield}+\mathrm{block}}{2}}\right)
$$

4. Calculate `defense` from [`input reflex`](../client/controls.md):

   ```
   if defender is parrying:
   		defense = block_defense * 2 * input_reflex
   elif defender is dodging:
   		defense = dodge_defense * 1.5 * input_reflex
   else:
   		defense = block_defense
   ```
5. Modify defense by flanking penalty
	1. a is the angle that the attacker is facing and b is the angle that the defender is facing
	2. In layman's terms, you have zero defense if someone attacks from behind, full defense if they attack from in front, but the modifier starts at 1 below 45 degrees and is 0 at 135 degrees, rather than at 0 and 180
 	3.
	
$$
D_{\text{final}} =D_{\text{base}} \cdot\mathrm{clamp}\left(\frac{\frac{3\pi}{4}-\left|\mathrm{atan2}(\sin(b-a), \cos(b-a))\right|}{\frac{\pi}{2}},0,1\right)
$$

6. Attack value is accuracy - defense
7. If attack is less than 0, miss and apply surplus defense as unbalance penalty to attacker
8. If attack is between 0 and 1, multiply attack force by attack
	1. 0.1 barely grazes the opponent, 1 is square-on, 0.5 is a glancing blow
9. If attack is *above* 1 and the attacker's weapon is precise, attacker now attempts to bypass armor with surplus attack.
	1. An armor's "coverage" is subtracted from the surplus attack to obtain the "critical attack"
	2. If critical attack is greater than 0, attack bypasses armor completely and its final damage is multiplied by this number
	3. Though not necessarily relevant for the MVP, critical attacks are relevant even when targets are unarmored because this allows the damage multiplier to exceed 1.0, allowing for instantaneous stealth one-hit-kills.
	4. If a critical hit cannot be made, then attack just stays at 1.0 for a direct hit

### Ranged attacks

Ranged attacks use the same attack-minus-defense exchange, armor coverage,
penetration, padding, and critical-hit rules as melee attacks. The attacker's
Bow, Crossbow, Firearm, or Throw distribution supplies the weapon check, both
arms contribute to aiming, and the weapon's projectile energy replaces muscular striking force. Focus adds the character's
Weapon accuracy and future input precision affect attacks; neither is a
character attribute. Agility governs physical-skill learning and mastery.

An alert defender may dodge a projectile or interpose a shield using the normal
Dodge and Block checks. An unaware defender has no active defense. A missed
projectile does not unbalance its attacker. Current projectile energy defaults
to 40 joules per kilogram of ranged weapon, giving the one-kilogram short bow a
40-joule baseline until ammunition carries its own mass and velocity.

The tactical implementation sends firing start and completion on one ordered
stream. The server derives the attacker from the connection and validates
opposing sides, incapacitation, a held ranged weapon, arrow availability,
weapon range, line of sight, windup, and cooldown. A validated shot consumes one
transient `arrow`, including a client-reported miss; Party use is carried in the
bounded terminal consequence receipt. Finite hit precision is intentionally
trusted from the client and remains bounded by the shared combat calculation.
Server-controlled offensive AI uses the same ranged windup, completion,
validation, and ammo-consumption path. It faces the nearest viable target,
maintains a bounded standoff distance while arrows remain, and returns to its
melee pursuit/attack cadence when it cannot make a ranged attack.

## Incapacitation
A character's incapacitation represents the sum of all disabling effects on them and corresponds to the state of their animation. When above half, they are "staggered" and each additional 1% of incapacitation causes a 2% penalty to movement and attribute checks, and when above 100% they are completely incapacitated (which also causes knockdown). Most negative effects that a character has can affect their incapacitation, past a certain threshold. Your incapacitation is displayed as a wheel in the center of the screen. If it is at 0%, the wheel is invisible, and as it increases it starts from 12 o'clock and extends as an arc clockwise. Each factor that contributes to incapacitation has a different color to differentiate them.

The tactical client draws this wheel with EGUI around the center reticle. It
preserves the strategic fear, fatigue, hunger, thirst, and temperature source
breakdown captured at mission enrollment, then combines those segments with
live recomputed pain and blood loss plus transient white imbalance. The arc
clamps at one revolution, and the reticle remains visible inside it.

The strategic character panel uses the same colors for its segmented incapacitation meter, source meters, and source icons. Hunger and thirst share centered meters with their physiological reserves: reserve fills right, while incapacitation fills left after crossing zero. Exact percentages remain available on hover and to assistive technology, while the default view emphasizes the relative contribution of each source.

Each of the following factors range from 0% to at least 100%.
### Imbalance (white)
> Halbe: This was written in terms of energy, but might make more sense in terms of momentum.

The most direct way of being incapacitated, attacks which impart force on your character or losing your footing in difficult terrain can cause imbalance. Imbalance constantly recuperates. Your mass and the directness of an attack determine how much imbalance you actually take, and your agility determines how quickly it is regenerated.
```rs
# use these for calibration
# direct hits by trained warrior in joules: halberd ~120, longsword ~70, shortsword ~30 dagger ~20
# longbow arrow 80
# kg: armored knight ~90, goblin ~40
const STAGGER_RESISTANCE_JOULES_PER_KG = 10
const UPPER_MUSCLE_KG_PER_STRENGTH = 5
const MUSCLE_KG_TO_JOULES = 2
const UPPER_MUSCLE_KG_TO_PUNCH_KG = 0.1

# attack_directness is 1.0 if square-on, 0.01 barely grazes, in-between is a glancing blow of some magnitude
fn balance_damage(attacker, defender, attack_directness):
	# todo: equation for calculating striking mass for a given weapon, for now its fixed
	# balance_factor is 0 for a weapon balanced at the hilt, 1 for a weapon balanced at the tip
	attacker_upper_muscle_kg = attacker.strength * UPPER_MUSCLE_KG_PER_STRENGTH
	punch_kg = UPPER_MUSCLE_KG_TO_PUNCH_KG * attacker_upper_muscle_kg
	striking_mass_kg = punch_kg + attacker.weapon.mass_kg * (1 + attacker.weapon.balance_factor * attacker.weapon.length_meters)
	joules_of_attack = attacker_upper_muscle_kg * MUSCLE_KG_TO_JOULES * striking_kg
	imparted_joules = attack_directness * joules_of_attack
	resistance = STAGGER_RESISTANCE_JOULES_PER_KG * defender.mass_kg
	defender.imbalance += imparted_joules / resistance
```
### Exhaustion (grey)
Exhaustion represents how out of breath your character is. Most actions will not actually exhaust faster than it recuperates, but climbing, sprinting, and fighting with heavy weapons, shield, and armor can.
In tactical combat it is transient, server-authoritative grey incapacitation.
The movement contribution is based on server-authoritative locomotion intent,
not measured physics velocity: full jogging contributes exactly zero, walking
or partial input recovers exhaustion, and sprinting adds it. External impulses
therefore cannot create breath exhaustion, while poison, climbing, combat, and
other future sources remain free to add independent rates. Tactical breath
changes use a 5x response scale so exertion and recovery resolve quickly enough
to matter during a fight without changing any movement-speed thresholds. Wheel
segments below 0.5% are hidden as subpixel display noise without changing state.
```rs
const BREATH_PER_METERS_PER_SECOND = 0.0034
const TACTICAL_BREATH_RESPONSE_SCALE = 5.0

# Sustainable jog speed is 1.8m/s at endurance 1, 2.0m/s at endurance 2,
# and the elite-marathon average of 5.83m/s at endurance 5. Between those
# anchors, most of the extraordinary performance is reserved for high endurance.
fn sustainable_jog_speed(endurance):
	if endurance <= 1:
		t = clamp(endurance, 0, 1)
		return lerp(1.4, 1.8, t * t * (3 - 2 * t))
	t = (clamp(endurance, 1, 5) - 1) / 4
	return 1.8 + 4.03 * pow(t, 2.166)
 
fn update_stamina(player):
	breath_delta = (character.velocity - sustainable_jog_speed(character.endurance)) * BREATH_PER_METERS_PER_SECOND
	player.breath_damage += dt * breath_delta * TACTICAL_BREATH_RESPONSE_SCALE
```
### Pain (pink)
[Injuries](../shared/health.md) are a source of constant pain. Pain is divided by will.

$$
\mathrm{pain}(\mathrm{damage}, \mathrm{will}) = \frac{\mathrm{damage}}{\mathrm{damage} + \alpha\cdot\mathrm{will}}\cdot e^{-\beta\cdot\mathrm{will}};\ \alpha=0.5,\ \beta=0.2
$$

```rs
fn update_pain_factor(character):
	damage = character.body_parts.iter().map(|p| p.damage).sum()
	character.pain = pain(damage, character.will)
```
### Blood loss (red)
Unbandaged [wounds](../shared/health.md) will cause you to bleed out, which will eventually incapacitate you.
### [Fear](../shared/morale.md) (blue)
Morale only starts affecting incapacitation when it goes below 0, at which point each negative point of morale becomes fear, translating to 1% incapacitation.
### [Fatigue](../shared/energy.md) (black)
This does not significantly accumulate in the course of combat, but is more a function of marching all day or going too long without sleeping. This probably has a threshold after which it starts applying nonlinearly ~halfway through the day.

## Penetrating
Each piece of armor has a "resistance" and "padding", both are in terms of
joules. Resistance opposes cutting edges and piercing points. When one of those
attacks connects, the imparted joules are reduced by resistance to determine
how much energy penetrates, if any. Weapons also have a "penetration"
coefficient. The actual resistance used for an edged or pointed attack is:

$$
\mathrm{resistance_{\text{final}}} = \mathrm{resistance_{\text{base}}} - \mathrm{flexibility} \cdot \mathrm{resistance_{\text{base}}} \cdot \mathrm{penetration}
$$

Penetration coefficient examples:
- Clubs: 0.1
- Maces: 0.5
- Swords/axes/musket ball: 1.0
- Broadhead arrows or spear: 2.0
- Mail breaker, rapier, or bodkin arrows: 4.0 

Any edged or pointed energy that penetrates is then applied as cut damage.

Pure blunt contact does not test against edge resistance: its force is
transmitted directly to padding, which dissipates energy before blunt damage is
applied. For mixed blunt-and-edged weapon definitions, penetrated force is
partitioned evenly between the two modes so it is not counted twice. Energy
absorbed by resistance still transmits 50% as blunt force and applies the other
50% as unbalance, as described above.
## Damage
### Cut
Cut damage is divided by the penetration coefficient before being applied. This represents the greater surface area of flesh that is being torn up. Essentially, this makes axes and swords particularly ineffective against armor, but does extra damage against flesh.

Calibration:
- 80kg male's forearm is about 1.2kg
- A 20j direct hit dagger stab against an unarmored forearm should do just enough damage to incapacitate
- The point of having more powerful attacks is not to do more damage to flesh, but to get past armor
- A knight in full-plate still should be vulnerable to a mail breaker or bodkin arrow in the gaps between plates which are guarded only by chainmail
- A 20j stab from a mail breaker should just barely be able to penetrate chainmail and damage flesh
### Blunt

> Halbe: We may want to distinguish between bruising and bone fracturing, perhaps by picking an arbitrary amount of blunt damage energy after which it starts to fracture the bone.

> Halbe: I'm not certain what a good physical base measurement is that we could use for mapping kj of energy to damage. Damage might be best represented as how many kgs of mass have been rendered inoperable, but its not clear to me how to convert between the two. Ultimately though, the damage value relevant to [stats](../shared/stats.md) maps "0" to "gains no function from the body part" and "1" means "body part is fully functioning", so the "displaced kgs of mass" would itself be an intermediate value not displayed to the player.
## Durability
Every durable item defines an elastic/yield threshold, catastrophic fracture threshold, ordinary
wear rate, and catastrophic failure share. Impacts below yield do no condition damage. Above yield,
ordinary wear accumulates continuously; above fracture, additional damage is assigned to the bin
matching the impact severity. The failure share makes segmented construction localize a broken
plate while a monolithic breastplate loses much more usefulness from a comparable fracture.

The five-bin condition remains one visually continuous bar. Bins indicate the Smithing skill needed
for weapons, armor, and shields, or Tailoring for clothing, not discrete named faults. The first two bins are yellow and field-repairable;
the last three are red and require settlement facilities. Stiff weapon steel has a relatively high
yield threshold but a closer fracture threshold. Ductile armor yields and dents sooner while being
harder to shatter.

Condition continuously lowers weapon precision (and other edge-sensitive performance) and increases
the handling/mobility penalty of armor and shields. Armor coverage is not reduced merely because a
local hole exists. Thus deformation of a helmet or breastplate can still impede movement without
pretending that the whole protected region has disappeared.

## Strategic autoresolve

The strategic autoresolver is a bounded abstract battle built from the pure
melee and ranged exchanges in `adventuresim-core`. It begins with two symmetric
pre-engagement phases:

1. Every melee combatant makes a contested Stealth attempt against a randomly
   selected enemy's average Eyesight and Hearing. Both checks add a seeded
   random value from 0 to 5. Success grants one full-precision melee attack
   against the flat-footed target, with no active or facing defense.
2. Ranged combatants fire while enemy melee combatants close. Their number of
   opening attacks is the ranged weapon's range divided by the fastest closer's
   movement speed and the weapon's attack interval. Melee combatants form a
   screen at two-meter intervals. If the closing side has surplus melee
   combatants able to bypass that screen, they must travel a semicircle around
   it; this detour increases the ranged firing window. Weapon melee reach and
   ranged range are separate autoresolve inputs.

During the main engagement, pairings are recomputed every round. Every active
defender receives one melee opponent before surplus attackers are distributed
for a second opponent, then a third, and so on. Every surplus attacker applies
the current 90-degree flanking penalty. A melee screen therefore forces an
equal number of enemy melee combatants to target it before exposed ranged
combatants, and the same rules apply to allies and enemies.

Melee remains round-based: every capable melee combatant attacks once per main
round. A faster melee weapon instead reduces the simulated input reflex of the
defender, representing less time to react. Ranged combat runs on elapsed time;
weapon attack interval determines how many shots occur in each one-second main
round. Ranged combatants target opposing ranged combatants before melee targets.
Every defender chooses dodge, parry/block, or no active response according to
the response with the best expected result.

Every ranged attack consumes one generic arrow. When a combatant runs out, it
becomes a melee combatant and uses its separately equipped melee weapon, if it
has one. Player ammunition spent in autoresolve is removed from personal
inventory. Enemy ranged profiles carry a bounded encounter supply and a melee
fallback.

Targeted body part and hit precision are drawn from a deterministic seeded
random stream. Pain, blood loss, existing strategic incapacitation, and
temporary imbalance can remove a combatant from the fight. The battle ends
when one side is incapacitated or after 256 main rounds, in which case it is a
stalemate.

Autoresolve persists final player wounds, blood loss, and spent ammunition. It
also writes a compact report containing the seed, victor, round count, summary,
and an expandable exchange log. Enemy health and temporary combat state remain
transient, so this diagnostic report does not change the tactical persistence
boundary.

Strategic encounters pass an explicit `Normal`, `AlliesSurprise`, or
`EnemiesSurprise` opening into autoresolve. Awareness is not rolled again in
combat, and exactly one side receives a surprise turn. Quest and random combat
share the complete persistent-outcome commit path (injuries and retained
projectiles, blood loss, ammunition, weapon/shield/armor contact wear, combat
dirt and blood filth, morale, loot classification, and diagnostics). Random
encounter reports use encounter IDs and never create quest battle results or
complete an active quest.

Dropped equipment uses a dedicated physics/query layer. Its authored box
collider supports terrain and pointing pickup queries, but melee hit selection
still targets only limb hitboxes and server combat line-of-sight explicitly
excludes tactical scene-item boxes. Equipment therefore cannot extend a melee
hit shape or provide improvised combat cover. Pickup remains server-authorized
and requires the pointed candidate to win the deterministic ray-distance then
entity-identity ordering, be in range, and have unobstructed line of sight.
