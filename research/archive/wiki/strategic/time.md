Settlements inhabit the canonical present. Their NPCs, calendars, seasons,
weather, holy days, and celestial cycles follow official time. Official time
advances continuously at the exact rate of one game year per real week: one
game minute per 84/73 real seconds, or about 52.14 times real time.

Player characters are changelings who do not belong entirely to that timeline.
Rest and adventure can carry them through years while scarcely any time passes
in the world. Their age, wounds, training, needs, and other personal
consequences still advance. Another player character does not have to catch up
before joining them.

A party that leaves a settlement begins one subjective journey clock. The
leader chooses its starting time of day. The party then tracks total elapsed
time since setting out, not merely a repeating 24-hour clock, so the travel
planner can retain the history and progress of a long expedition. Time of day
repeats within that elapsed time, but the canonical date, weekday, season, and
moon phase remain fixed. Camps and case sites belong to the journey clock.

Returning to any settlement restores its canonical time of day. Each arriving
character spends only the additional personal time needed to reach that time of
day, always less than 24 hours. The interval has the ordinary consequences of
rest and downtime. A character who can afford an inn pays for full board;
otherwise the church provides sanctuary. Subjective months in the wilderness
therefore never require months of arrival downtime.

## Current implementation

Reading is a saved settlement allocation and immediate activity. It selects
useful personal books first, deduplicated by stable item ID. If none can teach
the character, a City or Capital bookstore supplies free on-site study. The
lowest applicable band and then stable item ID determine selection; finishing a
band during a long interval cascades into the next eligible title. Reading is
allocated time rather than Leisure, and is disabled during travel and camp.

Disease is evaluated in the patient's personal character time. Travel, camp
rest, settlement rest, and other elapsed-time actions check every disease
boundary crossed by the interval. If terminal physiological failure occurs, the
clock and all work stop at that exact minute. This prevents a long skip from
jumping over a fatal peak into apparent recovery.

The same centralized interval evaluation assembles settlement exposure,
within-party close-contact transmission, and infected-blood exposure before
acquisition. Party travel, camp rest, and treatment first construct one
immutable interval plan for the characters explicitly advancing together.
That plan projects only those characters' open pair-presence spans through
their shared horizon, then supplies the same acquisition proposals to every
preview and commit. Participant iteration order therefore cannot change who
was protected or infected. Existing closed spans and companions who are not
co-advancing remain clamped to their recorded clocks. Splitting an otherwise
identical interval does not reroll exposure.

Solo elapsed-time actions do not project an absent companion forward and do not
retroactively substitute the character's current Physiology rank. They use only
recorded pair-presence history, including open span overlap through an
already-ahead peer's current clock but never beyond it. Point actions can use
the current organization role without changing elapsed-time semantics. Planning
fetches relevant spans through the low/high participant indexes, deduplicates
them, compiles piecewise coverage, and caps both spans and exposure work.

All assembled community, infected-blood, and contact candidates enter one
absolute-minute timeline. Candidates in the same minute resolve
simultaneously; a newly infected character becomes a contact source on the
next minute. This preserves multi-person transmission chains across whole and
chunked travel, sleep, treatment, and rest intervals.
Long-running outbreaks and local problems keep stable future attempts in that
timeline, so exposure resumes after an earlier episode resolves instead of
depending on where the player split the action.

The interval work reservation includes both the side-effect-free infected-blood
planning pass and the bounded checkpoint pass over the actually committed
prefix. Raw indexed presence rows are rejected as soon as their deduplicated
count exceeds the cap, before coverage materialization.

The server stores official time as an absolute number of game minutes rather
than a wrapping calendar value. A newly initialized world begins on August 20 at
00:00. A 365-day year is 525,600 minutes, and one game minute takes exactly
84/73 real seconds, making one game year one real week. Calendar displays wrap
this absolute number into a day-of-year and time-of-day, but comparisons never
wrap.

The server stores an epoch rather than updating the clock table continuously.
When a browser opens a page, it requests one snapshot of the character and
official clocks and renders that snapshot without a wall-clock timer. The
character snapshot also determines the interpolated location sky, the
edge-to-edge sun or moon position, and building illumination until an explicit
action returns a newer time. Authoritative reducers derive the current official
minute from the epoch when gameplay needs it.

Each character has an independent subjective minute. Rest, travel, waiting,
healing, training, and aging advance it without advancing official time. Party
formation does not synchronize these clocks. A journey separately persists
its canonical departure anchor, chosen starting time of day, total subjective
elapsed time, and movement progress. Camp rest advances elapsed progress without
changing distance.

After marriage, the spouse, household, and immediate family share the player
character's subjective advances. This does not make ceremonies subjective:
weddings remain scheduled events in official time. A character can therefore
spend decades adventuring between scheduling a wedding and attending it, with
no special dialogue required for the resulting age difference.

Persistent NPCs use this same `CharacterTime` row as their authoritative
personal date. An `NpcPolicy` is authority over an ordinary full Character,
not a parallel clock. NPC policy advancement rejects retroactive targets and
atomically settles due residence billing, weddings, and births after writing
the new frontier; a failed settlement rolls back the clock write with it.
The module seeds one private recurring causal processor. Each invocation first
settles one bounded queue containing both weddings and births, ordered by
`(effective minute, event-kind precedence, event identity)` without waiting
for a participant login. Weddings precede births when they share an exact
minute. Indexed due-minute scans are capped before merging, and a malformed
gameplay event is terminalized with a durable failure receipt so it cannot
stall later events or the recurring schedule. The processor then selects the
earliest exact-`CharacterTime` cohort, ordered by character ID and capped at
64. Every member records the day's policy decisions before any member
advances, after which each advances through at most one day of its ordinary
stationary schedule. A private character/day/phase receipt makes retries
idempotent. The first policy pass initializes an otherwise untouched NPC
schedule with six hours of Labor and a stable one-to-two hours of Socializing
in 15-minute units; any existing plan is preserved, and later passes never
overwrite it. Dead characters remain frozen. Scheduler jitter therefore
changes catch-up latency, not durable ordering, choices, or step size.
Actor-local dialogue, trade, guild, quest, and rest interactions may remain
asynchronous when they do not mutate that canonical NPC state. Those reducers
declare their temporal scope and never move the target's personal clock.
Autonomous trade, travel, and guild decisions are intentionally outside this
first lifecycle parity gate; future policies must use the same cohort and
receipt contract rather than adding another NPC clock.
Scheduled ceremonies materialize when official time reaches them. They neither
wait for nor synchronize the participants' subjective clocks.

Every character saves one global 24-hour downtime plan with integer-minute
allocations for activities; moving never edits that plan. At each execution
boundary the server makes an effective copy. Every 15-minute segment assigned to
an unavailable activity is independently reassigned to another available
non-Leisure activity already present in the plan, with selection probability
proportional to that activity's saved minutes. If there is no such activity, the
segment remains unallocated Leisure. The character ID seeds these draws so
authoritative execution and the schedule preview agree without persisting a
second plan. Thievery is available only inside settlements, Raiding only at
stationary named locations outside settlements (currently positive-distance case
sites that are not incident sites), and Carousing only in settlements whose
economy provides an inn. Training, income, Morale, reputation, fatigue,
restorative Leisure, and incident risk all use this effective copy. Walking
advances personal time and travel condition but never trains skills or performs
scheduled activities. Journey-camp rest retains its existing camp-safe rules and
offers no immediate activity controls. The pure training and activity
calculations are shared with the native strategic simulation harness; the
harness uses repeated one-day actions as its canonical cadence. A live bulk rest
evaluates one aggregate outcome and at most one incident interruption, so
rounded activity income and incidents can differ from an otherwise equivalent
sequence of one-day rests; bulk-rest strategy parity remains follow-up work.

The Social dialog can persist automatic chats for an exact actor/companion pair.
When that actor receives positive discretionary downtime after convalescence,
maintenance, and fatigue recovery, the server considers enabled companions in
stable character-ID order and selects an available approach for the first stable
unaddressed source by combining the actor's effective relevant skills with their
immutable personality. A Sanguine, Gregarious actor may lean toward humorous
Charm, for example, while an Ambitious, Brave actor may favor Command; the
strongest effective skill can outweigh that disposition. Exact-score ties favor
the riskier fitting action instead of collapsing to Listen. The ordinary
authoritative social action path still decides life, party, co-location, topic,
cooldown, skill, and outcome rules. At most three companion attempts occur per
downtime interval, and at most one attempt is made for each pair; an approach
currently on cooldown is excluded from selection. Disabling the option blocks
future automatic attempts without affecting manual actions or erasing history.
Travel, generic waits, and intervals consumed entirely by required recovery or
maintenance do not trigger automatic chats.

At settlements and stationary case sites, currently available explicit
activities can also be performed immediately by selecting their icons.
Unavailable activities remain visible and schedulable but explain their location
requirement instead of opening the dialog. The activity dialog chooses one to 24
whole hours, beginning at the character's current personal minute and showing
the resulting end time. The reducer rechecks authoritative location before
changing the clock or character state. This advances personal time and applies
that activity's training, economy, Morale, origin-settlement reputation,
Fatigue, and incident risk without changing the saved plan. Immediate activity
is not rest: it does not heal, wash, repair equipment, provide inn service, or
apply the plan's Leisure remainder. Prayer/Meditation and Carousing use their
saturating morale curves over the selected interval rather than pretending their
effects are linear.

Activities combine reduced-rate training with another strategic result:

- **Apprenticeship** is available after accepting a service NPC's offer to teach their profession. It costs Gold and divides conserved training time among that profession's associated skills. At profession rank 2, **Practice** replaces paid instruction and earns a small wage; at rank 4 it earns a substantially better master's income. Religious variants are called novice, cleric, and teacher rather than apprentice, journeyman, and master, and their visible independent practice earns local Fame instead of Gold.
- **Combat Training** includes sparring and target practice. It trains equipment-relevant Melee, Ranged, Dodge, and Block along with Will and Balance.
- **Carousing** requires a settlement with an inn, trains Charm, and grants saturating Morale. Ordinary carousing changes no reputation, but it can cause a disorder incident that adds local Infamy; Drunkards have substantially higher risk and Temperate characters lower risk.
- **Prayer** recites and practices prayers rather than studying doctrine. For a professed character it trains their own Religion tradition at 25% speed, and its saturating morale is scaled by the party's knowledge of that tradition. A character with no professed religion instead sees **Meditate**, receives one quarter of the ordinary saturating morale independently of party Religion, and gains no Religion hours, Fervor, or neglect.

Religion stores only direct hours in each tradition. Correlated knowledge is
derived from those direct hours and never fed back into storage. Religious
apprenticeship and practice train the tradition represented by the service NPC
rather than an aggregate Religion skill.

Within Combat Training, current equipped hands determine the relevant Melee,
Ranged, Dodge, and Block weights described in [Stats](../shared/stats.md).
Training deterministically catches the lowest normalized trained hours up before
maintaining their weighted balance, while also practicing Will and Balance.
Changing equipment redirects future training without rewriting the saved
schedule.
- **Labor** earns personal gold from effective Strength and Endurance checks during settlement downtime (`hours × (Strength + Endurance) / 4`, rounded) and trains Will at 25% speed.
- **Thievery** is available only inside settlements, earns more gold in more populous settlements, and trains Stealth at 25% speed. Stealth improves the take while reducing both Infamy and the continuous chance of discovery.
- **Raiding** is available only outside settlements at stationary named locations. It earns gold against the location's origin-settlement economy and feeds the same equipment-derived leaf-skill distribution as Combat Training at 25% speed. It does not prefer Ranged over Melee or derive Block and Dodge practice from armor. Raiding produces origin-settlement Infamy and a high retaliation chance.

The schedule previews each activity's daily Gold, Fame/Infamy, Morale, and
Fatigue at the currently assigned time. Fame is positive and Infamy is negative
in this compact preview; the two remain independent stored tracks.

See [settlement reputation](reputation.md) for population dilution, spillover,
NPC reactions, and authority consequences.

Thievery and Raiding discovery is resolved whenever effective downtime advances,
including explicit activity and off-screen catch-up. Case-site Raiding
attributes currency, reputation, and incident provenance to the case site's
authoritative origin settlement. The continuous exposure formulas are:

```rs
thievery_discovery = 1 - exp(-0.12 * hours * population_scale / (1 + stealth));
raiding_retaliation = 1 - exp(-0.35 * hours);
```

Raiding is checked first because an organized retaliation supersedes a watch
patrol. On discovery, the activity creates a typed strategic incident
independent from quests and contracts. **Caught Red-Handed** pits the party
against the town watch; **Retaliation at Dawn** pits it against armed retainers.
Both offer tactical combat, autoresolve, or retreat through the encounter map.
The party's active quest is never replaced or mutated.

At a settlement, rest is chosen in whole days and advances only that character's
subjective life. The same interval rules apply whether the character rests at
an inn, church, or residence. Field rest remains a party-local action and may
use sub-day intervals at camps and case sites.

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
cleared, and personal and party provisions are preserved. Ordinary settlement
water is also consumed automatically during non-inn settlement rest, clearing
thirst without spending carried water. Temple rest is free sanctuary intended
for characters down on their luck, but it does not provide food. A future karma
system will account for taking undue advantage of it.

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

Strategic travel adds calories to the fatigue reservoir at the current marching
calibration of 6,000 calories per full day. It also consumes food and water
proportionally through the persistent strategic-needs state. The fatigue
reservoir remains a separate representation of exertion and future sleep
pressure: eating does not erase the fatigue caused by marching. Travel, camp,
and private rest use carried provisions. Temple rest uses carried food and
ordinary settlement water; paid inn rest feeds and hydrates its guest as part of
full board. Recent morale events decay against each character's absolute
strategic minute, so resting and travel both move them toward expiry.

Every authoritative personal-clock path applies weather exposure once, after
its actually elapsed prefix is committed. This includes terminal-clipped
travel, settlement activity/rest, field waiting/rest, and official-time
synchronization. The pure minute calculation gives the same result when an
interval is partitioned. Settlement shelter blocks rain and wind; explicit
field rest instead uses the chosen bivouac or party-owned tent. Rest at an
inn, temple, or residence occurs indoors: exterior temperature cannot create
new thermal strain there, and existing wetness and strain recover toward
neutral.

The canonical calendar treats Day 7 and every seventh day thereafter as Sunday.
Religious observance follows that calendar rather than the journey's subjective
day count. Leaving on Saturday and spending subjective months in the wilderness
does not create months of missed Sundays. Only the official interval that
actually passed can create an observance consequence.

Throughout this wiki, "official time" means the settlement world's canonical
present. NPC facts occur there. An NPC can die during a quest or marry a player
without creating a past-versus-future paradox, because no player character's
subjective date determines the NPC's present.

## Implications

A changeling may speak to an NPC, spend several subjective decades resting and
adventuring, and speak to the same NPC again after only minutes of official
time. The NPC might reasonably fail to recognize them or be profoundly
confused. The initial implementation permits this without adding special
dialogue reactions.

## Language exposure

Actual elapsed settlement time grants conserved ambient Oral exposure in the
local distribution. Travel grants each party member at most one elapsed interval
of conversation exposure, chosen from a sorted pre-gain snapshot, so companions
do not multiply time. Profession work grants Written exposure from centralized
literacy profiles: merchants write substantially more than smiths; medical work
uses Latin; Catholic work uses Latin; Jewish work uses Hebrew and Yiddish; other
current religious work uses German. Herbalist, physician, and surgeon
organization identities are distinct even where medical literacy uses the same
language profile. Foraging advances the acting character's discrete personal
strategic clock by the actual injury/disease-safe prefix, exactly once. It is
not a wall-clock job.
