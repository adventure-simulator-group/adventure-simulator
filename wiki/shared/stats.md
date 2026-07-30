# Attributes

Herbalism is a first-class trained mental skill governed by Intelligence. It
prepares biological medicine and remains separate from patient-facing
Physiology and food-facing Cooking.

The Terrain family includes **Snow**, a mental, intuitive,
Intelligence-governed skill with a 30,000-hour curve. Snow has symmetric 0.20
ordinary correlation with Plains, Forest, Hills, Wetlands, and Urban. It is an
overlay skill: snow-covered forest still uses and trains Forest while Snow
blends into the check. Cover conservatively splits the existing
road-discounted exposure between Snow and the underlying biome.

The strategic interface represents attributes, skills, schedule activities,
condition metrics, Fervor, Morale, Age, settlement Fame/Infamy, and Religion with recolourable
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
The depth at which your character can think. Intelligence governs learning and
mastery for Physiology, Surgery, Cooking, Religion, Bestiary, and the Terrain
leaves. It does not add to their final checks.

0. Not capable of conscious thought
1. Low-functioning autistic, toddler
2. Would struggle to learn even basic math
3. Can learn high-school-level math
4. Can learn college-level math
5. Can meaningfully contribute to the field of mathematics

### Instinct
Your ability to make snap judgements without thinking. Instinct governs
learning and mastery for Will, Insight, Charm, Command, and Deception. It does
not add to their final checks.

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

## Book study

Written language, Religion, and Bestiary leaves can be studied from books
through rank 5. Physiology and Herbalism books stop at rank 4. Terrain,
Surgery, Cooking, Tailoring, Smithing, Command, and Charm books stop at rank 2.
Melee, ranged, Defense, and Stealth manuals only provide the 0→1 introduction.
Advancement above the bounded practical and embodied bands is reserved for
metis rather than book study.

Each title has the same 1–5 quality used by other items. Quality `N` teaches
the adjacent `N-1→N` rank band: the lower rank is a hard prerequisite and
progress clips at the quality rank. The inventory renders book names with the
existing quality colors, while the item-type column uses the icon of the exact
skill leaf the book trains. A readable medium requires rank 1 in that Written
language. Reading converts real hours to effective target hours at `written
rank / 5`, without applying the target aptitude's learning-speed multiplier
again. Aptitude still caps the effective target rank. Terrain is governed by
Instinct; its correlations and exposure sources are unchanged.
Every skill has exactly one governing aptitude: Intelligence, Instinct, or
Agility. Aptitude controls training speed and the effective-rank limit; trained
skill rank supplies the check itself.
## Training
Skills increase on a much longer timescale than is conventional for RPGs. They are not increased via an abstract XP/leveling system, and very little of their value comes from using them during tactical play. Instead they are trained through activities in the character's off-screen settlement-downtime schedule. Individual skill-study allocations are not available.

Combat Training practices the leaf skills relevant to the equipped weapons plus Dodge, Block, Balance, and Will; it includes both sparring and target practice. Carousing trains Charm and improves Morale; only a disorder incident adds Infamy. Prayer, Labor, Thievery, and Raiding retain their related training and strategic results when available at the character's current location. The saved daily allocation remains globally editable and unchanged when moving. At execution, every unavailable 15-minute segment is reassigned to one of the character's other available planned activities, weighted by those activities' saved allocations; it becomes Leisure only when that pool is empty. Profession activities cover Physiology, Surgery, Knife, Tailoring, Smithing, Command, and knowledge of the settlement church's religious tradition. An activity conserves its training time when it covers several skills rather than awarding the full allocation to every skill. Travel never performs scheduled settlement activities. Activity rows preview signed Gold, Fame/Infamy, Morale, and Fatigue generated per day. Leisure is the unallocated remainder and includes sleep.

Selecting an explicit activity icon previews and performs one continuous one-to-24-hour interval using the same training and outcome rules. Its preview is based on the chosen duration; Prayer/Meditation and Carousing are nonlinear, so increasing their duration has diminishing Morale returns. Immediate activity never includes implicit Leisure or modifies the recurring allocation.

A character may join multiple YAML-defined organizations. Each organization
chooses its own name, chapters, recognition, admission fee, recurring dues,
rank names and requirements, curriculum, rewards, and privileges. Requirements
may freely mix skills and professed religion; skills never imply membership. A
character can present as exactly one active, dues-current organization at a
time (or none). Presentation controls recognized privileges such as bearing
arms or wearing armor where settlement policy would otherwise forbid them.

Organization training and professional activity are available through the
schedule while the character is at a chapter. Rank advancement follows the
next rank's YAML requirements rather than universal
apprentice/journeyman/master thresholds. Skills with no invested training hours
remain omitted until training first awards hours.

An ordinary day generates 600 fatigue-reservoir units before tiring activities. Leisure removes 100 units per hour, so six hours exactly offsets ordinary wakefulness. Labor adds another 50 units per hour. Leisure beyond six hours first removes activity fatigue, then fatigue carried into the interval; only the portion of the interval after the reservoir reaches zero earns morale, approaching 4 points per full qualifying day with a 200-unit diminishing-return scale. The schedule displays a one-day preview, but the server awards the result proportionally to the settlement-downtime time actually applied. Earned Leisure morale is kept as one refreshable source capped at 4 points, rather than being projected from the post-rest schedule or stacked into separate events. It decays at a fixed rate when no qualifying Leisure is occurring; qualifying Leisure refreshes it while adding the newly earned amount. This makes the result independent of whether downtime is applied all at once or through frequent synchronization. The compact schedule preview shows one Fatigue point per 100 reservoir units: Labor therefore shows `+0.5` per hour, while Leisure includes baseline and recovery so all visible Fatigue rows sum to the authoritative net change. Positive preview values remain green and negative values red, including negative Fatigue values that represent recovery.

The rank meter is a five-segment display using the same yellow-green, yellow, orange, red, and violet progression as equipment repair difficulty. Daily allocations are changed in 15-minute steps with the left/right buttons or mouse wheel. Clicking a displayed allocation opens a time field. It accepts `h` or `hh` as whole hours, `h:mm` or `hh:mm`, and compact three- or four-digit times such as `830` or `0830`; entered values snap to the nearest 15 minutes and may not exceed `24:00`. The underlying schedule stores minutes, and the Leisure allocation shows the unallocated remainder. The editor updates these values immediately, serializes background saves, and reconciles with the server after the latest change is saved so live updates cannot momentarily restore an older plan. A failed save leaves the optimistic plan visible and presents a Retry action; making another edit also retries using the newest plan. Compact column icons label Currency (`💎`), Fame/Infamy (`⚖️`), Morale (`🙂`), Fatigue (`💤`), and daily allocation (`⌛`); each icon exposes the same label to assistive technology.

Character summaries use that same five-color rank progression on compact,
keyboard-focusable icons. Equipped hands contribute one icon for every unique
weapon leaf they exercise, including every leaf of a hybrid weapon. Armor
contributes one silhouette icon for the highest equipped coverage tier,
including an outlined unarmored silhouette at zero coverage, and uses the
stronger healthy Dodge or Block rank for its color. The quarter, half,
three-quarter, and full silhouettes progressively fill the body regions
protected by that armor. These combat and armor icons always precede
non-combat skills.

An exact non-combat skill appears in the summary at healthy rank 3 or higher.
Expandable families remain one icon: Social and context-free Terrain use their
means, Oral and Written languages use their strongest effective language and
that language's displayed identity, Religion uses the same contextual primary
tradition (or strongest-effective fallback) as its rail, and Bestiary uses its
aggregate effective coverage. Family tooltips list only qualifying leaves and
their exact ranks. Standalone skills use their own icon and rank. The visible
color is supplementary: every icon exposes its identity and score through the
shared instant tooltip and accessible name.

The main difference between this and directly allocating skill points is that if your character is [convalescing](health.md) or [traveling](../strategic/travel.md) they cannot train. Not all skills are equal though in terms of how much training time they need to be effective, they all have their own falloff curve. The number in parentheses next to a listed skill is its asymptotic training calibration; half that many effective hours produces rank 2.5. The rate of increase from training is lower the higher they get, providing an upper asymptote for skill rank.

Real training time is converted to effective learned hours by the governing
aptitude:

```rs
training_multiplier = max(0, 1 + 0.5 * (aptitude - 2.5))
```

Thus aptitude 0/1/2.5/4/5 learns at 0×/0.25×/1×/1.75×/2.25×.
An activity first conserves and divides its real-hour budget, then applies each
target skill's multiplier. Healthy conditioned aptitude, before injury,
determines both this multiplier and the maximum effective rank. Stored hours
above a lowered cap remain latent and become effective again if aptitude
returns. Effective gain that crosses or exceeds the cap is rejected exactly at
the boundary and feeds one shared, saturating **Mastery enjoyment** morale
source. Forty excess effective hours reaches about 63% of its four-point limit.
All rejected gains in one logical interval are combined before saturation; the
existing enjoyment first decays linearly through the interval, then the combined
award refreshes it at the endpoint. It reaches zero after seven days without
another award. Aptitude zero earns neither effective hours nor mastery morale.

The skill rail has three computed combat groups: **Melee**, **Ranged**, and **Defense**. They have no stored hours and are never used directly for a tactical check. Melee expands to Polearm, Axe, Bludgeon, Sword, and Knife; Ranged expands to Bow, Crossbow, Firearm, and Throw; Defense expands to Dodge, Block, Balance, and Will. Equipped weapon distributions determine the relevant weapon leaves. A shield gives Block full relevance; without one, the best-balanced equipped melee weapon gives Block a weight of `1 - balance`. Combat Training and Raiding divide their conserved activity award deterministically across those relevance weights.

Every weapon stores a nine-field skill distribution. A halberd uses Polearm, Axe, and Bludgeon equally; a glaive uses Polearm and Sword; short swords and daggers use Sword and Knife; a hand axe uses Axe and Knife. An attack averages the complete leaf-skill checks using those weights, including each check's attributes and penalties. Knife means short weapons rather than only literal knives.
## Intuitive vs Trained
This distinction applies only to correlated training. An intuitive target may
benefit from correlated hours without formal training in that target. A trained
target evaluates to zero until it has target-specific direct hours, regardless
of correlated knowledge. Correlation is derived in one pass, never stored, and
never produces mastery morale. Physiology, Surgery, Religion and Bestiary
leaves, and Written languages are trained; Oral languages are intuitive.

Ordinary skill transfer uses a deliberately sparse symmetric matrix: Cooking
and Knife transfer at 0.15; Sword and Knife, Dodge and Balance, and every pair
of Terrain leaves transfer at 0.20. Only direct hours enter this one pass. A
trained target receives at most as many transferred hours as it has direct
hours: zero remains zero and an introductory lesson cannot unlock a lifetime
of related experience all at once. Intuitive Terrain leaves do not use this
direct-study cap. Skill rails show direct, correlated, and resulting effective
hours separately in their tooltips. The meter's background extent is the
uncapped rank projected from effective hours, including correlation; it is not
a separate direct-hours layer. The brighter foreground remains the
aptitude- and injury-limited effective check.

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
fn skill_check(character, skill, limb_weights: LimbWeights):
	hours = character.hours_trained(skill)
	mut check = min(
		MAX_CHECK * (hours / (hours + skill.half())),
		character.healthy_governing_aptitude(skill),
	)
	check *= character.injury_usability(skill, limb_weights)
	if skill.type() == physical:
		# armor penalty ranges from 0-0.4, with full-plate being 0.4
		if skill.is_upper_body():
			check *= 1 - player.upper_body_armor_penalty()
		else:
			check *= 1 - player.lower_body_armor_penalty()
		check *= player.encumbrance_penalty()
	return check
```

Each skill is represented in the stats window with its uncapped rank projected from effective hours (direct plus correlated) behind its current aptitude- and injury-limited rank. Hover text reports the direct hours, correlated contribution, resulting effective hours, and governing aptitude. Penalties such as encumbrance, armor, or injuries reduce only the current effective portion.

## Mental
### Will (intuitive, 5000 hours)
Ability to resist [pain](../tactical/combat.md) or avoid [morale](morale.md) penalties.
0. Generalized anxiety disorder / panic disorder
1. Coward
2. Cautious, sensitive to pain
3. Professional soldier
4. Brave hero
5. Zen monk

### Social skills (intuitive)
There's no persuasion system or anything for the MVP, this is primarily a
[morale](morale.md) and relationship system. All current Social leaves are
governed by Instinct.

Insight reads others and oneself, Charm powers both humor and compatible flirtation, Command rallies and coordinates, and Deception sustains false impressions. Joke and Flirt remain separate morale actions: Grave actors cannot Joke and Proper actors cannot Flirt, while each of those reserved traits adds 0.35 to Rally Command. Party Command is led by the strongest individual check. Additional members receive a saturating coordination benefit, then contribute half of their deviation from a 2.5 baseline. Checks above 2.5 help and checks below 2.5 burden the party's social leadership. The result is capped from 0 to 5; adding arbitrarily many low-Command members cannot manufacture a high result. Character sheets summarize these four skills with an expandable Social meta-skill whose rank is their average.

0. Autistic
1. Cold and aloof
2. Boring
3. Friendly
4. Funny
5. Professional bard

### Physiology (trained, 10000 hours)
Physiology governs what a particular character can discern about illness.
Observation is passive while characters share a party and location; there is
no separate examination action. The observer's individual capability band
sets notebook cadence, symptom recognition, localization, confidence, and
awareness of interventions. When capability crosses a band boundary, a new
presence span begins so earlier notes retain the capability that produced
them. Cuts and blunt trauma remain visible without Physiology; private meter
causes are disclosed only through the deliberately many-to-one four-humour
projection. The chart never names a disease or recommends a treatment.

Characters administer a preparation they possess by checking its personal-
inventory checkbox. The trusted server consumes that concrete item and starts
one standard 1,000-milliunit whole-body course through the preparation's
intrinsic route. A compact current-medication status allows an active course to
be stopped and disappears after a stop or natural expiry. These actions operate
on generic, versioned physiology profiles rather than disease-keyed cures.

Physiology uses a bounded party-check equation. Individual checks are sorted strongest-first, the leader receives full weight, and successive contributors receive weights of `1/2`, `1/4`, `1/8`, and so on:

\[
P = 5\left(1-\prod_{i=1}^{n}\left(1-\frac{x_i}{5}\right)^{(1/2)^{i-1}}\right)
\]

A solo character retains their exact individual check. The result never exceeds 5 and needs no final clamp. Because all supporting weights together equal the leader's weight, arbitrarily many equally skilled supporters can add at most the influence of one additional copy of the leader: Physiology 1 approaches 1.8, Physiology 2 approaches 3.2, Physiology 3 approaches 4.2, and Physiology 4 approaches 4.8.

0. Provides no help to anyone injured
1. Knows to disinfect wounds with alcohol
2. Can treat common diseases (flu/cold)
3. Can treat some organ damage and uncommon diseases
4. Can treat most organ damage and rare diseases
5. Can treat all organ damage and all diseases

### Religion (trained, 5000 hours per tradition)
Religion represents knowledge, not conviction. It includes Roman Catholicism, Lutheranism, Reformed Christianity, Anglicanism, Eastern Orthodoxy, Islam, and Judaism. Canonical state records only hours learned in each tradition. Effective hours for a tradition are derived once by multiplying those hours by the symmetric correlation matrix; derived hours are never stored or recursively correlated. Prayer activity teaches the character's professed tradition. The skill rail can expand a primary tradition to show other traditions with nonzero direct knowledge, and each meter's hover text reports effective and directly learned hours.

The diagonal is 1.0. The upper-triangle correlations in stable order (Roman Catholic, Lutheran, Reformed, Anglican, Eastern Orthodox, Islam, Judaism) are: RC to the remaining traditions `0.80, 0.75, 0.80, 0.65, 0.10, 0.10`; Lutheran `0.90, 0.85, 0.50, 0.10, 0.10`; Reformed `0.85, 0.45, 0.10, 0.10`; Anglican `0.55, 0.10, 0.10`; Eastern Orthodox `0.15, 0.10`; and Islam to Judaism `0.35`.

A party's check for a particular religion includes every living member's effective knowledge of that tradition, regardless of what they personally profess. This permits a knowledgeable nonbeliever or member of another religion to lead prayers and sermons. The generic recruitment summary uses the character's maximum effective Religion check as a UI-only measure of coverage; authoritative morale and prayer always select the relevant tradition.

Conviction lives on the personality axis instead: Zealous contributes 5.0 pressure, Neutral 2.5, and Irreverent 0.0. A profession and conviction are separate; an Irreverent character may still officially profess a religion.

### Bestiary (trained, 5000 hours per category)

Bestiary is a meta-skill, following the Religion model, whose leaf skills
represent learned physical knowledge of Beast, Undead, Human,
Werekin, Elf, Dwarf, Fey, Spirit, Greenskin, Insectoid, Draconid, Construct,
and Wildmen creatures. A creature has one main type and may have several
secondary types. Skeletons and ghouls are primarily Undead with Human as a
secondary anatomical type; werewolves are primarily Werekin with Human and
Beast secondary types. One evidence result always evaluates exactly one
category. Transformed animal tracks can support Werekin without identifying
whether the host is Human, Elf, or Dwarf.

Direct category hours are canonical. Effective hours use one symmetric,
nonrecursive correlation pass after the target category has received direct
study. Wildmen knowledge correlates strongly with Human knowledge and
more modestly with Fey knowledge. The expandable skill rail shows effective
and directly studied hours for every category with transferred knowledge.
The parent Bestiary value is the mean effective coverage across every category,
not the character's single best category. A question-mark cursor marks category
icons as inspectable. Their hover/focus tooltips separate creatures for which
the category is the main type from creatures for which it is a secondary type,
and clicking pins the tooltip. Hovering or focusing an enemy type then shows
only strengths and weaknesses derived from mechanics currently consumed by
combat. Each strength appears on its own green line and each weakness on its
own red line; no category-wide generalizations or unimplemented folklore are
shown.

Against a creature, the attacker averages the Bestiary checks for every
category on that creature. Excess-accuracy damage is capped at 2× plus that
average check (up to 7×); even an untrained character retains the ordinary 2×
head-and-throat cap.

Physical evidence first requires its ordinary inspection check. The inspecting
character then makes hidden category-specific Bestiary checks for relevant
authored implications. One canonical record keeps the original physical
observation. Returning after later study may add newly recognized categories,
but never removes or duplicates an existing result. Results also remain in the
investigation journal after leaving the evidence site. No scheduled Bestiary
training activity is currently implemented.

## Physical
### Polearm, Axe, Bludgeon, Sword, and Knife (intuitive, 8000 hours)
These are the five melee weapon leaves. Their fixed training aptitude is the
average healthy Agility of both arms. Hybrid weapons use a weighted average of
all tagged leaves. Knife covers short weapons, including daggers, short swords,
hand axes, and compact butchery tools.

0. Has never been shown how to use a weapon or observed for an extended period of time
1. Can split firewood with an axe, zombies
2. Peasant levy, orcs, goblins
3. Professional soldier
4. Knight
5. Elven warrior

### Bow, Crossbow, Firearm, and Throw (intuitive, 15000 hours)
These are the four ranged weapon leaves. Their fixed training aptitude is the
average healthy Agility of both arms. Input precision remains a future
client-to-combat signal, while weapon accuracy remains an equipment statistic;
neither is a character attribute. Hybrid or throwable weapons may use more than
one leaf.

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
The larger your shield is, the less you rely on your block skill to use it effectively. A pavise requires almost none (though considerable strength), a buckler or weapon require high skill to use effectively.

0. Never been in a fight
1. Has been in some barfights
2. Fresh recruit
3. Professional soldier
4. Knights, heroes
5. Elven swordmasters

### Stealth (intuitive, 8000 hours)
Stealth uses the average healthy Agility of all four limbs for training speed
and mastery. Injury still penalizes current performance.

0. Has never even attempted to steal cookies from the cookie jar
1. Most people
2. Can tiptoe around the house in socks without waking anyone, usually
3. Novice hunter, professional mercenary trained in ambush tactics
4. Practiced thief, veteran hunter
5. Master thief, elven hunter

### Balance (intuitive, 30000 hours)
Relevant for poise in melee. Strategic terrain speed is handled by the
separate Terrain meta-skill and does not stack with Balance.

0. Cannot walk upright
1. Bit of a klutz, orcs
2. Can walk in high-heels, can dance in normal shoes, professional soldier
3. Can run and dance in high heels, amateur gymnast
4. Skilled gymnast or martial artist, can walk a tightrope
5. Graceful elf

### Surgery (mental, trained, 10000 hours)
Surgery represents trained operative wound care. Intelligence governs
its training speed and mastery cap, while head injury remains a performance
penalty. The Fellowship of Herbalists trains Herbalism; the College of
Physicians trains Physiology; and the Surgeons' Guild trains Surgery.

Projectile extraction, stitching, bandaging, and splinting all check Surgery
directly. Self-treatment applies the shared 2.5-point penalty. Surgery has
modest symmetric correlations with Knife and Tailoring, allowing related craft
practice to contribute indirectly without making either a procedure input.

### Terrain (computed meta-skill; intuitive subskills, 30000 hours each)

Terrain stores no hours of its own. Its expandable subskills are Plains,
Forest, Hills, Wetlands, and Urban. Each is an intuitive, Intelligence-governed
mental skill. A route cell supplies a normalized mixture, and the
displayed route/local Terrain value is the weighted combination of those
subskills; without context the character rail shows their unweighted mean.
Urban is stored and displayed now but has zero routing weight until the world
pipeline has an authoritative urban-coverage source. Roads exercise the
underlying terrain rather than Urban and reduce training in proportion to the
time they save.

### Tailoring (trained, 10000 hours)
Tailoring makes and repairs cloth goods. Settlement tailors and field maintenance use it for clothing durability. It is modestly correlated with Surgery but is not a direct stitching check.

### Smithing (trained, 10000 hours)
Smithing makes and repairs weapons, armor, and shields. It does not repair clothing.

### Languages

Oral and Written are expandable skill families rather than generic leaf `Skill` values. Oral includes East-central, West-central, Low, Yiddish, Latin, Romani, Elven, and Dwarfish; Written includes German chancery, Low, Latin, Hebrew, Yiddish, Elven, and Dwarfish. Direct hours are authoritative. Effective hours are a one-pass symmetric correlation, following the Religion model. A pair uses the language with the highest shared coefficient, with stable enum order breaking ties.
