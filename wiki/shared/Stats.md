# Attributes

The strategic interface represents attributes, skills, schedule activities,
condition metrics, Fervor, Morale, Age, Virtue, and Religion with recolourable
CSS masks. Most use locally vendored monochrome Game Icons; arm and leg Strength
and Agility plus Immunity retain the original strategic-interface artwork for
legibility at compact sizes. Labels and tooltips remain available to assistive
technology.
The maximum value of your characters' attributes is determined by their genetics, but the actual value may be quite a bit lower if they are not properly conditioned. For example, even if you have the theoretical ability to build a large amount of muscle, if you have poor nutrition or don't exercise then you will realize very little of it. Conditioning is different for each attribute, but generally no one will be able to condition all of their attributes to their maximum potential due to there only being 24 hours in a day.

Attributes are grouped between Chest/Stomach/Head/Limbs (L/R, A/L). Damage to one of these areas will affect all attributes within.

## Chest
### Endurance
Represents the strength of your heart, capacity of your lungs, and proportion of slow-twitch/fast-twitch muscle fiber. It determines how long you can go without suffering from exhaustion and how fast you move when traveling. Conditioned by traveling on foot.

0. Asphyxiated
1. Dainty sheltered nobles
2. City-folk
3. Knights and peasants
4. Professional soldiers
5. Adventuring heroes
6. Undead (cannot be tired)

## Stomach
### Immunity
This is essentially a combination of the liver, spleen, and other organs which regulate your immune system and ability to filter out toxins.

0. AIDS
1. Infants
2. Sheltered nobles and children
3. Rural commoners and knights
4. Elves and city-folk
5. Vampires (immune to disease)

### Gut
Your stomach, intestines, pancreas, and other organs involved with your digestive system. Determines how edible food needs to be in order for you to effectively digest it and how much variety you need to be decently healthy. Cooking makes food more edible, but food that is more fibrous and less nutritious can only be improved by so much.

0. Vampires (cannot digest food, must get calories directly from blood glucose)
1. Elves (can only eat meat, fat, and luxurious elven plants)
2. Nobles, orcs
3. Professional soldiers, goblins
4. Peasants (can survive almost entirely on grains without huge penalty)
5. Livestock

## Limbs
These attributes are separate among 4 limbs:
* Right arm
* Left arm
* Right leg
* Left leg
Every physical check will use some proportion of these. For example, swinging a sword in your right hand is largely dependent on your right arm, but your left arm is also being used for balance and your legs are helping put force into it. Your torso is also twisting to support this, but rather than being a separate limb, your torso is essentially a fuzzy mix of all limb attributes (mostly arms).

### Strength
Proportional to the total muscle mass of the limb. Arm-strength is important for attack damage, climb speed, and how well you keep your balance while blocking attacks. Leg-strength is important for movement speed and jump height.

0. Cripple
1. Child
2. Adult woman, pubescent boy
3. Adult man
4. Trained knight
5. Olympic athlete

### Agility
The speed of your muscular reflexes and your ability to control them. Arm-agility is important for accuracy and parrying, leg-agility is important for stealth and dodging.

0. Paralyzed, unaware, or tied up
1. Drunken oaf, orcs, zombies
2. Clumsy, goblins, skeletons
3. Professional soldiers and knights
4. Heroes, surgeons, locksmiths
5. Elven heroes

## Head
In theory eyesight/hearing should be further subdivided into eyes/ears for damage purposes, while intelligence and instinct are brain. In fact, ask a neurologist but intelligence/instinct would be correlated with different physical locations in the brain. But this is fine for now, we do not need infinite detail for the MVP.

### Intelligence
The depth at which your character can think at. Applies a bonus to mental skills. In order to use your intelligence bonus, a skill has to give you time to think about the problem.

0. Not capable of conscious thought
1. Low-functioning autistic, toddler
2. Would struggle to learn even basic math
3. Can learn high-school-level math
4. Can learn college-level math
5. Can meaningfully contribute to the field of mathematics

### Instinct
Your ability to make snap judgements without thinking. A character with high instinct makes for a good small group leader in battle, where making a quick, decent decision is more important than making an ideal decision but taking awhile to come to it. Applies a bonus to your mental skills. The bonus does not require focus, but is lower than an equal bonus from intelligence.

0. Unconscious
1. Takes a couple seconds to respond if you ask them a question
2. Absentminded
3. Alert
4. Veteran captain
5. Enlightened monk

### Eyesight
0. Blind
1. Needs glasses, many fantasy enemies like goblins or zombies
2. Below average human
3. Above average human
4. Elven warrior
5. Hawk, elven archer

### Hearing
0. Deaf
1. Muffled, many fantasy enemies like goblins or zombies
2. Attended too many rock concerts
3. Has never been to a rock concert
4. Deer
5. Blind monk

# Skills
Skills are divided into two categories: mental and physical. The former is governed by intelligence and instinct, the latter by agility. Physical skills are 
## Training
Skills increase on a much longer timescale than is conventional for RPGs, they are not increased via an abstract XP/leveling system and very little of their value comes from actually using them during gameplay. Instead they are *trained* during your character's off-screen settlement-downtime schedule, which allocates 1,440 minutes among explicit skill study, activities, and leisure. Travel never applies scheduled training or activities. Activities currently include Prayer, Labor, Thievery, and Raiding; they train related skills at 25% speed while also producing morale, gold, notoriety, or encounter risk where the character's context permits it. Their rows preview the signed Gold, Virtue, Morale, and Fatigue generated per day by the current allocation; notoriety-producing activities display that cost as negative Virtue. Leisure is the remaining allocation and includes sleep.

An ordinary day generates 600 fatigue-reservoir units before tiring activities. Leisure removes 100 units per hour, so six hours exactly offsets ordinary wakefulness. Labor adds another 50 units per hour. Leisure beyond six hours first removes activity fatigue, then fatigue carried into the interval; only the portion of the interval after the reservoir reaches zero earns morale, approaching 4 points per full qualifying day with a 200-unit diminishing-return scale. The schedule displays a one-day preview, but the server awards the result proportionally to the settlement-downtime time actually applied. Earned Leisure morale is kept as one refreshable source capped at 4 points, rather than being projected from the post-rest schedule or stacked into separate events. It decays at a fixed rate when no qualifying Leisure is occurring; qualifying Leisure refreshes it while adding the newly earned amount. This makes the result independent of whether downtime is applied all at once or through frequent synchronization. The compact schedule preview shows one Fatigue point per 100 reservoir units: Labor therefore shows `+0.5` per hour, while Leisure includes baseline and recovery so all visible Fatigue rows sum to the authoritative net change. Positive preview values remain green and negative values red, including negative Fatigue values that represent recovery.

The rank meter is a five-segment display using the same yellow-green, yellow, orange, red, and violet progression as equipment repair difficulty. Daily allocations are changed in 15-minute steps with the left/right buttons or mouse wheel. Clicking a displayed allocation opens a time field. It accepts `h` or `hh` as whole hours, `h:mm` or `hh:mm`, and compact three- or four-digit times such as `830` or `0830`; entered values snap to the nearest 15 minutes and may not exceed `24:00`. The underlying schedule stores minutes, and the Leisure allocation shows the unallocated remainder. The editor updates these values immediately, serializes background saves, and reconciles with the server after the latest change is saved so live updates cannot momentarily restore an older plan. A failed save leaves the optimistic plan visible and presents a Retry action; making another edit also retries using the newest plan. Compact column icons label Currency (`💎`), Virtue (`⚖️`), Morale (`🙂`), Fatigue (`💤`), and daily allocation (`⌛`); each icon exposes the same label to assistive technology.

The main difference between this and directly allocating skill points is that if your character is [convalescing](Health.md) or [traveling](../strategic/Travel.md) they cannot train. Not all skills are equal though in terms of how much training time they need to be effective, they all have their own falloff curve. The number in the parentheses next to a listed skill here is the number of hours that it takes for it to be 50% effective. The rate of increase from training is lower the higher they get, providing an upper-asymptote for how skilled a character can be in a particular skill. Additionally, skills atrophy with disuse, so even an immortal elf or vampire cannot become optimal at everything (though they may get close) due to there being only so many hours in a day.
## Intuitive vs Trained
Intuitive skills can be attempted without training, the check is an average between their associated attribute and the training rank. Trained skills on the other hand receive no benefit without actual training regardless of how high their associated attribute is, the training value is a ceiling. Most skills relevant to the MVP happen to be intuitive.

## Formula
```rs
# TODO: pain_penalty, morale_penalty

struct LimbWeights {
  left_arm: f32,
  right_arm: f32,
  left_leg: f32,
  right_leg: f32
}

impl LimbWeights {
  fn with_side(self, side) {
    match side {
      Side::Left => self,
      Side::Right => Self {
        left_arm: self.right_arm,
        right_arm: self.left_arm,
        left_leg: self.left_leg,
        right_leg: self.right_leg
      }
    }
  }
}


const CALORIES_PER_ENDURANCE = 1000
const FATIGUE_EXPONENT = 5
fn fatigue_penalty(player):
	fatigue = player.calories_used_today / (player.endurance * CALORIES_PER_ENDURANCE)
	1 - fatigue^FATIGUE_EXPONENT

const MAX_CHECK = 5
fn skill_check(character, skill, focus_level, limb_weights: LimbWeights):
	hours = character.hours_trained(skill)
	training = MAX_CHECK * (hours / (hours + skill.half()))
	(reflex_attribute, focus_attribute) = match skill.type():
		mental => (Attribute::Instinct, Attribute::Intelligence)
		physical => (Attribute::Agility, Attribute::Precision)
		
	let (reflex, focus) = if skill.type() == physical:
		(
			// this iterates through the 4 limb weights and
			// multiplies each attribute by the corresponding weight
			// then sums them up.
			// the weights should always total 1.0
			character.limbs.get_weighted_attribute(reflex_attribute, limb_weights)
			character.limbs.get_weighted_attribute(focus_attribute, limb_weights)
		)
	else {
		(
			character.mental[reflex_attribute],
			character.mental[focus_attribute],
		)
	}
		
	attribute_check = reflex + focus * focus_level
	mut check = if skill.is_intuitive():
		(training + attribute_check)/2
	else:
		min(training, attribute_check)
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

## Mental
### Will (intuitive, 5000 hours)
Ability to resist [pain](../tactical/Combat.md) or avoid [morale](Morale.md) penalties.
0. Generalized anxiety disorder / panic disorder
1. Coward
2. Cautious, sensitive to pain
3. Professional soldier
4. Brave hero
5. Zen monk

### Charisma (intuitive, 20000 hours)
There's no persuasion system or anything for the MVP, this is just a [morale](Morale.md) buff for the party. You lose focus during combat, so instinct gives you tactical morale while intelligence gives you traveling morale.

Party Charisma is led by the strongest individual check. Additional members receive a saturating coordination benefit, then contribute half of their deviation from a 2.5 baseline. Checks above 2.5 help and checks below 2.5 burden the party's social leadership. The result is capped from 0 to 5. One 4.5, three 3s, and a 4 paired with a 2 each produce approximately 4.5; adding arbitrarily many low-Charisma members cannot manufacture a high result.

0. Autistic
1. Cold and aloof
2. Boring
3. Friendly
4. Funny
5. Professional bard

### Medicine (trained, 10000 hours)
Medicine governs what a particular character can discern about illness. Below
an effective individual check of 2 no examination action appears, even for
oneself. Everyone can perceive where the body feels impaired, but the lay
health bars collapse non-cut and non-blunt causes into green. At 2, a physician
can spend 15 minutes examining a co-located patient; the resulting one-shot
view splits that green impairment per body region into the four humours. Each disease has a stage-dependent
diagnosis difficulty (never below 2); meeting it reveals a period-facing
diagnosis and permits a quoted gold-and-time treatment. Cuts and blunt trauma
remain distinct without Medicine; all other sources, including burns, use the
green lay category. Party Medicine still assists wound recovery, but party aggregation
never grants medical visibility or diagnosis.

Medicine and Surgery use the same bounded party-check equation. Individual checks are sorted strongest-first, the leader receives full weight, and successive contributors receive weights of `1/2`, `1/4`, `1/8`, and so on:

\[
P = 5\left(1-\prod_{i=1}^{n}\left(1-\frac{x_i}{5}\right)^{(1/2)^{i-1}}\right)
\]

A solo character retains their exact individual check. The result never exceeds 5 and needs no final clamp. Because all supporting weights together equal the leader's weight, arbitrarily many equally skilled supporters can add at most the influence of one additional copy of the leader: Medicine 1 approaches 1.8, Medicine 2 approaches 3.2, Medicine 3 approaches 4.2, and Medicine 4 approaches 4.8.

0. Provides no help to anyone injured
1. Knows to disinfect wounds with alcohol
2. Can treat common diseases (flu/cold)
3. Can treat some organ damage and uncommon diseases
4. Can treat most organ damage and rare diseases
5. Can treat all organ damage and all diseases

### Religion (trained, 5000 hours per tradition)
Religion represents knowledge, not conviction. It includes Roman Catholicism, Lutheranism, Reformed Christianity, Anglicanism, Eastern Orthodoxy, Islam, and Judaism. Canonical state records only hours directly studied in each tradition. Effective hours for a tradition are derived once by multiplying those direct hours by the symmetric correlation matrix; derived hours are never stored or recursively correlated. In the skill rail, the collapsed row is the tradition Auto-train would currently select: the character's professed religion, or otherwise the settlement's church. Expanding it shows only the other traditions in which the character has directly studied more than zero hours, as blue-purple icons rather than repeating the selected one. Each meter's hover text reports effective and directly studied hours. With Auto-train enabled, only the collapsed automatic budget is editable and manual allocations are muted; disabling it enables direct per-tradition allocation, with the selected tradition remaining available in the collapsed row.

The diagonal is 1.0. The upper-triangle correlations in stable order (Roman Catholic, Lutheran, Reformed, Anglican, Eastern Orthodox, Islam, Judaism) are: RC to the remaining traditions `0.80, 0.75, 0.80, 0.65, 0.10, 0.10`; Lutheran `0.90, 0.85, 0.50, 0.10, 0.10`; Reformed `0.85, 0.45, 0.10, 0.10`; Anglican `0.55, 0.10, 0.10`; Eastern Orthodox `0.15, 0.10`; and Islam to Judaism `0.35`.

A party's check for a particular religion includes every living member's effective knowledge of that tradition, regardless of what they personally profess. This permits a knowledgeable nonbeliever or member of another religion to lead prayers and sermons. The generic recruitment summary uses the character's maximum effective Religion check as a UI-only measure of coverage; authoritative morale and prayer always select the relevant tradition.

Conviction lives on the personality axis instead: Zealous contributes 5.0 pressure, Neutral 2.5, and Irreverent 0.0. A profession and conviction are separate; an Irreverent character may still officially profess a religion.

## Physical
### Melee (intuitive, 8000 hours)
Agility helps you hit enemies that are actively trying to dodge or block your [attacks](../tactical/Combat.md), precision helps you hit enemies that are unaware of you or staggered.

0. Has never been shown how to use a weapon or observed for an extended period of time
1. Can split firewood with an axe, zombies
2. Peasant levy, orcs, goblins
3. Professional soldier
4. Knight
5. Elven warrior

### Ranged (intuitive, 15000 hours)
You have to aim at a target for awhile to get your precision bonus, whereas agility helps more with point-shooting

0. Never practiced even throwing a baseball
1. Orcs, untrained peasants
2. Goblins, militia
3. Professional soldiers
4. Huntsmen
5. Elf rangers

### Dodge (intuitive, 20000 hours)

0. Zombies
1. Drunk or very old people, orcs
2. Peasant levy, goblins
3. Professional soldier
4. Knight
5. Elven warrior
### Block (intuitive, 12000 hours)
The larger your shield is, the less you rely on your block skill to use it effectively. A pavise requires almost none (though considerable strength), a buckler or weapon require high skill to use effectively. Precision doesn't give you a bonus to your defense when blocking, but does increase the amount of poise damage that actually does occur on a successful block (automatically turning it into a parry).

0. Never been in a fight
1. Has been in some barfights
2. Fresh recruit
3. Professional soldier
4. Knights, heroes
5. Elven swordmasters

### Stealth (intuitive, 8000 hours)
Agility reduces the noise that you make while moving, precision reduces the radius at which your party can be detected at when [traveling](../strategic/Travel.md).

0. Has never even attempted to steal cookies from the cookie jar
1. Most people
2. Can tiptoe around the house in socks without waking anyone, usually
3. Novice hunter, professional mercenary trained in ambush tactics
4. Practiced thief, veteran hunter
5. Master thief, elven hunter

### Balance (intuitive, 30000 hours)
Relevant both for poise in melee and speed in difficult terrain

0. Cannot walk upright
1. Bit of a klutz, orcs
2. Can walk in high-heels, can dance in normal shoes, professional soldier
3. Can run and dance in high heels, amateur gymnast
4. Skilled gymnast or martial artist, can walk a tightrope
5. Graceful elf

### Surgeon (trained, 10000 hours)
The party's bounded aggregate Surgery check uses the same geometric-support equation as Medicine and stabilizes fresh [autoresolve wounds](Health.md) after battle. Every 5% of wound damage requires 1 point of Surgery; missing the target worsens the wound in proportion to the shortfall, while meeting it prevents deterioration without undoing the original damage.

0. Cannot reliably apply a bandage
1. Can dress a wound or apply a tourniquet
2. Can stitch skin, probably shouldn't
3. Can remove a bullet, good at stitching skin
4. Can remove an appendix or stitch an organ
5. Brain/heart surgeries
