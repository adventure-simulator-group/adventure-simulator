# Strategic Web

SSR web UI and strategic backend for Adventure Simulator.

## Architecture

```
Browser
  | HTML forms/pages
  v
strategic-web (Axum + Maud)
  | SQLx
  v
SQLite

strategic-web --spawns--> adventuresim-tactical-server
tactical-server --HTTP callbacks--> strategic-web /internal/...
Browser tactical client --direct websocket--> tactical-server
```

The browser tactical flow is direct-connect. Mission status links to:

```text
/tactical/tactical.html?server=ADDR&id=CHARACTER_ID&autostart=1
```

There is no strategic-web WebSocket proxy.

## Running Locally

```bash
cargo build -p adventuresim-tactical-server
cargo run -p strategic-web
```

By default, the server listens on `127.0.0.1:8080`, opens `sqlite://adventuresim.db`,
runs migrations, enables WAL and busy timeout, and seeds the default world.

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `DATABASE_URL` | `sqlite://adventuresim.db` | SQLite database URL |
| `BIND_ADDRESS` | `127.0.0.1:8080` | HTTP bind address |
| `STATIC_DIR` | `crates/strategic-web/static` | Strategic static files |
| `TACTICAL_STATIC_DIR` | `crates/strategic-web/static/tactical` | Browser tactical shell and WASM output |
| `TACTICAL_SERVER_BIN` | `target/debug/adventuresim-tactical-server` | Tactical server executable |
| `STRATEGIC_INTERNAL_URL` | `http://127.0.0.1:8080` | Callback URL passed to tactical servers |

## Routes

- `GET /` - Dashboard
- `GET /characters`, `GET /characters/new`, `POST /characters`, `GET/POST /characters/:id`
- `GET /settlements`, `GET /settlements/:id`, settlement service pages, `POST /settlements/:id/travel`
- `GET /parties`, `GET /parties/new`, `POST /parties`, party join/leave/disband routes
- `GET /quests`, `GET /quests/:id`, quest accept/abandon routes
- `POST /missions/enter` - create mission row and spawn tactical server
- `GET /missions/:id/status` - mission status page or fragment
- `POST /missions/:id/cancel` - cancel a pending/starting/ready mission

Internal tactical callbacks:

- `POST /internal/missions/:id/ready`
- `GET /internal/missions/:id/players/:character_id/loadout`
- `POST /internal/missions/:id/players/:character_id/enter`
- `POST /internal/missions/:id/players/:character_id/leave`
- `POST /internal/missions/:id/result`

## Source Layout

```text
src/
  db.rs          SQLite pool, PRAGMAs, migrations
  models.rs      SQLx rows and tactical callback DTOs
  services.rs    Strategic rules formerly implemented as reducers
  routes/        Axum route handlers
  templates/     Maud templates
static/
  css/
  borders/
  textures/
  tactical/
migrations/
```
