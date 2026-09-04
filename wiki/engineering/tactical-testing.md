# Tactical automated testing

This covers CLI-driven, headless control of the tactical server/client (the
Bevy Remote Protocol) and the pytest suite built on top of it. It complements
the interactive workflows in [Development workflow](developing.md) -
`tactical-play`, `tactical-isolated`, `tactical`/`client` - which remain the
right tool for anything a human needs to watch happen.

## Melee combat iteration

`melee-combat-iteration` runs deterministic, melee-only duels without a client,
SpacetimeDB, networking, or wall-clock pacing. The tactical half advances a
minimal real server `App` at a fixed timestep. The autoresolve half runs its
discrete-event simulation over many more seeds. Both use the canonical John
Fabelgeist build against the same five deliberately different opponents: a
shield militiaman, demi-lancer, polearm veteran, hammer brute, and knife novice.

Run the default 32 tactical and 1,000 autoresolve seeds per matchup with:

```bash
just melee-combat-iteration
```

Use smaller batches while changing a mechanic, and keep each iteration's
evidence in a separate output directory:

```bash
just melee-combat-iteration \
  output=target/melee-combat-iteration-v17 \
  tactical_seeds=8 \
  autoresolve_seeds=128
```

The root `summary.json` compares tactical and autoresolve win rates, timeouts,
throughput, and causal counts. `acceptance-audit.json` rejects broken invariants
such as lost simultaneous contacts, ghost contacts from canceled attacks,
contacts before their committed windups finish, impossible movement, energy
creation, disagreement with the shared hit equation, or a polearm retaining
full head energy when the opponent is too close for its striking head.
Each matchup directory contains every tactical and autoresolve trace as
newline-delimited JSON, representative pretty-printed traces, its aggregate
summary, and a `reviewer-packet.json`.

The windup audit compares each contact with the contact tick recorded when
that attack began. Entering reach during movement must not rewrite that
commitment. Movement traces also record the authored speed limit for each
segment. The distance check uses that limit and elapsed time, not the larger
endpoint velocity: reaching a separation boundary can stop a fighter after
they have already moved during the segment.

Tactical condition traces distinguish `blood_loss_fraction`, the cumulative
fraction of blood volume lost, from `blood_loss_incapacitation`, its derived
disabling effect. Wound events record `blood_fraction_per_second`. Compare
those rates with changes in the raw fraction when checking integration; do not
compare them with the incapacitation contribution. A production-duel regression
checks that each raw increment equals the active wound flows multiplied by the
fixed timestep.

The reviewer packet explains the physically based balance goal, the practical
zero-to-five attribute and skill scale, and each combatant's ordinary equipment
and training. It points to the traces without disclosing implementation-only
coverage percentages or outcome probabilities. Give that packet and its linked
evidence to an independent reviewer before deciding whether an implausible
outcome warrants a mechanic change. A tactical/autoresolve win-rate difference
is diagnostic evidence, not by itself proof that either side is correct.

Treat a green audit as a prerequisite for review, not as a balance verdict.
Current polearm and hammer matchups still show substantial tactical/autoresolve
disagreement. In particular, a polearm can start a swing from useful measure
but contact with its haft after the opponent closes during the windup. The
review must distinguish correct accounting for that contact from poor attack
selection; changing damage to compensate would obscure the original cause.

## Incapacitation visibility checks

The melee iteration traces report one `fatigue` fraction, matching the black
incapacitation-wheel segment. Attack, defense, and dodge work must increase
that visible value; there are no separate local-muscle or oxygen-debt trace
fields. Recovery may reduce the same value, including fatigue brought into
combat.

Regression tests check that charging an action updates fatigue and
authoritative incapacitation together, that heavier work costs more, and that
neither calorie history nor encumbrance imposes a second hidden combat-skill
penalty. Equal total incapacitation must produce equal condition-based combat
performance even when its sources differ. Autoresolve movement speed must not
change with fatigue alone.

The tactical traces include encumbrance separately. Client tests check its
translucent-grey segment, black fatigue, and yellow forecasts for every source.
Forecasts use only unfilled wheel space and never count toward incapacitation.
The server projects known wound flow immediately and smooths other source
changes over recent simulation time, not wall-clock performance. The forecast
horizon and trend response are authored under
`presentation.incapacitation_forecast` in `content/tactical/combat.yaml`.

Run the core and tactical library tests and the client binary's UI tests, then
use `just melee-combat-iteration` for seeded server/autoresolver comparisons.
Passing these checks does not establish overall combat balance.

## Bevy Remote Protocol (BRP)

Both `adventuresim-tactical-server` and `adventuresim-tactical-client` can
expose a JSON-RPC-over-HTTP endpoint for external inspection and control, gated
behind the `debug` feature and only active when `--brp-port <port>` is passed.
This fork uses `world.*` method names, not upstream Bevy's `bevy/*` - the ones
in active use here are `rpc.discover`, `world.query`, `world.get_components`,
`world.insert_resources`, `world.remove_resources`, `world.spawn_entity`, and
`world.despawn_entity`.

Enable it on a manually-run server or client:

```bash
just tactical brp_port=15702
just client-headless brp_port=15703
```

`ButtonInput<KeyCode>` is not reflect-registered, so keypresses (F7-F12) cannot
be simulated over BRP - drive real windowed/physical input for those, or send
the underlying network message directly (see `DebugDumpWorldRequest` and
similar in `adventuresim-tactical-netcode`).

### `scripts/tactical_brp.py`

A thin JSON-RPC client plus a few helpers, meant to be imported (`import
tactical_brp`) or invoked directly:

- `BrpClient(port)` - `.call(method, params)` for raw JSON-RPC, plus typed
  wrappers around the common `world.*` methods: `.query(with_=[...], ...)`,
  `.get_components(entity, [ComponentType, ...])`, `.insert_resource(instance)`,
  `.remove_resource(ResourceType)`, `.despawn(entity)`. These take/return the
  generated `BevyComponent`/`BevyResource` classes (see below), not raw
  type-path strings or dicts.
- `find_entity_with_component` / `wait_for_entity_with_component` - locate the
  first entity carrying a given component class, with the latter polling up to
  a timeout.
- `python scripts/tactical_brp.py call <port> <method> [--params '{...}']` for
  ad hoc JSON-RPC calls from a shell - this one stays string/dict-based, since
  it's a generic escape hatch for arbitrary methods, not just the typed
  component/resource wrappers.
- `just tactical-brp-smoke-test` - drives an already-running `just tactical
  brp_port=...` + `just client-headless` pair: finds the local player, moves
  it, asserts the position changed.

### Generated types: `BevyComponent`/`BevyResource`

BRP identifies types by their exact Rust `TypePath::type_path()` string (e.g.
`"adventuresim_tactical_core::player::CharacterId"`). Hand-copying these into
Python is fragile - nothing catches a type getting renamed or moved to a
different module, and `tactical_brp.py` would just silently query for a type
path that no longer resolves to anything.

`just generate-brp-types` regenerates `scripts/adventuresim_brp_lib.py`, which
`tactical_brp.py` imports (`from adventuresim_brp_lib import *`), by
discovering every BRP-queryable type - anything with `ReflectComponent` or
`ReflectResource` registry data - directly from a real `AppTypeRegistry`:

```bash
just generate-brp-types
```

This is dynamic discovery, not a hand-maintained list: `reflect_auto_register`
(a bevy feature enabled workspace-wide) auto-registers every linked-in
`#[derive(Reflect)]` type the moment any `App` exists, via `inventory` -
`crates/adventuresim-tactical-brp-generator/src/main.rs` just builds a bare
`App::new()` (no plugins added, nothing run) and reads the registry back out.

Two base classes, `BevyComponent` and `BevyResource`, are always emitted
first. Every discovered Component/Resource type becomes its own subclass of
one of them - `CharacterId(BevyComponent)`, `PlayerInputOverride(BevyResource)`,
and so on - carrying `type_path: ClassVar[str]` plus `to_brp()`/`from_brp()`
(see below). `BrpClient`'s methods (`.query()`, `.get_components()`,
`.insert_resource()`, `.remove_resource()`, `find_entity_with_component`, ...)
all take and return these classes, not raw type-path strings - see the
`### scripts/tactical_brp.py` section above. There is no separate flat
`NAME_TYPE = "..."` string constant for anything - `ClassName.type_path` is
the *only* way to get at a type path, deliberately, so nothing in this
project references BRP types by bare string. Classes are grouped into
`# Components`/`# Resources`/`# Nested/helper types` sections (a type
registering as both a Component and a Resource, which doesn't currently
happen, would land in `# Components`). If two types share a short name, the
class name gets a `1`/`2`/... disambiguating suffix.

For the generator binary to see `adventuresim-tactical-server`'s and
`-client`'s otherwise-private types (e.g. `bot::MissionEnemy`,
`player::ClientPlayer` - both `mod`, not `pub mod`, in their respective
`main.rs`), each crate carries a `[lib]` target (`src/lib.rs`) that re-declares
just enough of its module tree as `pub`. That target is gated behind a
`remote-types` feature, off by default and not implied by `debug` - a normal
`cargo build`/`check` of either package (which always builds every target,
lib included) does not pay to compile that module tree a second time; only
`--features remote-types` does.

Regenerate after adding, renaming, or moving a `#[reflect(Component)]`/
`#[reflect(Resource)]` type. This has already caught a real gap: `EquipmentTopology`
(`adventuresim-tactical-core/src/inventory.rs`) derives `Reflect` but is
missing `#[reflect(Component)]`, so despite looking reflect-registered it is
not actually BRP-queryable - a hand-typed constant for it would have looked
correct and silently returned nothing at query time.

### `to_brp()`/`from_brp()`

Every generated class - a `BevyComponent`/`BevyResource` subclass, or one of
the plain nested helper dataclasses reachable through their fields (e.g.
`PlayerInputRequest` is a field of `PlayerInputOverride`) - has a `to_brp()`
method producing the exact value BRP expects on the wire, and a `from_brp()`
classmethod doing the reverse (used by `get_components()`'s return value).
This is what "autocompletion/type-verification for tactical tests" means in
practice:

```python
tactical_client.insert_resource(
    tactical_brp.PlayerInputOverride(
        value=tactical_brp.PlayerInputRequest(movement=[0.0, 1.0], look=[0.0, 0.0], jump=False, weapon_guard="Lowered")
    )
)
transform = tactical_client.get_components(entity, [tactical_brp.Transform])[tactical_brp.Transform]
print(transform.translation)
```

`weapon_guard` above is typed `Literal["Lowered", "Raised"]`, not `str` - a
type checker (or an IDE's inline diagnostics) flags a typo'd variant name
before the test ever runs.

Resolution (`crates/adventuresim-tactical-brp-generator/src/resolve.rs`) walks
each field's reflected `TypeInfo` recursively, but the reflected *shape*
of a type is not always its wire *encoding* - bevy's reflect serializer
special-cases some shapes, and the resolver has to know about each one rather
than assume the naive translation:

- A single-field tuple struct (`CharacterId(u64)`) is transparent on the wire
  - its value *is* the inner value, not a one-element array. As a *field* of
    some other type this inlines directly (no wrapper class at all); as a
    top-level Component/Resource it still gets a class (`CharacterId`, with a
    single `value: int` field) purely so the BrpClient API has a uniform
    class to work with, but `to_brp()` returns the bare `int`, not `{"value":
    ...}`.
- `Option<T>` is `None` or a bare `T`, not `{"Some": ...}`.
- A small set of `glam` types
  (`Vec2`/`Vec3`/`Vec3A`/`Vec4`/`Quat`/`IVec*`/
  `UVec*`) have a custom `Serialize` impl the reflected shape can't see at
  all - hardcoded to their known `list[float]`/`list[int]` wire shape rather
  than resolved generically.
- A unit-only enum (all variants carry no data, e.g. `EquipSlot`) becomes a
  `Literal[...]` of its variant names, matching the bare-string wire value.

Anything else the resolver doesn't confidently know how to encode - a
data-carrying enum (`ArmorItem.slot: ArmorSlot`, whose `Arms`/`Legs` variants
carry a nested `Option<ArmorSide>`), a `Map`, a `Set` - resolves to `Any` with
an `# unresolved: <type path>` comment rather than a guess. A generated
constructor that *looks* right but silently sends BRP a malformed payload
would be worse than no constructor at all, so an unresolved field is left
visibly unresolved: pass a raw dict/value shaped to match that type's own BRP
encoding for those.

## End-to-end pytest suite

`scripts/tests/tactical/` spawns real subprocesses - an isolated SpacetimeDB
instance, the tactical server, one or more headless tactical clients - and
drives them over BRP. This is meaningfully slower than the rest of
`scripts/tests/` (real compile/bootstrap cost: single-digit minutes on a warm
`target/` cache, much longer cold), so it is not part of the default `test`
recipe:

```bash
just tactical-test            # every test in scripts/tests/tactical
just tactical-test movement   # pytest -k filter on name substring
```

Layout:

- **`lib.py`** - process-spawning primitives shared by everything else:
  `SpawnedProcess` (background `just` recipe or direct `cargo run`, killed by
  process group on teardown), `spawn()`, `wait_for()` (polls a readiness
  check, fails fast if the process exits early, includes the log tail on
  timeout), `report_phase()`, `read_env_file()`, plus `REPO_ROOT`/`ENV_FILE`
  and the shared timeouts.
- **`conftest.py`** - the session-scoped fixtures most tests build on:
  `tactical_mission` (`just tactical-isolated`, default profile/ports),
  `tactical_server` (`just tactical`, depends on `tactical_mission`),
  `tactical_client` (`just client-headless`, depends on `tactical_server`,
  waits for a `ClientPlayer` entity to appear - i.e. the client has joined).
  Also a `pytest_collection_modifyitems` hook forcing `test_lifecycle.py` to
  collect last: it ties up the machine for a minute-plus building/tearing down
  its own isolated instance, and running that in between the shared fixtures'
  setup and `test_movement.py`'s tight movement-detection window left the
  idling shared client too starved to pass.
- **`test_connectivity.py`** - BRP reachability and player-entity-exists
  smoke tests, using the shared fixtures.
- **`test_movement.py`** - drives `PlayerInputOverride` and asserts the local
  player's `Transform` actually moves.
- **`test_lifecycle.py`** - self-contained (its own profile, ports, and
  `.env.tactical` write, not the shared fixtures), covering the
  join/despawn/disconnect character-count lifecycle. Runs with
  `--enemy-combat-scale-bps 0` so bots stay inert instead of ending the
  mission mid-test. See its own comments for why it can't use the `tactical`
  recipe for its server launch.
- **`test_combat_defense.py`** - self-contained combat tests exercising the
  bot parry/dodge mechanic end to end: standalone `--world-dump` server (no
  SpacetimeDB), a real headless client, a real melee attack (via
  `bevy_enhanced_input`'s `ActionMock`, not a debug bypass), and BRP mutation
  of the bot's `DefenseChances` to force deterministic dodge/parry outcomes.
  Uses a hand-calibrated world-dump fixture (`fixtures/combat_scenario.scn.ron`)
  so the bot's skills/attributes actually allow it to dodge. See its own
  comments for the more fiddly details (facing setup, outcome detection,
  port isolation between scenarios).

`.env.tactical` (written by `tactical-isolated`/`reseed-tactical-mission`) is
deleted on orderly shutdown - now including `SIGTERM`, not just `Ctrl+C`/
`SIGINT` (see `scripts/dev_stack.py::main`), so a `just`-recipe or pytest
fixture teardown cleans it up the same way an interactive `Ctrl+C` always did.
