Time between players is kept *somewhat* in-sync. The idea is that generally, time advances at 56x speed, so that one week in the real world is one year in-game. The main purpose of this is to account for the realistic [travel distances](Travel.md) and [healing rate](../shared/Health.md) that we use, as otherwise the game would be extraordinarily boring. However, its not *exactly* in-sync, because at minimum there is also a real-time simulation for things like [combat](../tactical/Combat.md) or navigating [difficult terrain](../shared/Terrain.md). Thus, each party is permitted to be a little-bit out-of-sync with each other, and can use accelerated downtime to catch up.

## Current implementation

The server stores official time as an absolute number of game minutes rather than a wrapping calendar value. A 365-day year is 525,600 minutes, and one game minute takes exactly 84/73 real seconds, making one game year one real week. Calendar displays wrap this absolute number into a day-of-year and time-of-day, but comparisons never wrap.

Each character has their own absolute minute. Character time advances lazily when their strategic page is accessed or their daily schedule is saved. If they are more than a year behind official time, the server advances them in one transaction to exactly one year behind and does not apply the triggering schedule change; the player can try again after the catch-up. Characters are not yet required to have matching times to join or remain in the same party.

The only implemented downtime effect is skill training. A character has a 24-hour daily budget with integer-minute allocations for every skill and labor. Leisure is the unallocated remainder and includes sleep. Server-side progression applies each skill's saved daily minutes proportionally over elapsed game time. Labor and leisure have no gameplay effects yet.

At a settlement, a player may spend whole days resting at an inn or temple, moving that character's personal time forward even if it passes official time. Rest first convalesces every injured body part at 5 percentage points per day; only the days left after the slowest injury has fully recovered apply the saved training schedule. Inn rest costs 1 gold per completed day. Temple rest is free sanctuary intended for characters down on their luck; a future karma system will account for taking undue advantage of it.

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
