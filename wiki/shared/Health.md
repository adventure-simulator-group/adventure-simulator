Damage and recuperation are based on real values, but if you did this in basically any other RPG it would become extremely boring and punishing. However, in our case [combat](../tactical/Combat.md) is designed around the assumption that players will reliably be able to avoid taking damage, either by dodging, blocking, or it being absorbed by armor, so this isn't a completely unreasonable target.

Foodborne illness uses existing Dysentery / Bloody flux. Eating evaluates a
direct dose from the lot's lazily grown contamination and amount consumed;
immunity applies and duplicate unresolved infection is prevented. Contamination
details remain private simulation state.

However, inevitably damage will occur, and to prevent this from being a fun-killer we must use the two [approaches](../Meta.md) to skip the tedium:
# Abstraction-based Approach
The first of which is that even if it takes a very long time to heal from injuries, we can simply [skip ahead](../strategic/Time.md) until your character is healed. This only works well when resting at [settlements](../strategic/Settlement.md) though. If your party is mid-quest and you take a serious injury, you're either going to have to call it off or fight with a handicap. Unless...
# Content-based Approach
The other "approach" is to use the fantasy elements to circumvent this. For example, we can say that fey-blood provides supernatural healing for [elves](../strategic/Character.md) or create a reasonably cost-effective sort of rapid healing potion that only works on them, such that they might be able to recover from most non-critical wounds after a mere day of rest or so.

Why elves? Because they are the designated race for casual players who aren't looking for a hardcore survival experience.

# Damage

## Disease

Diseases declare typed transmission vectors (close contact, food/water,
vermin, wounds, or blood) rather than a generic contagious flag. Dirt modestly
raises the standing infection risk of cuts. Blood only transmits a disease when
that disease explicitly supports the blood vector. Plague is the current starter
bloodborne disease; influenza is not. A blood deposit privately snapshots active,
blood-compatible source infection episodes at deposition. Exact source IDs and
disease snapshots live in private provenance tables. The public filth row and
character sheet expose only own, foreign, or unknown origin, so direct subscribers
cannot recover the source identity. Blood remains visibly dirty until washed, but follows one
global rule: infectiousness falls linearly to zero over two strategic days.
Foreign infected blood can establish a new episode during strategic time advance.
Open cuts make that risk dramatic, bandages reduce it substantially, and stitches
reduce it further; intact skin retains only a small baseline route. Evaluation
skips clean and expired intervals in O(deposits), scans only merged infectious
windows, and predicts bandaged/stitched wound closure so split and unsplit time
advances use the same route at each exposure minute.

Successful bandaging, stitching, and projectile extraction transfer 2 filth points
of the patient's blood to the acting surgeon. This occurs only after procedure
requirements and elapsed time succeed. Another patient's active Plague snapshot is
therefore reachable through ordinary care; self-treatment adds no procedure deposit.

Autoresolve currently creates self/attacker blood deposits as it commits strategic
wounds. The real-time tactical handoff does not yet carry per-hit wound provenance,
so equivalent tactical blood deposits remain a documented follow-up rather than
inventing tactical tick state in SpacetimeDB. Synthetic autoresolve opponents also
lack durable character identities, so their blood is recorded with unknown source
provenance.

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

At any settlement, the herbalist's **Feeling ill** dialogue option provides an
NPC examination for a positive gold fee. Only the patient's personal clock
advances by 15 minutes. Each herbalist has a stable Medicine rank from 2 through
4 and therefore may fail to identify difficult illnesses. On success they say
only the canonical disease name and recommended prepared medicine name; they do
not disclose symptoms, findings, vitals, humour channels, differential, stage,
contagion, infection identity, or their skill. Multiple confirmed active
diseases produce deduplicated disease/medicine pairs. Player-doctor examinations
remain free, time-only, and use their richer private one-shot presentation.

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
personal time. Herbalists sell ingredients and all eight pre-prepared courses,
including Medicine 5 preparations that exceed their current personal skill
because unlimited stock represents earlier or supplied inventory. Prepared
courses cost strictly more than buying their ingredients for player crafting.
Future foraging will provide another ingredient source.

Medication is a normal transferable inventory item until the patient equips it.
Medication uses an unbounded equipment list rather than body slots. Equipping
consumes the course, records the episode's treatment timestamp, and continuously
accelerates the remaining course while mitigating symptoms; it never instantly
cures disease. Unequipping discards the course, and medication records are also
removed automatically when their matching disease resolves. Immunity resists acquisition and attenuates severity; resolved episodes can
confer disease-specific acquired immunity. Open cuts may introduce wound disease
with the treating character's relevant Anatomy-based procedure check reducing residual risk; blunt damage does not.

Outbreak acquisition hashes each actual minute of presence. This keeps late
arrival, departure and re-entry exact and makes one long stay identical to the
same stay split among many actions without persisting disease state beyond an
infection episode. Autoresolve now commits each applied hit's limb, cut share,
blunt share, and projectile kind. Those durable limb injury rows are the sole
authority for physical health; `character_limbs` is refreshed once as their
projection rather than healed independently.

Ten percent or less remaining blood volume is terminal circulatory failure.
Gut impairment contributes to Choleric/homeostatic failure through disease;
long-term starvation and malabsorption reservoirs remain future antecedent
systems rather than making a raw Gut attribute of zero instantly lethal.
Damage can be applied to each of the 7 body parts. Each body part has a health ranging from 1.0 to an unspecified negative value. At zero, the body part is unusable and its associated [attributes](../shared/Stats.md) are 0, and effectiveness degrades proportionally between 1.0 and 0.

Below zero, you aren't any *less* effective, per-se, but the body part can continue to be damaged which will increase the time it takes to be healed (or whether it even *can* be healed). The "unspecified negative value" is essentially the point at which the body part is so damaged that further damage doesn't really mean anything, as if the flesh were essentially ground beef or it were severed.
> I believe the maximum amount of damage would be based on the mass or volume of flesh on the body part.

# Healing

## Current strategic implementation

During recovery, the existing bounded party Medicine check supplies **1 percentage point plus 1 percentage point per Medicine point** of natural recovery per full game day, combined with the wound category's own rate. This modifier is applied to the authoritative cut, bruise, and splinted-fracture components rather than directly to `character_limbs`. A retained projectile multiplies every healing component on that limb by 0.6. Resting characters convalesce before they train.

Autoresolve calculates every hit through the shared melee and ranged exchanges and commits its body part, cut and blunt shares, and projectile kind. Strategic wounds are split per limb into cuts, bruising, and fracture severity. Fracture severity is a condition within blunt trauma and never adds a second copy of the hit's health damage. Cuts remain open after battle: they deteriorate at 2.5% health per day and drain blood in proportion to wound size until manually bandaged. Bandaging consumes one bandage and permanently stabilizes that wound; its health-bar segment changes from solid red to banded pink. Bruising heals without a procedure. A single blunt hit over 18% limb health creates fracture severity proportional to the excess. Untreated fractures are graphite grey, while splinted fractures use lighter grey bands so treatment state is not communicated by color alone.

Clicking any limb preserves the character sheet and replaces its right rail with surgery. The normal limb bar and surgery view share the same per-limb cut, bruise, fracture, bandage, and projectile state. Procedures show supplies, time, and difficulty but keep infection odds hidden. The treating character and patient must share a location, party, and personal character time; the lagging participant waits to the later clock, then only those participants advance by the procedure duration. Self-treatment applies a 2.5-point penalty to the resulting procedure check. Bandaging and splinting use Anatomy. Stitching averages Anatomy and Tailoring and requires a reusable surgery kit; its quality accelerates healing. A splint's exact inventory row moves into a separate limb-applied slot while retaining its weight and owner, never displaces armor, and returns automatically when the fracture heals; anyone may remove it.

Bandaging, stitching, and projectile extraction may optionally consume one unit
of the acting character's soft soap. Soap improves infection control independently
of Surgery skill and other supplies; procedures remain possible without it.

Any unhealed cut accumulates deterministic standing wound exposure. Bandaging
reduces that exposure and stitch quality reduces it further; a diseased treating character
also worsens contamination exposure during a procedure. Retained projectiles do
not add a separate recurring complication roll.

Retained arrowheads and balls appear inside the affected limb bar. Extraction DC combines the individual hit's damage with a seeded depth component and is deliberately uncapped: shallow projectiles need little training while difficult positions may exceed DC 5. A procedure cannot be attempted until the treating character's relevant composite check meets its requirement. The procedure meter shows met skill brightly, unmet required skill darkly, and ranks beyond the requirement as empty. Retention imposes only a flat 40% healing-rate penalty. Successful extraction adds cut damage and bleeding, but it does not carry an additional recurring projectile complication. Projectile kind is an explicit extension point for later DC multipliers such as barbed arrows.

Projectile extraction averages Anatomy and Knife; procedures above DC 1 require a reusable surgery kit. Shallower
projectiles remain removable without one.

Characters also persist current and maximum blood volume. Maximum volume currently assumes a 70 kg body at 70 ml/kg. Autoresolve commits immediate blood loss alongside final body-part injuries, open cuts continue draining blood on every authoritative personal-time path, and settlement rest recovers 1% of maximum blood volume per day. Losing 30% of maximum blood volume contributes 100% strategic incapacitation.

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
	// manual, one selected limb and one bandage at a time

const PERCENT_BLOOD_LOSS_UNCONSCIOUS = 0.3
fn update_blood_loss_poise_factor(character):
	percentage_of_total_blood_volume = character.blood / character.max_blood
	character.blood_loss_poise_factor = (1 - percentage_of_total_blood_volume) / PERCENT_BLOOD_LOSS_UNCONSCIOUS
```

# Alcohol disinfection

Successful bloody procedures (bandaging, stitching, and projectile extraction)
automatically use the actor's best eligible personal alcohol unit when one is
available. Effectiveness is explicit item metadata; aqua vitae therefore adds
more hidden infection control than wine or beer. Equal candidates use the
lowest inventory-row ID. The chosen unit is consumed only after procedure time
successfully completes. Soap remains independently optional, and its bonus adds
to alcohol before the hidden infection check saturates. Lack of alcohol never
blocks treatment, and no UI exposes a numeric infection probability.
