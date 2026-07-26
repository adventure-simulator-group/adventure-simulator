# Character
Characters are created by investing some amount of [favor](../shared/magic.md) into them. The more powerful the character, as determined by their [stats](../shared/stats.md), the more favor you need to invest. The exact kind of favor you need also depends on what character you want. If you want an elf character, you need to go do some quests with the elves.

You aren't exactly spawning a character into the world; ostensibly, you are obtaining control over a character who already exists! This means you don't always have to start "fresh" with a young, untrained character with no background. You can create a wealthy, skilled character simply by spending a lot of favor on him.

## Personality

Characters have an immutable sparse personality drawn from discrete axes. Generated NPCs receive two to four randomly selected non-neutral axes; first-character candidates preview and persist the same exact generated axes. Personality changes raw morale reactions rather than replacing Will, Social skills, or Religion knowledge. The hygiene axis is Slovenly/Cleanly: Slovenly characters ignore filth morale, while Cleanly characters strongly dislike filth and appreciate being completely clean. Other characters never see these authoritative tags directly.

Authoritative personality is private. Other characters instead keep durable,
observer-specific beliefs with confidence and observation time. Beliefs may be
wrong and can later be corrected. Insight forms beliefs about other people and
is opposed by Deception where applicable; Self-awareness governs beliefs about
oneself. Public morale labels never reveal the true trait that changed a
reaction.

## Relationships

Affinity is directional: how the subject currently regards a particular actor.
It is anchored to the subject's personal strategic clock and exponentially
decays toward neutral with a 30-day half-life without crossing neutral.
Familiarity is symmetric shared-party time stored once for the canonical
character pair. Its displayed effective hours divide shared time by current
party size while both characters remain together, and use the undivided total
after they separate.

The former Charisma skill is replaced by the Social family: **Insight**,
**Self-awareness**, **Humor**, **Command**, **Deception**, and **Seduction**.
Social outcomes combine the action skill, current Affinity and Familiarity,
the actor's diagnosis, the target's true personality and topic sensitivity,
and a server roll. Listening is low-risk exploration; more presumptuous actions
have greater upside and downside. Only recognized negative morale concerns are
actionable. Their topic is derived by the server, and repeating the same
approach to the same topic has a cooldown even if the source row is refreshed.
Characters use Self-awareness to Reflect on their own concern; reflection can
revise a self-belief but never changes Affinity or Familiarity. The interface
shows only a qualitative, familiarity-weighted affinity estimate rather than
the authoritative value. Observed traits and morale-source interpretations use
greyer text when confidence is lower; their exact confidence is available on
hover together with a hint about which approaches that trait may favor or
resent.

The available social approaches are filtered by the concern rather than showing
every Social skill for every problem. Commiseration is always available as one
action: it uses Insight when the actor currently shares that kind of concern and
Deception when they do not, so sincere and feigned variants never appear at the
same time. Action labels remain grounded in facts the simulation actually knows.
The character sheet groups the six skills beneath an expandable **Social** row,
whose displayed value is their average.

First-time players first choose a life stage, then choose a generated whole
character. Young candidates are age 16, professionless, and offered as a varied
roster of five. Adult candidates are age 22 and freshly
journeyman-equivalent; old candidates are age 40 and master-equivalent. Adult
and old rosters each offer one candidate for merchant, weaponsmith, armourer,
tailor, herbalist, cook, learned religious practitioner, witch hunter, knight,
and forester. The specific eligible organization is deterministic but random
from the player's perspective.

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

Players may create multiple characters in the same browser. The strategic
header's portrait menu lists the browser's remembered non-temporary characters,
marks the current one, and switches between them. **Character select** returns
to the life-stage step so another character can be created. This prototype
roster is browser-scoped and is not an account or authentication boundary.

## Mortal

Character personality includes an immutable Temperance axis. **Temperate** and
**Drunkard** are visible non-neutral tags; the neutral state is omitted like
other neutral axes. Random mortal/NPC profiles still activate exactly two to
four distinct axes across the expanded nine-axis catalog.
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
