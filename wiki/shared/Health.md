Damage and recuperation are based on real values, but if you did this in basically any other RPG it would become extremely boring and punishing. However, in our case [combat](Combat) is designed around the assumption that players will reliably be able to avoid taking damage, either by dodging, blocking, or it being absorbed by armor, so this isn't a completely unreasonable target.

However, inevitably damage will occur, and to prevent this from being a fun-killer we must use the two [approaches](Meta) to skip the tedium:
# Abstraction-based Approach
The first of which is that even if it takes a very long time to heal from injuries, we can simply [skip ahead](Time) until your character is healed. This only works well when resting at [settlements](Settlement) though. If your party is mid-quest and you take a serious injury, you're either going to have to call it off or fight with a handicap. Unless...
# Content-based Approach
The other "approach" is to use the fantasy elements to circumvent this. For example, we can say that fey-blood provides supernatural healing for [elves](Character) or create a reasonably cost-effective sort of rapid healing potion that only works on them, such that they might be able to recover from most non-critical wounds after a mere day of rest or so.

Why elves? Because they are the designated race for casual players who aren't looking for a hardcore survival experience.

# Damage
Damage can be applied to each of the 7 body parts. Each body part has a health ranging from 1.0 to an unspecified negative value. At zero, the body part is unusable and its associated [attributes](Stats) are 0, and effectiveness degrades proportionally between 1.0 and 0.

Below zero, you aren't any *less* effective, per-se, but the body part can continue to be damaged which will increase the time it takes to be healed (or whether it even *can* be healed). The "unspecified negative value" is essentially the point at which the body part is so damaged that further damage doesn't really mean anything, as if the flesh were essentially ground beef or it were severed.
> I believe the maximum amount of damage would be based on the mass or volume of flesh on the body part.

# Healing

```
const ML_BLOOD_VOLUME_PER_KG_BODY_WEIGHT = 70
fn determine_max_blood(character):
	character.max_blood = character.body_weight * ML_BLOOD_VOLUME_PER_KG_BODY_WEIGHT

const PERCENT_BLOOD_VOLUME_CAPACITY_RECOVERED_PER_DAY = 0.01
const SECONDS_PER_DAY = 86400
fn update_blood(character, dt):
	unbandaged_damage = character.body_parts.iter().map(|p| p.damage - p.bandaged_damage - p.scarred_damage)
	character.blood += dt * character.max_blood * PERCENT_BLOOD_VOLUME_CAPACITY_RECOVERED_PER_DAY / SECONDS_PER_DAY
	character.blood -= dt * unbandaged_damage

fn update_scarred_damage:
	// todo - gradually convert open wounds to scarred wounds
	// also convert bandaged wounds to scarred wounds at faster rate(?)

fn bandage_wounds:
	// todo - use surgery skill check to convert open wounds into bandaged wounds

const PERCENT_BLOOD_LOSS_UNCONSCIOUS = 0.3
fn update_blood_loss_poise_factor(character):
	percentage_of_total_blood_volume = character.blood / character.max_blood
	character.blood_loss_poise_factor = (1 - percentage_of_total_blood_volume) / PERCENT_BLOOD_LOSS_UNCONSCIOUS
```