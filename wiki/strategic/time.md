Time between players is kept *somewhat* in-sync. The idea is that generally, time advances at 56x speed, so that one week in the real world is one year in-game. The main purpose of this is to account for the realistic [travel distances](travel.md) and [healing rate](../shared/health.md) that we use, as otherwise the game would be extraordinarily boring. However, its not *exactly* in-sync, because at minimum there is also a real-time simulation for things like [combat](../tactical/combat.md) or navigating [difficult terrain](../shared/terrain.md). Thus, each party is permitted to be a little-bit out-of-sync with each other, and can use accelerated downtime to catch up.

## Current implementation

Disease is evaluated in the patient's personal character time. Travel, camp
rest, settlement rest, and lazy catch-up check every disease boundary crossed
by the interval. If terminal physiological failure occurs, the clock and all
work stop at that exact minute. This prevents a long skip from jumping over a
fatal peak into apparent recovery.

The server stores official time as an absolute number of game minutes rather than a wrapping calendar value. A 365-day year is 525,600 minutes, and one game minute takes exactly 84/73 real seconds, making one game year one real week. Calendar displays wrap this absolute number into a day-of-year and time-of-day, but comparisons never wrap.

The server stores an epoch rather than updating the clock table continuously. When a browser opens a page, it requests one snapshot of the character and official clocks and renders that snapshot without a wall-clock timer. The character snapshot also determines the interpolated location sky, the edge-to-edge sun or moon position, and building illumination until an explicit action returns a newer time. Authoritative reducers derive the current official minute from the epoch when gameplay needs it.

Each character has their own absolute minute. Character time advances lazily when their strategic page is accessed or their daily schedule is saved. If they are more than a year behind official time, the server advances them in one transaction to exactly one year behind and does not apply the triggering schedule change; the player can try again after the catch-up. Characters may remain out of sync while idle. At departure every living party member advances forward to the latest compatible party minute; nobody is rewound, dead members are unchanged, and excessive skew is rejected. Journeys persist that absolute departure minute plus separate movement and elapsed coordinates, so camp rest advances progress without changing distance.

Every character saves one 24-hour settlement-downtime plan with integer-minute allocations for activities; individual skill-study allocations are replaced by activities. Its unallocated Leisure remainder includes sleep. Walking advances personal time and travel condition but never trains skills or performs scheduled activities. Camp time first clears each member's fatigue; the remainder applies only activities safe and meaningful in camp, including Prayer and Leisure, while masking settlement work and crime. The pure training and settlement-activity calculations are shared with the native strategic simulation harness; the harness uses repeated one-day actions as its canonical cadence. A live bulk rest evaluates one aggregate outcome and at most one incident interruption, so rounded activity income and incidents can differ from an otherwise equivalent sequence of one-day rests; bulk-rest strategy parity remains follow-up work.

The Social dialog can persist automatic chats for an exact actor/companion
pair. When that actor receives positive discretionary downtime after
convalescence, maintenance, and fatigue recovery, the server considers enabled
companions in stable character-ID order and selects an available approach for
the first stable unaddressed source by combining the actor's effective relevant
skills with their immutable personality. A Sanguine, Gregarious actor may lean
toward humorous Charm, for example, while an Ambitious, Brave actor may favor Command;
the strongest effective skill can outweigh that disposition. Exact-score ties
favor the riskier fitting action instead of collapsing to Listen. The ordinary
authoritative social action path still decides life, party, co-location, topic,
cooldown, skill, and outcome rules. At most three companion attempts occur per
downtime interval, and at most one attempt is made for each pair; an approach
currently on cooldown is excluded from selection. Disabling the option blocks
future automatic attempts without affecting manual actions or erasing history.
Travel, generic waits, and intervals consumed entirely by required recovery or
maintenance do not trigger automatic chats.

At a settlement, every explicit activity in that plan can also be performed immediately by selecting its icon. The activity dialog chooses one to 24 whole hours, beginning at the character's current personal minute and showing the resulting end time. This advances personal time and applies that activity's training, economy, Morale, Virtue, Fatigue, and incident risk without changing the saved plan. Immediate activity is not rest: it does not heal, wash, repair equipment, provide inn service, or apply the plan's Leisure remainder. Prayer/Meditation and Carousing use their saturating morale curves over the selected interval rather than pretending their effects are linear.

Activities combine reduced-rate training with another strategic result:

- **Apprenticeship** is available after accepting a service NPC's offer to teach their profession. It costs Gold and divides conserved training time among that profession's associated skills. At profession rank 2, **Practice** replaces paid instruction and earns a small wage; at rank 4 it earns a substantially better master's income. Religious variants are called novice, cleric, and teacher rather than apprentice, journeyman, and master, and their independent practice earns Virtue instead of Gold.
- **Combat Training** includes sparring and target practice. It trains equipment-relevant Melee, Ranged, Dodge, and Block along with Will and Balance.
- **Carousing** trains Charm, grants saturating Morale, and imposes a small Virtue penalty.
- **Prayer** recites and practices prayers rather than studying doctrine. For a professed character it trains their own Religion tradition at 25% speed, and its saturating morale is scaled by the party's knowledge of that tradition. A character with no professed religion instead sees **Meditate**, receives one quarter of the ordinary saturating morale independently of party Religion, and gains no Religion hours, Fervor, or neglect.

Religion stores only direct hours in each tradition. Correlated knowledge is derived from those direct hours and never fed back into storage. Religious apprenticeship and practice train the tradition represented by the service NPC rather than an aggregate Religion skill.

Within Combat Training, current equipped hands determine the relevant Melee, Ranged, Dodge, and Block weights described in [Stats](../shared/stats.md). Training deterministically catches the lowest normalized trained hours up before maintaining their weighted balance, while also practicing Will and Balance. Changing equipment redirects future training without rewriting the saved schedule.
- **Labor** earns personal gold from effective Strength and Endurance checks during settlement downtime (`hours × (Strength + Endurance) / 4`, rounded) and trains Will at 25% speed.
- **Thievery** earns more gold in more populous settlements during downtime and trains Stealth at 25% speed. Stealth improves the take while reducing both notoriety and the continuous chance of discovery.
- **Raiding** earns gold during downtime and feeds the same equipment-derived leaf-skill distribution as Combat Training at 25% speed. It does not prefer Ranged over Melee or derive Block and Dodge practice from armor. Raiding produces high notoriety and a high retaliation chance.

The schedule previews each activity's daily Gold, Virtue, Morale, and Fatigue at the currently assigned time. Notoriety is presented as negative Virtue so future honorable activities can use positive values on the same scale. Positive preview values are green, negative values are red, and zero is neutral.

Notoriety is persisted per character and displayed as strategic state, but it has no downstream consequences yet.

Thievery and Raiding discovery is resolved whenever settlement downtime advances, including explicit rest and off-screen catch-up. The continuous exposure formulas are:

```rs
thievery_discovery = 1 - exp(-0.12 * hours * population_scale / (1 + stealth));
raiding_retaliation = 1 - exp(-0.35 * hours);
```

Raiding is checked first because an organized retaliation supersedes a watch patrol. On discovery, the activity creates a typed strategic incident independent from quests and contracts. **Caught Red-Handed** pits the party against the town watch; **Retaliation at Dawn** pits it against armed retainers. Both offer tactical combat, autoresolve, or retreat through the encounter map. The party's active quest is never replaced or mutated.

At a settlement, a player may rest at an inn or temple, moving that
character's personal time forward even if it passes official time. Rest may be
entered as whole days, or as an `HH:MM` duration keyed to a selected wake time.
The wake-time slider moves in whole-hour intervals, uses the travel planner's
solid night, sunrise, day, and sunset colors, and defaults to 08:00; the
duration's minus and plus controls change it by one hour while direct entry
accepts exact minutes. Settlement rest always schedules at least 24 hours
before the next selected clock time, preserving the character clock's exact
minute. The same control is available from the Map for free, party-wide rest
at the party's current settlement, en-route camps, and quest destinations.
Field rest permits a sub-day interval, defaults to the time needed to clear the
most-fatigued living member, and lets the leader wait for a chosen departure or
combat time; selecting the current clock time means its next-day occurrence.

Scheduled downtime uses the shared Leisure calculation documented in
[Stats](../shared/stats.md): six hours offsets baseline fatigue, tiring
activities such as Labor must then be offset, and only recovery left after the
fatigue carried into that interval reaches zero earns diminishing-return
morale. That earned result updates one capped recent-morale source at the
interval's end, so refreshing state cannot award prospective morale or stack
repeated syncs. The automatic "until healed" recommendation includes health,
field-repairable yellow equipment condition, and the remaining ETA of items
left with a craftsperson at the current settlement.

Inn rest costs 2 gold per started day and includes full board: elapsed calories
and ordinary drinking water are covered, existing food and water deficits are
cleared, and personal and party provisions are preserved. Temple rest is free
sanctuary intended for characters down on their luck; it does not provide food
or drinking water. A future karma system will account for taking undue
advantage of it.

Convalescence, blood recovery, and automatic field maintenance use only the
interval's unallocated Leisure minutes. A fully allocated 24-hour schedule
therefore grants no passive healing, while an empty schedule grants the full
interval; the absolute-minute calculation is invariant under splitting the same
interval into several rest calls. Bleeding and infection exposure still advance
through every elapsed minute, and scheduled activities apply over the same
calendar interval rather than being delayed until recovery finishes. Immediate
activities remain non-rest actions.

Inn affordability is checked against the requested stay before any rest effect
is applied. If disease or another physiological boundary clips an affordable
stay early, only the started days in the actual elapsed prefix are charged.

The rest summary itemizes the selected inn full-board charge separately from
other net spending during the interval, such as alcohol or apprenticeship,
without attributing that additional spending to a single activity.

Strategic travel adds calories to the fatigue reservoir at the current marching calibration of 6,000 calories per full day. It also consumes food and water proportionally through the persistent strategic-needs state. The fatigue reservoir remains a separate representation of exertion and future sleep pressure: eating does not erase the fatigue caused by marching. Travel, camp, temple, and private rest use carried provisions; paid inn rest feeds and hydrates its guest as part of full board. Recent morale events decay against each character's absolute strategic minute, so resting and travel both move them toward expiry.

The calendar treats Day 7 and every seventh day thereafter as Sunday. A religious character who is in a settlement on Sunday receives an explicit call to keep a full day of worship and rest. Traveling during any part of Sunday counts as refusing that call. The server applies the same continuous Fervor- and party-Command-based morale penalty once for that Sunday, including when a journey begins Saturday night and ends Monday morning. A pending Sunday demand is automatically resolved as refused when the party departs; already resolved Sundays cannot be penalized twice.

Throughout this wiki, the term "official time" refers to the *most current* time according to the server. Your character can be exactly one year behind official time, beyond that they will have to catch up with downtime (resting or training) before you can do anything else. Characters can move ahead of official time through settlement downtime; party time synchronization and its UI will be refined later.

For example, in the [example scenario](../scenario.md), at the time that they venture forth from a settlement they are in sync with the official time, but their four ~20-minute simulated encounters incurs a time-debt of 80 real-world minutes. 80 minutes in the real world is about 3 days of official time, thus when their characters return to a settlement they will ostensibly be recovering, training, relaxing, traveling between [settlements](settlement.md), or working some non-adventurous job for at least 3 days before they set out again.

> Why bother keeping players within a year of each other?

Its not necessary for the game to work, true, but it would contribute to a sense in which the world feels like a real simulation rather than an abstract game as many MMOs do. When you end up at the mercy of the simulation and sustain a very serious injury that kills your weekly playtime, there is always the last resort of just creating an additional [character](character.md). In fact, it is *expected* that players will maintain multiple alts for this purpose.

> Ok, then why allow players to desync in the first place?

Because the world is to-scale with realistic healing rates. It would be really boring even if it were constantly at 56x speed. Plus, there couldn't be a real-time tactical layer.

# Implications
The most odd implication of this is that ostensibly, characters that are further from official time are sort of prescient about certain things in the future, and characters closer to official time do not know the outcome of events which have ostensibly happened. As an example:

> A griffon has nested near a town. Geoffrey, who is 3 weeks behind official time, decides that he wants to try and slay it. However, while he is forming his party, Jack, who is 5 months behind official time, also takes the quest and slays the griffin since he put his party together very quickly. Geoffrey then learns that actually, the griffon was apparently slain several months ago.

>When Jack put his party together, he was joined by Derthert, who was actually an entire year behind official time when he saw the open party. This means that Derthert must have consulted a diviner or seen this opportunity in a dream then decided to spend 7 months training/working in this settlement before the prophecy of this quest comes true. Bizarre, but it somehow works out.

We are not actually simulating an economy (at least for the MVP) nor do quests originate from circumstances of the simulation (at least for the MVP, they're just totally arbitrary fetch/bounty quests), so the implications of this shouldn't be a big deal, but it *is* weird to think about. 

## Language exposure

Actual elapsed settlement time grants conserved ambient Oral exposure in the local distribution. Travel grants each party member at most one elapsed interval of conversation exposure, chosen from a sorted pre-gain snapshot, so companions do not multiply time. Profession work grants Written exposure from centralized literacy profiles: merchants write substantially more than smiths; medical work uses Latin; Catholic work uses Latin; Jewish work uses Hebrew and Yiddish; other current religious work uses German. A distinct physician service remains follow-up work; medical Latin currently uses the existing herbalist/medical profession seam.
Foraging advances the acting character's discrete personal strategic clock by
the actual injury/disease-safe prefix, exactly once. It is not a wall-clock job.
