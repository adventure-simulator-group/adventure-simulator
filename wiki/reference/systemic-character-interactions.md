# Systemic Character interactions

Surrender, defection, captivity, ransom, property, and theft are ordinary
Character systems. They are not quest scripts. A quest may require their trusted
typed outcome facts, but it neither decides the outcome nor owns the underlying
state.

## Contextual disposition and surrender

Disposition belongs to a `(context, Character)` pair. A Character is never
intrinsically hostile. Every transition checks an expected revision and stable
source ID; an exact retry is a no-op and conflicting reuse is rejected.

Parties may offer or demand surrender only after mutual awareness. The server
uses private morale, fear, strategic incapacitation, directional affinity,
familiarity, leverage, and authored typed obligations in a deterministic rule
matrix. Clients see offered terms and the outcome, not those inputs. Refusal is
durable and leaves the Character active. Acceptance keeps the same Character ID,
deactivates only that contextual participation, and derives group resolution
when no active hostile remains. Combat remains available for active hostiles.

A tactical server may attest that one of its authenticated, immutable mission
participants yielded. Strategic authority commits only that result and exposes a
tactical exclusion. HP, damage, position, morale rolls, and other tick state are
not persisted.

## Recruitment and control

Recruitment and defection transfer one Character through a single membership
primitive. It revalidates life, exact one-party membership, destination capacity,
an explicit active contact, mutual awareness, the exact surrendered disposition
revision, and co-location before mutation. The actor must lead the destination
party. Reducer atomicity means a failed transfer changes no membership or control
state. Successful recruitment uses the canonical `BrowserCharacterGrant` table
with a typed `Recruitment` origin, so exclusivity is shared with starting-character
and adult-descendant ownership; an enemy party is never merged wholesale.

## Global custody

Character custody is distinct from case asset/subject custody. A Character is
`Captive`, `Released`, or `Escaped`, with a typed custodian (`Party`, `Character`,
`Faction`, `Site`, or `None`), version, source receipt, optional trusted case hook,
and optional ransom terms. `Captive` always has a real typed custodian;
`Released` and `Escaped` never do. Capture requires exact-context surrender or
incapacitation, co-location, and control of the destination. Handoff requires
control of both custodians, release requires control of the current custodian,
and only the captive may escape. Case facts use the custody row's validated,
explicit context provenance rather than searching unrelated cases.

Ransom checks its exact receipt before current custody state. Payment atomically
transfers checked currency lots from the payer party to the concrete party or
Character custodian, credits that recipient, then releases the captive. Exact
retry succeeds after release; conflicting source reuse fails.

## Legal property and theft

Legal ownership is separate from physical holding. Property is owned by a
person, party, faction, abandoned estate, or corpse/estate authority. Item and
currency lots transfer through the same checked conservation preflight with an
exact expected owner and version. Every legal lot retains an exact durable
physical inventory or escrow row binding. Full, partial, and multi-hop transfers
deduct that binding before crediting a newly bound destination lot; a derived
legal ID cannot stand in for physical possession. Splits preserve identity,
provenance, and metadata. Exact source retries succeed; conflicting reuse fails.

Taking living personal, party, or faction property without authority is theft.
Abandoned property is not. Corpse property follows corpse/estate authority rather
than being treated as living theft. Each transfer records actor, victim, property,
location, time, and a sorted deduplicated snapshot of co-present aware witnesses.
The actor must share the authoritative holder's settlement or active context.
Settlement witnesses must actually be present; encounter witnesses come only
from active context membership under mutual awareness. A party ID is never used
as a fallback location.
Witnessed theft enters the existing discovered-offense/reputation seam. Guard,
warrant, and law-response simulation is intentionally deferred.

## Quest and UI boundary

Typed objectives can require surrender, recruitment/defection, ransom, custody
handoff or escape, ownership transfer, and theft. Generated action manifests name
the owning systemic producer. Browsers invoke only visible actions with revision
and source coordinates; they never submit a hidden outcome fact or private
decision input. Owning encounter/quest producers author typed obligations through
a private adapter. Acceptance executes or records typed disarm, leave-site,
custody, ransom, and testimony effects. Gateway projections use indexed
party/owner/context scope; property-event projection is capped to the newest 256
relevant events per party.

## Follow-ups

- faction law should determine authorization, severity, warrants, and guard response;
- estate adjudication should decide disputed corpse ownership before ordinary loot;
- captivity needs travel burden, escape opportunities, and humane-treatment effects;
- recruitment needs species/culture/obligation policy and long-term betrayal behavior;
- surrender-term defaults still need consequence, enforcement, and forgiveness policy.
