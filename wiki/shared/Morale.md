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
- Religious conviction and mixed-faith discord.
- Morale restored by individual allies.

Food quality and disease will become additional named sources when those systems are implemented.

# Lifting allies

Party Charisma no longer contributes a permanent flat morale source. Instead, positive-morale party members share one party-wide restoration budget. Charisma uses its own social-coverage aggregation rather than the generic party skill formula. The strongest member supplies the base check. Additional members provide a rapidly saturating coordination bonus, then help or hinder according to how far their individual check is above or below the neutral 2.5 baseline:

```rs
let coordination = 1.125 * (1.0 - (1.0 / 3.0).powi(supporter_count));
let support = supporters.map(|check| 0.5 * (check - 2.5)).sum();
let party_charisma = (best_check + coordination + support).clamp(0.0, 5.0);
```

This produces approximately 4.5 from one character at 4.5, three characters at 3, or a 4 and a 2. Adding large numbers of characters at 1 or 2 lowers the result once their limited coordination benefit is exhausted.

The party's positive base-morale values are aggregated with the same ranked diminishing returns. The resulting restoration percentage approaches, but never reaches, a limit of 5% per point of the aggregate party Charisma check:

```rs
let party_charisma = aggregate_party_charisma(member_charisma_checks);
let party_surplus = cumulative_morale(member_positive_base_morale);
let saturation = 1.0 - (-party_surplus / 10.0).exp();
let party_restoration = saturation * 0.05 * party_charisma;
```

Ten aggregated surplus morale reaches about 63% of the party's limit, 20 reaches about 86%, and 30 reaches about 95%. Party Charisma is capped at 5, so the shared restoration limit cannot exceed 25% regardless of party size.

The party budget is divided among positive-morale members in proportion to their individual surplus, allowing the UI to show who is doing the encouraging without applying the party bonus more than once. All surplus values are calculated before receiving help from allies. This makes the relationship acyclic: two high-morale characters cannot recursively increase one another's output. If support would restore more than the listener's entire deficit, the named contributions are reduced proportionally. Ally support can lift a character only to zero and can never create surplus morale.

# Fear and the morale meter

Each negative morale point produces one percentage point of fear incapacitation, so -100 morale is the meaningful left endpoint of the meter. The center represents neutral morale. The right side shows the character's allocated share of the party's current ally-restoration percentage relative to the party's present `5% × aggregate Charisma` limit. Hovering or focusing the meter shows every named contribution and its signed value.

The strategic condition and morale-source tables are refreshable projections. Durable state remains in character condition, injuries, strategic time, and time-stamped morale events.

# Religion

A character makes or changes their religious profession by speaking with a priest at a church. Each settlement currently has one church and one fixed faith; its priest can convert a character only to that faith. Religion is a dialogue topic even when the priest also has a quest to discuss, rather than a service-menu choice. A priest cannot make a character faithless. Characters renounce their current faith from the Religion entry on their own biography instead. Large cities may eventually support multiple churches, but that is outside the current settlement model.

Each professed character receives a conviction source from their same-faith party cohort. The generic ranked party check combines the Faith checks in that cohort, with a minimum cohort check of 1 so a lone believer still draws strength from personal conviction. The cohort check is capped at 5. Faithless characters receive no conviction source and do not form a cohort that pressures believers.

For each believer, the other religious cohorts are combined into foreign faith pressure. Mixed-faith tension is deliberately subtractive: party Charisma is subtracted from that pressure, and only the uncovered remainder becomes raw negative morale. This means capable social leadership can remove discord entirely rather than merely dividing it down:

```rs
let foreign_pressure = aggregate_party_check(other_cohort_checks).clamp(0.0, 5.0);
let discord = 3.0 * (foreign_pressure - party_charisma).max(0.0);
```

The resulting `Religious discord` source then receives the same negative-source ranking and Will mitigation as other morale penalties. A unified party therefore gets the largest available conviction benefit without discord; a mixed party retains the conviction of each faith but generally pays a leadership-dependent cost.

# Fervor

Fervor is a bounded strategic pressure meter, not another morale source. It shows how close religious conviction is to becoming inflexible behavior. Individual Faith, the character's same-faith cohort check, and surplus morale raise pressure; aggregate party Charisma is subtracted as restraint. Faithless characters always have zero Fervor.

```rs
let pressure = (individual_faith + cohort_check + positive_morale / 10.0
    - party_charisma - 2.5).max(0.0);
let fervor = 1.0 - (-pressure / 5.0).exp();
```

The curve lets arbitrarily high pressure approach 100% without reaching it. The strategic character rail displays this value from Calm through Fervent to Frenzy.

Once per strategic day, each character gets a stable roll against their current Fervor. A roll below Fervor can create a personal demand, subject to a two-day cooldown. Thus 20% Fervor creates a 20% daily chance and 80% creates an 80% chance, with no unlock threshold. The basic demands alternate between reserving at least two hours of every daily schedule for prayer and observing a full holy day. The player receives an explicit choice:

- **Observe:** accept the practical cost. Prayer permanently adjusts the training schedule; a holy day spends one full day in settlement. The character receives a small positive morale event.
- **Do not observe:** keep complete freedom of action. The raw morale penalty is `max(0, 8 × Fervor − 1.6 × party Charisma)`, so both Fervor and Charisma change the result continuously and a Charisma check of 5 eliminates even the maximum penalty.

Resolving one demand cannot recursively produce another because the two-day cooldown begins at creation. Demands are choices, not involuntary character actions.

## A Quarrel at the Gate

The first severe Fervor incident is a single cross-faith settlement scenario. When a party arrives at a settlement, the highest-Fervor member who follows a different religion rolls against their current Fervor. On success they insult the local faith and draw an armed crowd. A character at 20% therefore has a 20% arrival chance and one at 80% has an 80% chance, with no unlock threshold. Each party can trigger this incident only once per settlement.

The incident deliberately reuses the quest-location combat flow. Arrival is interrupted at a zero-distance encounter named **A Quarrel at the Gate**. The party can:

- Initiate tactical combat using the normal tactical-server request.
- Autoresolve using the normal quest autoresolve damage and battle-result path.
- Open the encounter map and travel away without fighting.

The incident temporarily occupies the party's active-encounter slot while preserving any real active quest. Winning or leaving restores that quest. Leaving marks the incident avoided and does not immediately trigger another incident at the destination reached by that retreat. There are no religious quest-choice demands; quest dialogue consequences for mixed-faith parties remain future work.
