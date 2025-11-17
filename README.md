# _Adventure Simulator_ (working title)
_Adventure Simulator_ is a web-first pseudo-MMO using Datastar, Wasm, and WebGPU to make a kind of game which has been impossible to build until now.

## A web-first pseudo-MMO
The golden age of browser gaming yielded a number of highly successful "pseudo-MMO" games, like _Neopets_ and _Club Penguin_,[^1] whose markets have since been captured by mobile apps and native desktop games. However, new technologies like [Wasm](https://webassembly.org/), [WebGPU](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API), and [Datastar](https://Datastar.dev/) allow us to make a new kind of web-first game, one that hasn't been possible until very recently: one with near-feature and performance parity with native applications.

[^1]: And actual MMOs, such as _Runescape_ and _AdventureQuest_.

### Bulletin-board world
A traditional MMO relies on a central server to maintain the live state of the game world, run simulation logic in real time, and push state updates to clients dozens of times per second. Designing and implementing this server presents a host of complex networking challenges. Consider that to make an MMO, you have to build a backend that can handle massive concurrency, synchronize thousands of players in real time, ensure consistency so everyone sees the same world, and maintain low latency. It's not so fun. In related news, most people don't make MMOs.

Our plan, however, is to sidestep these challenges altogether by representing our world, not as a continuous simulation on a server, but as a ["bulletin board"](https://en.wikipedia.org/wiki/Bulletin_board_system)[^3]: a database contains information about players, places, and quests, and players interact with this world-database by taking discrete actions through an asynchronous, [hypertext (web-style)](https://en.wikipedia.org/wiki/HATEOAS) interface. Unlike an MMO's world-server, a bulletin board has no active connections to maintain; as soon as it responds to your request, it forgets you exist. When players do need real-time action, e.g. when they engage enemies in combat, we create a private virtual server just for their party, though as _any_ real-time networking can quickly become dangerously complex, we intend to keep as much state as possible on the server, using server-sent events to push updates directly to the client as events happen.[^4]

Player characters spend their downtime in settlements, which are persistent, bulletin board-like social hubs where they can purchase equipment, join parties, and embark on quests. When their party sets out on a quest, players load into a real-time, WebGPU-rendered combat simulation when they arrive their destination or are randomly attacked along the way; when the real-time simulation is no longer required, players transition back into the discrete hypertext format.

This is all to say that we aren't building a "normal" web game which uses Wasm and WebGPU to run in the browser. We are building a hypertext bulletin-board game which can *act like* a normal game when needed, like in combat, and where most of the game logic isn't even running in the browser but rather streamed, via Datastar, from the server.

[^3]: You can think of a bulletin board as halfway between a Discord server and an Internet forum. Think of an imageboard: the threads are more live than reddit or forums, less live than chatrooms. The benefit of the format is that it works both synchronously and asynchronously; you can have a nearly live chat with a guy on /tg/, but the format also works even if you're the only live user on a given thread at that time. This isn't an unheard-of inspiration for a fantasy RPG; [_Dragon's Dogma_](https://www.dragonsdogma.com/en-us/)'s internal project name was ["BBS-RPG"](https://www.dragonsdogma.com/assets/images/gallery/gallery_img18_01_en.jpg) due to the ["custom mercenary character" system](https://www.dragonsdogma.com/assets/images/gallery/gallery_img19_01_en.jpg). We would be taking the idea much further than _DD_ did, of course. 

[^4]: We end up rendering a sort of network-driven ["immediate mode"](https://en.wikipedia.org/wiki/Immediate_mode_(computer_graphics)) view of the world.

## Gameplay
The nearest games for inspiration are [*Mount and Blade*](https://www.taleworlds.com/en/games/mountandblade), [*Battle Brothers*](https://battlebrothersgame.com/), [*Jagged Alliance*](https://store.steampowered.com/app/1084160/Jagged_Alliance_3/), [*Starsector*](https://fractalsoftworks.com/), and to some extent [*Kenshi*](https://lofigames.com/).

Like the former three, the world of _Adventure Simulator_ is separated between the "tactical" layer (a real-time simulation) and the "strategic" layer (which advances in discrete chunks of time, generally after fast travel or resting). We have the same basic gameplay formula where the player recruits a party to adventure with, defeats enemies in randomly generated missions, and uses their hard-earned rewards to buy equipment for future missions.

Like in _Kenshi_, _Battle Brothers_, and _Jagged Alliance_, players can control multiple characters, though in _Adventure Simulator_, characters can be either mortal or immortal. Mortal characters offer a more roguelike/"extraction" experience, with fast progression and frequent deaths; when one of your mortal characters dies, any wealth not on their person will be inherited by your other characters. Immortal characters offer a more conventional RPG/MMO experience, which emulates the cost of mortal characters with costly respawns and slow healing.[^5]

If there's any design choice in particular that makes our approach unique, it is specifically that we relinquish the vision of having a continuous, immersive world. _Kenshi_ clings to that vision, despite all the systems of the game going against it,[^6] and most MMOs try to reach that ideal before networking gets in the way. We take our inspiration from singleplayer games like _Starsector_, _Jagged Alliance_, and _Mount and Blade_ which all *chose* to have a strategic layer, not because they had to for networking, but because their gameplay loop would be really boring without one. You can actually walk around cities in _Warband_ and _Bannerlord_, but zero players actually do this outside of sieges because walking around is boring. Thus, we take those games' basic design and combine it with the one infamous problem it incidentally solves: MMO networking.

[^5]: Mortal characters will probably be randomly generated by default. The idea is that players who prefer custom characters will naturally gravitate to the immortal option.

[^6]: Even at 4x speed, which most computers can barely handle simulating, you're still spending most of your time watching your characters travel or rest.

## Setting
The world of _Adventure Simulator_ is a fantastical version of Earth. Players of _Warhammer Fantasy_ or readers of [pre-Tolkien fantasy](https://www.gutenberg.org/ebooks/60184) will be familiar with the idea: essentially, the setting is historical Renaissance Earth with generic fantasy elements inexplicably sprinkled throughout.

The heuristic for the fantasy elements is to put them in places that don't fundamentally alter historical conditions. Elves generally keep to forests or fictitious islands; Dwarves dwell within mountains; and creatures like Orcs, Goblins, Beastmen, and the Undead either roam as hordes or infest caves, crypts, and abandoned Dwarven settlements. To the extent that the kingdoms of Men interact with these fantastical elements, it is generally in hiring heroes to deal with the nuisances caused by hostile fantasy creatures. Elves and Dwarves are uninterested in Human political squabbles over borders and wars of succession, and fantastical enemies don't really pose a strategic threat to Human kingdoms, so the historical and fantastical elements of the setting can generally avoid stepping on each others' toes.

As for the historical elements, the year is approximately [1543 AD](https://en.wikipedia.org/wiki/1543): being the [height](https://upload.wikimedia.org/wikipedia/commons/5/53/Habsburg_Empire_of_Charles_V.png) of [Charles V](https://en.wikipedia.org/wiki/Charles_V,_Holy_Roman_Emperor)'s transatlantic empire *and* the year that Portugal reached Japan, it is the earliest possible time in which all major cultures of the world are at least indirectly aware of each other.[^7] For the MVP, the playable section of Earth will be limited to Italy.[^8] In the long term, we will gradually expand to all of Europe and beyond.

[^7]: Any later and your combat has too much ["shot"](https://en.wikipedia.org/wiki/Musketeer), not enough ["pike."](https://en.wikipedia.org/wiki/Landsknecht) We briefly considered [1650 AD](https://en.wikipedia.org/wiki/1650) as it's a very dynamic and rich setting, with the EIC under Cromwell's England and VOC of the Dutch Republic scrambling to take East Asian colonies from Portugal and Spain as Russian explorers reach the Pacific and Japanese pirates roam the seas, but by that time, swords and pikes just aren't seeing enough use in combat for our purposes. Someone else should totally try it, though.

[^8]: Renaissance Italy offers huge cultural and geographic diversity in a relatively small area: in just a few hundred square kilometers, you have bustling urban centers, Alpine mountain fortresses, rural farmland and villages, and ancient Roman ruins. With warring city-states and feuding families aplenty, Italy also offers the perfect economy for professional adventurers, and all the while, it is instantly recognizable even to non-historians (Leonardo da Vinci, Machiavelli, Medicis, etc.).

## Philosophy
Below are some guiding principles for _Adventure Simulator_ development.

### Open source
We tentatively intend to keep everything [GPLv3](https://www.gnu.org/licenses/gpl-3.0.en.html), but we're willing to hear out the case for other licenses.

_Adventure Simulator_ is not designed _solely_ for generic fantasy. Its open-source nature will allow modders to come in and take it in all sorts of unexpected directions in the future. They may create [total conversions](https://en.wikipedia.org/wiki/Total_conversion) to other fantasy settings, sci-fi, or... [something else entirely](https://fxtwitter.com/warlockracy/status/1489001741337169926).

### Graphics: flexibility > fidelity
It should be easy for players to create content for the game, so to that end, we will use low-fidelity procedural assets to greatly reduce the barrier to entry. This doesn't mean that we don't care about fidelity at all; it means fidelity must necessarily come from procedural iteration rather than a trained CG artist's skill. The system Nintendo uses for [Miis](https://en.wikipedia.org/wiki/Mii), for example, is a better example of how we might approach a character creator than, say, [_Baldur's Gate III_](https://baldursgate3.game/). But that doesn't mean that we're going for an especially cartoony art style, either; there's nothing to prevent us applying a system like to more realistically proportioned characters ([as Nintendo did](https://www.reddit.com/r/Games/comments/kq4a65/npcs_in_the_legend_of_zelda_breath_of_the_wild/), more or less, with _Breath of the Wild_ and its sequel).

### Gameplay design
We would like the underlying gameplay systems to be *realistic*, as the real world can generally offer an unambiguous answer to any design question. It's not always easy to [*discover* that answer](https://en.wikipedia.org/wiki/Scientific_method), nor is it always easy to implement it without resorting to simplified abstractions,[^9] but all the imperfect solutions at least point in the same direction.

[^9]: Quantum physics is not in-scope for the MVP, to say the least.

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
All positions are remote-only and with no Zoom meetings (unless you actually want them). Contact <halbe@adventuresim.org> to apply.

### Full-stack developer - $40k USD/yr
Design and implement the asynchronous strategic layer of the game: the database, [HATEOAS](https://en.wikipedia.org/wiki/HATEOAS) interface, and gameplay systems.

#### Required skills
- Attribute-driven frontend frameworks required ([Datastar](https://Datastar.dev), [HTMX](https://htmx.org/), or [Alpine](https://alpinejs.dev/), etc.)
- Diverse enough array of database architectures to have a strong opinion on which should be used here.
- [Rust](https://rust-lang.org/)

#### Recommended skills
- Entity component systems like [Bevy](https://bevy.org)
- [Datastar](https://Datastar.dev)
- Real-time networking
- Devops
- Cloud infrastructure

### Game programmer - $40k USD/yr
Design and implement the real-time layer of the game, both the server and the client. If you think that you are a cracked 10xer wizard that can do both this and webdev, you can apply for both and negotiate for *up to* $80k.

#### Required skills
- [Bevy](https://bevy.org)
- Real-time networking

#### Recommended skills
- [Datastar](https://Datastar.dev)
- Devops
- Cloud infrastructure
- Newtonian physics (gameplay equations generally put variables in SI units)
- Linear algebra

### Procedural graphics programmer - $40k USD/yr
Design and implement a plugin to generate models for game objects. The plugin, like the rest of the game, will be a dual license of GPLv3 + proprietary. You will be able to license its proprietary use for your own project so long as it isn't in the fashion industry, as we are in the process of setting up a deal with a fashion company (if it goes through, we'll also be hiring for more positions).

#### Required skills
- [CSG](https://en.wikipedia.org/wiki/Constructive_solid_geometry) primitives and operations
- Advancing front
- Distance fields
- [Rust](https://rust-lang.org)

#### Recommended skills
- [wgpu](https://wgpu.rs/)
- [Bevy](https://bevy.org)
- Procedural modeling (Houdini, Blender geometry nodes, etc)
- Procedural textures
- Character creators (morph targets, texture compositing, etc)
- Physically based rendering concepts

### Other contributions
If you think that you can help in some other way, like writing or testing, send an email to <halbe@adventuresim.org>.
