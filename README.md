# _Oblitus_
_Oblitus_ is a web-first pseudo-MMO using Datastar, Wasm, and WebGPU to make a kind of game which has been impossible to build until now.

## A web-first pseudo-MMO
The golden age of browser-based web games yielded a number of "pseudo-MMO" games, like _Neopets_ and _Club Penguin_,[^1] whose markets have since largely been captured by mobile apps and native desktop games. However, new technologies like [Wasm](https://webassembly.org/), [WebGPU](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API), and [Datastar](https://Datastar.dev/) allow us to make a kind of web-first game that has not been possible until recently: one with near-feature and performance parity with native applications.

[^1]: And actual MMOs, such as _Runescape_ and _AdventureQuest_.

### Bulletin-board world
A traditional MMO relies on a central, authoritative server which maintains the live state of the game world, runs simulation logic in real time, and pushes state updates to clients dozens of times per second. Designing and implementing this server presents a host of enormously complex networking challenges.[^2] Our plan is to sidestep these challenges by representing our world as a ["bulletin board"](https://en.wikipedia.org/wiki/Bulletin_board_system):[^3] a database contains information about players, places, and quests, and players interact with this world-database by making discrete actions through an asynchronous, [web-style](https://en.wikipedia.org/wiki/HATEOAS) interface. When players do need real-time action, e.g. when they engage enemies in combat, we create a private virtual server just for their party. Since _any_ real-time networking can quickly become dangerously complex, we intend to keep as much state as possible on the server, using server-sent events to push updates directly to the client as events happen.[^4]

Player characters spend their downtime in "hubs": persistent, bulletin board-like social hubs where they can purchase equipment, join parties, and take missions. When their party sets out on a mission, players will load into the real-time, WebGPU-rendered combat simulation. When the real-time simulation is no longer required, players transition back into the hypertext interface. In short, we aren't building a normal web game which uses Wasm and WebGPU to run in the browser; we are building a hypertext bulletin board game which can act like a normal game when needed, like in combat, and where most of the game logic isn't even running in the browser but rather streamed, via Datastar, from the server.

[^2]: This unified server has to handle massive concurrency, synchronize thousands of players in real time, ensure consistency so everyone sees the game world, and maintain low latency. However, we don't have to do any of this on a bulletin board because there are no active connections to maintain. As soon as the server responds to your request, it forgets that you exist.

[^3]: You can think of a bulletin board as halfway between a Discord server and an Internet forum. Think of an imageboard: the threads are more live than reddit or forums, less live than chatrooms. The benefit of the format is that it works both synchronously and asynchronously; you can have a nearly live chat with a guy on /tg/, but the format also works even if you're the only live user on a given thread at that time. This isn't an unheard-of inspiration for a fantasy RPG; [_Dragon's Dogma_](https://www.dragonsdogma.com/en-us/)'s internal project name was ["BBS-RPG"](https://www.dragonsdogma.com/assets/images/gallery/gallery_img18_01_en.jpg) due to the ["custom mercenary character" system](https://www.dragonsdogma.com/assets/images/gallery/gallery_img19_01_en.jpg). We would be taking the idea much further than _DD_ did, of course. 

[^4]: We end up rendering a sort of network-driven ["immediate mode"](https://en.wikipedia.org/wiki/Immediate_mode_(computer_graphics)) view of the world.

## Gameplay
The nearest games for inspiration are [*Mount and Blade*](https://www.taleworlds.com/en/games/mountandblade), [*Battle Brothers*](https://battlebrothersgame.com/), [*Jagged Alliance*](https://store.steampowered.com/app/1084160/Jagged_Alliance_3/), [*XCOM*](https://store.steampowered.com/app/268500/XCOM_2/), [*Starsector*](https://fractalsoftworks.com/), and to some extent [*Kenshi*](https://lofigames.com/).

Like the former five, the world of _Oblitus_ is separated between the "tactical" layer (a real-time simulation) and the "strategic" layer (which advances in discrete chunks of time, generally after fast travel or resting). We have the same basic gameplay formula: a player recruits a party to work with, defeats enemies in randomly generated missions, and uses their hard-earned rewards to buy equipment.

Like in _Kenshi_, _Battle Brothers_, *XCOM*, and _Jagged Alliance_, players can control multiple characters, though _Oblitus_ characters can be either mortal or immortal. Mortal characters offer a more roguelike/extraction-esque experience, with fast progression and frequent deaths. When one of your mortal characters dies, any wealth not on their person will be inherited by your other characters. Immortal characters offer a more conventional RPG/MMO experience, which emulates the cost of mortal characters with costly respawns and slow healing.[^5]

If there is any design choice in particular that makes our approach unique, it is specifically that we relinquish the vision of having a continuous, immersive world. _Kenshi_ clings to it despite all the systems of the game going against it,[^6] and it makes real sandbox MMOs impossible for networking, but we look to singleplayer games like _Starsector_, _Jagged Alliance_, and _Mount and Blade_ which *chose* to have a strategic layer, not because they had to for networking, but because their gameplay loop would be really boring and repetitive without one. You can actually walk around cities in _Warband_ and _Bannerlord_, but zero players actually do this outside of sieges because walking around is boring. Thus, we take the basic design of those games and combine it with the one infamous problem which their design incidentally solves: MMO networking.

[^5]: Mortal characters will probably be randomly generated by default. The idea is that players who prefer custom characters will naturally gravitate to the immortal option.

[^6]: Even at 4x speed, which most computers can barely handle simulating, you're still spending most of your time watching your characters travel or rest.

## Setting
The world of _Oblitus_ is the secret supernatural "underworld" of the modern world. It is essentially a combination of the world of the [SCP Foundation](https://scp-wiki.wikidot.com/) and [World of Darkness (Vampire the Masquerade)](https://whitewolf.fandom.com/wiki/World_of_Darkness), though toned down from each. 

For the unfamiliar, the "Foundation" is a secret international organization dedicated to the containment of supernatural phenomena. They have a near unlimited budget, in our version largely funded through contributions from UN member states, and carry out their mission with a much more flexible system of ethics than would be permissible under other circumstances.

Vampire clans exist on a spectrum. On one end you have the "lawful evil" bloodlines that participate in a criminal syndicate known as the "Midnight Tribunal". They enforce a strict code of conduct that prohibits wanton murder, limits the siring of new vampires, and polices any action which might draw the attention of human governments or the wrath of the Foundation. Because of this, the Foundation generally doesn't try *too* hard to work against the Tribunal.

On the other end of the spectrum are the free clans who are small, ambitious, and much less careful than those in the Tribunal. Some will pledge themselves to Tribunal clans to gain protection, others stay independent, and all are constantly squabbling over territory and are frequent antagonists to the Foundation.

Player characters are those who are privy to this secret world and work for one of the factions involved with it. For the MVP, this will just be the Foundation and one or more vampire clans. Missions will generally revolve around territory warfare against or between free clans, but as we flesh out the game we can create missions for theft, espionage, sabotage, assassination, eliminating witnesses, and containing other supernatural phenomena.

Even when a large enough playerbase does not exist for PvP to be reliable, there will always be NPC-run free clans to fight, ensuring that our game is still enjoyable with only one active player.

## Philosophy
Below are some guiding principles for _Oblitus_ development.

### Open source
We tentatively intend to keep everything GPL-3.0, but we are willing to hear out the case for other licenses.

_Oblitus_ is not designed _solely_ to facilitate a fantasy game. Its open-source nature means that modders can come in and take it in all sorts of unexpected directions in the future. They may create [total conversions](https://en.wikipedia.org/wiki/Total_conversion) to other fantasy settings, sci-fi, or... [something else](https://fxtwitter.com/warlockracy/status/1489001741337169926).

### Graphics: flexibility > fidelity
It should be easy for players to create content for the game, and to that end, we use low-fidelity procedural assets to greatly reduce the barrier to entry. This doesn't mean that we don't care about fidelity at all; it means fidelity must necessarily come from procedural iteration rather than a trained CG artist's skill. The system Nintendo uses for [Miis](https://en.wikipedia.org/wiki/Mii), for example, is a better example of how we might approach a character creator than, say, [_Baldur's Gate III_](https://baldursgate3.game/). But that doesn't mean that we're going for an especially cartoony art style, either; there's nothing preventing a system like that from applying to more realistically proportioned characters ([as Nintendo did](https://www.reddit.com/r/Games/comments/kq4a65/npcs_in_the_legend_of_zelda_breath_of_the_wild/), more or less, with _Breath of the Wild_ and its sequel).

### Gameplay design
We would like the underlying gameplay systems to be *realistic*, as the real world can generally offer an unambiguous answer to any design question. It's not always easy to [find out what that answer *is*](https://en.wikipedia.org/wiki/Scientific_method), nor is it always easy to implement the real world solution without resorting to simplified abstractions,[^9] but all the imperfect answers at least point in the same direction.

[^9]: Quantum physics is not in-scope for the MVP, to say the least.

The real world is not always as fun as a game ought to be. Fortunately, there are two ways to get around this:

#### Abstraction-based approach
We can abstract away the parts of the real world that are not particularly fun.

Waiting in line at the airport to travel between cities is not particularly fun, nor is resting for several months to heal a serious injury, but if we put these activities in the ["strategic layer"](#Gameplay) of the game (separate from the real-time "tactical layer"), a player can skip them by fast-forwarding in time.

Likewise, micromanaging inventory is not particularly fun, but as the game becomes complex enough to necessitate this, we can also add tools to automate it, such as setting a desired weight limit and value/weight ratio for loot.

#### Content-based approach
We can design the non-real parts of the world to be more fun.

Being that this is a fantasy world, the fantastical elements are free variables for us to balance the game with. Suppose real-life firearms are too accurate for it to be viable to reliably avoid most attacks. Players who are looking for more of an action game experience can play as vampires with supernatural speed, skirting around enemies faster than they can turn their weapons. Or suppose stealth is too frustrating with realistic detection ranges, we can give vampires the supernatural ability to blend into the shadows.

## Funding and legal
Adventure Simulator Group LLC is a for-profit company owned by Bruno Segovia (CEO) and Adler Halbe (Director). The founders are willing to invest serious portions of their incomes to see at least a prototype of this through. As they will be maintaining full-time employment throughout the development process, their contributions will largely be in the form of cash, but they will be available most days to provide guidance and strategic direction, primarily during evenings and weekends (PST).

Once the game works well enough to start hosting (and is sufficiently fun to be worth anyone's time), the founders will try and transition to a more sustainable funding model: one where players may have a single character per account for free but multi-character accounts require a subscription fee, which is used to hire more developers, pay for server costs, and hopefully obtain some profit.

Due to being open source, if at any time Oblitus Group starts "[enshittifying](https://en.wikipedia.org/wiki/Enshittification)" the service, the community can simply fork it and host their own instance of the server. This hopefully will never happen, however, as the threat of it ought to suffice to keep everyone's incentives aligned. At face value, this is a terrible business decision (to willingly give up one's monopoly power), but the success of [Patreon](https://www.patreon.com) and [Substack](https://substack.com/) is evidence that relying on the goodwill of the community *can be* a genuinely viable business model, especially for an inherently creative product like a game. Will it actually work? We don't know. Let's find out!

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
- Entity Component Systems like [Bevy](https://bevy.org)
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
Design and implement a plugin to generate models for game objects. The plugin, like the rest of the game, will be a dual license of GPL-3.0 + proprietary. You will be able to license its proprietary use for your own project so long as it isn't in the fashion industry, as we are in the process of setting up a deal with a fashion company (if it goes through then we'll also be hiring for more positions).

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
