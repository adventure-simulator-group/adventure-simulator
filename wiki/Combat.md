# Equations
## Does it hit?
[input_accuracy](Controls) is the value from 0 to 1 representing how close the player put their cursor on the center of any of the target's hitboxes. Its exact formula will have to come about through testing. 

[input_defense](Controls) is the value from 0 to 1 representing how quickly the defender pressed the dodge/parry button after the attack began. Like with input_accuracy, we aren't sure exactly what this will be but a value of 1.0 would correspond to pro-gamer reaction time (0.1s) and ~0.75 would correspond to old person reaction time (0.25s).

[skill_check](Stats) is based on your skills, attributes, and focus
For NPCs these numbers are just randomized in a normal distribution around an average player skill level.
```
fn diminishing_bonus(value)
	5 * (1-e^(-value/2))

const CARRY_WEIGHT_PER_STRENGTH = 5
fn encumbrance_penalty(character):
	e^-(character.strength * CARRY_WEIGHT_PER_STRENGTH)

accuracy = skill_check(melee, 0) + weapon_accuracy) * input_accuracy
if defender_is_dodging:
	armor_dodge_penalty = 0.5-1, not based on weight but rather whether movement is restricted on joints
	encumbrance_penalty = 0-1
	defense = dodge_input * (skill_check(dodge, 0) * armor_dodge_penalty)
if defender_is_blocking:
	block_check = skill_check(block, 0)
	shield_size_bonus = "weapon:0, buckler:1-2, normal shield:2-4, pavise:5"
	defense = diminishing_bonus(block_check, shield_size_bonus)
if defender_is_parrying:
	block_check = skill_check(block, 0)
	defense = parry_input * block_check
hit < 0:
    miss
hit >=0 and <1:
	glancing blow (attacker is now off-balance)
hit >=1:
	direct hit on armor/shield, the full value of the attack force is imparted on the enemy
hit >=2 and weapon is precise:
	precise_hit = min((hit-2), attacker_precision_accuracy)
critical_hit = precise_hit - armor_coverage:
critical_hit >= 0:
	ignore armor
else:
	stay as a direct hit
```
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
## Poise (white)
A character's poise represents their footing, and corresponds to the state of their animation. When below half, they are "staggered" and cannot take any actions other than (slowly) moving, and when below zero, they are incapacitated. Most negative effects that a character has can affect their poise, past a certain threshold. Your poise is displayed as a white wheel in the center of the screen. Each factor that negatively affects your poise has a different color to differentiate them. The exact animation that gets displayed when stunned/incapacitated.

Each of the following factors range from 0 to at least 1, and are all subtracted from your poise.
### Imbalance (grey)
The most direct way of losing poise, attacks which impart force on your character or losing your footing in difficult terrain can cause imbalance. Imbalance constantly recuperates. Your mass and the directness of an attack determine how much imbalance you actually take, and your agility determines how quickly it is regenerated.
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
### Exhaustion (black)
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
[Injuries](Health) are a source of constant pain. Pain is divided by will. 

$$
pain(damage, will) = \frac{damage}{damage + \alpha will}\, e^{-\beta will};\alpha=0.25;\beta=0.2
$$

```
fn update_pain_factor(character):
	damage = character.body_parts.iter().map(|p| p.damage).sum()
	character.pain = pain(damage, character.will)
```
### Blood loss (red)
Unbandaged wounds will cause you to bleed out, which will eventually incapacitate you due to poise damage.
### [Fear](Morale) (blue)
Morale only starts affecting poise when it goes below 0, at which point each negative point of morale becomes fear, translating to -1% of poise damage.
### [Fatigue](Energy) (clear)
This does not significantly deteriorate in the course of combat, but is more a function of marching all day or going too long without sleeping. This probably has a threshold after which it starts applying nonlinearly ~halfway through the day.
## Stats



* Skills should account for the fact that they can be related
	* If we want different skills for different melee weapon types, they should have some way to overlap
	* If you spend 10000 hours mastering the blade while your classmates are out partying, you're probably at least decent with an axe even if you've never used one
	* Option A: skills can be "correlated"
		* The sword skill has a 0.5 correlation with the axe skill. Every time you gain one point of sword, you have 0.5 points in axe
		* Keep track of the base values of these to prevent feedback loops. What does not happen is you put 1 point into sword which gives 0.5 in axe which gives 0.25 in sword which gives 0.125 in axe and so on.
	* Option B: skills are broken down further
		* What swords and axes have in common is swinging attacks and keeping the edge pointed in the direction of motion. 
		* Where they're different is that axes are unbalanced and have comparatively short edges.
		* It could be broken up into six skills: swing attacks, thrust attacks, edge alignment, balanced weapons, unbalanced weapons, and lengthwise accuracy. Each weapon involves four of these
		* Simple weapons like clubs and would naturally require fewer skills for mastery
		* "Allistic" people would not care about their character's lengthwise accuracy, so this would probably go hand-in-hand with some abstraction that lets them ignore this
		* "Swords" is a meta-skill which includes all the skills involved in using a sword. If they pick the swords skill, it will just put points into all of them evenly
		* This effectively recreates the correlations from option A, because now if you have the sword skill then you have half the skills that go into axes
		* The abstractions can even have abstractions, "melee" is a meta-skill that includes all the melee skills
		* Abstractions would be done entirely client-side, and we might provide some defaults in the settings analogous to a player selecting their difficulty. Essentially you can choose how complex you want the mechanics to be. More complex = more information and micromanagement
		* This might also be helpful for accounting for the fact that radically different settings still need to share the same underlying skill system. Abstractions could be reworked per-setting, like "alchemy" is pretty similar to "chemistry" but not quite the same.