# Quests

Quests begin as problems in the world, not tasks waiting for a player to
activate them. A settlement may already be suffering thefts, disappearances,
dangerous creatures, disease, or disrupted trade before any character hears
about the cause.

Investigation skills answer different questions. Bestiary narrows which
creatures fit received descriptions and successfully interpreted physical
clues. Terrain follows and recovers tracks. Hearing testimony passively uses
Insight to form a fallible impression of each specific claim, while Charm,
Command, and Deception offer increasingly risky and forceful ways to question
that claim. Insight reads demeanor, not hidden facts: a sincere mistake can
look sincere, and evasive or partly truthful accounts are inherently ambiguous.
Witnesses do not always have quest information withheld, and the wording of an
initial or released account does not identify its reliability. A character
may question any highlighted claim, even one that looks truthful. A failed
challenge strains affinity; Command always imposes some strain. Time together
builds familiarity, while the NPC's private personality and morale also matter.

Tracks are followed as a short sequence rather than one all-or-nothing check.
Each successful section contributes a physical finding before the final section
locates a site; unseen later sections and their destination remain private.

## Discovery

Players learn about problems through tavern rumors, local conversations,
witnesses, and physical evidence. The journal records what a character has
actually heard or observed. It does not expose hidden truth, calculate a single
confidence score, or mark every useful person and place in advance.

Accounts can be incomplete, mistaken, evasive, or contradictory. Different
characters may therefore know different things about the same case.

Exact map pins require exact believed or visited location knowledge. Textual
directions and approximate areas remain text until investigation produces a
specific destination.

## Cases and contracts

A **case** is the underlying world problem. A **contract** is an agreement to
pay for some result concerning that problem.

Accepting a contract does not create the case, and abandoning one does not
erase it. Some generated problems have no contract at all and are encountered
solely through rumors and investigation. Direct bounties are the simpler
exception: their issuer may disclose a known site and promise payment for a
specific result.

Contracts that require reporting are paid only after the party returns to the
issuer and reports the completed work.

## Investigation

Investigations can combine social and physical routes. A witness may point to
another person, a place, or a suspicious event. Tracks, wounds, objects, and
other evidence may support several interpretations until a character has
enough relevant knowledge.

Bestiary combines reports a character has actually received with diagnostic
clues that character successfully learned. The evidence view and journal list
possible monster kinds with qualitative support and provenance. Failure,
numeric scores, and the canonical enemy remain private.

Different routes may reach the same finale. A failed lead does not delete an
independent route, and a later correction does not rewrite what the character
previously believed.

## Planning and travel

Before leaving, a party should consider:

- the likely opposition and plausible mistaken identities;
- terrain, distance, provisions, daylight, and opportunities to camp;
- relevant social, investigative, medical, and combat skills;
- equipment suited to armor, large enemies, groups, or ranged threats;
- whether capture, rescue, retrieval, proof, or negotiation may matter more
  than killing.

Knowing an exact case site makes it available to strategic route planning. It
does not accept a contract, resolve an objective, or grant a reward.

## Confrontation and outcomes

At a hostile site, the party may enter tactical combat or use strategic
autoresolve. Both consume the party's durable condition, equipment, skills,
injuries, fatigue, and ammunition. Tactical positions and damage exchanges
remain transient; strategic wounds, spent supplies, loot, morale, custody, and
case outcomes are committed only at the validated result boundary.

Victory is not synonymous with killing everything. Depending on what the party
has learned and prepared, a confrontation may support defeat, driving enemies
off, capture, rescue, retrieval, exposure, or another case-specific result.
Strategic authority chooses only among outcomes that are still valid for that
party, site, and case.

Defeat can wound the party and leave the problem unresolved. An incapacitated
party may withdraw to recover but cannot continue ordinary combat or travel
until it is ready.

## Continuing problems

Unresolved problems may worsen over time. New incidents can add witnesses,
victims, evidence, settlement consequences, public awareness, and stronger
hostile groups without rewriting earlier events. NPC adventuring companies
still recruit, but they no longer investigate or resolve quests automatically.
Conspicuous hostile cases eventually become public and can be referred through
nearby innkeepers or eligible organization representatives. The referral
creates a durable journal case and exact destination pin containing only the
threat type, safe site label, and approximate count.

This keeps cases part of a shared world: ignored threats become a growing
combat problem and their public rumor radius expands.

## Rewards

Rewards should compensate expected danger, consumed resources, travel, and the
chance of serious injury or death. Strong parties are safer but more expensive;
weak parties may have almost no chance against a difficult threat. The intended
planning problem is to find an appropriately prepared party, not simply the
largest available one.

Battle loot enters the shared party inventory and follows the party's stake
rules. Contract payment remains separate and is awarded only through the
contract's reporting flow.

## Technical references

The implementation deliberately separates player-visible knowledge from hidden
world authority:

- [Quest generation and investigation](../reference/quest-generation-and-investigation.md)
  covers authored content, deterministic generation, testimony, evidence, and
  observer knowledge.
- [Quest authority](../reference/quest-authority.md) covers cases, contracts,
  objectives, missions, outcomes, local problems, recruitment, and incidents.
- [Bestiary authority](../reference/bestiary.md) covers stable threat identity,
  physical knowledge, and preparation information.
- [Strategic simulation](../reference/strategic-simulation.md) covers automated
  balance and regression evaluation.
