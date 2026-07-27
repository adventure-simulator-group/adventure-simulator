# Dialogue architecture

Settlement dialogue is the markerless discovery boundary for local problems.
Inns surface unknown unresolved symptoms; overview is fallback only when no inn
NPC is available. Locals repeat referrals. Hidden causes and destinations stay private.

Witness questioning uses proposition-granular authority. Hearing each atomic
claim automatically creates a private, fallible Insight assessment for that
observer. The gateway exposes only an opaque challenge token, the exact
displayed claim boundary, and a bounded `unknown`, `likely_false`, or
`likely_true` presentation signal. It never exposes reliability, canonical
truth, proposition identity, rolls, thresholds, or correctness.
Insight reads demeanor rather than acting as supernatural fact detection:
sincere mistakes lean the same way as sincere accurate testimony, deliberate
deception leans the other way, and evasive or partly truthful accounts have no
private directional signal. Noise can still make every assessment wrong.

Each fresh NPC encounter may issue a private, dialogue-session-scoped witness
social capability for that participant, but the gateway does not project it
until the observer actually selects and hears that witness's quest testimony
in the current session. Generic greetings and ordinary conversation therefore
show no claim controls. Once engaged, each highlighted claim is a real
accessible control, including green and uncertain claims. Activating one opens
its local Charm, Command, and Bluff responses immediately below the utterance.
Relationship, familiarity, and demeanor remain in the normal social popup.
Hidden concern binding, diagnostic correctness, personality fit, checks, rolls,
and chances remain private.
Hearing testimony spends no additional time for Insight. Charm is the
lowest-risk and lowest-leverage response, Command is medium and always strains
affinity, and Bluff is highest-risk and highest-leverage. Each response spends
five strategic minutes. A claim can receive at most one response; other claims
remain actionable. Action receipts and session revisions make retries
idempotent and stale requests fail closed.

Ordinary conversation is deliberately outside the quest dialogue controls.
The normal social menu offers a duration-selectable chat with a present local;
claim responses appear only in the relevant active dialogue session after that
witness's quest testimony is heard. Casual
chat can still change that NPC's
private morale, directional affinity, and familiarity, so time spent getting
to know someone can affect a later confrontation without revealing whether
they have quest information.

A challenge succeeds only when that particular claim is factually inaccurate
and its social check succeeds. Accurate claims and insufficient checks share the same safe
failure wording. Success may release only the canonical withheld testimony
already authored for the exact witness; released testimony follows the same
structured claim and passive-assessment path. The response shows only the
realized clamped affinity change, never the exact relationship value.

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
topic fragments. It may also contain an allowlisted typed runtime slot for a
speaker's visible identity, place, symptom, claim, uncertainty, referral,
evidence, testimony, or contract terms. Authored literals and runtime slots
remain distinct in the compiled catalog and source map. The server resolves
slots from authoritative strategic rows and persists only bounded inert text;
generated values are never scripts, conditions, effects, or canonical truth.
Prompts support `yes_no`, `single`, and `multi` choices and
`first_response`, `unanimous`, `majority`, or `all_respondents` resolution.
Choices may contain `result_turns`; these are appended to the durable transcript
only after the prompt resolves and its effects succeed.

Conditions are a typed tree: `always`, `all`, `any`, `not`, and `fact`. Fact keys
are allowlisted in `FactKey`; participant profession, organization, religion,
familiarity, clothing,
service role, location, time period, quest state, and flags are supported. New
world facts require a Rust resolver change. Never put executable code, SQL, or
client-trusted effects in content. Effects are likewise a closed enum. A client
sends catalog revision and stable topic/choice IDs; the authoritative reducer
resolves turns and effects from the embedded catalog.

Investigation dialogue uses generic facts and effects rather than per-case
content IDs. A local-problem referral records the character-owned safe rumor
receipt immediately when the tavern/overview conversation starts, without
accepting a contract. Its observer-safe presentation is persisted once in the
dialogue transcript immediately after the authored greeting. Referral turns
name a known contact or describe them,
give their occupation/relationship and expected location, and retain explicit
uncertainty. Truthfulness, private motives, hidden causes, and undiscovered
evidence never participate in topic eligibility.
When the addressed NPC is the named contact, the referral switches to
first-person wording and presents the testimony subject as an inline clickable
phrase. A different same-named NPC is still explicitly disambiguated.

Generated return and exposure finales reuse compiled generic topics in both
service and resident conversations. A topic is projected only when the server
can pre-issue exactly one generated case/objective binding for the addressed
NPC. Execution revalidates the public/canonical mapping, recipient, evidence or
custody, session revision, and one-use binding before emitting a typed fact.

Run `just dialogue-check` before review. Use
`cargo run -p adventuresim-dialogue --bin dialogue-check -- explain <id>` to
inspect response priorities. Equal highest priorities at runtime are rejected
as ambiguous instead of depending on file order.

## Persistence and multiplayer

SpacetimeDB stores dialogue sessions, named participants, attributed events,
open prompts, idempotent action receipts, per-character answers, and
per-character topic knowledge. All raw dialogue rows are private; fail-closed
gateway views are the only subscription surface, and the trusted web process
additionally filters them to the selected character. Each player participant
receives a projection row; nonparticipants receive none. The authored
condition/effect catalog is never sent to browsers. Player organization facts
come only from a current, locally recognized presented organization. The player
profession is that organization's canonical `starting_role.profession`, rather
than its organization ID; religion comes from authoritative profession-of-faith
state. Reducers verify gateway
authority, same-party membership, shared settlement, role cardinality, catalog
and session revisions, topic eligibility, and stable choice IDs. Every NPC role
is bound to a real persistent NPC, and every mutation revalidates each NPC's
exact session location and current schedule. Selecting an NPC creates a fresh
encounter so contextual and prior-interaction facts are reevaluated; old
sessions remain history rather than an indefinitely reusable active view.
Free-form `local_chat_message` remains an independent stream.

The web conversation surface exposes topics only as highlighted phrases in
NPC dialogue. Clicking one asks about that subject; there is no separate list
of generic or undiscovered topics. While a prompt is open, the shared composer
matches its choices, shows a unique prefix as grey inline completion, and lets
Tab accept it. Multi-select answers use comma-separated choice labels. Other
text continues through the independent free-form chat stream.

## Developer mode and source editing

The hammer button immediately left of the character portrait toggles developer
mode. It is off by default and persisted locally in the browser. Its
content-editing consumers include dialogue and item definitions: authored NPC
lines receive keyboard-accessible GitHub editor links, and expanding a concrete
inventory row reveals an **Edit YAML** button. Repository and ref are centralized by
the server (`ADVENTURESIM_SOURCE_REF`, default `main`); source paths and spans
come from compilation, so writers never maintain line numbers. Unsupported or
unsafe paths do not produce links. Extend developer mode only by querying the
root `data-developer-mode` attribute; do not create independent toggles.

Schema changes are pre-launch and intentionally have no migration or legacy
compatibility path. Recreate/reseed the development database and regenerate the
SpacetimeDB client when deploying this schema.
