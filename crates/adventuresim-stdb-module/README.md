# adventuresim-stdb-module

The SpacetimeDB module owns Adventure Simulator's persistent strategic state and
authoritative strategic mutations.

## Persistence boundary

The module stores durable facts such as characters, parties, progression,
schedules, injuries, needs, inventory, equipment, currency, journeys,
organizations, cases, investigations, contracts, mission authority, final
battle results, finalized loot, and compiled world records.

It does not store tactical positions, enemies, physics, attacks, temporary
health, per-tick damage, or other live combat state. Those values exist only in
the headless tactical server.

See the repository [architecture reference](../../wiki/reference/architecture.md)
for the complete boundary.

## Content and authority

Generated cases retain private catalog revision, deterministic context,
canonical manifest, and replay information. Materialization creates the linked
local problem, investigation graph, objectives, custody, sites, hostiles, and
finales atomically. See
[Quest generation and investigation](../../wiki/reference/quest-generation-and-investigation.md)
and [Quest authority](../../wiki/reference/quest-authority.md).

Player-facing callers act through `strategic-web`, which owns the registered
strategic-gateway identity. Browsers do not connect to this module directly.
The current local gateway does not yet provide a complete
player-identity-to-character ownership model.

## Tactical completion

Mission requests bind a party, scene, one-use tactical-server claim, and private
strategic mission authority. The registered tactical child keeps live
simulation state in memory and calls `end_tactical_server` with its terminal
resolution.

The module then validates that server and mission, selects only a compatible
private strategic outcome, and commits durable consequences idempotently.
Tactical code does not choose a case objective, capture subject, contract
state, loot entitlement, or reward.

## Development

Use the repository-pinned SpacetimeDB CLI and root `justfile` recipes:

```bash
just build-strategic
just publish
just web
```

General canonical reset recipes intentionally refuse to delete data. The
world-specific `just load-world` command is the explicit local exception: it
reset-publishes the selected loopback `adventuresim-*` database and discards
all of its existing data before importing the pinned world. For other
disposable schema work, use an explicitly isolated profile:

```bash
just web-isolated-strategic module-dev 23100
```

The full local workflow is documented in
[Development workflow](../../wiki/reference/developing.md).
