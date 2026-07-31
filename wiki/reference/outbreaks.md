# Outbreak investigations

Outbreaks are generated investigation cases, not a second disease simulation.
They materialize ordinary character infection episodes and use the same
strategic health progression, corpse custody, autoresolve injuries, physiology,
surgery, bestiary, dialogue, evidence, objective, and rumor systems as other
play.

## Private truth and public presentation

`GeneratedCase::outbreak` is private canonical truth. It identifies the disease
and transmission route, one sanitation, behavior, environmental, or
threat-vector source, a physical source site, an optional responsible NPC and
culpability, patient chronology, and the exact remediation. Public local-problem
state says only that an unusual number of locals are ill. Different causes
deliberately share that early wording.

Patient rows and outbreak authority are private; no synthetic public Character
row is created. Each patient retains the exact episode, immunity, phenotype
version, presentation NPC, and disease-course inputs used by the core
evaluator. Disease fatalities occur only at a real terminal crossing and create
an injury-free disease body with pathology captured from that exact state. A
carrier-attack fatality instead runs the modeled threat through strategic
autoresolve. Bodies and living-patient portraits become known only after normal
rumor discovery.

## Investigation and resolution

Every outbreak supplies independent physical and social routes. Examining
patients, a source site, or an available corpse can establish physiological
and environmental facts. Questioning residents can establish chronology,
practice, responsibility, or a carrier's presence. Neither route requires a
corpse, so a buried or inaccessible victim cannot deadlock the case.

Living patients progress against authoritative world time and appear through an
observer-scoped portrait after discovery. Their Physiology action exposes only
bounded findings. Environmental and carrier exposure requires presence at the
exact source site; only community sanitation or behavior sources apply across
settlement presence.

Completion requires a typed `RemediateSource` objective. Closing a contaminated
well, changing a dangerous practice, removing an environmental reservoir, or
defeating/driving off the exact carrier group records `SourceRemediated` only
when it matches private outbreak authority. Diagnosis or an unrelated battle
cannot complete the case.

Fantastic diseases use the same physical meters and transmission machinery as
ordinary disease. Their unusually clean traditional profiles make Physiology
more useful without making humour theory reliable for ordinary illness. See
[Fantastic diseases](fantastic-diseases.md).

## Authoring, replay, and evaluation

The `outbreak` template and relations live in
`content/quests/generation.yaml`. Automatic generation and the developer quest
compiler persist the full typed outbreak payload in the replay manifest; all
author-local patient, site, hostile-group, action, evidence, objective, and
remediation references are observer-namespaced before materialization.

The offline strategic investigation evaluator includes
`TemplateFamily::Outbreak` in its golden suite and accepts `--family outbreak`
when promoting replay candidates.

## Development demo

Run `just outbreak-demo`, create or select a character, enable browser-local
developer mode, and choose **Outbreak demo** from a settlement. The gated loader
raises useful investigation skills, supplies a surgery kit, and materializes
one deterministic generated outbreak with private progressing patients and an
optional exact-course disease or carrier-autoresolve corpse. It does not grant
a journal entry or evidence. Ask
around for local rumors to discover it normally, then follow either route to
the exact remediation.
