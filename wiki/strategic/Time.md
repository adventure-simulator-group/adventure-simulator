Time between players is kept *somewhat* in-sync. The idea is that generally, time advances at 56x speed, so that one week in the real world is one year in-game. The main purpose of this is to account for the realistic [travel distances](Travel.md) and [healing rate](../shared/Health.md) that we use, as otherwise the game would be extraordinarily boring. However, its not *exactly* in-sync, because at minimum there is also a real-time simulation for things like [combat](../tactical/Combat.md) or navigating [difficult terrain](../shared/Terrain.md). Thus, each party is permitted to be a little-bit out-of-sync with each other, and can use accelerated downtime to catch up.

## Current implementation

Disease is evaluated in the patient's personal character time. Travel, camp
rest, settlement rest, and lazy catch-up check every disease boundary crossed
by the interval. If terminal physiological failure occurs, the clock and all
work stop at that exact minute. This prevents a long skip from jumping over a
fatal peak into apparent recovery.

The server stores official time as an absolute number of game minutes rather than a wrapping calendar value. A 365-day year is 525,600 minutes, and one game minute takes exactly 84/73 real seconds, making one game year one real week. Calendar displays wrap this absolute number into a day-of-year and time-of-day, but comparisons never wrap.

The server stores an epoch rather than updating the clock table continuously. When a browser opens a page, it requests one snapshot of the character and official clocks and renders that snapshot without a wall-clock timer. The character snapshot also determines the location sky and building illumination until an explicit action returns a newer time. Authoritative reducers derive the current official minute from the epoch when gameplay needs it.

Each character has their own absolute minute. Character time advances lazily when their strategic page is accessed or their daily schedule is saved. If they are more than a year behind official time, the server advances them in one transaction to exactly one year behind and does not apply the triggering schedule change; the player can try again after the catch-up. Characters are not yet required to have matching times to join or remain in the same party.

Implemented schedule effects include skill training, activities, and strategic-condition recovery. Every character saves one 24-hour settlement-downtime plan with integer-minute allocations for every skill plus Prayer, Labor, Thievery, and Raiding. Its unallocated Leisure remainder includes sleep. Travel advances personal time and travel condition but never trains skills or performs scheduled activities. The pure training and settlement-activity calculations are shared with the native strategic simulation harness; the harness uses repeated one-day actions as its canonical cadence. A live bulk rest evaluates one aggregate outcome and at most one incident interruption, so rounded activity income and incidents can differ from an otherwise equivalent sequence of one-day rests; bulk-rest strategy parity remains follow-up work.

Activities combine reduced-rate training with another strategic result:

- **Prayer** recites and practices prayers rather than studying doctrine. During settlement downtime it trains Faith at 25% speed, adds a saturating daily-prayer morale source, and covers a Fervor-scaled prayer obligation.
- **Labor** earns personal gold from effective Strength and Endurance checks during settlement downtime and trains Will at 25% speed.
- **Thievery** earns more gold in more populous settlements during downtime and trains Stealth at 25% speed. Stealth improves the take while reducing both notoriety and the continuous chance of discovery.
- **Raiding** earns gold during downtime and trains weapon-appropriate combat skills at 25% speed. Equipped ranged weapons train Ranged; other weapons train Melee; heavier armor adds Block practice while lighter armor adds Dodge practice. Raiding produces high notoriety and a high retaliation chance.

The schedule previews each activity's daily Gold, Virtue, Morale, and Fatigue at the currently assigned time. Notoriety is presented as negative Virtue so future honorable activities can use positive values on the same scale. Positive preview values are green, negative values are red, and zero is neutral.

Notoriety is persisted per character and displayed as strategic state, but it has no downstream consequences yet.

Thievery and Raiding discovery is resolved whenever settlement downtime advances, including explicit rest and off-screen catch-up. The continuous exposure formulas are:

```rs
thievery_discovery = 1 - exp(-0.12 * hours * population_scale / (1 + stealth));
raiding_retaliation = 1 - exp(-0.35 * hours);
```

Raiding is checked first because an organized retaliation supersedes a watch patrol. On discovery, the activity reuses the same temporary quest-backed combat interruption as the religious settlement incident. **Caught Red-Handed** pits the party against the town watch; **Retaliation at Dawn** pits it against armed retainers. Both offer tactical combat, autoresolve, or retreat through the encounter map. The party's real active quest is restored after victory or retreat.

At a settlement, a player may spend whole days resting at an inn or temple, moving that character's personal time forward even if it passes official time. Rest first convalesces every injured body part at 1 percentage point plus 1 percentage point per point of the party Medicine check per day (capped at 6%) and restores 1% of maximum blood volume per day. Remaining time automatically maintains carried equipment, repairing condition bins one and two when the character's Smithing rating reaches the bin; only time left after health and maintenance applies the saved training schedule. Convalescence and maintenance time count as pure rest for fatigue. Scheduled downtime uses the shared Leisure calculation documented in [Stats](../shared/Stats.md): six hours offsets baseline fatigue, tiring activities such as Labor must then be offset, and only recovery left after the fatigue carried into that interval reaches zero earns diminishing-return morale. That earned result updates one capped recent-morale source at the interval's end, so refreshing state cannot award prospective morale or stack repeated syncs. The automatic "until healed" recommendation includes health, field-repairable yellow equipment condition, and the remaining ETA of items left with a smith at the current settlement. Inn rest costs 1 gold per completed day. Temple rest is free sanctuary intended for characters down on their luck; a future karma system will account for taking undue advantage of it.

Strategic travel adds calories to the fatigue reservoir at the current marching calibration of 6,000 calories per full day. It also consumes food and water proportionally through the persistent strategic-needs state. The fatigue reservoir remains a separate representation of exertion and future sleep pressure: eating does not erase the fatigue caused by marching. Settlement rest and lazy settlement catch-up keep characters fed and hydrated, while travel automatically uses carried personal provisions. Recent morale events decay against each character's absolute strategic minute, so resting and travel both move them toward expiry.

The calendar treats Day 7 and every seventh day thereafter as Sunday. A religious character who is in a settlement on Sunday receives an explicit call to keep a full day of worship and rest. Traveling during any part of Sunday counts as refusing that call. The server applies the same continuous Fervor- and party-Charisma-based morale penalty once for that Sunday, including when a journey begins Saturday night and ends Monday morning. A pending Sunday demand is automatically resolved as refused when the party departs; already resolved Sundays cannot be penalized twice.

Throughout this wiki, the term "official time" refers to the *most current* time according to the server. Your character can be exactly one year behind official time, beyond that they will have to catch up with downtime (resting or training) before you can do anything else. Characters can move ahead of official time through settlement downtime; party time synchronization and its UI will be refined later.

For example, in the [example scenario](../Scenario.md), at the time that they venture forth from a settlement they are in sync with the official time, but their four ~20-minute simulated encounters incurs a time-debt of 80 real-world minutes. 80 minutes in the real world is about 3 days of official time, thus when their characters return to a settlement they will ostensibly be recovering, training, relaxing, traveling between [settlements](Settlement.md), or working some non-adventurous job for at least 3 days before they set out again.

> Why bother keeping players within a year of each other?

Its not necessary for the game to work, true, but it would contribute to a sense in which the world feels like a real simulation rather than an abstract game as many MMOs do. When you end up at the mercy of the simulation and sustain a very serious injury that kills your weekly playtime, there is always the last resort of just creating an additional [character](Character.md). In fact, it is *expected* that players will maintain multiple alts for this purpose. 

> Ok, then why allow players to desync in the first place?

Because the world is to-scale with realistic healing rates. It would be really boring even if it were constantly at 56x speed. Plus, there couldn't be a real-time tactical layer.

# Implications
The most odd implication of this is that ostensibly, characters that are further from official time are sort of prescient about certain things in the future, and characters closer to official time do not know the outcome of events which have ostensibly happened. As an example:

> A griffon has nested near a town. Geoffrey, who is 3 weeks behind official time, decides that he wants to try and slay it. However, while he is forming his party, Jack, who is 5 months behind official time, also takes the quest and slays the griffin since he put his party together very quickly. Geoffrey then learns that actually, the griffon was apparently slain several months ago.

>When Jack put his party together, he was joined by Derthert, who was actually an entire year behind official time when he saw the open party. This means that Derthert must have consulted a diviner or seen this opportunity in a dream then decided to spend 7 months training/working in this settlement before the prophecy of this quest comes true. Bizarre, but it somehow works out.

We are not actually simulating an economy (at least for the MVP) nor do quests originate from circumstances of the simulation (at least for the MVP, they're just totally arbitrary fetch/bounty quests), so the implications of this shouldn't be a big deal, but it *is* weird to think about. 
