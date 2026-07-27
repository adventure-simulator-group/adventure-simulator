# Bestiary authority

Canonical authoring records live in `content/quests/bestiary.yaml`. They are
sorted, validated, embedded and content-hashed at build time; see
[Quest generation and investigation](quest-generation-and-investigation.md).
Combat weaknesses use the ordinary
physical resistance/padding model. Skeleton bone, for example, has edge
resistance and no innate padding rather than a flat cut/blunt multiplier.

Generated physical-evidence topics may carry an optional diagnostic clue kind.
It is learned only when both the physical inspection and the relevant Bestiary
check succeed. Foot morphology, grave residue, and unexplained missing blood
currently feed the same forward candidate-ranking vocabulary used by witness
descriptions. Candidate output is a deduction, never canonical identity:
observer presentation uses qualitative strong, plausible, or weak support
bands rather than exposing raw likelihood products or authored priors.

`adventuresim-core::bestiary` is the shared authority for strategic threat
identity. Persisted `Quest.enemy_type` and `StrategicEncounter.archetype`
strings are bounded, open `ThreatId` values such as `skeleton` and
`grave_robber`. Display names and aliases never select behavior, and IDs absent
from the startup catalog are rejected at strategic combat and loot boundaries
instead of becoming a generic enemy.

## Profiles and current consumers

Every catalog entry has typed combat and investigation profiles. Combat covers
reusable humanoid/quadruped rig, sustainable speed, perception-facing traits,
attack/loadout, protection, innate material resistance/padding, fear/disease,
temperament, encounter scaling, and loot. Investigation covers
habitat/activity/victims, tracks,
wounds, disturbances, sounds, silhouettes, odors, mistaken identities,
distinguishing evidence, visibility, and preparation advice.
Track, wound, disturbance, odor, and distinguishing-evidence IDs are bounded
open catalog strings. New physical trace identities therefore do not require a
Rust enum edit; code changes only when a new executable interpretation is
needed.

The strategic autoresolver consumes identity, loadout, protection, speed,
loot, perception/stealth, morale, encounter scaling, and innate protection.
Skeleton bone contributes full-coverage resistance but no padding through the
ordinary armor calculation, so cutting attacks are inefficient while blunt
contact remains damaging. No species-level damage multiplier is applied;
weapon force and penetration, worn armor, ranged loadouts, speed, and party
size also produce mechanically testable preparation choices. Fire, silver,
daylight, and ritual courage are stored separately as **unimplemented investigation
hypotheses**. UI copy must not claim they modify autoresolve. Other fields are typed for the
investigative generator and future tactical combat; they are not all simulated
yet. Tactical servers do not yet receive bestiary identity, and tactical enemy
instances, position, HP, and damage remain transient.

Innate protection and `Protection::Armored` cannot currently coexist in one
catalog profile because autoresolve has not modeled overlapping anatomical and
worn layers. Catalog validation rejects that combination rather than silently
overwriting either layer.

## Creature categories and knowledge

Every threat profile carries one or more typed physical-knowledge facets:
Beast, Undead, Human, Werekin, Elf, Dwarf, Fey, Spirit, Greenskin,
Insectoid, Draconid, Construct, and Wildmen. One facet is authored as the
creature's primary type; any others are secondary physical traits. These remain
overlapping rather than exclusive: a werewolf is primarily Werekin with Human
and Beast traits, while a spectral hound is primarily Spirit with a secondary
Beast trait. Skeletons and ghouls are primarily Undead and only secondarily
Human. Wildmen are their own primary category.

`BestiaryHours` stores only direct study by category. Effective knowledge is a
single, nonrecursive pass through a symmetric diagnostic-correlation matrix,
then capped at the Bestiary skill's 5,000-hour mastery calibration so several
related fields cannot add beyond the skill's authored mastery range. Correlation means transferable identification
knowledge, not merely that two tags can coexist. Wildmen transfer strongly
with Human (`0.65`) and more modestly with Fey (`0.30`).

Surgery is expressed through separate trained skills rather than a Surgery
leaf. Projectile extraction averages Anatomy with Knife, stitching averages
Anatomy with Tailoring, and bandaging and splinting use Anatomy alone. Each
skill retains its own governing aptitude, mastery cap, and injury penalties.

Procedural physical-evidence topics may author atomic Bestiary implications.
Each implication names exactly one category, a fixed support value from 0 to
10,000 basis points, a hidden category-specific lore threshold, and safe
interpretation text of at most 1,024 UTF-8 bytes. Both the dependency-light raw
catalog validator and the typed runtime validator enforce that byte limit.
Support is a stable property of the observed clue; it is not computed from
catalog population or hidden case truth. A transformed pawprint may support
Beast and Werekin but never reveals whether the host is Human, Elf, or Dwarf.

Only the inspecting character's category-specific effective knowledge is used.
Successful results are persisted on one canonical inspection record. Revisits
keep the physical observation stable while current knowledge may add newly
successful categories; existing results are never removed or duplicated. The
same safe structured results persist in the investigation journal. Category
lore separates enemies for which that category is the main type from enemies
for which it is a secondary type. Enemy-specific hover details include only
facts derived from fields consumed by current combat, such as a skeleton's edge
resistance and lack of innate padding. Unimplemented folklore such as fire,
silver, daylight, and ritual courage is not presented as gameplay knowledge.

## Weighted context and inference

The catalog owns sparse forward likelihoods. Ecological base rate and curation
weight are separate; habitat, activity time, visibility, distance, and witness
capability provide context. `rank_candidates` computes inverse conclusions from
those forward likelihoods, priors, and evidence. No inverse table is authored.

Zero means impossible and is never raised by curation. A low positive weight
means rare. The public habitat-selection API validates improbable combinations
and requires a typed `CausalBridge` with evidence outputs. For
example, skeletons in an occupied house require a cellar crypt, graveyard
tunnel, or resident controller. Causal bridge identities and their event,
evidence, and action outputs are authored as bounded open IDs.

The MVP northern-Germany regional prior is a small typed authoring context,
separate from curation; it is not yet derived from imported world geography.
`evidence_limited_preparation` accepts only visible reports and evidence, never
a hidden threat ID. Direct bounty quests currently confirm opposition and may
show canonical preparation advice. Pure deterministic validation and ranking
APIs expose ambiguity cardinality, distinguishing clues, normalized
plausibility/curation marginals, dominance, bridge coverage, reachability, and
numeric/duplicate invariants. Evidence inputs are deduplicated and bounded.

## Folklore provenance and adaptation

These are game adaptations, not claims that every motif was believed across
northern Germany in 1544. Names, dates, regions, and motifs changed between
tellings. The small current subset also fits reusable humanoid/quadruped rigs.

- **Kobold:** the Grimms' collected [Der Kobold](https://de.wikisource.org/wiki/Der_Kobold_%28Br%C3%BCder_Grimm%29).
- **Werewolf:** the Grimms' collected [Der Wärwolf](https://de.wikisource.org/wiki/Der_W%C3%A4rwolf); silver is an unimplemented investigative hypothesis and not asserted by that text.
- **Nachzehrer/Wiedergänger:** early-modern mortuary context in this [academic overview](https://www.eaz-journal.org/index.php/eaz/article/view/851); fire is an unimplemented investigative hypothesis.
- **Wild man:** early-modern German visual/cultural context in this [art-historical study](https://onlinelibrary.wiley.com/doi/10.1111/j.1467-8365.2008.00607.x), not one uniform folk belief.
- **Spectral hound:** the later regional [Der schwarze Hund (1839)](https://de.wikisource.org/wiki/Der_schwarze_Hund_%28Gr%C3%A4ve%2C_1839%29). Its later date makes it evidence of a collected tradition, not proof of the exact motif in 1544.
- **Alp:** included conservatively as a nocturnal identification challenge;
  its mechanics are an adaptation pending dedicated region-specific sourcing.

The Grimm [collection context](https://de.wikisource.org/wiki/Deutsche_Sagen)
documents nineteenth-century collection of traditions; it does not prove every
adapted motif in the MVP's exact place and year.
