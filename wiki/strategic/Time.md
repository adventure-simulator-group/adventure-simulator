Time between players is kept *somewhat* in-sync. The idea is that generally, time advances at 56x speed, so that one week in the real world is one year in-game. The main purpose of this is to account for the realistic [travel distances](Travel.md) and [healing rate](Health.md) that we use, as otherwise the game would be extraordinarily boring. However, its not *exactly* in-sync, because at minimum there is also a real-time simulation for things like [combat](Combat.md) or navigating [difficult terrain](Difficult Terrain). Thus, each party is permitted to be a little-bit out-of-sync with each other, and can use accelerated downtime to catch up.

Throughout this wiki, the term "official time" refers to the *most current* time according to the server. Your character can be exactly one year behind official time, beyond that they will have to catch up with downtime (resting or training) before you can do anything else. You cannot go *ahead* of official time.

For example, in the [example scenario](Scenario), at the time that they venture forth from a settlement they are in sync with the official time, but their four ~20-minute simulated encounters incurs a time-debt of 80 real-world minutes. 80 minutes in the real world is about 3 days of official time, thus when their characters return to a settlement they will ostensibly be recovering, training, relaxing, traveling between [settlements](Settlement.md), or working some non-adventurous job for at least 3 days before they set out again.

> Why bother keeping players within a year of each other?

Its not necessary for the game to work, true, but it would contribute to a sense in which the world feels like a real simulation rather than an abstract game as many MMOs do. When you end up at the mercy of the simulation and sustain a very serious injury that kills your weekly playtime, there is always the last resort of just creating an additional [character](Character). In fact, it is *expected* that players will maintain multiple alts for this purpose. 

> Ok, then why allow players to desync in the first place?

Because the world is to-scale with realistic healing rates. It would be really boring even if it were constantly at 56x speed. Plus, there couldn't be a real-time tactical layer.

# Implications
The most odd implication of this is that ostensibly, characters that are further from official time are sort of prescient about certain things in the future, and characters closer to official time do not know the outcome of events which have ostensibly happened. As an example:

> A griffon has nested near a town. Geoffrey, who is 3 weeks behind official time, decides that he wants to try and slay it. However, while he is forming his party, Jack, who is 5 months behind official time, also takes the quest and slays the griffin since he put his party together very quickly. Geoffrey then learns that actually, the griffon was apparently slain several months ago.

>When Jack put his party together, he was joined by Derthert, who was actually an entire year behind official time when he saw the open party. This means that Derthert must have consulted a diviner or seen this opportunity in a dream then decided to spend 7 months training/working in this settlement before the prophecy of this quest comes true. Bizarre, but it somehow works out.

We are not actually simulating an economy (at least for the MVP) nor do quests originate from circumstances of the simulation (at least for the MVP, they're just totally arbitrary fetch/bounty quests), so the implications of this shouldn't be a big deal, but it *is* weird to think about. 