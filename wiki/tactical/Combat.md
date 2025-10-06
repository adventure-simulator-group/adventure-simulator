# Equations
## Does it hit?
Broadly speaking, the flow goes like this:
1. Calculate accuracy based on:
	1. The attacker's melee [skill check](Skills)
	2. Multiply by weapon term (small knife: 2.0, long hammer: 0.5)
	3. Multiply final value by [input precision](Controls.md)
2. calculate dodge_defense:
	1. Calculate armor_dodge_term from their armor.
		1. This isn't actually the weight of the armor, its based on articulations on joints.
		2. Full-plate gives 0.6, full-body chainmail is 0.8, and unobstructed joints is 1.0
	2. Calculate [encumbrance_term](Encumbrance.md) from total weight versus leg-strength
	3. Multiply a dodge [skill_check](Stats.md) by armor_dodge_term and encumbrance_term
3. calculate block_defense:
	1. block = defender.skill_check(block)
	2. shield = defender.shield_bonus()
		1. # 0 for weapon, 1-2 for a small shield, 2-4 for normal, 5 for pavise
$$
\operatorname{defense}(\mathrm{shield},\mathrm{block}) = 5 \cdot \left(1 - e^{-\tfrac{\mathrm{shield}+\mathrm{block}}{2}}\right)
$$
4. if character is parrying:
	1. defense = block_defense * 1.5 * [input reflex](Controls.md)
5. if character is dodging:
	1. defense = dodge_defense * input_reflex
6. else:
	1. defense = block_defense
7. Attack value is accuracy - defense
8. If attack is less than 0, miss and apply surplus defense as unbalance penalty to attacker
9. If attack is between 0 and 1, multiply attack force by attack
	1. 0.1 barely grazes the opponent, 1 is square-on, 0.5 is a glancing blow
10. If attack is *above* 1 and the attacker's weapon is precise, attacker now attempts to bypass armor with surplus attack.
	1. An armor's "coverage" is subtracted from the surplus attack to obtain the "critical attack"
	2. If critical attack is greater than 0, attack bypasses armor completely and its final damage is multiplied by this number
	3. Though not necessarily relevant for the MVP, critical attacks are relevant even when targets are unarmored because this allows the damage multiplier to exceed 1.0, allowing for instantaneous stealth one-hit-kills.
	4. If a critical hit cannot be made, then attack just stays at 1.0 for a direct hit
## Attacking
* Accuracy is determined by how accurate you are--how close do you keep the crosshairs on the enemy for the duration of the attack.
* If its locked-on to the center of their neck for the entire attack then you have a 1.0 multiplier on accuracy
* If you aren't even looking at them its 0.0 multiplier.
## Defending
* Defense is determined by how quickly you react--when an enemy starts their attack, after how long do you press the appropriate defensive button
* If you press it with superhuman reflexes you get a 1.0 multiplier to your character's defensive stat
* If you press it the microinstant before the attack actually hits, or not at all, you get a 0.0 multiplier
* You also may get a penalty if you use the wrong defensive option. If an enemy is doing a R->L diagonal attack, and you duck to the left, you get no defense bonus. If they stab and you duck instead of dodging or blocking you get no defense bonus.
* Blocking might basically always "succeed" but you are staggered somewhat by each attack, especially if they are very heavy. Do not attempt to block a giant's club, for example.
## Incapacitation
A character's incapacitation represents the sum of all disabling effects on them and corresponds to the state of their animation. When above half, they are "staggered" and each additional 1% of incapacitation causes a 2% penalty to movement and attribute checks, and when above 100% they are completely incapacitated (which also causes knockdown). Most negative effects that a character has can affect their incapacitation, past a certain threshold. Your incapacitation is displayed as a wheel in the center of the screen. If it is at 0%, the wheel is invisible, and as it increases it starts from 12 o'clock and extends as an arc clockwise. Each factor that contributes to incapacitation has a different color to differentiate them.

Each of the following factors range from 0% to at least 100%.
### Imbalance (white)
The most direct way of being incapacitated, attacks which impart force on your character or losing your footing in difficult terrain can cause imbalance. Imbalance constantly recuperates. Your mass and the directness of an attack determine how much imbalance you actually take, and your agility determines how quickly it is regenerated.
```
# use these for calibration
# direct hits by trained warrior in joules: halberd ~600, longsword ~300, shortsword ~200
# kg: armored knight ~90, goblin ~40
const STAGGER_RESISTANCE_JOULES_PER_KG = 50
const UPPER_MUSCLE_KG_PER_STRENGTH = 5
const MUSCLE_KG_TO_JOULES = 10
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
```
const BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND = 0.002
# someone with 2 endurance (poorly fed Napoleonic soldier) can march 1.2m/s all day. Therefore a simple linear ratio between velocity and breath must be about:
const BREATH_PER_METERS_PER_SECOND = 0.0034
 
fn update_stamina(player):
	player.breath_damage += dt * character.velocity * BREATH_PER_METERS_PER_SECOND
	player.breath_damage -= dt * character.endurance * BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND 
```
### Pain (pink)
[Injuries](Health.md) are a source of constant pain. Pain is divided by will. 

$$
\operatorname{pain}(\mathrm{damage}, \mathrm{will}) = \frac{\mathrm{damage}}{\mathrm{damage} + \alpha\cdot\mathrm{will}}\, e^{-\beta\cdot\mathrm{will}};\;\alpha=0.5;\;\beta=0.2
$$

```
fn update_pain_factor(character):
	damage = character.body_parts.iter().map(|p| p.damage).sum()
	character.pain = pain(damage, character.will)
```
### Blood loss (red)
Unbandaged [wounds](Health.md) will cause you to bleed out, which will eventually incapacitate you.
### [Fear](Morale.md) (blue)
Morale only starts affecting incapacitation when it goes below 0, at which point each negative point of morale becomes fear, translating to 1% incapacitation.
### [Fatigue](Energy.md) (black)
This does not significantly accumulate in the course of combat, but is more a function of marching all day or going too long without sleeping. This probably has a threshold after which it starts applying nonlinearly ~halfway through the day.