# Organization
## Strategic Layer
The "strategic layer" is the layer of the game which essentially exists at the map- level. It is asynchronous and facilitated by a [HATEOAS](https://en.wikipedia.org/wiki/HATEOAS) HTML interface (generated via [maud](https://maud.lambda.xyz/)). It is essentially akin to the [map/travel](Travel.md) screens of Mount and Blade, Battle Brothers, or Starsector, where the party fast-travels from location to location and interacts with services in [settlements](Settlement.md) via menus.

It is in this layer that you create [characters](Characters), embark on [quests](Quests.md), recover [health](Health.md), and manage [inventory](Inventory.md). [Time](Time.md) does not advance continuously for each client, it advances in discrete chunks whenever the party leader interacts with the server. For the MVP, there is not actually a visible map client-side, quests just tell you how long the server calculates it will take to get there and what hazards you will/might encounter along the way. Later we can implement a webgpu-rendered map layer and float HTML elements on top of it like Google Maps.

Further details about the implementation are described in the [networking](Networking) page

>Halbe: *My* assumption is that the underlying database for this is Postgres and the server is Rocket, but I am a web developer, and to a hammer every problem is a nail. I can vaguely conceive of an unusual configuration Bevy with an HTTP plugin that is essentially running on an event-based schedule which could sort of be made to act like Rocket+SQL, but I have no idea if this would be a good idea. It would presumably only be something that we'd do for consistency with the tactical layer.
## Tactical Layer
When the party enters [combat](Combat.md), approaches enemies in [stealth](Stealth.md), or must navigate dangerous [terrain](Terrain), the strategic layer composes a scene and passes it to the WASM binary of the "tactical layer". This is a real-time simulation in Bevy implemented by a generalist game programmer. Since UDP is not available on the web, the [networking](Networking) must be facilitated by websockets or data-star.

For at least the MVP, the simulation tactical layer is (tentatively) entirely done through the server, latency be damned. What we like about this is that it places an upper bound on how effective a cheater can be and it cleanly separates what clients are responsible for and what the server is responsible for. Clients handle input and rendering, server handles all the game logic. The felt-latency is mitigated by the fact that the animation system can eagerly act on input and that combat is mostly centered around weapons that attack more slowly than server latency. Only crossbows and firearms will necessarily have felt-latency to your inputs.

There *is* physics running on the tactical layer, but characters are just capsules since nothing about their animation state or model can be assumed on this layer.

>Halbe: If the developer implementing this feels that rollback with more advanced physics is hardly more difficult to implement, or even easier, then they can make their case for it. I am skeptical though.

Once it concludes, the outcome of the tactical simulation is passed back to the strategic layer.

## Client Layer
This exclusively exists on the client. The current state of the tactical layer is constantly received from the tactical layer server. The client layer has only two responsibilities, rendering the state of the tactical layer to the client, and passing input back to the server. Since combat is *not* dependent on animations, it is free to adjust the characters' animation state to immediately respond to inputs, and has a window of flexibility with things like the exact position of any character.

For example, when a character is standing still and the user gives a forward movement input, the *model* of their character can immediately begin moving forward, though slowly enough to start that after about a second their server-authoritative position will sync up with their actual position and their speed will increase to whatever it actually is.

Likewise, when the user starts an attack, their character can begin a wind-up animation while it waits until the server-authoritative time that their attack actually begins. If its an attack against another player, that player will receive the packet indicating that they are being attacked *after* the server-authoritative time, and will render a faster attack animation to catch up such that from the perspective of both clients and the server, the attack actually lands at the same time. When an attack begins, the server is saying "Character A will attack character B at time T".

There is only one way that the client-layer can affect what happens on the tactical layer: input. But input *can* be based on client-authoritative animation state in one narrow context: deciding the [precision](Controls.md) of an attack and what part of the enemy's body the player is trying to attack. Cheaters can report always having perfectly precise attacks with instant dodge/parry reaction times, which is fine, because they're still constrained by character [stats](Stats.md).

Though not necessarily in the MVP, secondary physics can be implemented at this layer for things like animation physics or particle effects. Basically they have no effect on the tactical layer.

## Model Layer
The actual [models](Models.md) used for all client-rendered assets are procedurally generated client-side. They will not look very good, at least for the MVP, but essentially there is a plugin that takes all relevant information about a character or object and generates a model for it.

Though it is *on* the client, it can be thought of as a different layer since the developer implementing the client layer can know nothing about this other than adding the model-generating plugin to the client app.
## Shared Library
There are two types of things which need to be shared between layers, components and functions.
### Components
#### Shape
* For the MVP this is just Avian3d's collider component.
* Not all collider types would be supported for the MVP though
* Later on this will be a more fundamental component that allows for more complex shapes from which the Avian3d collider is generated.
#### [StrataMap](shared/StrataMap)
* Describes everything you need to know about the material composition of an entity
* When combined with Shape (and Transform for scale), contains all of the information needed to compute all physics properties *and* generate the model.
* Simple placeholder version for MVP presumes that each entity only is made of one material
### Functions
Some gameplay-related functions are shared between the strategic layer and the tactical layer. Generally, the tactical layer simulates combat so the programmer implementing it has the authority on how they work, but the strategic layer should be able to *use* these functions. For example to calculate how much reward a quest should give based on how powerful of a party it calculates would be needed to complete it. Or whether an enemy party should attempt to flee.

The developer of the tactical layer decides things like what attributes there are and gives some idea of what would constitute a "high" or "low" value, but the developer of the strategic layer is the one actually responsible for deciding what attributes a given character actually has. 

# Team
## Adler Halbe (Director)
Can help greatly with the strategic layer (mainly frontend) and somewhat with the client-layer (mainly animation). Can also make placeholder assets in Blender and once an editor for the procedural mesher is created, can start making procedural assets.

Something of a jack-of-all-trades, so should be able to at least point you in the right direction when you're stuck, but also is only working part time due to having a day job (which is where your salary comes from).

Has been responsible for all of the design and documentation so far, but is not especially committed to the exact details of anything. Adler will only block changes to the design if he thinks it conflicts with the meta directives.
## Bruno Segovia (CEO)
Will handle admin and also be writing a procedural audio plugin. The goal is that instead of recording tons of sound effects, we generate them by writing them as functions with parameters. So for example, the length, diameter, and amount of gunpowder are parameters in a function that produces the sound of a gunshot. In the case of collisions the plugin can play most audio automatically (given that each colliding object have most or all of the info needed for this in its physics components like collider shape or restitution). Until Bruno succeeds with this, stock effects (or silence) are sufficient, therefore its not a blocker.
## Strategic-Layer Full-stack Developer
Implements the strategic-layer, needs some experience with HTMX and/or Alpine AJAX as well as whichever backend database the strategic layer uses.
## Tactical+Client-Layer Game Programmer
Implements all of the tactical layer and a simple version of the client-layer (input, not necessarily animation).
## Graphics programmer
Will be implementing a plugin for the [model](Models.md) layer, to be used in the client layer. Should have experience with procedural modeling algorithms, specifically advancing front, and comfortable with geometry and linear algebra.