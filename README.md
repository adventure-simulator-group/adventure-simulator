# _Adventure Simulator_ (working title)
_Adventure Simulator_ is a web-first pseudo-MMO using data-star, Wasm, and WebGPU to make a sort of game which has been impossible to build until now.

## A web-first pseudo-MMO
The golden age of browser-based web games yielded a number of "pseudo-MMO" games, like _Neopets_ and _Club Penguin_,[^1] whose markets have since largely been captured by mobile games and native desktop games. However, new technologies like [Wasm](https://webassembly.org/), [WebGPU](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API), and [data-star](https://data-star.dev/) allow us to make a kind of web-first game that has not been possible to do until recently: one with near-feature and performance parity with native applications.

We aren't building a normal web game which uses Wasm and WebGPU to run in the browser. We are building a hypertext ["bulletin board"](https://en.wikipedia.org/wiki/Bulletin_board_system) game which can act like a normal game when needed, like in combat, and where most of the game logic isn't even running in the browser but rather streamed, via data-star, from the server.

[^1]: And actual MMOs, such as _Runescape_ and _AdventureQuest_.

### Bulletin-board world
A traditional MMO relies on a central, authoritative server which maintains the live state of the game world, runs simulation logic in real time, and pushes state updates to clients dozens of times per second. Designing and implementing this server presents a host of enormously complex networking challenges.[^2] Our plan is to sidestep these challenges by representing our world as a bulletin board:[^3] a database contains information about players, places, and quests, and players interact with this world-database by making discrete actions through an asynchronous, web-style [HATEOAS](https://en.wikipedia.org/wiki/HATEOAS) interface. When players do need real-time action, e.g. when they engage enemies in combat, we create a private virtual server just for their party. Since _any_ real-time networking can quickly become dangerously complex, we intend to keep as much state as possible on the server, using server-sent events to push updates directly to the client as events happen.[^4]

Player characters spend their downtime in settlements: persistent, bulletin board-like social hubs where they can purchase equipment, join parties, and embark on quests. When their party sets out on a quest, players will load into the real-time, WebGPU-rendered combat simulation when they arrive their destination or are randomly attacked along the way. When the real-time simulation is no longer required, players transition back into the discrete hypertext format.

[^2]: This unified server has to handle massive concurrency, synchronize thousands of players in real time, ensure consistency so everyone sees the game world, and maintain low latency. However, we don't have to do any of this on a bulletin board because there are no active connections to maintain. As soon as the server responds to your request, it forgets that you exist.

[^3]: You can think of a bulletin board as halfway between a Discord server and an Internet forum. Think of an imageboard: the threads are more live than reddit or forums, less live than chatrooms. The benefit of the format is that it works both synchronously and asynchronously; you can have a nearly live chat with a guy on /tg/, but the format also works even if you're the only live user on a given thread at that time. This isn't an unheard-of inspiration for a fantasy RPG; [_Dragon's Dogma_](https://www.dragonsdogma.com/en-us/)'s internal project name was ["BBS-RPG"](https://www.dragonsdogma.com/assets/images/gallery/gallery_img18_01_en.jpg) due to the ["custom mercenary character" system](https://www.dragonsdogma.com/assets/images/gallery/gallery_img19_01_en.jpg). We would be taking the idea much further than _DD_ did, of course. 

[^4]: We end up rendering a sort of network-driven ["immediate mode"](https://en.wikipedia.org/wiki/Immediate_mode_(computer_graphics)) view of the world.

## Gameplay
The nearest games for inspiration are [*Mount and Blade*](https://www.taleworlds.com/en/games/mountandblade), [*Battle Brothers*](https://battlebrothersgame.com/), [Jagged Alliance](https://store.steampowered.com/app/1084160/Jagged_Alliance_3/), [*Starsector*](https://fractalsoftworks.com/), and to some extent [*Kenshi*](https://lofigames.com/).

Like the former three, the world of _Adventure Simulator_ is separated between the "tactical" layer (a real-time simulation) and the "strategic" layer (which advances in discrete chunks of time, generally after fast travel or resting). We have the same basic gameplay formula: a player recruits a party to adventure with, defeats enemies in randomly generated missions, and uses their hard-earned rewards to buy equipment.

Like in _Kenshi_, _Battle Brothers_, and *Jagged Alliance* players can control multiple characters, though _Adventure Simulator_ characters can be either mortal or immortal. Mortal characters offer a more roguelike/extraction-esque experience, with fast progression and frequent deaths. When one of your mortal characters dies, any wealth not on their person will be inherited by your other characters. Immortal characters offer a more conventional RPG/MMO experience, which emulates the cost of mortal characters with costly respawns and slow healing.[^5]

If there is any design choice in particular that makes our approach unique, it is specifically that we relinquish the vision of having a continuous, immersive world. _Kenshi_ clings to it despite all the systems of the game going against it,[^6] and it makes real sandbox MMOs impossible for networking, but we look to singleplayer games like _Starsector_, *Jagged Alliance*, and _Mount and Blade_ which *chose* to have a strategic layer, not because they had to for networking, but because their gameplay loop would be really boring and repetitive without one. You can actually walk around cities in _Warband_ and _Bannerlord_, and zero players actually do this outside of sieges because walking around is boring. Thus, we take the basic design of those games and combine it with the one infamous problem which their design incidentally solves: MMO networking.

[^5]: Mortal characters will probably be randomly generated by default. The idea is that players who prefer custom characters will naturally gravitate to the immortal option.

[^6]: Even at 4x speed, which most computers can barely handle simulating, you're still spending most of your time watching your characters travel or rest.

## Setting
The world of _Adventure Simulator_ is a fantastical version of Earth. _Warhammer Fantasy_ players or readers of pre-Tolkien fantasy will be familiar with the idea: essentially, the setting is historical Renaissance Earth with generic fantasy elements inexplicably sprinkled throughout.[^7]

The heuristic here is to place fantasy elements in places that don't fundamentally alter historical conditions too much. Elves generally keep to forests or fictitious islands; Dwarves dwell within mountains; and creatures like Orcs, Goblins, Beastmen, and the Undead either roam as hordes or infest caves, crypts, and abandoned Dwarven settlements. To the extent that the kingdoms of Men interact with these fantastical elements, it is generally in hiring heroes to deal with the nuisances caused by hostile fantasy creatures. Elves and Dwarves are uninterested in Human political squabbles over borders and wars of succession, and fantastical enemies never really pose a strategic threat to Human kingdoms, so the historical and fantastical elements of the setting can generally avoid stepping on each others' toes.

For the MVP, the playable section of Earth will be limited to Italy.[^8] In the long term, we will gradually expand to all of Europe and beyond.

[^7]: The year is approximately 1543 AD: the [height](https://upload.wikimedia.org/wikipedia/commons/5/53/Habsburg_Empire_of_Charles_V.png) of [Charles V](https://en.wikipedia.org/wiki/Charles_V,_Holy_Roman_Emperor)'s transatlantic empire, the year the Portuguese reached Japan, and the earliest possible time in which all major cultures of the world are at least indirectly aware of each other. (Any later and your combat has too much "shot", not enough "pike.")

[^8]: Renaissance Italy offers huge cultural and geographic diversity in a relatively small area: within a few hundred kilometers, you have bustling urban centers, Alpine mountain fortresses, rural farmland and villages, and ancient Roman ruins. With warring city-states and feuding families aplenty, it also offers the perfect economy for professional adventurers, and all the while, it is instantly recognizable even to non-historians (think Leonardo da Vinci, Machiavelli, and the Medicis).

## Philosophy
Below are some guiding principles for _Adventure Simulator_ development.

### Open source
We tentatively intend to keep everything GPL-3.0, but we are willing to hear out the case for other licenses.

_Adventure Simulator_ is not designed _solely_ to facilitate a fantasy game. Its open-source nature means that modders can come in and take it in all sorts of unexpected directions in the future. They may create [total conversions](https://en.wikipedia.org/wiki/Total_conversion) to other fantasy settings, sci-fi, or... [something else](https://fxtwitter.com/warlockracy/status/1489001741337169926).

### Graphics: flexibility > fidelity
It should be easy for players to create content for the game, and to that end, we use low-fidelity procedural assets to greatly reduce the barrier to entry. This doesn't mean that we don't care about fidelity at all; it means fidelity must necessarily come from procedural iteration rather than a trained CG artist. The system Nintendo uses for [Miis](https://en.wikipedia.org/wiki/Mii), for example, is a better example of how we might approach a character creator than, say, [_Baldur's Gate III_](https://baldursgate3.game/). But that doesn't mean that we're going for an especially cartoony art style, either; there's nothing preventing a system like that from applying to more realistically proportioned characters ([as Nintendo did](https://www.reddit.com/r/Games/comments/kq4a65/npcs_in_the_legend_of_zelda_breath_of_the_wild/), more or less, with _Breath of the Wild_ and its sequel).

### Gameplay design
We would like the underlying gameplay systems to be *realistic*, as the real world can generally offer an unambiguous answer to any design question. It's not always easy to [find out what that answer *is*](https://en.wikipedia.org/wiki/Scientific_method), nor is it always easy to implement the real world solution without resorting to simplified abstractions,[^9] but all the imperfect answers at least point in the same direction.

[^9]: Quantum physics is not in-scope for the MVP, to say the least.

The real world is not always as fun as a game ought to be. Fortunately, there are two ways to get around this:

#### Abstraction-based approach
We can abstract away the parts of the real world that are not particularly fun.

Holding W to walk 50 km between settlements is not particularly fun, nor is resting for several months to heal a serious injury, but if we put these activities in the ["strategic layer"](#Gameplay) of the game (separate from the real-time "tactical layer"), a player can skip them by fast-forwarding in time.

Likewise, micromanaging inventory is not particularly fun, but as the game becomes complex enough to necessitate this, we can also add tools to automate it, such as setting a desired weight limit and value/weight ratio for loot.

#### Content-based approach
We can design the non-real parts of the world to be more fun.

Being that this is a fantasy world, the fantastical elements are free variables for us to balance the game with. Suppose real-life combat is too fast for it to be viable to reliably dodge most attacks. We can simply give common fantasy enemies like Goblins, Orcs, and Skeletons such poor melee skills that an agile player character can reliably dodge them. Or suppose stealth is too frustrating with realistic detection ranges. We can just ensure that these fantasy creatures tend to have very poor eyesight.

## Funding and legal
We are in the process of registering with [Open Source Collective](https://oscollective.org/), a 501(c)(6) that handles the legal and administrative overhead for facilitating donors to pay contributors. The two founders, Bruno Segovia and Adler Halbe, are willing to contribute serious portions of their incomes to see at least a prototype of this through. Their contributions will largely be in the form of cash, however, as their day jobs from which this cash originates are pretty intensive. But they will be available most days, especially in the evenings (PST), to offer guidance and help coordinate the project.

Once the game works well enough to start hosting (and is sufficiently fun to be worth anyone's time), we will try and transition to a more sustainable funding model: one where players may have a single character per account for free but multi-character accounts require a subscription fee, which we use to hire more developers and pay for server costs.

Everything will remain completely open source and non-profit throughout this. Adler and Bruno have no intention of profiting from this project. They simply wish for it to exist.

## Open (paid) positions
All positions are remote-only and with no Zoom meetings (unless you actually want them). Contact <halbe@adventuresim.org> to apply.

### Strategic-layer programmer - $40k USD/yr
Design and implement the asynchronous strategic layer of the game: the database, [HATEOAS](https://en.wikipedia.org/wiki/HATEOAS) interface, and gameplay systems.

#### Required skills
- Attribute-driven frontend frameworks required ([data-star](https://data-star.dev), [HTMX](https://htmx.org/), or [Alpine](https://alpinejs.dev/), etc.)
- Diverse enough array of database architectures to have a strong opinion on which should be used here. (We could even unify with the tactical-layer ECS, but that would be weird.)
- [Rust](https://rust-lang.org/)

#### Recommended skills
- Entity Component Systems like [Bevy](https://bevy.org)
- [data-star](https://data-star.dev)
- Real-time networking
- Devops
- Cloud infrastructure

### Tactical-layer programmer - $40k USD/yr
Design and implement the real-time tactical layer of the game, both the server and the client. If you think that you are a cracked 10xer wizard that can do both layers, you can apply for both and negotiate for *up to* $80k.

#### Required skills
- [Bevy](https://bevy.org)
- Real-time networking

#### Recommended skills
- [data-star](https://data-star.dev)
- Devops
- Cloud infrastructure
- Newtonian physics (gameplay equations generally put variables in SI units)
- Linear algebra

### Procedural graphics programmer - $40k USD/yr
Design and implement a plugin to generate models for game objects. The plugin, like the rest of the game, will be tentatively GPL-3.0, but if you want a dual license or MIT/Apache, we're willing to negotiate.

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
