# Combat
**Combat** is the solemn duty of any good knight or mercenary, and until we have a working [fashion](../client/Models.md#fifth-version-clothing-and-armor) module, it'll be what players spend most of their time doing. So let's get it right!

## Attacking
When the player clicks the [Attack button](../client/Controls.md#direct-controls), initiating an attack animation, we run a shapecast in front of the player character. If there is an intersection between the attacker's hitreg and some other actor's hitbox, we calculate [input precision](../client/Controls.md#direct-controls). Then comes the skill check algorithm.

### Skill check algorithm
Broadly speaking, the flow goes like this:
1. Calculate accuracy based on:
	1. The attacker's weighted weapon [skill check](../shared/Stats.md#Skills). Each weapon distributes its check across Polearm, Axe, Bludgeon, Sword, Knife, Bow, Crossbow, Firearm, and Throw; hybrid tags are normalized.
		1. pass in LimbWeights configured for whatever limb(s) they are attacking with
		2. If they are two handing, 0.75 for main and 0.25 for off-hand
	2. Multiply by weapon term (small knife: 2.0, long hammer: 0.5)
	3. Multiply final value by [input precision](../client/Controls.md)

The weapon term also provides the strategic recruitment **weapon precision** scale. The current discrete recommendations are 0.5 for clubs and hammers, 1.0 for axes, 1.5 for ordinary swords and spears, and 2.0 for purpose-built precise weapons such as rapiers or bodkin ammunition. Damage type is not a recruitment role: slash, pierce, and blunt weapons are compared through this single precision scale instead.
2. calculate `dodge_defense`:
	1. Calculate `armor_dodge_term` from their armor.
		1. This isn't actually the weight of the armor; it's based on articulations on joints.
		2. Full-plate gives 0.6, full-body chainmail is 0.8, and unobstructed joints is 1.0.
	2. Calculate [encumbrance_term](../shared/Encumbrance.md) from total weight versus leg-strength
	3. Multiply a dodge [skill_check](../shared/Stats.md#Skills) by `armor_dodge_term` and `encumbrance_term`
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

4. Calculate `defense` from [`input reflex`](../client/Controls.md):

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
Precision to both melee and ranged accuracy; Agility remains the reflex term.

An alert defender may dodge a projectile or interpose a shield using the normal
Dodge and Block checks. An unaware defender has no active defense. A missed
projectile does not unbalance its attacker. Current projectile energy defaults
to 40 joules per kilogram of ranged weapon, giving the one-kilogram short bow a
40-joule baseline until ammunition carries its own mass and velocity.

## Incapacitation
A character's incapacitation represents the sum of all disabling effects on them and corresponds to the state of their animation. When above half, they are "staggered" and each additional 1% of incapacitation causes a 2% penalty to movement and attribute checks, and when above 100% they are completely incapacitated (which also causes knockdown). Most negative effects that a character has can affect their incapacitation, past a certain threshold. Your incapacitation is displayed as a wheel in the center of the screen. If it is at 0%, the wheel is invisible, and as it increases it starts from 12 o'clock and extends as an arc clockwise. Each factor that contributes to incapacitation has a different color to differentiate them.

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
```rs
const BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND = 0.002
# someone with 2 endurance (poorly fed Napoleonic soldier) can march 1.2m/s all day. Therefore a simple linear ratio between velocity and breath must be about:
const BREATH_PER_METERS_PER_SECOND = 0.0034
 
fn update_stamina(player):
	player.breath_damage += dt * character.velocity * BREATH_PER_METERS_PER_SECOND
	player.breath_damage -= dt * character.endurance * BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND 
```
### Pain (pink)
[Injuries](../shared/Health.md) are a source of constant pain. Pain is divided by will. 

$$
\mathrm{pain}(\mathrm{damage}, \mathrm{will}) = \frac{\mathrm{damage}}{\mathrm{damage} + \alpha\cdot\mathrm{will}}\cdot e^{-\beta\cdot\mathrm{will}};\ \alpha=0.5,\ \beta=0.2
$$

```rs
fn update_pain_factor(character):
	damage = character.body_parts.iter().map(|p| p.damage).sum()
	character.pain = pain(damage, character.will)
```
### Blood loss (red)
Unbandaged [wounds](../shared/Health.md) will cause you to bleed out, which will eventually incapacitate you.
### [Fear](../shared/Morale.md) (blue)
Morale only starts affecting incapacitation when it goes below 0, at which point each negative point of morale becomes fear, translating to 1% incapacitation.
### [Fatigue](../shared/Energy.md) (black)
This does not significantly accumulate in the course of combat, but is more a function of marching all day or going too long without sleeping. This probably has a threshold after which it starts applying nonlinearly ~halfway through the day.

## Penetrating
Each piece of armor has a "resistance" and "padding", both are in terms of joules. When attack connects, the imparted_joules is subtracted by the resistance to determine how much energy penetrates the armor, if any. Weapons also have a "penetration" coefficient. The actual resistance used for the attack is:

$$
\mathrm{resistance_{\text{final}}} = \mathrm{resistance_{\text{base}}} - \mathrm{flexibility} \cdot \mathrm{resistance_{\text{base}}} \cdot \mathrm{penetration}
$$

Penetration coefficient examples:
- Clubs: 0.1
- Maces: 0.5
- Swords/axes/musket ball: 1.0
- Broadhead arrows or spear: 2.0
- Mail breaker, rapier, or bodkin arrows: 4.0 

Any energy that penetrates is then applied as cut damage.

Energy which is absorbed by the resistance, is the blunt energy. 50% of blunt energy is applied as unbalance, as described above, and the other 50% is subtracted by the padding to determine blunt damage.
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

> Halbe: I'm not certain what a good physical base measurement is that we could use for mapping kj of energy to damage. Damage might be best represented as how many kgs of mass have been rendered inoperable, but its not clear to me how to convert between the two. Ultimately though, the damage value relevant to [stats](../shared/Stats.md) maps "0" to "gains no function from the body part" and "1" means "body part is fully functioning", so the "displaced kgs of mass" would itself be an intermediate value not displayed to the player.
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
