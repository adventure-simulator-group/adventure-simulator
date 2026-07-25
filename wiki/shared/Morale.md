Morale is a signed strategic stat. Zero is emotionally neutral. Negative morale creates fear incapacitation, while surplus morale above zero lets a character with Command lift the spirits of allies who are below zero.

The character-sheet morale meter is informational. A raised Social meta-skill
icon beside its heading opens the observer-specific morale sources, beliefs,
and available social actions in a modal dialog without replacing either
character rail. The icon remains inset while that dialog is open; the privacy
boundary remains the active observer's beliefs rather than authoritative
personality state.

Living, co-located companions show an observer-specific Party Rail badge for
each current actionable negative source row that this character has not yet
successfully addressed. Separate sources count separately even when they share
a topic. Failed attempts, another actor's successes, and successes for another
target do not clear the badge. Selecting a notified portrait or its badge opens
that companion's Social dialog directly; the hover actions retain personal
inspection.

Successful addresses are kept as a compact actor-target-source projection only
while that exact morale source remains current. The durable interaction log
still retains both successes and failures for history, but routine Party Rail
reads never replay that append-only history.

# Morale sources

Every current morale effect is retained as a named signed source for the UI. Positive and negative sources are ranked separately by absolute magnitude. The strongest source on each side contributes fully, the second contributes one half, the third one third, and so on. Will mitigates only the ranked negative contributions:

```rs
positive_contribution = positive_source / positive_rank;
negative_contribution = negative_source / negative_rank / will_check.max(0.25);
base_morale = sum(positive_contributions) - sum(negative_contributions);
```

The current strategic sources are:

- Injuries.
- Recent victories and setbacks, which decay linearly over seven days of the affected character's strategic time.
- The difference between allied and enemy power at a quest location. Undead use a 1.5 fear multiplier and demons use 3.0; other enemies use 1.0.
- Religious conviction and mixed-faith discord.
- Morale restored by individual allies.
- Standing cleanliness: Neutral characters moderately dislike filth; Slovenly characters ignore it; Cleanly characters suffer a severe scaling penalty and receive a modest benefit while completely clean.

Food quality and disease will become additional named sources when those systems are implemented. **Comfort-seeking/Ascetic** is a good follow-up personality axis only after food and lodging distinguish quality levels, so it has meaningful conditions to react to.

# Personality reactions

Personality is stored as nine immutable, mutually-exclusive axes. Neutral axes are not shown in the UI: **Brave/Fearful**, **Ambitious/Content**, **Sanguine/Brooding**, **Gregarious/Solitary**, **Compassionate/Callous/Cruel**, **Proud/Humble**, **Zealous/Irreverent**, **Slovenly/Cleanly**, and **Temperate/Drunkard**. Conscience is present but has no morale hook until outcomes can carry durable moral context.

Alcohol preference is evaluated for every absolute nightly rest opportunity:
the sleep window begins at 18:00 and continues through 08:00, so a rest that
starts after 18:00 still processes that evening. Temperate characters neither
seek alcohol nor react to its absence.
Neutral characters seek 15 ml pure ethanol on ordinary evenings and 45 ml on
the first evening without another qualifying heavy evening in the prior seven
days; satisfaction grants +1 or +3 morale and failure gives -1 or -3. Drunkards
seek 45 ml every evening, gaining +5 when satisfied and -5 when unsatisfied.
The values are named balancing constants. A durable per-character/evening row
records consumed ethanol and whether morale was evaluated, so long rests,
short-rest sequences, departure clock synchronization, and emergency drinking
cannot duplicate an evening. The latest result replaces one refreshable
alcohol morale source at that evening's absolute 18:00 timestamp; nightly
bonuses and penalties therefore age correctly and never accumulate as an
unbounded series. This is preference, not physiological dependence.

Carousing remains a social leisure activity and Charisma-training allocation.
Its existing schedule effect is not interpreted as an additional inventory-
backed alcohol reward; the nightly alcohol event is the only alcohol-specific
morale source.

Reactions modify each raw source before positive/negative ranking and Will mitigation. Brave/Fearful halves/doubles outmatched fear; Ambitious/Content multiplies victory and defeat by 1.5/0.5; Sanguine favors positive sources by 1.25 and negative sources by 0.75 while Brooding does the reverse. Sanguine negative events last half the normal duration and Brooding ones last twice as long. Proud multiplies victory by 1.5 and defeat by 3, while Humble multiplies both by 0.75. Zealous/Irreverent multiplies religious conviction, prayer, discord, neglect, and religious events by 1.5/0.5. Gregarious/Solitary multiplies incoming named ally restoration by 1.5/0.5 before the existing cap at neutral morale.

Personality changes what an event means, but true tags are never appended to public source labels. Will governs coping with ranked negative morale, Command governs the party restoration budget, Religion governs tradition-specific knowledge, and Conviction governs personal and cohort ardor.

# Lifting allies

Party Command does not contribute a permanent flat morale source. Instead, positive-morale party members share one party-wide restoration budget. Command uses its own social-coverage aggregation rather than the generic party skill formula. The strongest member supplies the base check. Additional members provide a rapidly saturating coordination bonus, then help or hinder according to how far their individual check is above or below the neutral 2.5 baseline:

```rs
let coordination = 1.125 * (1.0 - (1.0 / 3.0).powi(supporter_count));
let support = supporters.map(|check| 0.5 * (check - 2.5)).sum();
let party_command = (best_check + coordination + support).clamp(0.0, 5.0);
```

This produces approximately 4.5 from one character at 4.5, three characters at 3, or a 4 and a 2. Adding large numbers of characters at 1 or 2 lowers the result once their limited coordination benefit is exhausted.

The party's positive base-morale values are aggregated with the same ranked diminishing returns. The resulting restoration percentage approaches, but never reaches, a limit of 5% per point of the aggregate party Command check:

```rs
let party_command = aggregate_party_command(member_command_checks);
let party_surplus = cumulative_morale(member_positive_base_morale);
let saturation = 1.0 - (-party_surplus / 10.0).exp();
let party_restoration = saturation * 0.05 * party_command;
```

Ten aggregated surplus morale reaches about 63% of the party's limit, 20 reaches about 86%, and 30 reaches about 95%. Party Command is capped at 5, so the shared restoration limit cannot exceed 25% regardless of party size.

The party budget is divided among positive-morale members in proportion to their individual surplus, allowing the UI to show who is doing the encouraging without applying the party bonus more than once. All surplus values are calculated before receiving help from allies. This makes the relationship acyclic: two high-morale characters cannot recursively increase one another's output. If support would restore more than the listener's entire deficit, the named contributions are reduced proportionally. Ally support can lift a character only to zero and can never create surplus morale.

# Fear and the morale meter

Within the negative half, successful `social_interaction` morale is shown as a
separate purple striped segment beside the remaining fear. Its size is computed
from the same current, ranked morale-source projection shown in the Social
dialogue, and is capped by the gross current actionable negative contribution.
It is therefore an explanation of the current net morale, like a treated
injury segment, rather than extra morale. Color, striping, and meter text all
identify the segment.

Each negative morale point produces one percentage point of fear incapacitation, so -100 morale is the meaningful left endpoint of the meter. The center represents neutral morale. The right side shows the character's allocated share of the party's current ally-restoration percentage relative to the party's present `5% × aggregate Command` limit. Selecting the meter opens the dedicated social panel and source actions.

The strategic condition and morale-source tables are refreshable projections. Durable state remains in character condition, injuries, strategic time, time-stamped morale events, and static personality. A negative event's persisted expiration already includes its Sanguine or Brooding duration adjustment, so `expires_at_minute` is authoritative rather than a projection-only reinterpretation. Personality is assigned before ordinary NPC events are recorded and is immutable thereafter. Personality is strategic identity, never tactical tick state. A missing legacy personality row is safely treated as fully neutral.

# Religion

A character makes or changes their religious profession by speaking with a priest at a church. Each settlement currently has one church and one fixed faith; its priest can convert a character only to that faith. Religion is a dialogue topic even when the priest also has a quest to discuss, rather than a service-menu choice. A priest cannot make a character faithless. Characters renounce their current faith from the Religion entry on their own biography instead. Large cities may eventually support multiple churches, but that is outside the current settlement model.

Only a professed Zealous character receives the positive religious-leadership morale source. Its magnitude comes from the party's aggregate effective Religion check for that character's own tradition, and any living member may contribute regardless of profession. Same-profession social pressure instead comes from personality Conviction: Zealous contributes 5.0, Neutral 2.5, and Irreverent 0.0. A character with no professed religion receives neither this source nor religious pressure.

For each believer, the other religious cohorts are combined into foreign faith pressure. Mixed-faith tension is deliberately subtractive: party Command is subtracted from that pressure, and only the uncovered remainder becomes raw negative morale. This means capable social leadership can remove discord entirely rather than merely dividing it down:

```rs
let foreign_pressure = aggregate_party_check(other_cohort_checks).clamp(0.0, 5.0);
let discord = 3.0 * (foreign_pressure - party_command).max(0.0);
```

The resulting `Religious discord` source then receives the same negative-source ranking and Will mitigation as other morale penalties. A unified party therefore gets the largest available conviction benefit without discord; a mixed party retains the conviction of each faith but generally pays a leadership-dependent cost.

# Fervor

Fervor is a bounded strategic pressure meter, not another morale source. It shows how close religious conviction is to becoming inflexible behavior. Individual personality Conviction, the character's same-profession Conviction cohort, and surplus morale raise pressure; aggregate party Command is subtracted as restraint. Characters with no professed religion always have zero Fervor.

```rs
let pressure = (individual_conviction + cohort_conviction + positive_morale / 10.0
    - party_command - 2.5).max(0.0);
let fervor = 1.0 - (-pressure / 5.0).exp();
```

The curve lets arbitrarily high pressure approach 100% without reaching it. The strategic character rail displays this value from Calm through Fervent to Frenzy.

Daily prayer is an activity in the settlement-downtime schedule rather than a dialogue-style demand. Its existing saturating morale is multiplied by the party's tradition-specific Religion check divided by five, then receives the normal religious personality reaction. It trains direct hours in the professed tradition at 25% of explicit study speed. Fervor creates a continuous desired prayer allocation of up to two hours per day. A character without a profession instead meditates: this gives 25% of the saturating morale independently of Religion checks and personality religious scaling, and creates no Religion study, Fervor, or neglect. Scheduled prayer and meditation are not attempted while traveling.

Sunday remains an explicit demand rather than a random Fervor event. Day 7 and every seventh calendar day thereafter is Sunday. A professing character with nonzero Fervor who is at a settlement receives the choice once that Sunday:

- **Observe:** spend one full day in settlement. The character receives a small positive morale event.
- **Do not observe:** keep complete freedom of action. The raw morale penalty is `max(0, 8 × Fervor − 1.6 × party Command)`, so both Fervor and Command change the result continuously and a Command check of 5 eliminates even the maximum penalty.

Any strategic journey that overlaps Sunday counts as choosing not to observe it and applies exactly the same penalty once per character for that Sunday. This includes leaving Saturday night and returning Monday morning. A pending Sunday prompt is resolved as refused on departure, while an already answered Sunday cannot be charged twice. Demands are choices, not involuntary character actions, but spending Sunday on the road is itself the party leader's choice.

## A Quarrel at the Gate

The first severe Fervor incident is a single cross-faith settlement scenario. When a party arrives at a settlement, the highest-Fervor member who follows a different religion rolls against their current Fervor. On success they insult the local faith and draw an armed crowd. A character at 20% therefore has a 20% arrival chance and one at 80% has an 80% chance, with no unlock threshold. Each party can trigger this incident only once per settlement.

The incident deliberately reuses the quest-location combat flow. Arrival is interrupted at a zero-distance encounter named **A Quarrel at the Gate**. The party can:

- Initiate tactical combat using the normal tactical-server request.
- Autoresolve using the normal quest autoresolve damage and battle-result path.
- Open the encounter map and travel away without fighting.

The incident temporarily occupies the party's active-encounter slot while preserving any real active quest. Winning or leaving restores that quest. Leaving marks the incident avoided and does not immediately trigger another incident at the destination reached by that retreat. The same shared encounter machinery also handles Thievery and Raiding discoveries, although those activities use their own scenario text and risk formulas. There are no religious quest-choice demands; quest dialogue consequences for mixed-faith parties remain future work.
# Social responses

Morale sources are not dialogue memories. Each source has a closed topic such
as defeat, injury, fatigue, hunger, faith, or filth. The character page's
Morale meter opens a social panel where a party member can listen, commiserate,
use humor, rally with Command, offer a deceptive reframe, or flirt. Labels are
generic and grounded in the durable source; the game does not invent incidental
details about a battle or conversation.

Successful realized morale improvement increases the recipient's directional
Affinity toward the actor. Gains diminish near the positive cap. Failure,
exposure, or a boundary-crossing response can lower Affinity. Repeating the same
approach to the same source has a 24-hour target-clock cooldown. Passive party
morale projection uses Command but never creates Affinity.

Command contributed to another party member is multiplied by that pair's best shared Oral-language coefficient. The same coefficient scales other directed Social skill checks; self-directed reflection is unaffected.
