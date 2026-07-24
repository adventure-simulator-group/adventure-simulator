Cases are persistent world problems with private investigation truth and a
typed AND/OR resolution graph. Contracts are separate agreements offered by
NPCs concerning those cases. Accepting or abandoning a contract never creates
or deletes its case. The current direct-bounty generator remains the first
golden path: it creates a case, a defeat objective bound to one materialized
hostile group, a separate contract, and an independently known case site.
Combat contributes an authenticated outcome fact; the objective evaluator,
not tactical code, decides whether the case is resolved.

A party leader may request one of an NPC company's open roles while both parties occupy the same location and the company's typed recruitment offer remains open and unexpired. The company and offer are generated independently from quests. Acceptance merges the applying party into the destination party: the destination leader retains command, the applying leader fills the selected role, and every other source member becomes an ordinary member. Source recruitment roles and pending applications close, while party-inventory items, reserve value, and each character's absolute stake transfer intact. Generated NPC leaders auto-approve these requests in local development so the complete merge flow can be previewed.

Recruiting NPC companies are discovered at the inn rather than through quest ownership or a global party browser. Their service presentation reveals the company leader and each open role. Role links show exact recommendations on hover and are colored blue for unrestricted roles, green when a member of the viewer's party meets every recommendation, yellow when a member meets some, and red when no member meets any. Selecting a role replaces the side panels with its detailed recommendations, party context, and the request-to-join action.

Strategic incidents are also independent from quests. They own typed source,
site, hostile-group, and lifecycle identity; entering or resolving one never
replaces the party's active quest or changes an objective.

For local development, creating a recruitment role seeds capable temporary NPCs at the party's current location and immediately fills its requested slots. This is a testing aid; ordinary player recruitment continues to use applications and the leader's approval.

Opening the issuing service adds the NPC's greeting to party chat with a linked description of the problem. Following the linked dialogue reveals the estimated opposition and reward; choosing the linked interest response formally accepts the quest. Accepting does not teleport the party, but the direct-bounty issuer explicitly discloses its seeded case site. Only that observer-safe exact disclosure makes the destination eligible for an exact map pin and travel. Selecting or tracking the site is navigation state only and cannot accept the quest or grant rewards. An accepted quest can be abandoned until the party reaches its case site. There are no standalone quest-list, quest-detail, or party-list pages. Travel to a case site and travel onward from it are straight-line, off-road journeys at one quarter of the normal 5 km/h settlement travel speed. Case sites therefore do not need a Viabundus road connection.

The quest destination uses the same location header and party-portrait overlay as settlement pages, but omits settlement-service tabs. Its left rail always shows unclaimed loot and starts empty; its right rail always shows the shared party inventory. Resolving combat adds enemy equipment and any quest gold to the loot rail. Loot transfers are staged with the shared inventory arrows, then committed from a fixed confirmation popup without leaving the location screen.

At the destination, the strategic page retains the normal chat placeholder, shows a location-image placeholder, and offers tactical combat or autoresolve. Autoresolve builds combatants from the party's current attributes, skills, equipment, injuries, blood volume, fatigue, and strategic incapacitation, then runs deterministic seeded melee and ranged exchanges against enemies scaled from the quest type and difficulty. Victory is not guaranteed. Its summary and every exchange in the persisted diagnostic report appear chronologically as Info rows in the shared chat stream rather than in a separate report block. Fresh per-limb wounds and blood loss persist on either outcome. Cuts remain open and continue bleeding and deteriorating on each wounded character's personal clock until manually bandaged according to [Health](../shared/Health.md). Defeat adds a recent morale setback and leaves the objective unresolved. On allied victory, strategic authority—not the tactical process—deterministically selects a compatible result from the exact unresolved approaches earned for that case and site. Defeat alone produces hostile-group loot; drive-off and capture emit typed case consequences, with capture also requiring and transferring current subject custody. Abstract rounds, enemies, approach weights, and tactical state are never public persistent gameplay state.

An incapacitated party may withdraw from a quest location to a settlement to recover, but cannot undertake further combat or ordinary travel until its members are ready.

Completing the objective does not immediately close the quest or pay its promised reward. The party must travel back to the issuing settlement and speak to the same service NPC. Before completion the returning NPC says they are still awaiting results; after completion they ask whether the party has **finished**, and following that linked response turns in the quest. After the server confirms success, an Info row states the exact gold added to the party inventory. The promised gold enters the shared party inventory and its value is divided equally among the current members' stakes, with any indivisible remainder entering the captain's reserve. Only then is the quest removed from the party tracker and replacement settlement activity generated.
## Diegetic discovery and navigation

Quest problems are discovered through local rumors and NPC testimony, not
exclamation markers. The journal records only what the active character knows:
sources, uncertainty, contradictions, corrections, witness descriptions, and
learned expected locations.

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

## Planning
Since you know what enemies you will face at the destination and can estimate what enemies you'll possibly face on the journey, you should plan a party in advance to account for this. For now this is mainly a question of whether you expect to encounter armored enemies (and therefore need hammers/rondels), hordes of weak enemies (therefore want a cleaving sword/axe/hammer and heavy armor), large enemies (therefore want a polearm), and enemies with projectiles (therefore want armor/full-plate). You will also want to account for difficult terrain or the need for stealth/detection during [travel](Travel.md) in your party composition, the latter is important if you're traveling through an area with enemies that are too dangerous for your party to fight against.
### Mixed-Level
Quests are not balanced around the assumption that all members of a party have a similar power level, in fact its generally the case that you *want* a mixed-strength party. When you set out to clear out a vampire crypt, you need a very skilled armored duelist or two to fight the vampire. But you'll also be encountering plenty of zombies and skeletons, which can be efficiently dealt with by a small group of decent semi-armored combatants with clubs and axes. 
## Reward
Rewards are proportional to difficulty, which is better thought of as a measurement of risk. Each [character](../strategic/Character.md) has a value calculated in [favor](../shared/Magic.md), and each quest has a reward that can be expressed in terms of favor, so to calculate what the reward should be we can choose a power level that is ~90% likely to successfully complete a quest, estimate the favor needed to create a party of that power level times the chance of death plus the resources spent on completing the quest (food, arrows, healing), and multiply the final reward by some constant like ~1.2-1.8 so that its actually profitable. The chance of death versus difficulty of a quest though is not linear for *your* party level though. A powerful party might be twice as expensive as a weak one but only marginally less likely to die when killing a pack of goblins, so its a waste of time, and likewise the likelihood of a ragtag group of nobodies killing a vampire is basically zero, so there's always a theoretically optimal party strength for every quest.
## Interface
Case sites use the shared strategic location shell but expose no settlement services. The location name and description come from the site authority; Map lists the nearest off-road settlement destinations and reveals travel details only after selection; Loot contains unclaimed battle loot and the shared party inventory. Party portraits and character inspection use location-relative routes, so they work at both settlements and case sites.

Location headers do not contain a quest tracker. The Map building tab carries a red exclamation while the party has an active quest. Map destination rows use a gold exclamation for available quests and a red exclamation for the party's active quest route or destination. Resolving the objective does not change the active marker to gold: it remains red while guiding the party back to the issuing settlement, until turn-in removes the active quest. Quest locations do not display a separate resolution badge over the center view.

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
