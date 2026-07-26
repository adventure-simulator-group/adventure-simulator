# Quests

## Current architecture

In developer mode, settlement pages provide a catalog-driven editor for
spawning latent investigation quests. Spawned quests do not become known to the
active character automatically; tavern and NPC rumors remain the discovery
mechanism. The editor's explicit compatibility override permits intentionally
unlikely catalog combinations, but cannot bypass structural validity or
reference integrity. This browser-local developer mode is presentation only,
not a server authorization boundary.

Cases are persistent world problems with private investigation truth and a
typed AND/OR resolution graph. Contracts are separate agreements offered by
NPCs concerning those cases. Accepting or abandoning a contract never creates
or deletes its case. The current direct-bounty generator remains the first
golden path: it creates a case, a defeat objective bound to one materialized
hostile group, a separate contract, and an independently known case site.
Combat contributes an authenticated outcome fact; the objective evaluator,
not tactical code, decides whether the case is resolved.

Quest templates, witnesses, evidence, sites, descriptions, threats and their
weighted relationships are repository-authored YAML. Its content digest is
recorded on newly generated cases; existing authority is never silently
reinterpreted against different content.

A party leader may request one of an NPC company's open roles while both parties occupy the same location and the company's typed recruitment offer remains open and unexpired. The company and offer are generated independently from quests. Acceptance merges the applying party into the destination party: the destination leader retains command, the applying leader fills the selected role, and every other source member becomes an ordinary member. Source recruitment roles and pending applications close, while party-inventory items, reserve value, and each character's absolute stake transfer intact. Generated NPC leaders auto-approve these requests in local development so the complete merge flow can be previewed.

An expired generated-company offer is renewed in place when its company,
leader, and settlement presence remain valid. This keeps recurring settlement
activity supplied without consuming a fresh NPC on every expiry cycle; a stale
offer whose bindings no longer exist closes instead. Renewal and joining use
the same authority check: party and living leader must be bound to the offer's
settlement, the NPC must belong there, and the NPC's current presence must
match the offer's exact advertised location. A closed invalid offer does not
reserve its NPC against a valid replacement.

Recruiting NPC companies are discovered at the inn rather than through quest ownership or a global party browser. Their service presentation reveals the company leader and each open role. Role links show exact recommendations on hover and are colored blue for unrestricted roles, green when a member of the viewer's party meets every recommendation, yellow when a member meets some, and red when no member meets any. Selecting a role replaces the side panels with its detailed recommendations, party context, and the request-to-join action.

Standalone strategic encounter incidents are also independent from quests.
They own typed source, site, hostile-group, and lifecycle identity; entering or
resolving one never replaces the party's active quest or changes an objective.

For local development, creating a recruitment role seeds capable temporary NPCs at the party's current location and immediately fills its requested slots. This is a testing aid; ordinary player recruitment continues to use applications and the leader's approval.

Opening the issuing service adds the NPC's greeting to party chat with a linked description of the problem. Following the linked dialogue reveals the estimated opposition and reward; choosing the linked interest response formally accepts the quest. Accepting does not teleport the party, but the direct-bounty issuer explicitly discloses its seeded case site. Only that observer-safe exact disclosure makes the destination eligible for an exact map pin and travel. Once the observer knows that exact physical site, its originating settlement remains disclosure provenance rather than a permanent routing constraint: the pin is available on the map at any current settlement, and a new journey is planned from the party's authoritative current settlement or case site. Selecting or tracking the site is navigation state only and cannot accept the quest or grant rewards. An accepted quest can be abandoned until the party reaches its case site. There are no standalone quest-list, quest-detail, or party-list pages. Travel to a case site and travel onward from it are straight-line, off-road journeys at one quarter of the normal 5 km/h settlement travel speed. Case sites therefore do not need a Viabundus road connection.

The quest destination uses the same location header and party-portrait overlay as settlement pages, but omits settlement-service tabs. Its left rail always shows unclaimed loot and starts empty; its right rail always shows the shared party inventory. Resolving combat adds enemy equipment and any quest gold to the loot rail. Loot transfers are staged with the shared inventory arrows, then committed from a fixed confirmation popup without leaving the location screen.

At the destination, the strategic page retains the normal chat placeholder, shows a location-image placeholder, and offers tactical combat or autoresolve. Autoresolve builds combatants from the party's current attributes, skills, equipment, injuries, blood volume, fatigue, and strategic incapacitation, then runs deterministic seeded melee and ranged exchanges against enemies scaled from the quest type and difficulty. Victory is not guaranteed. Its summary and every exchange in the persisted diagnostic report appear chronologically as Info rows in the shared chat stream rather than in a separate report block. Fresh per-limb wounds and blood loss persist on either outcome. Cuts remain open and continue bleeding and deteriorating on each wounded character's personal clock until manually bandaged according to [Health](../shared/Health.md). Defeat adds a recent morale setback and leaves the objective unresolved. On allied victory, strategic authority—not the tactical process—deterministically selects a compatible result from the exact unresolved approaches earned for that case and site. Defeat alone produces hostile-group loot; drive-off and capture emit typed case consequences, with capture also requiring and transferring current subject custody. Abstract rounds, enemies, approach weights, and tactical state are never public persistent gameplay state.

An incapacitated party may withdraw from a quest location to a settlement to recover, but cannot undertake further combat or ordinary travel until its members are ready.

Completing the objective does not immediately close the quest or pay its promised reward. The party must travel back to the issuing settlement and speak to the same service NPC. Before completion the returning NPC says they are still awaiting results; after completion they ask whether the party has **finished**, and following that linked response turns in the quest. After the server confirms success, an Info row states the exact gold added to the party inventory. The promised gold enters the shared party inventory and its value is divided equally among the current members' stakes, with any indivisible remainder entering the captain's reserve. Only then is the quest removed from the party tracker and replacement settlement activity generated.

An active contract also constrains party lifecycle. A party cannot disband
while a completed contract is ready to report, which prevents abandoning a
claimable shared reward. If the final living member dies during an accepted or
ready-to-report contract, strategic authority withdraws that contract and
clears its case-site tracking while retaining the party record and pooled
inventory. Already paid contracts remain historical records.

If the final living member dies, the retained party and pooled assets cease all
strategic travel: camp destination, private route and encounter entropy, public
itinerary, strategic encounter, and pending party action, leadership, and join
requests are cleared together with recruitment roles. Both general and
role-specific join requests reject a party without a living leader. Active
tactical mission and server-assignment authority is intentionally untouched so
tactical outcome ownership is not rewritten by strategic cleanup.
## Diegetic discovery and navigation

Quest problems are discovered through local rumors and NPC testimony, not
exclamation markers. A book button beside the current location and time toggles
a location-preserving journal tab: the left rail becomes the quest list and the
right rail becomes the selected quest's log while the current location remains
in the center. Open quests appear first, newest update first, followed by
completed and failed quests in the same recency order; the first quest is
selected by default. The journal is deliberately a dry record of reports and
observations with their stated sources. It does not score confidence, identify
contradictions or corrections, expose probabilities, interpret implications,
list referrals or destinations, suggest investigation methods, or provide
action and travel controls.

Available-quest, quest-giver/service, route-to-issuer, and turn-in markers have
been removed. Textual directions, landmarks, approximate areas, and route
segments remain descriptions. Exact pins require exact believed knowledge, so
an incorrect account can create an incorrect pin until corrected. The accepted
legacy direct bounty explicitly reveals its exact case site, but the resulting
pin is a knowledge projection rather than a quest marker. Recruitment
indicators remain separate from quest markers.

The first modular generator supports recurring depredation and
disappearance/loss cases. Both begin as observable local consequences rather
than a named monster. Witness descriptions, circumstances, reliability,
evidence, sites, and habitats are weighted independently, so the template does
not reveal the answer. Impossible combinations have zero weight; rare ones
remain possible only when the case generates a discoverable causal bridge.

Each case offers two different routes to the same real target. Physical tracks
and social inquiry may fail or contradict one another without deleting the
alternate route. Canonical cause determines the finale: rescue a concealed or
abducted person, retrieve and return a lost asset, expose a fabricated claim,
or defeat/drive off recurring attackers. Unsupported endings are not rolled.
The modular cases create no contracts: entering the tavern guarantees a rumor
entry point, and referrals identify witnesses by appearance and expected
location. Recurring problems begin with that exact referred contact and unlock
their approach and watch branches only after the contact succeeds;
disappearance/loss cases retain independent physical and witness starts.
A route first reveals travel-capable exact knowledge; only after the
party travels to and occupies that site can a separate inspection, ambush,
retrieval, or rescue action resolve it.

### Continuing incidents

An unresolved generated problem does not remain frozen at the moment the case
is created. Every two days of authoritative world time it may produce another
append-only incident: a new occurrence with its own persistent witness, victim,
circumstance, site binding, event, and physical evidence. These additions do
not rewrite the original case manifest or invalidate earlier testimony.
Characters who already know the problem can hear a dry report of the new
occurrence when rumors next reach them; the report does not disclose hidden
evidence, interpret its implications, or recommend an action.

Each incident increases the problem's settlement penalties by 25 percent of
the initial amount, affecting applicable prices, encounter pressure, and
disease exposure before the ordinary safety caps. The current temporary limit
is five incidents including the original offence, at which point penalties are
twice their starting severity.

Old cases with multiple incidents become eligible for a resident NPC
adventuring company. The company waits while a player has investigated
recently or occupies one of the case sites. Otherwise it follows a physical,
pattern, or social route that is actually present in the generated testimony
and investigation-action graph. A failed route produces a specific setback and
a later attempt tries another supported route when possible. Once the
investigation reaches the finale, the company can resolve the case, reduce its
effects, fail, or defer and retry later. This is a strategic result, not a
simulated tactical battle. Anyone who had already heard about the problem may
later receive a dry journal notice about the result.

## Planning

### Automated investigation evaluation

There are two gameplay evaluation paths. The server-side strategic simulator exercises
the real NPC adventuring companies described above. Scripted decisions are the
default; an optional LLM can choose only among observer-safe strategies offered
by the server, while the server remains the sole outcome authority. Every run
writes a Markdown anthology of the problem discovery, exact dialogue, actions,
preparation, route-specific setbacks or finale, and outcome as the NPC company
experienced them.

The separate end-to-end browser evaluator is LLM-only. It sees a screenshot,
text inside the visible viewport, and opaque handles for visible controls, then
plays through the same web interface as a person. It produces a screenshot
timeline rather than a Markdown story. It receives no canonical cause, true
destination, generation weights, reducer names, or hidden case identifiers.
Since you know what enemies you will face at the destination and can estimate what enemies you'll possibly face on the journey, you should plan a party in advance to account for this. For now this is mainly a question of whether you expect to encounter armored enemies (and therefore need hammers/rondels), hordes of weak enemies (therefore want a cleaving sword/axe/hammer and heavy armor), large enemies (therefore want a polearm), and enemies with projectiles (therefore want armor/full-plate). You will also want to account for difficult terrain or the need for stealth/detection during [travel](Travel.md) in your party composition, the latter is important if you're traveling through an area with enemies that are too dangerous for your party to fight against.

A third developer tool, `quest-analyze`, is an offline generator/content
analyzer. It uses a synthetic observer-safe projection and can run with a
scripted policy or a credential-free strict-JSON mock. It produces separate
public traces/stories and developer-only truth/factor audits. It is useful for
finding generator dead ends, loops, route dominance, correction persistence,
and policy fingerprints, but it is not reducer-authoritative gameplay or
browser evidence. Unsupported contract, language, tactical-combat, perception,
and causal skill-benefit measures remain explicitly unavailable.
### Mixed-Level
Quests are not balanced around the assumption that all members of a party have a similar power level, in fact its generally the case that you *want* a mixed-strength party. When you set out to clear out a vampire crypt, you need a very skilled armored duelist or two to fight the vampire. But you'll also be encountering plenty of zombies and skeletons, which can be efficiently dealt with by a small group of decent semi-armored combatants with clubs and axes. 
## Reward
Rewards are proportional to difficulty, which is better thought of as a measurement of risk. Each [character](../strategic/Character.md) has a value calculated in [favor](../shared/Magic.md), and each quest has a reward that can be expressed in terms of favor, so to calculate what the reward should be we can choose a power level that is ~90% likely to successfully complete a quest, estimate the favor needed to create a party of that power level times the chance of death plus the resources spent on completing the quest (food, arrows, healing), and multiply the final reward by some constant like ~1.2-1.8 so that its actually profitable. The chance of death versus difficulty of a quest though is not linear for *your* party level though. A powerful party might be twice as expensive as a weak one but only marginally less likely to die when killing a pack of goblins, so its a waste of time, and likewise the likelihood of a ragtag group of nobodies killing a vampire is basically zero, so there's always a theoretically optimal party strength for every quest.
## Interface
Case sites use the shared strategic location shell but expose no settlement services. The location name and description come from the site authority; Map lists the nearest off-road settlement destinations and reveals travel details only after selection; Loot contains unclaimed battle loot and the shared party inventory. Party portraits and character inspection use location-relative routes, so they work at both settlements and case sites.

Generated case sites do not require or fabricate contracts. Once the active
character knows the exact site and the party occupies it, the location exposes
only investigation actions currently authorized at that site. A combat
autoresolve action appears only for an open generated objective whose active
hostile group and finale are bound to that exact site; evidence and other
noncombat sites do not gain combat controls merely because the party arrived.
When generated case authority resolves, including through a noncombat rescue
or recovery, the site shows a plain completion notice and removes its
pre-finale rest controls without exposing private cause or manifest data.
Legacy direct bounties continue to require their accepted active contract.

Physical evidence at the occupied site appears alongside the other
counterparties as circular portraits. Selecting one replaces the central
description with an italic observation. Its highlighted topic phrases inspect
specific parts of the object, such as glass shards or a damaged frame. Some
parts are mundane; others require a named eyesight, intelligence, or instinct
check before their clue is recorded.

These checks do not roll dice when clicked. Their hidden difficulty was fixed
when the threat created the evidence, so the same character can retry freely
but will get the same result until the relevant attribute changes. Neither the
difficulty nor the character's numerical value is shown. The journal records a
successful observation as a dry fact and does not explain what it implies.

Location headers do not contain a quest tracker. The Map building tab carries a red exclamation while the party has an active quest. Map destination rows use a gold exclamation for available quests and a red exclamation for the party's active quest route or destination. Resolving the objective does not change the active marker to gold: it remains red while guiding the party back to the issuing settlement, until turn-in removes the active quest. The generated-case completion notice is ordinary page content rather than a marker over the center visual.

So that new players don't naively accept difficult quests since they have no frame of reference for estimating their own/enemies power level, the quest should display recommended power level and their current value before they head out.
### Equations
TODO: come up with a first-pass starting point to use to assess party power level.

### Automated strategic-loop coverage

The opt-in reducer-backed NPC simulator exercises the quest lifecycle through
the same strategic reducers as a player: party join request/accept, quest
acceptance, provisioned travel and camps, autoresolve, loot storage, return,
turn-in, liquidation, and equipment purchase/equip. Its defeat policy retreats
and heals at a settlement before a bounded retry, so it also detects loops that
would otherwise repeatedly autoresolve an incapacitated party.

## Bestiary identity

Quest opposition is stored as a stable shared-bestiary ID rather than free-form
display text. Current direct bounties reveal that identity and preparation
advice; future investigations will use evidence-limited ranking so hidden truth
is not leaked. Combat, loot, fear, movement speed, habitat, and activity data
come from `docs/BESTIARY.md`.
Local problems are learned from NPCs rather than new quest-giver markers. A rumor is not contract acceptance and cannot change active quest state. Exact destination markers require later knowledge of an exact location.
