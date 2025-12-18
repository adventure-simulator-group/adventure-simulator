# Adventure Simulator Demo (RPG loop)

This is a small playable Bevy demo that talks to the Postgres-backed strategic layer over HTTP:

- Press `WASD` to move.
- Press `E` near the quest giver cube to accept a quest.
- Press `E` near the cat sphere to complete the quest (grants XP + items atomically).
- A red hazard bot chases you and can kill you; death drops your inventory into a loot bag in Postgres.
- After respawning, press `E` near the loot bag marker to claim it back (transactional move back into inventory).

## Run

Start the strategic server first:

`DATABASE_URL=postgres://postgres:postgres@localhost:5432/strategic cargo run -p strategic-server`

Then run the demo:

`cargo run -p adventure-simulator-demo`

Optionally open the Datastar overlay:

`http://127.0.0.1:8080/overlay/`

