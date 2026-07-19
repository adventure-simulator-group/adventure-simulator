Damage and recuperation are based on real values, but if you did this in basically any other RPG it would become extremely boring and punishing. However, in our case [combat](../tactical/Combat.md) is designed around the assumption that players will reliably be able to avoid taking damage, either by dodging, blocking, or it being absorbed by armor, so this isn't a completely unreasonable target.

However, inevitably damage will occur, and to prevent this from being a fun-killer we must use the two [approaches](../Meta.md) to skip the tedium:
# Abstraction-based Approach
The first of which is that even if it takes a very long time to heal from injuries, we can simply [skip ahead](../strategic/Time.md) until your character is healed. This only works well when resting at [settlements](../strategic/Settlement.md) though. If your party is mid-quest and you take a serious injury, you're either going to have to call it off or fight with a handicap. Unless...
# Content-based Approach
The other "approach" is to use the fantasy elements to circumvent this. For example, we can say that fey-blood provides supernatural healing for [elves](../strategic/Character.md) or create a reasonably cost-effective sort of rapid healing potion that only works on them, such that they might be able to recover from most non-critical wounds after a mere day of rest or so.

Why elves? Because they are the designated race for casual players who aren't looking for a hardcore survival experience.

# Damage

## Disease

Characters do not innately know their diseases. Everyone can see compact,
deduplicated outward symptoms. A completed examination can additionally find
deterministic incidental complaints that obscure the underlying cause. Vitals and diagnoses appear only after a
specific character spends 15 personal minutes examining a co-located patient;
the patient spends the same interval. Examination knowledge belongs to that
doctor and appears in a centered one-shot result. Closing it discards the
result, so examinations never become medical history on the character sheet
and do not administer treatment.
An equipped medication course adds a public line beneath the symptoms stating that
the patient is taking medication for its period-facing disease name. The line
disappears when the episode resolves; it is treatment status, not medical
history.

An examination within one point of a disease's stage-dependent difficulty
produces a weighted differential of compatible period ailments. Meeting the
difficulty confirms the disease. Repeating an unchanged
examination does not reroll the deterministic findings or check. Party-aggregate
Medicine does not grant medical knowledge.

Body-region health is always perceptible, but its cause is not. A lay view uses
only white for sound tissue, red for visible cut damage, light purple for blunt
damage, and green for every other impairment. After a Medicine 2 character
spends 15 personal minutes examining a co-located patient, the one-shot result
replaces green on each affected region with the four period medical channels:
pink Sanguine, blue Phlegmatic, yellow Choleric, and dark-purple Melancholic.
The channels are a period vocabulary over modern internal physiology, derived
per region from the infection episode rather than persisted as disease state.

Episodes retain only identity/associations, contraction minute, and optional
treatment minute. Severity and incidental findings are deterministically seeded
from those associations. Pending examination results are transient action state,
not additional disease state, and are deleted on dismissal. A Medicine 2
character has an alchemy panel listing every preparation at or below their own
Medicine rank; recipe access is intentionally independent of diagnosis and each
recipe has its own treatment DC. Preparation consumes its explicit ingredients
from either personal or shared inventory and advances only the apothecary's
personal time. Herbalists sell ingredients, while future foraging will provide
another source.

Medication is a normal transferable inventory item until the patient equips it.
Medication uses an unbounded equipment list rather than body slots. Equipping
consumes the course, records the episode's treatment timestamp, and continuously
accelerates the remaining course while mitigating symptoms; it never instantly
cures disease. Unequipping discards the course, and medication records are also
removed automatically when their matching disease resolves. Immunity resists acquisition and attenuates severity; resolved episodes can
confer disease-specific acquired immunity. Open cuts may introduce wound disease
with Surgery reducing residual risk; blunt damage does not.

Outbreak acquisition hashes each actual minute of presence. This keeps late
arrival, departure and re-entry exact and makes one long stay identical to the
same stay split among many actions without persisting disease state beyond an
infection episode. Current committed combat provenance is an aggregate cut
amount. The strategic display apportions that known cut share across regions
that actually have physical damage and displays the remainder as blunt damage.
Exact per-region source provenance remains a future tactical result-format
improvement.

Ten percent or less remaining blood volume is terminal circulatory failure.
Gut impairment contributes to Choleric/homeostatic failure through disease;
long-term starvation and malabsorption reservoirs remain future antecedent
systems rather than making a raw Gut attribute of zero instantly lethal.
Damage can be applied to each of the 7 body parts. Each body part has a health ranging from 1.0 to an unspecified negative value. At zero, the body part is unusable and its associated [attributes](../shared/Stats.md) are 0, and effectiveness degrades proportionally between 1.0 and 0.

Below zero, you aren't any *less* effective, per-se, but the body part can continue to be damaged which will increase the time it takes to be healed (or whether it even *can* be healed). The "unspecified negative value" is essentially the point at which the body part is so damaged that further damage doesn't really mean anything, as if the flesh were essentially ground beef or it were severed.
> I believe the maximum amount of damage would be based on the mass or volume of flesh on the body part.

# Healing

## Current strategic implementation

For the current settlement-rest MVP, each body part recovers **1 percentage point plus 1 percentage point per point of the party Medicine check** per full game day. The bounded party check uses geometrically diminishing support and cannot exceed 5, so recovery ranges from 1% to 6% per day without clamping the aggregate. A character without a party uses their own Medicine check. The check is taken when the rest action begins and applies to the entire selected stay. Resting characters convalesce before they train.

Autoresolve calculates each wound by running the shared melee and ranged combat exchanges until one side is incapacitated or the simulation reaches its bounded round limit. It then uses the party Surgery check to determine whether each fresh body-part wound deteriorates during immediate post-battle treatment. Every 5 percentage points of wound damage require 1 point of Surgery to stabilize fully. A shortfall adds a proportional amount of deterioration, up to doubling the wound at Surgery 0; meeting or exceeding the target prevents deterioration but never erases the original autoresolve damage. The Surgery check is taken before autoresolve wounds are applied, so the battle's injuries do not retroactively weaken the treatment roll. Deterioration also contributes to the final committed blood loss.

Characters also persist current and maximum blood volume. Maximum volume currently assumes a 70 kg body at 70 ml/kg. Autoresolve commits final blood loss alongside final body-part injuries, and settlement rest recovers 1% of maximum blood volume per day. The open-wound and bandaging model below is not implemented yet, so blood does not continue draining after the final strategic result is committed. Losing 30% of maximum blood volume contributes 100% strategic incapacitation.

```rs
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
	// todo - replace the current immediate post-battle Surgery check with detailed wound treatment

const PERCENT_BLOOD_LOSS_UNCONSCIOUS = 0.3
fn update_blood_loss_poise_factor(character):
	percentage_of_total_blood_volume = character.blood / character.max_blood
	character.blood_loss_poise_factor = (1 - percentage_of_total_blood_volume) / PERCENT_BLOOD_LOSS_UNCONSCIOUS
```
