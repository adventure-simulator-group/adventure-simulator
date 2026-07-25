# Recruitment and incident authority

Recruitment and strategic incidents are independent world systems. Neither is
a contract, objective, or legacy `Quest`.

## NPC recruitment

An NPC company is advertised by a stable `RecruitmentOfferId`. Its
`RecruitmentSourceId` is the deduplication identity for the settlement/company
source. The offer owns the recruiting party, leader, settlement, creation and
expiry minutes, and an `Open`, `Closed`, or `Expired` lifecycle.

Settlement activity population creates recruiting companies and their ordinary
party recruitment roles without accepting or creating a quest. Requests and
acceptance revalidate the open offer, expiry, leader, role capacity, and
co-location inside one reducer transaction. Repeating the same pending request
is a successful no-op. Player-authored general recruitment roles remain
independent and need no NPC offer.

Each generated company is anchored to a persistent settlement NPC and that
NPC's scheduled observable presence. The offer source derives from the stable
NPC identity; stale party, leader, settlement, or presence bindings close the
offer. The service API returns recruitment companies separately from quest
contracts, so an inn can surface a company when no quest posting exists.

The public offer contains only social presentation identity. It has no
investigation case, witness evidence, hidden cause, or threat data.

## Strategic incidents

`IncidentId`, `IncidentSourceId`, `IncidentKind`, and `IncidentStatus` form a
private strategic authority. The source ID is the retry/deduplication key. Each
incident owns its party, instigator, settlement, case-site binding, hostile
group binding, creation time, and lifecycle.

An incident uses the normal case-site location authority and mission/hostile
group authority, but it does not create a quest or contract. Starting one moves
the party to its incident site without changing `Party.active_quest_id`.
Leaving that site marks the incident avoided. A victorious tactical or
autoresolved mission matches the exact hostile-group ID and marks it resolved
before any legacy quest projection is considered.

Departure synchronization permits retreat only when the sole pending incident
is the incident at the party's exact departing site. Incidents created at a
different location, or multiple inconsistent pending incidents, invalidate the
stale journey request. Activity incidents derive their source identity from the
persisted activity occurrence minute rather than the random occurrence roll.

Consequently, an unrelated battle or incident can never complete a quest, and
resolving or avoiding an incident cannot mutate quest/objective state.
Tactical positions, health, damage, and tick state remain transient.
