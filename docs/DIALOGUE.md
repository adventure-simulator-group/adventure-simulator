# Dialogue architecture

## Persistent settlement actors

Settlement dialogue is authorized against persistent `settlement_npc` identities and
their authoritative strategic `settlement_npc_presence`, rather than a client-created
`<settlement>:<service>` name. A location may contain several NPCs; changing the
addressed portrait changes the actor while the character remains at that location.
Service providers retain their service conversation, while ordinary residents use the
compiled `local-resident` conversation and cannot receive service-only topics.

The public NPC row contains only visible identity and presentation: name, age band, sex,
height, build, hair/facial hair, complexion, visible features, clothing, profession,
household, and local role. Population seed explanations and relation weights are a
private diagnostic table. Dialogue facts include typed age, profession, status,
clothing presence, prior interaction, language compatibility, observable location role,
and time period. Hidden causal circumstances remain private until a future discovery
system deliberately reveals them. Greeting response priority remains deterministic.

Population choices use contextual weighted relations. Zero plausibility is a hard
exclusion; low positive values remain rare. Curation weight stays separate from world
plausibility, and unusual demographic/location combinations require a causal bridge.
One relation owns each conditional weight; inverse tables are not duplicated. Production
population creation calls the canonical typed evaluator in `adventuresim-core`, and its
private serialized explanation records the input context and every selected relation,
factor, decision, and required bridge.

Scripted dialogue is a compiled, server-authoritative strategic system. It is
separate from free-form local chat. Authors edit the JSON-compatible subset of
YAML in `content/dialogue/*.yaml`; builds validate and embed a deterministic
catalog, its SHA-256 revision, and compiler-derived source locations. Runtime
servers do not read loose content files.

## Authoring model

Each conversation has a stable ID and named participant roles. A role declares
`player` or `npc` plus minimum/maximum cardinality, so one authored exchange can
require a shopkeeper and assistant or address several players. Optional
`on_start` responses use the same conditions, priority rules, attributed turns,
effects, and automatic source mapping as topic responses. The server evaluates
one start response exactly once when it creates a session; use it for greetings
instead of making the browser select a topic implicitly. Topics have
stable IDs, labels, knowledge/eligibility conditions, and explicitly prioritized
responses. A response contains attributed turns composed of text and inline
topic fragments. Prompts support `yes_no`, `single`, and `multi` choices and
`first_response`, `unanimous`, `majority`, or `all_respondents` resolution.
Choices may contain `result_turns`; these are appended to the durable transcript
only after the prompt resolves and its effects succeed.

Conditions are a typed tree: `always`, `all`, `any`, `not`, and `fact`. Fact keys
are allowlisted in `FactKey`; participant profession, familiarity, clothing,
service role, location, time period, quest state, and flags are supported. New
world facts require a Rust resolver change. Never put executable code, SQL, or
client-trusted effects in content. Effects are likewise a closed enum. A client
sends catalog revision and stable topic/choice IDs; the authoritative reducer
resolves turns and effects from the embedded catalog.

Run `just dialogue-check` before review. Use
`cargo run -p adventuresim-dialogue --bin dialogue-check -- explain <id>` to
inspect response priorities. Equal highest priorities at runtime are rejected
as ambiguous instead of depending on file order.

## Persistence and multiplayer

SpacetimeDB stores dialogue sessions, named participants, attributed events,
open prompts, idempotent action receipts, per-character answers, and
per-character topic knowledge. Answer and knowledge rows are private; the web
gateway exposes only a participant-authorized conversation view and never sends
the authored condition/effect catalog to browsers. Reducers verify gateway
authority, membership, shared settlement, role cardinality, catalog and session
revisions, topic eligibility, and stable choice IDs. Every mutation revalidates the
character's settlement and the persistent NPC's exact session location and current
schedule. Selecting an NPC creates a fresh encounter so contextual and prior-interaction
facts are reevaluated; old sessions remain history rather than an indefinitely reusable
active view. Free-form `local_chat_message` remains an independent stream.

The web conversation surface keeps known eligible topics in a pane on the
right side of the chat itself. The transcript and shared composer occupy the
left side. Clicking an inline or listed topic remains supported; keyboard users
can instead type a topic label and press Enter. A unique label prefix appears
as grey inline completion and Tab accepts it. While a prompt is open, the same
composer matches its choices instead of topics. Multi-select answers use
comma-separated choice labels. Text that does not exactly match the active
dialogue topics or choices continues through the independent free-form chat
stream.

## Developer mode and source editing

The hammer button immediately left of the character portrait toggles developer
mode. It is off by default and persisted locally in the browser. Its only
initial consumer is dialogue: authored NPC lines receive keyboard-accessible
GitHub editor links in developer mode. Repository and ref are centralized by
the server (`ADVENTURESIM_SOURCE_REF`, default `main`); source paths and spans
come from compilation, so writers never maintain line numbers. Unsupported or
unsafe paths do not produce links. Extend developer mode only by querying the
root `data-developer-mode` attribute; do not create independent toggles.

Schema changes are pre-launch and intentionally have no migration or legacy
compatibility path. Recreate/reseed the development database and regenerate the
SpacetimeDB client when deploying this schema.
