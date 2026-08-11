# Game layers

Fabelgeist alternates between an asynchronous strategic game and a
real-time tactical game. The split exists to make realistic travel, recovery,
trade, and social play practical without pretending the whole world is one
continuous simulation.

## Strategic layer

The strategic layer is the persistent world. Players create and manage
[characters](../strategic/character.md), organize parties, travel between
[settlements](../strategic/settlement.md), investigate [quests](../strategic/quests.md),
trade, rest, train, and manage [inventory](../shared/inventory.md).

Time advances in explicit chunks when a party travels, rests, trains, works, or
performs another strategic action. The interface is a server-rendered hypertext
application rather than a freely traversable 3D overworld.

## Tactical layer

Combat and other immediate physical challenges use a private real-time Bevy
simulation for the participating party. The server owns gameplay state and the
client owns input and presentation. Positions, enemies, damage exchanges, and
other live simulation details disappear with the tactical session; only its
validated strategic consequences survive.

Quest combat and strategic incidents currently use this handoff. Tactical
stealth and hazardous-terrain scenes remain future extensions.

## Client and model layers

The tactical client renders replicated state, sends input, and may begin
animations immediately to disguise network latency. Client animation never
changes the authoritative outcome.

Procedural model generation is a separate concern. Gameplay code should depend
on stable physical and presentation inputs rather than a particular finished
mesh or animation.

## Shared rules

Strategic autoresolve, tactical play, content validation, and simulation tools
share dependency-light Rust calculations for attributes, skills, equipment,
combat, health, and other durable mechanics. Sharing a calculation does not
move authority between layers.

For the current technical boundaries, persistence rules, transport, and trust
model, see [Architecture](architecture.md). For deployment topology
and networking design, see [Networking](networking.md).
