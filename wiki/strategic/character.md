# Character

Literacy is not universal. A character whose authoritative relational estate
is Noble spends part of the simulated student-age phase learning local written
German or Low German through the normal Intelligence learning rate and cap;
Burgher estate alone grants nothing. Merchant-family literacy is represented
by the merchant starting profession. Learned religious curricula teach their
authored languages over time (Latin and German for Catholic and Lutheran
organizations; Hebrew, Yiddish, and German for the Jewish organization).
Professing a religion or merely joining an organization grants no instant
literacy. Bidirectional German–Latin and German–Low German primers let each
currently authored literate upbringing enter the wider book catalog.
Characters are created by investing some amount of [favor](../shared/magic.md) into them. The more powerful the character, as determined by their [stats](../shared/stats.md), the more favor you need to invest. The exact kind of favor you need also depends on what character you want. If you want an elf character, you need to go do some quests with the elves.

You aren't exactly spawning a character into the world; ostensibly, you are obtaining control over a character who already exists! This means you don't always have to start "fresh" with a young, untrained character with no background. You can create a wealthy, skilled character simply by spending a lot of favor on him.

## Personality

Characters have sparse visible personality traits derived from thirteen hidden,
mutable continuous scores. Generated NPCs receive two to four randomly
selected endpoint scores; first-character candidates preview and persist the
same derived traits. Deeds move scores in either direction, with bounded
potency changing before a discrete label appears or disappears at its
threshold. Personality changes raw morale reactions rather than replacing
Will, Social skills, or Religion knowledge. The hygiene axis remains
Slovenly/Cleanly and Temperance remains alcohol preference rather than the
broader chivalric virtue. Other characters never see authoritative scores or
tags directly.

Authoritative personality is private. Other characters instead keep durable,
observer-specific beliefs with confidence and observation time. Beliefs may be
wrong and can later be corrected. Insight forms beliefs about other people and
is opposed by involuntary Deception modified by Transparency; Insight also
governs reflection about oneself. Public morale labels never reveal the true trait that changed a
reaction.

## Relationships

Affinity is directional: how the subject currently regards a particular actor.
It is anchored to the subject's personal strategic clock and exponentially
decays toward neutral with a 30-day half-life without crossing neutral.
Familiarity is symmetric shared-party time stored once for the canonical
character pair. Its displayed effective hours divide shared time by current
party size while both characters remain together, and use the undivided total
after they separate.

### Temporal relationship scope

Relationship systems distinguish **pairwise-soft** history from canonical,
exclusive history. Affinity, familiarity, and ordinary Socializing are
presented in the Conversation Dock rather than a separate Social modal. The
header's qualitative regard face opens an observer-only impression containing
familiarity, perceived traits, confidence, and approach hints; it can be pinned
for pointer or keyboard inspection. Self-selection omits the chat composer and
uses Recent Tidings for reflection. Age, faith, fame, and infamy remain on the
biography. These pairwise-soft values may be updated on the acting character's clock and never
consume another character's schedule. A person who is engaged or married can
therefore still become a close friend of somebody whose personal date is in
their past. The resulting friendship does not create romantic eligibility.

Courtship, gifts, discovery by family, engagements, marriage, pregnancy, and
birth are canonical or exclusive actions. NPC-controlled people are full
Characters with the same stats, attributes, skills, limbs, personality,
condition, needs, equipment capability, and `CharacterTime` components as
player characters. `NpcPolicy` changes who drives that character; it does not
create a second identity or a reduced character type. Persistent NPCs begin
outside a party, while player creation retains its normal solo-party behavior.
NPC generation takes an explicit upbringing settlement and stable policy seed,
so its life simulation and personality do not depend on reducer RNG, insertion
order, or retries.

`CharacterTime` is the one authoritative personal frontier for both players
and NPCs; there is no parallel NPC clock. Canonical NPC actions may only move
that frontier forward and settle residence bills, weddings, and births in the
same transaction. Player travel and account-owned schedule reducers never
advance an NPC on its behalf. Institutional services (guild admission,
renewable market trade, ordinary rest, and actor-local quest journal updates)
remain asynchronous only when they neither read nor write dynamic canonical
NPC state. A private bounded scheduler advances living NPCs in
exact-personal-minute cohorts, then character-ID order, by no more than one
day per pass. It records schedule, housing, and romance decisions for every
member of a cohort before advancing any member.
Weddings and births use separate bounded effective-date queues. NPC participants
causally advance through a ceremony boundary without login; a wedding involving
a lagging player remains reserved until that player's frontier arrives, so the
ceremony never evaluates location against stale history. Relationship projections are
evaluated against the projected character's personal minute: a globally
reserved engagement may block a conflicting romance while a marriage,
pregnancy, exposure, or child which is still in that character's future is not
shown.

Socializing is a 15-minute allocated strategic activity, distinct from both
Leisure and Carousing. One deterministic companion receives all of a daily
allocation, selected in priority order: a romantic partner, a co-located party
member, a positive-affinity acquaintance, then another co-located person.
Socializing records asynchronous directed affinity and symmetric familiarity;
it does not assert that the selected NPC lost finite canonical time. Party
familiarity is the exception: the shared-clock presence path already owns
those minutes, so Socializing grants affinity and training without adding the
same familiarity twice. Actor/day/target receipts conserve the total applied
frontier, making retries and time-advance chunks equivalent while still
allowing a later chronological slice to retarget if the earlier companion has
died, left the settlement, or otherwise become unavailable.
Each slice selects against its own effective start minute rather than the
actor's already-written interval-end clock, so a later birth or relationship
boundary cannot rewrite an earlier day's companion. A candidate whose personal
frontier is later than that slice is unavailable, because their mutable current
settlement may already describe a future move. The actor's current settlement
is safe for the whole interval because scheduled Socializing is applied only by
the stationary advancement path, which captures one fixed execution location;
travel time does not run this schedule.
Ambiguous targets are scored by a stable hash of actor, absolute calendar day,
location, and target; character ID is the final tie break. Iteration or table
insertion order therefore cannot change the selected companion.

Autonomous romance considers at most 16 deterministically ordered, currently
present settlement residents. Both people must have `NpcPolicy`; policy never
targets a player-owned character. The ordinary living-adult, mutual
attraction, co-location, non-kinship, and exclusivity checks use the later of
the two independent personal dates without synchronizing either clock.
Formal and informal affinity, father approval, personality threshold, and
secrecy rules are the same rules exposed to players. A successful courtship
atomically reserves its wedding one year ahead; expected ineligibility is a
durable no-op receipt, while missing canonical infrastructure aborts the
scheduler transaction.

Socializing trains one conserved Social training budget at the ordinary
activity rate. Gregarious, Neutral, and Solitary actors direct 60%, 50%, and
40% respectively to Charm. Transparency splits the remainder: Open directs
all of it to Insight, Neutral divides it equally, and Guarded directs all of it
to Deception. The three integer basis-point weights always sum to 10,000.

### Courtship

Courtship is an explicit canonical action, never an automatic threshold
transition. Both participants must be living adults, co-located, mutually
attracted according to their private inclination and observable presentation,
not close kin, and evaluated at the later participant frontier without moving
the lagging participant's clock. The resulting courtship is effective-dated,
and an immediate exclusive wedding reservation blocks conflicting romance even
when its details remain in a character's future. High affinity
with a person who has an exclusive commitment remains friendship rather than a
counterfactual romantic relationship.

Formal courtship currently requires a biologically male suitor and biologically
female partner, enough affinity from the partner, and enough affinity from her
known father. The approving father's identity and the dowry amount implied by
his wealth tier are frozen at that shared courtship minute; later opinion,
wealth, or clock changes cannot rewrite the approval. Scheduling the wedding
reserves that promised amount from the father in a private escrow. A cancelled
or expired wedding refunds it; a fulfilled wedding pays it to the husband.
Partner and father affinity thresholds are projected at that same effective
minute. If a compact affinity row was replaced by a newer anchor and its past
value can no longer be reconstructed, the exclusive action fails closed
instead of using a future opinion.
Informal courtship works for any mutually attracted eligible
pair, but requires substantially more affinity; Amorous partners lower that
threshold and Proper partners raise it. A wedding can only be scheduled from
an active courtship. Scheduling creates an immediate exclusive engagement for
both people and fixes the ceremony one year later. The wedding materializes
only after both personal frontiers reach the ceremony; ending the resulting
marriage likewise requires both spouses at the same personal date.

When an informal courtship begins, every living adult parent or sibling then
co-located with the couple becomes a frozen observer, with that observer's
Insight and the weaker partner's Deception frozen at the same effective
minute. While the courtship remains secret and Active, each observer makes one
Insight-versus-Deception check per relationship day. These checks run from the canonical
lifecycle boundary independently of that day's Socializing target. Every
success and failure has an immutable receipt; the first success exposes the
facade and stops later checks. Discovery knowledge is available only through a
gateway projection scoped to the discovering observer. An active retry of the
same courtship kind is idempotent, a different active kind is rejected, and an
ended pair is final in this first-pass history model. A check waits until both
participants and the frozen observer have reached that day. Because its cohort
and skill values are historical facts captured at courtship establishment,
settlement does not depend on later location, skill, clock-chunk, or NPC
advancement order. An observer who dies ceases to be eligible from the death
minute onward and cannot hold the remaining living observer cohort at an
unreachable personal frontier; post-death days are deterministic skips and do
not create discovery-attempt receipts for that observer.
The discovery receipt keeps that historical attempt minute, while any affinity
penalty is anchored at the observer's current frontier where the mutable
affinity value was evaluated; delayed settlement therefore cannot decay the
same elapsed interval twice.

Opposite-sex adult spouses who are co-located can conceive from qualifying
spouse Leisure. Only the integer intersection of their realized Leisure
intervals at an identical location qualifies. Joint minutes are conserved
across checkpoints; each crossing of 60 minutes creates one deterministic
trial with a stable ordinal and exact crossing minute. Pregnancy is exclusive
to the mother, lasts exactly 280 days,
and has no complications in this pass. A due pregnancy materializes one full,
NPC-policy-controlled dependent child, then creates parent/child and household
edges atomically. Every character has an authoritative birth coordinate; age
is derived at the effective minute and its cached display value advances at
yearly lifecycle boundaries, so dependents naturally become adults. Seeded
town residents are arranged into deterministic households with parent/child
and sibling edges without creating duplicate identities. Dependent NPCs keep
an unallocated Leisure schedule; the autonomous adult labor and Socializing
plan is installed only when authoritative birth chronology reaches adulthood.
Naturally born children use the same persistent `Character`, identity,
attributes, skills, needs, inventory, location, and personal-clock authority as
everyone else. They are not rerun through the starting-character life
simulation. Instead, a private child-development row freezes one of four safe
activity focuses at birth: Play, Study, Household help, or Social learning.
Before age six their time is care and rest and awards no skill training. From
six through eleven and twelve through fifteen, the focus awards a small,
aptitude-aware budget to ordinary skills. Childhood policy never invokes paid
work, crime, combat training, incidents, or organization curricula. The clock
splits exactly at the sixth, twelfth, and sixteenth birthdays, and a durable
training cursor makes retries and different advancement chunks equivalent.
Each curriculum track also records its own accepted effective-hour
contribution. Only the interval after the cursor is evaluated with that
interval's attributes, and its gain is added to independently earned skill
hours under the ordinary aptitude cap.

Birth also freezes at most one private lineage-control claim. If both parents
have browser grants, the mother's owner wins; otherwise the one available
owner is used, and a birth with no granted parent creates no claim. At the
existing age-sixteen adulthood boundary, a living claimed descendant receives
an idempotent browser grant whose typed provenance is `AdultDescendant`, leaves
NPC clock policy, becomes an `AdultChild` in the household, and receives a solo
party if needed. This preserves the same character ID, personal date, skills,
inventory, and location and does not select the character automatically. Dead
or underage descendants cannot be selected. Grant provenance is structural:
adult descendants carry a typed source-parent ID while starting candidates
carry a starting-claim request key, and the unused provenance arm must be
empty. While a living character is selected, descendant roster visibility and
selection use that character's personal date; with no valid living selection,
the descendant's own adult frontier permits successor recovery.

Mortal death creates one private, effective-dated estate disposition. The heir
is frozen as the eldest living direct child already born at the death minute
(birth minute, then character ID), otherwise the living spouse; with neither,
the estate is unclaimed. Personal inventory is all that passes: currency and
ordinary carried or equipped items retain their inventory IDs, amounts, food
lots, and condition, while equipment authority is removed before ownership
changes. `Character.gold` is legacy and non-authoritative. Residence holdings,
party property, debts, items in repair custody, and organization assets are
excluded. If the heir's personal frontier has not reached the death minute,
the disposition remains pending and invisible at earlier dates. Settlement is
retry-safe at the heir lifecycle hook. If later causal information shows the
frozen heir died no later than the effective minute, the estate becomes
unclaimed rather than selecting a replacement; if the heir dies after crossing
the effective minute, the first estate settles before that heir's own estate is
chosen. If an earlier estate materializes only after that later estate has
already settled, its items follow the already-effective succession chain to
the chronological owner exactly once. A later estate that is still pending or
unclaimed remains the explicit staging or terminal point rather than leaving
new property behind an already-completed transfer.

The household projection is scoped to the selected browser owner and the
selected parent's personal date. It presents every known child with a
qualitative stage and focus, a maturity bar, and an adult-playable icon. Exact
age/progress text is confined to accessible labels and tooltips. Estate
projections apply the same owner and chronology boundary.

Contraception, infertility, miscarriage, childbirth risk, parent-edited child
activities, detailed education, multi-owner control, residence inheritance,
debts, and inheritance law beyond the direct-child/spouse order remain future
work.
Child identity, name choice, sex, and home placement use separate stable seed
domains based on the canonical parent pair, pregnancy ordinal, birth minute,
and home location, so retries and insertion order cannot change the result.
Full `Character` rows and their identity-bearing durable component rows are
private and broadly readable only through registered-gateway projections.
Browser-visible
relationship summaries are filtered by the selected character's personal
minute, so a realized child is not disclosed to a parent whose frontier is
still before the due date.

Death closes any active courtship immediately. A reserved wedding is cancelled
atomically, releasing both exclusive participant claims and refunding any
dowry escrow; it never waits for a dead participant's frozen clock to reach the
ceremony. If the deceased character is pregnant, the pregnancy ends and its
reserved child identity is released. A surviving pregnant mother may still
carry a deceased father's child to term.

Browser ownership is separate from character identity. Candidate confirmation
atomically records a private starting-character claim and character grant under
the gateway's pseudonymous owner key. A deterministic candidate may be retried
by that owner, but another owner cannot replay the coordinates to take it.
Selection is server-side and is permitted only for a granted character.

The Social family is **Insight**, **Charm**, **Command**, and **Deception**.
Social outcomes combine the action skill, current Affinity and Familiarity,
the actor's diagnosis, the target's true personality and topic sensitivity,
and a server roll. Listening is low-risk exploration; more presumptuous actions
have greater upside and downside. Only recognized negative morale concerns are
actionable. Their topic is derived by the server, and repeating the same
approach to the same topic has a cooldown even if the source row is refreshed.
Characters use Insight to Reflect on their own concern; reflection can
revise a self-belief but never changes Affinity or Familiarity. The interface
shows only a qualitative, familiarity-weighted affinity estimate rather than
the authoritative value. Observed traits and morale-source interpretations use
greyer text when confidence is lower; their exact confidence is available on
hover together with a hint about which approaches that trait may favor or
resent.

The selected local's dialogue includes a contextual conversation topic. Its
spoken responses offer a brief exchange, an unhurried visit, or an evening
together; exact durations remain available to assistive technology and on
hover without making raw minutes the primary interface. Courtship and wedding
proposals likewise appear as dialogue topics and spoken responses rather than
standalone action buttons. A conversation with a settlement NPC must fit wholly inside that
NPC's current presence window. Each quarter hour uses the speaker's Charm and Insight together
with mutual personality fit and the existing relationship. Familiarity always
records the shared time, but morale and directional affinity can rise or fall;
even a skilled, compatible pair can have an awkward conversation, and a poor
match can occasionally connect. The observer-facing result remains
qualitative and never exposes checks, personality fit, rolls, or numeric
deltas.

The available social approaches are filtered by the concern rather than showing
every Social skill for every problem. Commiseration is always available as one
action: it uses Insight when the actor currently shares that kind of concern and
Deception when they do not, so sincere and feigned variants never appear at the
same time. Action labels remain grounded in facts the simulation actually knows.
The character sheet groups the four skills beneath an expandable **Social** row,
whose displayed value is their average.

First-time players first choose a life stage, then choose a generated whole
character. Young candidates are age 16, professionless, and offered as a varied
roster of five. Adult candidates are age 22 and freshly
journeyman-equivalent; old candidates are age 40 and master-equivalent. Adult
and old rosters each offer one candidate for merchant, weaponsmith, armourer,
tailor, herbalist, cook, learned religious practitioner, witch hunter, knight,
and forester. The specific eligible organization is deterministic but random
from the player's perspective when a family has multiple options. Witch
hunters join The Hunt of the Pale Lantern, knights join The Order of St.
George, and foresters join The Lodge of the Hart King; these three
organizations have no profession-of-faith requirement.

Professional candidates preview and receive their complete plausible package:
organization and mapped rank, current dues and presentation, required
profession of faith, qualifying skills, equipment, ammunition, and currency.
Packages are authored per profession and life stage rather than layered over a
generic combat archetype: a newly qualified adult has a modest working kit and
purse, while an old master has veteran equipment, more supplies, a larger
purse, and profession-sensitive experience and presentation. Only deliberately
equipped items contribute to the previewed combat capability.
Players cannot customize individual fields. The roster is reproduced from a
private random seed stored for the browser tab, but nothing is stored on the
server until a candidate is confirmed. Age is intended to carry further
tradeoffs later; those tradeoffs are deliberately not specified yet.

Initial training is not an authored final-hours package. At authoritative
creation, a deterministic pure simulation advances coarse life phases from age
six through the character's current age. Childhood and student/apprentice
activities may participate in this creation-only simulation without appearing
in the adult live schedule UI; professional phases use the selected
organization's authored curriculum. The calculation is analytical in the
number of phases rather than ticking days or minutes, and normal aptitude
learning rates and caps apply.

This is deliberately not event sourcing. Persistence retains only the current
`CharacterSkills` projection (plus the character's other current state), never
historical activity transitions, schedules, or phase records. Recreating the
same candidate coordinates reproduces the same result, but gameplay never
replays a stored life history. Generic full Characters and persistent
settlement NPCs use the same one-time simulation and must pass the same
full-component invariant. Temporary tactical enemies, exact strategic-simulator
evaluation profiles, and purpose-built fixtures remain outside the persistent
NPC contract.

Native oral language is an acquired identity supplied by the character's
upbringing settlement, rather than credited study hours competing for a daily
training budget. Written language is different: organization curricula and
noble literacy both consume aptitude-aware historical study, and persistence
does not patch a minimum written rank after simulation.

Every confirmed, durable character also receives one relational social-estate
basis. Estate is derived from an organization role rather than stored as a
writable Character field: a lordship serf, explicit civic free resident,
urban civic citizen (Burgher), or noble-house member. The deterministic,
domain-separated assignment does not alter candidate IDs, previews,
professional membership, rank, equipment, dues, or presentation. Multiple
orthogonal roles are supported (for example, a noble-house member who is also
a learned religious practitioner), but estate-dependent gameplay rules are
intentionally deferred. Denomination-specific starting organizations supply
the professional role without changing their existing membership or
presentation behavior.

Players may create multiple characters in the same browser. The strategic
header's portrait menu lists the browser's remembered non-temporary characters,
marks the current one, and switches between them. **Character select** returns
to the life-stage step so another character can be created. This prototype
roster is browser-scoped and is not an account or authentication boundary.

## Mortal

Character personality includes a mutable Temperance score whose visible axis is **Temperate** and
**Drunkard** are visible non-neutral tags; the neutral state is omitted like
other neutral axes. Random mortal/NPC profiles still activate exactly two to
four distinct axes across the expanded thirteen-axis behavioral catalog.
Mirth, Courtship, Transparency, and Self-knowledge join the existing axes.
Presentation and Inclination are always assigned outside that sparse count;
private Sex supplies demographic truth and never participates in attraction.
The displayed inclination traits are Attracted to men, Attracted to men and
women, Attracted to women, and Attracted to neither. Apparent gender identity
is Man/Ambiguous/Woman. Man and Woman are normally learned on contact;
Ambiguous identity requires an Insight discovery check. Deliberately presenting
traits that differ from a character's true traits is reserved for a broader
Deception-based feature.

Each actual personality discovery check conserves 0.25 real training hours.
An Open subject awards all of it to the observer's Insight; a Neutral subject
splits it evenly between observer Insight and subject Deception; a Guarded
subject awards it all to subject Deception. Unsupported contexts produce
neither a check nor training.
Mortal characters [age](../strategic/time.md) normally and eventually die. They cannot have their physical features customized; when rolling them, players must choose from a limited selection of randomly generated characters. They are cheap and efficient, ideal for players who want a [roguelike](https://en.wikipedia.org/wiki/Roguelike)/extraction-esque experience of frequently rolling new characters, quickly obtaining power, dying, and starting over.
### Humans
[Default](https://en.wikipedia.org/wiki/Human).
### Dwarves
Inspired by their Tolkien/*Warhammer* depiction. A proud, stubborn, greedy, ~~short~~ sturdy, and strong race. Dwarves dwell in underground mountain cities. In their days of glory, the Dwarves built extensive tunnel networks between these cities; these tunnels have since been infested by foul creatures.

Dwarves who shame their kin by dishonoring the ancestors, breaking oaths, or engaging with prissy Elven nonsense like magic may be exiled at best, at worst compelled to redeem their honor by undertaking various suicide missions to retake an ancestral realm.

As the race needs a lot of Dwarf-specific assets, Dwarves will not be included in the [MVP](../roadmap.md) or tentatively even the next phase.
### Halflings
Inspired by their Tolkien/*Warhammer* depiction. A small, jovial, provincial people generally unconcerned with the matters of the "big people." Would be found in small idyllic villages here and there. Not important enough for the MVP.
### Orcs/Goblins
Inspired by *Warhammer* greenskins, though less comedic and specifically only grown from nasty underground funky pools. An Orc is just a Goblin who had lots of fresh meat thrown into its spawning pool (and must maintain this diet). 

If you aren't familiar with *Warhammer*, the idea is that they are a fungus-based lifeform with genetic memories. The point is for them not to need a complex civilization to be threatening (they already know how to fight and speak) and for you to not feel bad for slaughtering them (no women or children, they emerge REDY 2 FITE). Quest fodder for the MVP.
### Ratlings
Inspired by *Warhammer Fantasy* Skaven, though with the technology level toned down somewhat. These are wretched, craven humanoid rats who dwell underground, both in stolen Dwarven cities and in their own subterranean creations beneath prosperous human cities. They don't need to be in the MVP.
## Immortal
Immortal characters [do not age](https://en.wikipedia.org/wiki/Biological_immortality), will respawn if killed, and can be customized in detail. Their purpose is to give players the option of a more conventional RPG playstyle than the punishing roguelike experience of mortal characters.[^1]

[^1]: However, everyone's first character (and probably the next several) will be mortal; mortal characters are playable on free accounts, and players can obtain their first with zero [favor](../shared/magic.md).

Respawning an immortal character requires a [favor](../shared/magic.md) cost equivalent to the death cost of a similarly valuable mortal character. The cost may even be *higher* for immortal characters, so players would be ill-advised to use them for suicide missions. However, what immortal characters lack in cost efficiency, they compensate for with a higher effective skill ceiling, having unlimited time to train their [skills](../shared/stats.md).[^2]

[^2]: Albeit with drastically diminishing returns.

Immortality is based on race, not an abstract per-character flag. All immortal races are said to have "fey blood." In the current [roadmap](../roadmap.md), Elves are the only immortal race planned.
### Elves
Inspired by their Tolkien/*Warhammer* depiction. Tall, beautiful, and haughty, Elves live in either deep forests or fictitious islands. They are generally morally good. Exceptions include the evil "Dark Elves" and the somewhat more neutral, ecoterroristic "Wood Elves."
### Dragons
Intelligent [dragons](https://en.wikipedia.org/wiki/Dragon) who can take on a human form. It would be extraordinarily expensive to actually create a full-blooded dragon character. Some dragons may be unable or unwilling to take a human form. Absolutely not in the MVP and almost certainly not in the polished product unless one of the devs is very insistent.

> Halbe: *I* am certainly not going to try and animate dragon flight.
### Beastmen
*(rename "Beastlings"? "Shifters"?)*

Beastmen have both a beast form and human form that they may shift between. The exact type of beast depends on whatever would be local to them. Can be felines, canines, serpentines, equestrians, lizards, and more.

Probably not in the MVP. Might be in the polished product at least for wolves.
### Half/quarter/etc.-blooded
These generally look like normal humans, except they can be immortal and customized. They are for players who want the mechanics of Elves/Dragons/Beastmen but don't want the pointy ears or shapeshifting. These come from breeding between mundane people and the fey-blooded.

In the case of feybloods who can shift between forms, the half-bloods may be unable to shift. Instead, they might take on some intermediate characteristics of the two forms.

> Halbe: Yes, half-blooded beastmen are the "designated furry race." And I think gnomes are just elf-halfling hybrids.

> Bruno: And I was expecting something tasteful and classy, like the half-bloods are our way of capturing the aesthetic of ancient Egyptian deities in a post-Christian world. Alas.
### Vilebloods
When a mundane character consumes fey blood, he can *become* fey-blooded. However, this is evil, so it also curses him. The exact nature of the curse depends on the kind of fey blood.
* Werewolves/bears/etc are beastmen-blooded.
* True vampires are elf-blooded.
    * Mostly analogous to *Warhammer* Dark Elves. They do not burn in the sun, and they are not actually undead.
* The aesthetic of Freaky Devil-Looking Thing (e.g. [imps](https://en.wikipedia.org/wiki/Imp)) is captured by mongrel vilebloods. They may take features from a mix of reptilian, mammalian, Draconic, and/or Elven blood.
### Undead
Mortals risen from the dead through unnatural magic.
* A vampire is created when another vampire offers a mortal his blood and buries him alive. After the mortal suffocates to death, he becomes a vampire.
* Zombies and skeletons are not elf-blooded; they are risen via necromancy. They are mindless and must be consciously puppeted by a necromancer.
    * Ghouls/wights are zombies/skeletons who have a soul bound to them (the ritual requires elf blood). They are not mindless, and though bound to their necromantic masters, they can act autonomously.
* A lich is a necromancer who has turned himself into a wight. As his soul is bound to himself, a lich is the only type of wight with free will. (This implies the possibility of ghoul-liches, who retain their flesh.)
## Death

Characters begin alive. Death is an authoritative strategic transition: `Character.alive` is the fast life-state flag and one immutable `CharacterDeath` row retains the first typed cause, source, optional committed-outcome identifier, and the character's personal strategic minute. Repeating the transition is idempotent and cannot replace the original context. Tactical combat may submit a final death outcome, but tactical positions, hit points, enemies, and tick state never enter strategic persistence.

`CharacterDeath` and derived morale-source rows are private authority. The
registered gateway receives broad backend projections for simulation and
server-rendered UI work; a player-facing death view must hide any receipt whose
strategic minute is later than the selected observer's personal date. Keeping
morale sources behind the same boundary prevents labels such as spouse Leisure
from disclosing a private relationship to unrelated direct clients. The web
gateway applies that rule to shared party portraits, settlement resident lists,
action previews, and remembered characters by reconstructing life state at the
observer's personal minute. If the observer time cannot be read, the gateway
does not use the broad current `alive = false` value to disclose or suppress a
character; strategic reducers remain authoritative for attempted actions.

Dead characters remain visible for history and party context, but cannot train, rest, travel, trade, manage equipment or inventory, enter combat, use party actions, recruit, change membership, or chat. Party readiness, forecasts, provisioning, movement, needs, condition updates, and combat construction consider living members only; a corpse's personal minute and location remain fixed while survivors continue. Dead members are not recorded as battle participants, receive no victory morale, mission experience, loot stake, or quest reward, and participant life state is checked again when loot is stored. A disposable simulation capability provides the deterministic death path used for integration testing; ordinary production identities cannot invoke it.

Characters initialize with the German vernacular selected deterministically from their final settlement profile. NPC Yiddish incidence is also deterministic; every selected Yiddish speaker retains a decent local German dialect at the documented 0.8 effective shared-language coefficient. Quest-company leaders atomically replace both Oral and Written language identity after being moved to their authoritative settlement, so a random creation origin cannot leak into their language record.
