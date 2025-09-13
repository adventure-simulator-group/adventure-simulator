# Attributes
The maximum value of your characters' attributes is determined by their genetics, but the actual value may be quite a bit lower if they are not properly conditioned. For example, even if you have the theoretical ability to build a large amount of muscle, if you have poor nutrition or don't exercise then you will realize very little of it. Conditioning is different for each attribute, but generally no one will be able to condition all of their attributes to their maximum potential due to there only being 24 hours in a day.
## Strength
Proportional to the total muscle mass of your character's upper body. Increases climb speed and attack damage.
0. Cripple
1. Child
2. Adult woman, pubescent boy
3. Adult man
4. Trained knight
5. Olympic athlete
## Speed
Proportional to the total muscle mass of your character's lower body. Increases movement speed and jump height.
0. Cripple
1. Child
2. Pubescent
3. Adult
4. Trained knight
5. Olympic athlete
## Intelligence
The depth at which your character can think at. Applies a bonus to mental skills. In order to use your intelligence bonus, a skill has to give you time to think about the problem.
0. Not capable of conscious thought
1. Low-functioning autistic, toddler
2. Would struggle to learn even basic math
3. Can learn high-school-level math
4. Can learn college-level math
5. Can meaningfully contribute to the field of mathematics
## Instinct
Your ability to make snap judgements without thinking. A character with high instinct makes for a good small group leader in battle, where making a quick, decent decision is more important than making an ideal decision but taking awhile to come to it. Applies a bonus to your mental skills. The bonus does not require focus, but is lower than an equal bonus from intelligence.
0. Unconscious
1. Takes a couple seconds to respond if you ask them a question
2. Absentminded
3. Alert
4. Veteran captain
5. Enlightened monk
## Precision
Your ability to perform skills that require a delicate touch. Applies a bonus to physical skills, but only when they can be focused on. Analogous to intelligence.
0. Hand tremors
1. Toddlers, zombies
2. Skeletons, orcs, and goblins
3. Professional soldiers
4. Decent pickpockets/craftsman
5. Elven heroes
## Agility
Your capacity to learn skills which rely on having quick, sensitive nerves. Applies a bonus to physical  skills instantaneously, analogous to instinct.
0. Paralyzed, unaware, or tied up
1. Drunken oaf, orcs, zombies
2. Clumsy, goblins, skeletons
3. Professional soldiers and knights
4. Heroes
5. Elven heroes
## Endurance
Represents the strength of your heart and capacity of your lungs. It determines how long you can go without suffering from exhaustion and how fast you move when traveling. Conditioned by traveling on foot.
0. Asphyxiated
1. Dainty sheltered nobles
2. City-folk
3. Knights and peasants
4. Professional soldiers
5. Adventuring heroes
6. Undead (cannot be tired)
## Immunity
How well your immune system protects against disease and poisons.
0. AIDS
1. Infants
2. Sheltered nobles and children
3. Rural commoners and knights
4. Elves and city-folk
5. Vampires (immune to disease)
## Gut
How how edible food needs to be in order for you to effectively digest it and how much variety you need to be decently healthy. Cooking makes food more edible, but food that is more fibrous and less nutritious can only be improved by so much.
0. Vampires (cannot digest food, must get calories directly from blood glucose)
1. Elves (can only eat meat, fat, and luxurious elven plants)
2. Nobles, orcs
3. Professional soldiers, goblins
4. Peasants (can survive almost entirely on grains without huge penalty)
5. Livestock

# Skills
Skills are divided into two categories: mental and physical. The former is governed by intelligence and instinct, the latter by precision and agility.
## Training
Skills increase on a much longer timescale than is conventional for RPGs, they are not increased via an abstract XP/leveling system and very little of their value comes from actually using them during gameplay. Instead they are *trained* during your character's off-screen daily schedule. We are not actually simulating a schedule, at least not for the MVP, instead you're basically just allocating a ratio between your skills (which is given a sensible default when picking a character) and your character is ostensibly spending time training them on their own time.

The main difference between this and directly allocating skill points is that if your character is [convalescing](Health) or [traveling](Travel) they won't be able to train most of their skills. Not all skills are equal though in terms of how much training time they need to be effective, they all have their own falloff curve.

## Formula
```
# TODO: pain_penalty, morale_penalty


const LOWER_MUSCLE_MASS_PER_SPEED = 5
const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS = 30
# todo: should this be linear or nonlinear?
fn encumbrance_penalty(player):
	lower_muscle_mass = player.speed * LOWER_MUSCLE_MASS_PER_SPEED
	weight_capacity = WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS * lower_muscle_mass
	return 1 - ((player.body_weight + player.carry_weight) / weight_capacity)

const CALORIES_PER_ENDURANCE = 1000
const FATIGUE_EXPONENT = 5
fn fatigue_penalty(player):
	fatigue = player.calories_used_today / (player.endurance * CALORIES_PER_ENDURANCE)
	1 - fatigue^FATIGUE_EXPONENT

const MAX_CHECK = 5
fn skill_check(player, skill, focus_level):
	hours = player.hours_trained(skill)
	training = MAX_CHECK * (hours / (hours + skill.half()))
	(reflex, focus) = match skill.type():
		mental => (instinct, intelligence)
		physical => (agility, precision)
	attribute = reflex + focus * focus_level
	mut check = if skill.is_intuitive():
		(training + attribute)/2
	else:
		min(training, attribute)
	if skill.type() == physical:
		# armor penalty ranges from 0-0.4, with full-plate being 0.4
		if skill.is_upper_body():
			check *= 1 - player.upper_body_armor_penalty()
		else:
			check *= 1 - player.lower_body_armor_penalty()
		check *= player.encumbrance_penalty()
	return check
```

Each skill is represented in the stats window as a pair of horizontal bars. One represents the value as if focus_level was 0, the other as if it were 2. Each bar is measured in terms of hours trained, with non-equidistant ticks at each whole number value of the skill check. Any penalties such as from encumbrance, armor, or injuries are designated by a color on the bar, so that the white portion shows how much the skill currently is, while the colored portions show how much each penalty is affecting the final value.
## Focus
Intelligence and precision can only apply when a character can "focus", generally this refers to situations where you don't need to *react* to something or are not in combat. This bonus is not applied all at once after a fixed amount of time, rather it increases over time with diminishing returns (with an asymptote). Instinct and agility on the other hand apply all the time, though at only half of the maximum possible focus bonus.
## Mental
### Will (intuitive)
Ability to resist [pain](Combat) or avoid [morale](Morale) penalties.
0. Generalized anxiety disorder / panic disorder
1. Coward
2. Cautious, sensitive to pain
3. Professional soldier
4. Brave hero
5. Zen monk
### Charisma (intuitive)
There's no persuasion system or anything for the MVP, this is just a [morale](Morale) buff for the party. You lose focus during combat, so instinct gives you tactical morale while intelligence gives you traveling morale.
0. Autistic
1. Cold and aloof
2. Boring
3. Friendly
4. Funny
5. Professional bard
### Medicine
The MVP is not going to have a herbalism system or diseases, so whoever has the highest medicine skill simply gives a party-wide bonus to [health recovery speed](Health).
0. Provides no help to anyone injured
1. Knows to disinfect wounds with alcohol
2. Can treat common diseases (flu/cold)
3. Can treat some organ damage and uncommon diseases
4. Can treat most organ damage and rare diseases
5. Can treat all organ damage and all diseases
## Physical
### Melee (intuitive)
Agility helps you hit enemies that are actively trying to dodge or block your [attacks](Combat), precision helps you hit enemies that are unaware of you or staggered.
0. Has never been shown how to use a weapon or observed for an extended period of time
1. Can split firewood with an axe, zombies
2. Peasant levy, orcs, goblins
3. Professional soldier
4. Knight
5. Elven warrior
### Ranged (intuitive)
You have to aim at a target for awhile to get your precision bonus, whereas agility helps more with point-shooting
0. Never practiced even throwing a baseball
1. 
### Block (intuitive)
The larger your shield is, the less you rely on your block skill to use it effectively. A pavise requires almost none (though considerable strength), a buckler or weapon require high skill to use effectively. Precision doesn't give you a bonus to your defense when blocking, but does increase the amount of poise damage that actually does occur on a successful block (automatically turning it into a parry).
### Stealth (intuitive)
Agility reduces the noise that you make while moving, precision reduces the radius at which your party can be detected at when [traveling](Travel).
### Climb (intuitive)
### Balance (intuitive)
Relevant both for poise in melee and speed in difficult terrain
### Surgeon (intuitive)
The speed at which you bandage/splint [wounds](Health) and the healing rate, once-bandaged