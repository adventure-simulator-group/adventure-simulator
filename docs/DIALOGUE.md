# Dialogue architecture

Scripted dialogue is a compiled, server-authoritative strategic system. It is
separate from free-form local chat. Authors edit the JSON-compatible subset of
YAML in `content/dialogue/*.yaml`; builds validate and embed a deterministic
catalog, its SHA-256 revision, and compiler-derived source locations. Runtime
servers do not read loose content files.

## Authoring model

Each conversation has a stable ID and named participant roles. A role declares
`player` or `npc` plus minimum/maximum cardinality, so one authored exchange can
require a shopkeeper and assistant or address several players. Topics have
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
revisions, topic eligibility, and stable choice IDs. Synthetic service actors
(`<settlement>:<service>`) remain a temporary NPC identity boundary. Free-form
`local_chat_message` remains an independent stream.

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
