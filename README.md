# _Adventure Simulator_
_Adventure Simulator_[^0] is an open source browser game using novel technologies to revive the golden age of pseudo-MMOs.

[^0]: Working title.

## A web-first pseudo-MMO
The mid-2000s yielded a number of highly successful "pseudo-MMO" browser games, like _Neopets_ and _Club Penguin_,[^1] whose markets have since been captured by mobile apps and native desktop games. However, new technologies like [Wasm](https://webassembly.org/), [WebGPU](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API), and [Datastar](https://Datastar.dev/) allow us to make a new kind of browser game, one with near-feature and performance parity with native applications: a kind of game that has been impossible to build until very recently.

[^1]: And actual MMOs, such as _Runescape_ and _AdventureQuest_.

### Bulletin-board world
A traditional MMO uses a central server to maintain the live state of the game world, run simulation logic in real time, and push state updates to clients dozens of times per second. Designing and implementing this server presents a host of complex networking challenges; building a backend that can handle massive concurrency, synchronize thousands of players in real time, ensure consistency so everyone sees the same world, and maintain low latency isn't a lot of fun, which is why most people don't make traditional MMOs.

We aren't making a traditional MMO either. Our plan is to sidestep these challenges altogether by representing our world, not as a continuous simulation on a server, but as a [bulletin board](https://en.wikipedia.org/wiki/Bulletin_board_system).[^2] A database contains information about players, places, and quests, and players interact with this world-database by taking discrete actions through an asynchronous, [hypertext (web-style)](https://en.wikipedia.org/wiki/HATEOAS) interface. Unlike an MMO's world-server, a bulletin board database has no active connections to maintain; as soon as it responds to your request, it forgets you exist. When players do need real-time action, e.g. when they engage enemies in combat, we create a private virtual server just for their party, though as _any_ real-time networking can quickly become dangerously complex, we intend to keep as much state as possible on the server, using server-sent events to push updates directly to the client as events happen.[^3]

Player characters spend their downtime in settlements, which are persistent, bulletin board-like social hubs where they can purchase equipment, join parties, and embark on quests. When their party sets out on a quest, players load into a real-time, WebGPU-rendered combat simulation when they arrive their destination or are randomly attacked along the way; when the real-time simulation is no longer required, players transition back into the discrete hypertext format.

This is all to say that we aren't building a "normal" web game that uses Wasm and WebGPU to run in the browser. We are building a hypertext bulletin-board game which can *act like* a normal game when needed, like in combat, and where most of the game logic isn't even running in the browser but rather streamed, via Datastar, from the server.

[^2]: You can think of a bulletin board as halfway between a Discord server and an Internet forum. Think of an imageboard: the threads are more live than reddit or forums, less live than chatrooms. The benefit of the format is that it works both synchronously and asynchronously; you can have a nearly live chat with a guy on /tg/, but the format also works even if you're the only live user on a given thread at that time. This isn't an unheard-of inspiration for a fantasy RPG; [_Dragon's Dogma_](https://www.dragonsdogma.com/en-us/)'s internal project name was ["BBS-RPG"](https://www.dragonsdogma.com/assets/images/gallery/gallery_img18_01_en.jpg) due to the ["custom mercenary character" system](https://www.dragonsdogma.com/assets/images/gallery/gallery_img19_01_en.jpg). We would be taking the idea much further than _DD_ did, of course. 

[^3]: We end up rendering a sort of network-driven ["immediate mode"](https://en.wikipedia.org/wiki/Immediate_mode_(computer_graphics)) view of the world.

## Gameplay

Strategic balance and core-loop regression testing are supported by the NPC
simulator documented in the
[strategic simulation reference](wiki/reference/strategic-simulation.md).
Its live mode drives the same party, quest, travel, autoresolve, loot, trade,
and equipment reducers as players, against an explicitly disposable local
database.
The nearest games for inspiration are [*Mount and Blade*](https://www.taleworlds.com/en/games/mountandblade), [*Battle Brothers*](https://battlebrothersgame.com/), [*Jagged Alliance*](https://store.steampowered.com/app/1084160/Jagged_Alliance_3/), [*Starsector*](https://fractalsoftworks.com/), and to some extent [*Kenshi*](https://lofigames.com/).

Like the former three, the world of _Adventure Simulator_ is separated between the "tactical" layer (a real-time simulation) and the "strategic" layer (which advances in discrete chunks of time, generally after fast travel or resting). We have the same basic gameplay formula where the player recruits a party to adventure with, defeats enemies in randomly generated missions, and uses their hard-earned rewards to buy equipment for future missions.

Like in _Kenshi_, _Battle Brothers_, and _Jagged Alliance_, players can control multiple characters, though in _Adventure Simulator_, characters can be either mortal or immortal. Mortal characters offer a more roguelike/"extraction" experience, with fast progression and frequent deaths; when one dies, their personal inventory passes to their eldest living direct child, otherwise their living spouse, once the heir's personal date reaches the death. Immortal characters offer a more conventional RPG/MMO experience, which emulates the cost of mortal characters with costly respawns and slow healing.[^5]

If there's any design choice in particular that makes our approach unique, it is specifically that we relinquish the vision of a continuous, immersive world. _Kenshi_ clings to that vision, despite all the systems of the game going against it,[^6] and most MMOs try to reach that ideal before networking gets in the way. We take our inspiration from singleplayer games like _Starsector_, _Jagged Alliance_, and _Mount and Blade_ which all *chose* to have a strategic layer, not because they had to for networking, but because their gameplay loop would be really boring without one. You can actually walk around cities in _Warband_ and _Bannerlord_, but zero players actually do this outside of sieges because walking around is boring. Thus, we take those games' basic design and combine it with the one infamous problem it incidentally solves: MMO networking.

[^5]: Mortal characters will probably be randomly generated by default. The idea is that players who prefer custom characters will naturally gravitate to the immortal option.

The prototype's first-character flow begins with a life-stage choice. Young
characters are age 16 and professionless; adults are age 22 and newly
journeyman-equivalent in one of ten profession families; old characters are
age 40 and master-equivalent. Professional rosters contain one candidate per
family, with the specific eligible organization selected deterministically
from the tab's private seed when a family has multiple options. Witch hunters,
knights, and foresters each have one denomination-neutral organization and
therefore always receive that family's organization. Candidate state remains untrusted browser
coordinates until confirmation authoritatively regenerates and atomically
persists the selected character.
Confirmed characters accumulate in a browser-scoped roster and can be switched
from the strategic header; Character select returns to the life-stage flow to
create another.

Character training is generated by a deterministic, one-time life simulation
when an authoritative full Character is created. The simulation advances a
small number of coarse childhood, student/apprentice, and professional phases
from age six and applies the same aptitude-aware activities and authored
curricula used by live training. Only the resulting current skill state is
stored: activity transitions, schedules, and life-history events are not
persisted or replayed. Native speech is treated as upbringing identity;
written literacy is earned inside the simulated student or institutional
curriculum under the ordinary Intelligence learning rate and cap.

[^6]: Even at 4x speed, which most computers can barely handle simulating, you're still spending most of your time watching your characters travel or rest.

## Setting
The world of _Adventure Simulator_ is a [historical fantasy](https://en.wikipedia.org/wiki/Historical_fantasy) version of Earth. Players of _Warhammer Fantasy_ or readers of [pre-Tolkien fantasy](https://www.gutenberg.org/ebooks/60184) will be familiar with the concept: the setting is a real-world historical period with generic fantasy elements inexplicably sprinkled throughout.

> Science fiction historian Brian Stableford has defined "historical fantasy" as "a term applied to fantasies in which the actual history of the primary world is conscientiously reproduced, save for limited infusions of working magic located within a 'secret history.'"

The heuristic for the fantasy elements is to put them in places that don't fundamentally alter historical conditions. Elves generally keep to forests or fictitious islands; Dwarves dwell within mountains; and creatures like Orcs, Goblins, Beastmen, and the Undead either roam as hordes or infest caves, crypts, and abandoned Dwarven settlements. To the extent that the kingdoms of Men interact with these fantastical elements, it is generally in hiring heroes to deal with the nuisances caused by hostile fantasy creatures. Elves and Dwarves are uninterested in Human political squabbles over borders and wars of succession, and fantastical enemies don't really pose a strategic threat to Human kingdoms, so the historical and fantastical elements of the setting can generally avoid stepping on each others' toes.

As for the historical elements, the year is approximately [1544 AD](https://en.wikipedia.org/wiki/1544). Being both the [height](https://upload.wikimedia.org/wikipedia/commons/5/53/Habsburg_Empire_of_Charles_V.png) of [Charles V](https://en.wikipedia.org/wiki/Charles_V,_Holy_Roman_Emperor)'s transatlantic empire and a year after the first Europeans reached Japan, it's just about the earliest feasible date in which all major cultures of the world can be at least indirectly aware of each other.[^7] For the MVP, the playable section of Earth will be limited to northern Germany, around the Baltic Sea;[^8] in the long term, we will gradually expand to all of Europe and beyond.

[^7]: Any later and your combat has too much ["shot"](https://en.wikipedia.org/wiki/Musketeer), not enough ["pike."](https://en.wikipedia.org/wiki/Landsknecht) We briefly considered [1650 AD](https://en.wikipedia.org/wiki/1650) as it's a very dynamic and rich setting (the EIC under Cromwell's England and VOC of the Dutch Republic scramble to take East Asian colonies from Portugal and Spain as Russian explorers reach the Pacific and Japanese pirates roam the seas), but by that time, swords and pikes just aren't seeing enough use in combat for our purposes. Someone else should totally make that game, though.

[^8]: Thanks to [the Hansa](https://en.wikipedia.org/wiki/Hanseatic_League) keeping very detailed maps of its trade routes, we have [Viabundus](https://www.landesgeschichte.uni-goettingen.de/handelsstrassen/map.php): an extremely high-quality CC-BY-SA data source for northern Europe's roads, terrain, and settlements. Presently, only the Hanseatic trade zone is in scope for Viabundus, but the project is gradually expanding into greater Europe. We thank the University of Göttingen for maintaining Viabundus.

## Philosophy
Below are some guiding principles for _Adventure Simulator_ development.

### Open source software
We tentatively intend to keep everything [AGPLv3](https://www.gnu.org/licenses/agpl-3.0.en.html), but we're willing to hear out the case for other licenses.

The AGPL applies to Adventure Simulator software unless a file or artifact says
otherwise. Generated strategic map tiles and terrain-routing packs are data
artifacts with a separate licence boundary: project-owned contributions are
offered under CC BY-SA 4.0 and underlying datasets retain their own terms. See
[MAP_DATA_LICENSE.md](MAP_DATA_LICENSE.md) before distributing or hosting those
artifacts.

It's clear to us that _Adventure Simulator_ is very much the kind of project which will benefit from collaboration and indefinite iteration, which makes open source the obvious choice by a country mile. For instance, though our MVP for _Adventure Simulator_ is (deliberately)[^9] generic historical fantasy, we don't intend or hope for it to stay that way. The project's open source nature will allow modders to come in and take it in all sorts of unexpected directions in the future; they may create [total conversions](https://en.wikipedia.org/wiki/Total_conversion) to other fantasy settings, sci-fi settings, or... [something else entirely](https://fxtwitter.com/warlockracy/status/1489001741337169926).

[^9]: Think of this as a high-effort tech demo in the spirit of Valve (cf. *Half-Life*). We really enjoy "weird fiction" like *Morrowind* and *Dune*, but at least for *Adventure Simulator*'s first iteration, the goal is to innovate in tech, not aesthetic. For now, our aesthetic is what has been proven to work.

### Procedural assets

Third-party asset licenses and attribution are recorded in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
It should be easy for players to create content for the game, so to that end, we will use low-fidelity procedural assets to greatly reduce the barrier to entry. This doesn't mean that we don't care about fidelity at all; it means fidelity must necessarily come from procedural iteration rather than a trained CG artist's skill. The system Nintendo uses for [Miis](https://en.wikipedia.org/wiki/Mii), for example, is a better example of how we might approach a character creator than, say, [_Baldur's Gate III_](https://baldursgate3.game/). But that doesn't mean that we're going for an especially cartoony art style, either; there's nothing to prevent us applying a system like to more realistically proportioned characters ([as Nintendo did](https://www.reddit.com/r/Games/comments/kq4a65/npcs_in_the_legend_of_zelda_breath_of_the_wild/), more or less, with _Breath of the Wild_ and its sequel).

The same principle for graphics applies to audio. A good introduction to procedural audio may be found in [*Designing Sound*](https://mitpress.mit.edu/9780262014410/designing-sound/) by Andy Farnell.

### Physically based gameplay
We would like the underlying gameplay systems to be *realistic*, as the real world can generally offer an unambiguous answer to any design question. It's not always easy to [*discover* that answer](https://en.wikipedia.org/wiki/Scientific_method), nor is it always easy to implement it without resorting to simplified abstractions,[^10] but all the imperfect solutions at least point in the same direction. Call this philosophy *physically based gameplay*, parallel to ["physically based rendering"](https://en.wikipedia.org/wiki/Physically_based_rendering) for graphics.[^11]

[^10]: Quantum physics is not in-scope for the MVP, to say the least.

[^11]: A game like *Team Fortress 2*, deliberately cartoony and unrealistic-looking, still employs "physically based rendering" in that its visuals are *based on* real-world lighting and material values, just tweaked and exaggerated to produce an unreal effect. The base values come from somewhere other than pure arbitrary imagination. Also known as "you need to know the rules in order to break them."

The real world is not always as fun as a game ought to be. Fortunately, there are two ways to get around this:

#### Abstraction-based approach
We can abstract away the parts of the real world that are not particularly fun.

Holding W to walk 50 km between settlements is not particularly fun, nor is resting for several months to heal a serious injury, but if we put these activities in the ["strategic layer"](#Gameplay) of the game (separate from the real-time "tactical layer"), a player can skip them by fast-forwarding in time. Likewise, micromanaging inventory is not particularly fun, but as the game becomes complex enough to necessitate it, we can also add tools to automate it, such as setting a desired weight limit and value/weight ratio for loot.

#### Content-based approach
We can design the non-real parts of the world to be more fun.

Being that this is a fantasy world, the fantastical elements are free variables for us to balance the game with. Suppose real-life combat is too fast for it to be viable to reliably dodge most attacks; we can simply give common fantasy enemies like Goblins, Orcs, and Skeletons such poor melee skills that an agile player character can reliably dodge them. Or suppose stealth is too frustrating with realistic detection ranges; we can just ensure that these fantasy creatures tend to have very poor eyesight.

## Funding and legal
Adventure Simulator Group LLC is a for-profit company owned by Bruno Segovia (CEO) and Adler Halbe (Director). The founders are willing to invest serious portions of their incomes to see at least a prototype of this through. As they will be maintaining full-time employment throughout the development process, their contributions will largely be in the form of cash, but they will be available most days to provide guidance and strategic direction, primarily during evenings and weekends (PST).

Once the game works well enough to start hosting (and is sufficiently fun to be worth anyone's time), the founders will try and transition to a more sustainable funding model: one where players may have a single character per account for free but pay a subscription fee for multi-character accounts. This funding will be used to hire more developers, pay server costs, and hopefully obtain some profit.

Due to being open source, if at any time Adventure Simulator Group starts "[enshittifying](https://en.wikipedia.org/wiki/Enshittification)" the service, the community can simply fork it and host their own instance of the server. This hopefully will never happen, however, as the threat of it ought to suffice to keep everyone's incentives aligned. At face value, this is a terrible business decision (to willingly give up one's monopoly power), but the success of [Patreon](https://www.patreon.com) and [Substack](https://substack.com/) is evidence that relying on the goodwill of the community *can be* a genuinely viable business model, especially for an inherently creative product like a game. Will it actually work? We don't know. Let's find out!

### Are you accepting investors?
Probably not. We want to be very selective about adding board members. However, if you think that you can make a good case, send an email to our CEO, [Bruno Segovia](mailto:bruno@adventuresim.org).

## Open (paid) positions
~~All positions are remote-only and with no Zoom meetings (unless you actually want them). Contact <halbe@adventuresim.org> to apply.~~

Having hired our first round of developers, we are not currently seeking applicants for any positions. We will likely initiate another developer hiring round in March. In the meantime, if you think you can contribute in some other way like writing or testing, send an email to <halbe@adventuresim.org>.
