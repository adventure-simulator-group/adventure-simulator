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

Patient membership and outbreak authority are private. Every patient is an
existing settlement resident's canonical `Character`, and their illness is an
ordinary `InfectionEpisodeRow`; the outbreak does not create a presentation
proxy or retain a shadow copy of immunity, phenotype, or disease state.
Disease fatalities cross the ordinary terminal threshold at the authored
minute and use the normal Character death, relationship, corpse, and pathology
flows. Bodies and living-patient context membership become known only through
the normal rumor and exact-site presence rules.

## Investigation and resolution

Every outbreak supplies independent physical and social routes. Examining
patients, a source site, or an available corpse can establish physiological
and environmental facts. Questioning residents can establish chronology,
practice, responsibility, or a carrier's presence. Neither route requires a
corpse, so a buried or inaccessible victim cannot deadlock the case.

After the party hears the ordinary outbreak rumor, each modeled patient and
their relatives from canonical kinship authority can discuss the fevers
directly. Generated cases do not carry a second family or carer field. The
party does not have to follow the rumor's witness referrals in a specific
order, and unrelated residents do not gain outbreak testimony merely because
they live in the settlement. A generated social route always targets a
surviving witness rather than a patient whose authored course is fatal.

Living patients progress against authoritative world time and appear in the
case site's shared counterparty roster after discovery. The standard Talk and
Bandage interactions address the same Character identity used in the
settlement, relationship, physiology, and surgery systems; there is no outbreak
examination reducer or findings table. While a patient membership is active,
shared resident presence suppresses their ordinary schedule and services
without rewriting that schedule. Remediation deactivates the case context but
does not cure an infection: health suppression remains until authoritative
world time reaches recovery, and remains permanent for a dead Character. Every
service, dialogue, investigation, and social availability check uses that same
projection. Environmental and carrier exposure requires presence at the exact
source site; only community sanitation or behavior sources apply across
settlement presence.

Completion requires a typed `RemediateSource` objective. Closing a contaminated
well, changing a dangerous practice, removing an environmental reservoir, or
defeating/driving off the exact carrier group records `SourceRemediated` only
when it matches private outbreak authority. Diagnosis or an unrelated battle
cannot complete the case.

### Fixture-drawn water

A generated dysentery outbreak may materialize its ordinary source-site fixture
as a finite water source. Collection is an ordinary five-minute strategic
action issued through an observer-owned investigation capability: the client
never supplies or receives the private case, fixture, source-lot, or contaminant
identity. Fixture use and alteration of the carried container are authorized
separately, and volume plus microbial load are conserved through an immutable
draw receipt and a distinct output material lot.

Containers disclose only the coarse rule that their water was fixture-drawn;
clean and contaminated fixture water have the same actions, errors, timing, and
public shape. In this bounded integration, fixture-drawn water cannot be mixed
with or poured into legacy pooled water and cannot satisfy ordinary hydration.
It must be cooked in the existing fireplace flow. Cooking consumes that exact
container lot, applies the existing method/doneness heat kill, and carries its
private contribution into the resulting `FoodContamination`. Food splitting,
party transfer, eating, protection, and dysentery acquisition continue through
their existing systems. A successful infection is recorded as a private typed
world event at the eater's actual strategic place.

Closing the well disables only future draws. Water already collected, food
already cooked, infection episodes, and evidence already learned remain
unchanged. Direct source inspection records a bounded digest as ordinary
observer-owned evidence provenance, allowing testimony, physiology, and
material investigation to converge without exposing the private outbreak
truth or maintaining a second disease state.

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

Run the single isolated strategic scenario-gallery command, then select the
**Discovered outbreak** scenario character. Use browser-local developer mode
to expose the gallery and inspector. Bootstrap has already raised useful
investigation skills, supplied a surgery kit, and materialized one deterministic
generated outbreak in a scenario-owned settlement, with private progressing patients and an
optional exact-course disease or carrier-autoresolve corpse. It does not grant
a journal entry or evidence. Instead, it privately marks the generated
outbreak as that character's next eligible ordinary rumor, so pre-existing
settlement quests cannot hide the demo. Ask around for local rumors to discover
it normally, then follow either route to the exact remediation.
