# Bestiary authority

`adventuresim-core::bestiary` is the shared authority for strategic threat
identity. Persisted `Quest.enemy_type` and `StrategicEncounter.archetype`
strings are stable `ThreatId` values such as `skeleton` and `grave_robber`.
Display names and aliases never select behavior, and unknown IDs are rejected
at strategic combat and loot boundaries instead of becoming a generic enemy.

## Profiles and current consumers

Every catalog entry has typed combat and investigation profiles. Combat covers
reusable humanoid/quadruped rig, sustainable speed, perception-facing traits,
attack/loadout, protection, vulnerability, fear/disease, temperament, encounter
scaling, and loot. Investigation covers habitat/activity/victims, tracks,
wounds, disturbances, sounds, silhouettes, odors, mistaken identities,
distinguishing evidence, visibility, and preparation advice.

The strategic autoresolver consumes identity, loadout, protection, speed,
loot, and cut/blunt response. Cutting attacks are inefficient against
skeletons while blunt contact is amplified. Other fields are typed for the
investigative generator and future tactical combat; they are not all simulated
yet. Tactical servers do not yet receive bestiary identity, and tactical enemy
instances, position, HP, and damage remain transient.

## Weighted context and inference

The catalog owns sparse forward likelihoods. Ecological base rate and curation
weight are separate; habitat, activity time, visibility, distance, and witness
capability provide context. `rank_candidates` computes inverse conclusions from
those forward likelihoods, priors, and evidence. No inverse table is authored.

Zero means impossible and a low positive weight means rare. Improbable
combinations can require a typed `CausalBridge` with evidence outputs. For
example, skeletons in an occupied house require a cellar crypt, graveyard
tunnel, or resident controller. Witness demographics belong to the future case
model, not this threat-focused catalog.

`evidence_limited_preparation` accepts only visible reports and evidence, never
a hidden threat ID. Direct bounty quests currently confirm opposition and may
show canonical preparation advice. Pure deterministic validation and ranking
APIs support strict-ID, ambiguity, re-ranking, bridge, reachability, and
identification-challenge tests.

## Folklore provenance and adaptation

These are game adaptations, not claims that every motif was believed across
northern Germany in 1544. Names, dates, regions, and motifs changed between
tellings. The small current subset also fits reusable humanoid/quadruped rigs.

- **Kobold:** the Grimms' collected [Der Kobold](https://de.wikisource.org/wiki/Der_Kobold_%28Br%C3%BCder_Grimm%29).
- **Werewolf:** the Grimms' collected [Der Wärwolf](https://de.wikisource.org/wiki/Der_W%C3%A4rwolf); silver weakness and clues are game design, not asserted details of that text.
- **Nachzehrer/Wiedergänger:** early-modern mortuary context in this [academic overview](https://www.eaz-journal.org/index.php/eaz/article/view/851); the fire weakness is an adaptation.
- **Wild man:** early-modern German visual/cultural context in this [art-historical study](https://onlinelibrary.wiley.com/doi/10.1111/j.1467-8365.2008.00607.x), not one uniform folk belief.
- **Spectral hound:** the later regional [Der schwarze Hund (1839)](https://de.wikisource.org/wiki/Der_schwarze_Hund_%28Gr%C3%A4ve%2C_1839%29). Its later date makes it evidence of a collected tradition, not proof of the exact motif in 1544.
- **Alp:** included conservatively as a nocturnal identification challenge;
  its mechanics are an adaptation pending dedicated region-specific sourcing.

The Grimm [collection context](https://de.wikisource.org/wiki/Deutsche_Sagen)
documents nineteenth-century collection of traditions; it does not prove every
adapted motif in the MVP's exact place and year.
