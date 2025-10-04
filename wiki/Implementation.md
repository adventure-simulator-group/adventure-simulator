# Organization
## Strategic Layer
The "strategic layer" is the layer of the game which essentially exists at the map- level. It is asynchronous and facilitated by a HATEOAS HTML interface (generated via [maud](https://maud.lambda.xyz/)). It is essentially akin to the [map/travel](Travel) screens of Mount and Blade, Battle Brothers, or Starsector, where the party fast-travels from location to location and interacts with services in [settlements](Settlement) via menus.

It is in this layer that you create [characters](Characters), embark on [quests](Quests), recover [health](Health), and manage [inventory](Inventory). [Time](Time) does not advance continuously for each client, it advances in discrete chunks whenever the party leader interacts with the server. For the MVP, there is not actually a visible map client-side, quests just tell you how long the server calculates it will take to get there and what hazards you will/might encounter along the way. Later we can implement a webgpu-rendered map layer and float HTML elements on top of it like Google Maps.

For the MVP, the strategic layer is a single global server. To scale things later on, it can be subdivided per-settlement or settlements can be dynamically grouped into regions that attempt to maintain a minimum and maximum number of parties (if a region has too many parties, it can split, too few and it merges with nearby regions).

>Halbe: *My* assumption is that the underlying database for this is Postgres and the server is Rocket, but I am a full-stack web developer, and to a hammer every problem is a nail. I can vaguely conceive of an unusual configuration Bevy with an HTTP plugin that is essentially running on an event-based schedule which could sort of be made to act like Rocket+SQL, but I have no idea if this would be a good idea. It would presumably only be something that we'd do for consistency with the tactical layer.
## Tactical Layer
When the party enters [combat](Combat), approaches enemies in [stealth](Stealth), or must navigate dangerous [terrain](Terrain), the strategic layer composes a scene and passes it to the WASM binary of the "tactical layer". This is a real-time simulation in Bevy implemented by a generalist game programmer. Since UDP is not available on the web, the [networking](Networking) must be facilitated by websockets.

For at least the MVP, the simulation tactical layer is (tentatively) entirely done through the server, latency be damned. What we like about this is that it places an upper bound on how effective a cheater can be and it cleanly separates what clients are responsible for and what the server is responsible for. Clients handle input and rendering, server handles all the game logic. The felt-latency is mitigated by the fact that the animation system can eagerly act on input and that combat is mostly centered around weapons that attack more slowly than server latency. Only crossbows and firearms will necessarily have felt-latency to your inputs.

There *is* physics running on the tactical layer, but characters are just capsules since nothing about their animation state or model can be assumed on this layer.

>Halbe: If the developer implementing this feels that rollback with more advanced physics is hardly more difficult to implement, or even easier, then they can make their case for it. I am skeptical though.

Once it concludes, the outcome of the tactical simulation is passed back to the strategic layer.

## Client Layer
This exclusively exists on the client. The current state of the tactical layer is constantly received from the tactical layer server. The client layer has only two responsibilities, rendering the state of the tactical layer to the client, and passing input back to the server. Since combat is *not* dependent on animations, it is free to adjust the characters' animation state to immediately respond to inputs, and has a window of flexibility with things like the exact position of any character.

For example, when a character is standing still and the user gives a forward movement input, the *model* of their character can immediately begin moving forward, though slowly enough to start that after about a second their server-authoritative position will sync up with their actual position and their speed will increase to whatever it actually is.

Likewise, when the user starts an attack, their character can begin a wind-up animation while it waits until the server-authoritative time that their attack actually begins. If its an attack against another player, that player will receive the packet indicating that they are being attacked *after* the server-authoritative time, and will render a faster attack animation to catch up such that from the perspective of both clients and the server, the attack actually lands at the same time. When an attack begins, the server is saying "Character A will attack character B at time T".

There is only one way that the client-layer can affect what happens on the tactical layer: input. But input *can* be based on client-authoritative animation state in one narrow context: deciding the [precision](Controls) of an attack and what part of the enemy's body the player is trying to attack. Cheaters can report always having perfectly precise attacks with instant dodge/parry reaction times, which is fine, because they're still constrained by character [stats](Stats).

Though not necessarily in the MVP, secondary physics can be implemented at this layer for things like animation physics or particle effects. Basically they have no effect on the tactical layer.

## Model Layer
The actual [models](Models) used for all client-rendered assets are procedurally generated client-side. They will not look very good, at least for the MVP, but essentially there is a plugin that takes all relevant information about a character or object and generates a model for it. Though it is *on* the client, it can be thought of as a different layer since the developer implementing the client layer can know nothing about this other than adding the model-generating plugin to the client app.

## Shared Library
There are two types of things which need to be shared between layers, components and functions.
### Components
#### Shape
* Tentatively, this is just avian3d's collider component
* The client layer needs this for secondary physics and input raycasting
* The model layer needs this for generating a mesh
#### Strata3D
```
struct Strata3D {
  map: [[SmallVec<Layer, 4>; 32]; 32]
  materials: SmallVec<Material, 4>
}
struct Layer { material_index: u4, depth: UnitScalar #0.0..1.0 }
enum Material {
  Skin { melanin: f32, carotin: f32, ???},
  Fat,
  Muscle,
  Bone,
  Steel,
  Paint { rgba: Rgba },
  Cloth,
  ...
}

impl Material {
  pub fn albedo(&self) -> f32,
  pub fn roughness(&self) -> f32,
  pub fn restitution(&self) -> f32,
  pub fn density(&self) -> f32,
  # ... other functions relevant for either graphics or physics
}
```


This tells you everything that you need to know about an entity's physical properties, given a shape and the UV map for its Strata3D. Its not *quite* the same as a regular UV map though, because it has to be able to also convert a "depth" to a point in 3D. The total depth of all layers is essentially a heightmap for the surface of a mesh, and generally only this and the outermost layer are relevant to the [model generator](Models). All of the layers are needed to compute physics properties, most of which are used at the client-layer for secondary physics, but the sum total of all mass from the Strata3D for all entities associated with a character can be relevant at the tactical-layer for [combat](Combat) equations or the strategic layer for calculating [fatigue](Energy) from [traveling](Travel) with [encumbrance](Encumbrance).

For the MVP, the function which generates the Strata3D for a character and their equipment is likely the responsibility of the programmer implementing the model layer since the information is most directly relevant to them. For the MVP, this all sounds fantastically complicated, so Strata3D can have a quick n' dirty temporary version that is just a wrapper around a single Material, forget the 2D map and layering, and combine a bunch of materials like Skin/Muscle/Fat/Bone into "Flesh" or different rock types into "Rock". The characters would basically look like 3D stick figures and the only weapons will be clubs and maces, but its fine for now.

The *eventual* purpose of Strata3D being so fantastically detailed is not only to be able to generate all of our models from it and have very accurate physics calculations, but it can also represent things like how the clothes and armor on a character (added on as layers) affect damage. An outer layer of steel followed by some padding would distribute force very effectively. Or a *gap* in the steel layer (for the slit of a visor) translates to a weak point that a character could attack with sufficiently high accuracy, with the animation system using inverse kinematics to place their weapon in the exact weakpoint. This enables the simulation at the tactical level to become extraordinarily detailed in the future after the bare minimum is implemented for the MVP.

In the MVP, there are only two shapes which a Strata3D is expected to support: the [pseudo-capsule](Models) and a flat terrain mesh. A shape which can support Strata3D is one which unambiguously maps a U, V, and depth value to an X, Y, and Z in an entity's local Transform space. There can be no UVDs which map to multiple XYZs or vice-versa.
>Adler: Bruno, what do you call this? Some geometric term that includes the word "transformation" I assume, like "functional transformation".

The following shapes should be able to facilitate this:
1. Cylinder (U: longitude, V: length, D: radius)
2. Capsule (U: longitude, V: length + top and bottom latitude, D: radius)
3. Sphere (U: longitude, V: latitude, D: radius)
4. Box (one face is chosen as the surface)
5. Blade
	1. May need two separate shapes for single-edge and double-edge
	2. There are a couple ways to map it, but it needs to support both pointed and flat heads
### Functions
Some gameplay-related functions are shared between the strategic layer and the tactical layer. Generally, the tactical layer simulates combat so the programmer implementing it has the authority on how they work, but the strategic layer should be able to *use* these functions. For example to calculate how much reward a quest should give based on how powerful of a party it calculates would be needed to complete it. Or whether an enemy party should attempt to flee.

The developer of the tactical layer decides things like what attributes there are and gives some idea of what would constitute a "high" or "low" value, but the developer of the strategic layer is the one actually responsible for deciding what attributes a given character actually has. 

# Team
In general, 
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
Will be implementing a plugin for the [model](Models) layer, to be used in the client layer. Should have experience with procedural modeling algorithms, specifically advancing front, and comfortable with geometry and linear algebra.