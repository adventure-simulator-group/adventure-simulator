# Networking

Adventure Simulator deliberately avoids a continuously simulated MMO
overworld. Most players interact with persistent places and people through
ordinary strategic web pages, while a temporary private server is created only
for real-time play.

## Strategic experience

Being “in” a settlement means that the character's persistent strategic
location is that settlement. Many players can read and act there without
sharing a real-time physics simulation.

Strategic actions use ordinary HTTP requests. Server-sent events refresh
relevant page regions when persistent state changes, but the complete page
remains usable through normal links and forms. Browsers never connect directly
to SpacetimeDB or receive its credentials.

This model should scale by partitioning or replicating strategic services
without changing the player-facing metaphor. A crowded city is primarily a
busy shared bulletin board, not ten thousand networked bodies in one scene.

## Tactical experience

A tactical server is provisioned for an immediate physical encounter. It is
authoritative: clients send input and receive replicated state, with no
peer-to-peer authority.

The current client and server communicate over WebSockets. Local animation may
respond eagerly to input, but it cannot decide hits, movement, damage, enemy
state, or mission completion. When the session ends, clients return to the
strategic interface and only validated durable consequences are committed.

There is no PvP in the MVP. Future PvP should preserve the same priorities:

- a server-authoritative outcome;
- bounded advantage from dishonest client timing or precision;
- no requirement to persist tactical ticks in the strategic database;
- animation techniques that hide latency without granting authority.

## Accounts and authorization

The desired product should let a new player begin with minimal account setup
and attach recovery or identity credentials later. That product design is not
the current authorization model.

The current local strategic surface uses a selected-character cookie and a
trusted gateway identity. It must not be treated as a public multi-user account
system until explicit player-to-character ownership is implemented.

See [Architecture](architecture.md) for the current transport,
credential, subscription, and tactical lifecycle boundaries.
