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

Characters have an immutable sparse personality drawn from discrete axes. Generated NPCs receive two to four randomly selected non-neutral axes; first-character candidates preview and persist the same exact generated axes. Personality changes raw morale reactions rather than replacing Will, Social skills, or Religion knowledge. The hygiene axis is Slovenly/Cleanly: Slovenly characters ignore filth morale, while Cleanly characters strongly dislike filth and appreciate being completely clean. Other characters never see these authoritative tags directly.

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

The same social menu also supports ordinary conversation with another living,
co-located party member or a currently present settlement NPC. The player
chooses 15 minutes to eight hours in 15-minute increments, with 30 minutes as
the default. A conversation with a settlement NPC must fit wholly inside that
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
replays a stored life history. Generic full Characters and settlement NPCs
materialized as recruiting-party leaders use the same one-time simulation;
lightweight settlement demographic rows, temporary tactical enemies, exact
strategic-simulator evaluation profiles, imports without a full Character, and
purpose-built fixtures remain outside that contract.

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

Character personality includes an immutable Temperance axis. **Temperate** and
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

Dead characters remain visible for history and party context, but cannot train, rest, travel, trade, manage equipment or inventory, enter combat, use party actions, recruit, change membership, or chat. Party readiness, forecasts, provisioning, movement, needs, condition updates, and combat construction consider living members only; a corpse's personal minute and location remain fixed while survivors continue. Dead members are not recorded as battle participants, receive no victory morale, mission experience, loot stake, or quest reward, and participant life state is checked again when loot is stored. A disposable simulation capability provides the deterministic death path used for integration testing; ordinary production identities cannot invoke it.

Characters initialize with the German vernacular selected deterministically from their final settlement profile. NPC Yiddish incidence is also deterministic; every selected Yiddish speaker retains a decent local German dialect at the documented 0.8 effective shared-language coefficient. Quest-company leaders atomically replace both Oral and Written language identity after being moved to their authoritative settlement, so a random creation origin cannot leak into their language record.
