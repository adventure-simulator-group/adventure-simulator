# Architecture MVP - Adventure Simulator

Adventure Simulator is split into a strategic layer and short-lived tactical
servers.

- **Strategic layer**: `strategic-web`, an Axum + Maud SSR app that owns SQLite through SQLx.
- **Database**: SQLite, with migrations in `crates/strategic-web/migrations`.
- **Tactical layer**: `adventuresim-tactical-server`, a headless Bevy server with in-memory real-time state.
- **Browser tactical client**: connects directly to the tactical server address from the mission status page.

## Key Principle

Tactical gameplay state lives in the tactical server process. SQLite stores
persistent strategic state:

- Characters, attributes, stats, skills, limbs, equipment
- Items and inventory
- Settlements and quests
- Parties and party membership
- Mission lifecycle and idempotent result commits

## Mission Flow

1. A party leader posts `/missions/enter`.
2. `strategic-web` validates the party and active quest, inserts a `missions` row, chooses a port, and spawns `adventuresim-tactical-server`.
3. The tactical server opens its websocket listener and posts `/internal/missions/:id/ready`.
4. The mission status page renders `/tactical/tactical.html?server=ADDR&id=CHARACTER_ID&autostart=1`.
5. On client join, the tactical server fetches player loadout from `strategic-web`, marks the character entered, and spawns the player directly.
6. On disconnect, timeout, or end, the tactical server posts leave/result callbacks.
7. `strategic-web` commits results idempotently and clears mission state.

There is no strategic-web websocket proxy.

## Missions Table

`missions` collapses the old request/server split:

- `id TEXT PRIMARY KEY`
- `scene_key TEXT`
- `status TEXT` as `requested`, `starting`, `ready`, `ended`, `failed`, or `cancelled`
- nullable `party_id`, `quest_id`, `requester_character_id`
- nullable `addr`, `cert_digest`, `pid`, `success`
- `xp_gained INTEGER DEFAULT 0`
- `result_committed BOOLEAN DEFAULT false`
- timestamps

## Tactical Server Contract

The tactical server performs low-frequency HTTP calls only:

- ready: report listen address
- loadout: fetch character, inventory, attributes, stats, skills, limbs
- enter/leave: update strategic mission presence
- result: commit final success and XP once

No database work happens on the tactical gameplay tick.

## Constraints

- Keep the backend simple: Axum, Maud, SQLx, SQLite.
- Use SQLx bind parameters for database operations.
- Keep tactical process state ephemeral.
- Keep mission result commits idempotent.
- Browser clients connect directly to tactical servers.
