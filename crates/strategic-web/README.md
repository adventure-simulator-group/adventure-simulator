# Strategic Web

SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.

## Architecture

```
┌─────────────────┐     HTTP     ┌──────────────────┐
│  Browser        │◄────────────►│  strategic-web   │
│  (Datastar.js)  │   HTML/frags │  (Axum + Maud)   │
└─────────────────┘              └────────┬─────────┘
                                         │ HTTP
                                         ▼
                                ┌──────────────────┐
                                │  SpacetimeDB     │
                                │  (adventuresim-stdb-module)  │
                                └──────────────────┘
```

## Features

- **SSR (Server-Side Rendering)**: All HTML is rendered on the server using Maud templates
- **HATEOAS**: Hypermedia-driven navigation with Datastar for partial page updates
- **SpacetimeDB Integration**: Uses the HTTP API to query and call reducers
- **Parchment Theme**: Medieval illuminated manuscript-inspired design

## Running Locally

### Prerequisites

1. SpacetimeDB running locally with `adventuresim-stdb-module` module published
2. Rust toolchain

### Start SpacetimeDB

```bash
# In another terminal
spacetime start

# Publish the adventuresim-stdb-module module
spacetime publish adventuresim-stdb-module --project-path crates/adventuresim-stdb-module

# Seed the world (optional)
spacetime call adventuresim-stdb-module seed_world
```

### Run the Web Server

```bash
cargo run -p strategic-web
```

The server will start on `http://localhost:8080`.

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDRESS` | `0.0.0.0:8080` | Server bind address |
| `STATIC_DIR` | `static` | Path to strategic-web static files |
| `TACTICAL_STATIC_DIR` | `crates/adventuresim-stdb-module/static` | Path to tactical web client static files |
| `SPACETIMEDB_HOST` | `http://localhost:3000` | SpacetimeDB HTTP API URL |
| `SPACETIMEDB_DATABASE` | `adventuresim-stdb-module` | SpacetimeDB database name |
| `SPACETIMEDB_TOKEN` | (none) | Optional auth token |
| `EDGEGAP_API_URL` | `https://api.edgegap.com` | Edgegap API base URL |
| `EDGEGAP_API_TOKEN` | (none) | Edgegap API token (enables direct deployment mode) |
| `EDGEGAP_APPLICATION_NAME` | `tactical-server` | Edgegap application name |
| `EDGEGAP_VERSION_NAME` | `latest` | Edgegap application version |

## Routes

### Home
- `GET /` - Dashboard with character/party overview

### Characters
- `GET /characters` - List characters
- `GET /characters/new` - Create character form
- `POST /characters` - Create character
- `GET /characters/:id` - Character sheet
- `POST /characters/:id` - Update character

### Settlements
- `GET /settlements` - World map / settlement list
- `GET /settlements/:id` - Settlement overview
- `GET /settlements/:id/noticeboard` - Quest board
- `GET /settlements/:id/tavern` - Party recruitment
- `GET /settlements/:id/merchants` - Shop (placeholder)
- `GET /settlements/:id/smith` - Smithy (placeholder)
- `GET /settlements/:id/inn` - Rest (placeholder)
- `POST /settlements/:id/travel` - Travel to settlement

### Parties
- `GET /parties` - List parties
- `GET /parties/new` - Create party form
- `POST /parties` - Create party
- `GET /parties/:id` - Party details
- `POST /parties/:id/join` - Join party
- `POST /parties/:id/leave` - Leave party
- `POST /parties/:id/disband` - Disband party

### Quests
- `GET /quests` - List all quests
- `GET /quests/:id` - Quest details
- `POST /quests/:id/accept` - Accept quest
- `POST /quests/:id/abandon` - Abandon quest

### Missions
- `POST /missions/enter` - Enter a tactical mission (party leader only)
- `GET /missions/:id/status` - Mission status page/fragment (authorized members only)
- `POST /missions/:id/cancel` - Cancel mission (party leader or solo owner)

## Docker

Build and run with Docker:

```bash
# From workspace root
docker build -f crates/strategic-web/Dockerfile -t strategic-web .
docker run -p 8080:8080 -e SPACETIMEDB_HOST=http://host.docker.internal:3000 strategic-web
```

## Development

### Project Structure

```
strategic-web/
├── src/
│   ├── main.rs              # Axum server entry
│   ├── config.rs            # Environment config
│   ├── auth.rs              # Auth middleware (TODO)
│   ├── spacetimedb/
│   │   ├── client.rs        # HTTP client wrapper
│   │   └── types.rs         # Response types
│   ├── routes/
│   │   ├── home.rs
│   │   ├── characters.rs
│   │   ├── settlements.rs
│   │   ├── parties.rs
│   │   └── quests.rs
│   └── templates/
│       ├── layout.rs        # Base HTML layout
│       ├── components.rs    # Reusable components
│       └── *.rs             # Page templates
└── static/
    ├── css/                 # Stylesheets
    ├── borders/             # SVG ornaments
    └── textures/            # Background textures
```

### Datastar Integration

Forms use Datastar attributes for AJAX-style submissions:

```html
<form data-on-submit="@post('/characters')">
  <input name="name" required />
  <button type="submit">Create</button>
</form>
```

Server returns HTML fragments that get merged into the page.
