Morale is a signed strategic stat. Zero is emotionally neutral. Negative morale creates fear incapacitation, while surplus morale above zero lets a charismatic character lift the spirits of allies who are below zero.

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
- Morale restored by individual allies.

Food quality and disease will become additional named sources when those systems are implemented.

# Lifting allies

Party Charisma no longer contributes a permanent flat morale source. Instead, positive-morale party members share one party-wide restoration budget. Individual Charisma checks are first combined with the normal diminishing-returns party aggregation: the strongest check counts fully, the second at one half, the third at one third, and so on. This prevents a group of similar charismatic characters from stacking independent full-strength bonuses.

The party's positive base-morale values are aggregated with the same ranked diminishing returns. The resulting restoration percentage approaches, but never reaches, a limit of 5% per point of the aggregate party Charisma check:

```rs
let party_charisma = aggregate_party_check(member_charisma_checks);
let party_surplus = cumulative_morale(member_positive_base_morale);
let saturation = 1.0 - (-party_surplus / 10.0).exp();
let party_restoration = saturation * 0.05 * party_charisma;
```

Ten aggregated surplus morale reaches about 63% of the party's limit, 20 reaches about 86%, and 30 reaches about 95%. Five party members with Charisma 4, for example, aggregate to about 9.13 rather than producing five separate 20% bonuses; their shared limit is about 45.7%.

The party budget is divided among positive-morale members in proportion to their individual surplus, allowing the UI to show who is doing the encouraging without applying the party bonus more than once. All surplus values are calculated before receiving help from allies. This makes the relationship acyclic: two high-morale characters cannot recursively increase one another's output. If support would restore more than the listener's entire deficit, the named contributions are reduced proportionally. Ally support can lift a character only to zero and can never create surplus morale.

# Fear and the morale meter

Each negative morale point produces one percentage point of fear incapacitation, so -100 morale is the meaningful left endpoint of the meter. The center represents neutral morale. The right side shows the character's allocated share of the party's current ally-restoration percentage relative to the party's present `5% × aggregate Charisma` limit. Hovering or focusing the meter shows every named contribution and its signed value.

The strategic condition and morale-source tables are refreshable projections. Durable state remains in character condition, injuries, strategic time, and time-stamped morale events.

# Religion

A character makes or changes their religious profession by speaking with a priest at a church. Each settlement currently has one church and one fixed faith; its priest can convert a character only to that faith. Religion is a dialogue topic even when the priest also has a quest to discuss, rather than a service-menu choice. A priest cannot make a character faithless. Characters renounce their current faith from the Religion entry on their own biography instead. Large cities may eventually support multiple churches, but that is outside the current settlement model.
