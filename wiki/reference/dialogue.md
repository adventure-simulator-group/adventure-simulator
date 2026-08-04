# Dialogue architecture

Outbreak witnesses use ordinary proposition-granular quest dialogue for
chronology, practices, responsibility, and vector accounts. Permission to
examine or exhume an outbreak victim remains generic corpse-permission
dialogue: family, the local priest, or a secular authority may grant it, with
the established family-bypass consequences. See
[Outbreak investigations](outbreaks.md).

Settlement dialogue is the markerless discovery boundary for local problems.
Inns surface unknown unresolved symptoms; overview is fallback only when no inn
NPC is available. Locals repeat referrals. Hidden causes and destinations stay private.

Publicly notorious recurring hostile cases are the exception to investigation
secrecy. An eligible innkeeper, or an explicitly capable organization chapter
representative speaking to a dues-current member, may state the canonical
threat, exact site, and approximate count band. This is one shared authoritative
dialogue disclosure; it carries no testimony, evidence, preparation advice, or
client-selected case ID. The disclosure upserts a durable observer journal case
and exact public-alias pin; later referrals refresh the count without adding a
second entry.

Witness questioning uses proposition-granular authority. Hearing each atomic
claim automatically creates a private, fallible Insight assessment for that
observer. The gateway exposes only an opaque challenge token, the exact
displayed claim boundary, and a bounded `unknown`, `likely_false`, or
`likely_true` presentation signal. It never exposes reliability, canonical
truth, proposition identity, rolls, thresholds, or correctness.
The testimony draft separately authors the exact challengeable substring and
may author any subset of Charm, Command, and Bluff lines. Present lines must be
nonempty and unique; the client has no generic fallback. Surrounding narration
and punctuation remain ordinary text, and speaker attribution replaces
redundant phrases such as “The witness says.”
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
Relationship, familiarity, and demeanor appear as icon-and-meter context beside
the selected speaker.
Hidden concern binding, diagnostic correctness, personality fit, checks, rolls,
and chances remain private.
Hearing testimony spends no additional time for Insight. Charm is the
lowest-risk and lowest-leverage response, Command is medium and always strains
affinity, and Bluff is highest-risk and highest-leverage. Each response spends
five strategic minutes. A claim can receive at most one response; other claims
remain actionable. Action receipts and session revisions make retries
idempotent and stale requests fail closed.

Ordinary conversation and relationship proposals are contextual dialogue
topics for the selected local, not a separate social popup. Choosing the
conversation icon reveals a few spoken time-commitment responses; choosing the
rose reveals only currently legal courtship or wedding responses. The exact
duration is available in each response's tooltip and accessible label rather
than displayed as a raw number. Claim responses remain available only in the
relevant active dialogue session after that witness's quest testimony is
heard. Casual chat can still change that NPC's
private morale, directional affinity, and familiarity, so time spent getting
to know someone can affect a later confrontation without revealing whether
they have quest information.

Contextual contact applies to every living, co-present Character role,
including road-encounter counterparties and consenting Patients. Road content
does not own a parallel dialogue or affinity table: **Talk** uses the shared
social mutation and durable relationship edge. At a combat choice point,
contact establishes mutual awareness and removes the opening stealth choice.
For a non-combat road conversation it still records contact revision and
ordinary social consequences, leaving persuasion and nonviolent resolution to
#373.
The resulting contextual exchange is client dialogue presentation: local-chat
live reconciliation preserves it, while selecting another NPC or reloading the
page may clear it. The social and romance reducers remain the authoritative
record of the underlying time and relationship mutation.

Social and romance state uses closed Rust discriminants across the module,
generated client, and web gateway: affinity, familiarity, morale, chat outcome,
chat target, courtship kind, and romance action are not free-form strings.
The gateway validates conversation durations and idempotency keys into dedicated
types before route logic runs. Reducer rejections carry a stable typed courtship
reason alongside human-readable detail, so dialogue presentation never depends
on matching error prose. JSON keeps the corresponding snake-case values as the
browser wire representation.

A challenge succeeds only when that particular claim is factually inaccurate
and its social check succeeds. Accurate claims and insufficient checks share the same safe
failure wording. Success may release only the canonical withheld testimony
already authored for the exact witness; released testimony follows the same
structured claim and passive-assessment path. The response shows only the
realized clamped affinity change, never the exact relationship value.

## Persistent settlement actors

Settlement dialogue is authorized against persistent `settlement_resident` identities and
their authoritative strategic `settlement_resident_presence`, rather than a client-created
`<settlement>:<service>` name. A location may contain several NPCs; changing the
addressed portrait changes the actor while the character remains at that location.
Service providers retain their service conversation, while ordinary residents use the
compiled `local-resident` conversation and cannot receive service-only topics.

The public NPC row contains only visible identity and presentation: name, age band,
presentation, height, build, hair/facial hair, complexion, visible features, clothing,
profession, household, and local role. Private demographic sex, the internal projection
key, population seed explanations, and relation weights remain private. Dialogue facts
include typed age, profession, status,
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

The distinction between generic and quest dialogue is authority, not a
separate rendering system. A direct topic response may contain the typed
`runtime: testimony` binding when—and only when—the same response applies
`receive_referred_testimony`. For the exact generated witness, the server
expands that slot through the normal turn pipeline into persisted `text`,
`claim`, and `text` fragments. A claim fragment transports only its displayed
value and event-local order. Proposition identity, reliability, factual
accuracy, demeanor, checks, and rolls never enter event JSON. The private
assessment row must match the exact session event, claim order, and displayed
text before the gateway makes the fragment interactive; a missing or
mismatched row leaves ordinary inert text. `period_claim` remains display-only
and literal YAML cannot manufacture claim authority.
Conversation-start responses and prompt result turns cannot contain
authoritative testimony because those execution paths do not carry the exact
emitted claim-event sequence into the receiving effect.

Each conversation has a stable ID and named participant roles. A role declares
`player` or `npc` plus minimum/maximum cardinality, so one authored exchange can
require a shopkeeper and assistant or address several players. Optional
`on_start` responses use the same conditions, priority rules, attributed turns,
effects, and automatic source mapping as topic responses. The server evaluates
one start response exactly once when it creates a session; use it for greetings
instead of making the browser select a topic implicitly. Topics have
stable IDs, labels, knowledge/eligibility conditions, and explicitly prioritized
responses. A response contains attributed turns composed of text and inline
topic fragments. Every turn explicitly addresses either the acting participant,
one role that must bind exactly one participant, or an explicitly group-addressed
role. It may also contain an allowlisted typed runtime slot for a
speaker's visible identity, place, symptom, claim, uncertainty, referral,
evidence, testimony, or contract terms. Authored literals and runtime slots
remain distinct in the compiled catalog and source map. The server resolves
slots from authoritative strategic rows and persists only bounded inert text;
generated values are never scripts, conditions, effects, or canonical truth.
Runtime testimony is the one structured binding: each authoritative draft
becomes a claim boundary with surrounding punctuation retained as text, and
multiple drafts retain deterministic event-local order. The compiler rejects
a testimony slot without its receive effect, the effect without exactly one
slot, and attempts to place more than one testimony slot in a response.
Prompts support `yes_no`, `single`, and `multi` choices and
`first_response`, `unanimous`, `majority`, or `all_respondents` resolution.
Choices may contain `result_turns`; these are appended to the durable transcript
only after the prompt resolves and its effects succeed.

Conditions are a typed tree: `always`, `all`, `any`, `not`, and `fact`. Fact keys
are allowlisted in `FactKey`; participant role/profession, organization, religion,
familiarity, clothing,
service role, location, time period, quest state, and flags are supported. New
world facts require a Rust resolver change. Never put executable code, SQL, or
client-trusted effects in content. Effects are likewise a closed enum. A client
sends catalog revision and stable topic/choice IDs; the authoritative reducer
resolves turns and effects from the embedded catalog.

`participant_role` is a multi-valued boolean fact resolved from every
server-authoritative organization-role assignment. It may match a specific
role ID, profession, or both without enumerating organization IDs in dialogue.
This permits `noble`, `serf`, `citizen`, and professional identities to coexist
without overwriting one another. `participant_profession` retains the
resident's presented service profession semantics where that narrower fact is
desired.

Direct address uses typed runtime fragments for the visible title and the
second-person subject, object, possessives, reflexive, and verb agreement.
Only an explicitly group-addressed role receives plural formal `you`. A singular addressee receives
familiar `thou` when the speaker socially outranks them or the pair are spouse,
active lovers/courtiers, immediate parent/child/siblings, or have forty shared
hours; otherwise speech uses formal `you`. The register is resolved once when
the shared transcript event is authored, never separately per viewer. Public
role metadata chooses one winning public identity by address priority, and
both title and social precedence come from that same role: clergy overrides
family, noble family overrides citizen, and unrecognized roles cannot leak.

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

Organization business uses the dedicated compiled
`organization-representative` conversation. Join, dues/reactivation,
promotion, and presentation are closed effects with no authored or
client-submitted organization ID. Strategic authority derives the institution
only from the organization-bound representative NPC, verifies that the NPC
occupies that institution's exact authored chapter location, then applies the
existing membership authority. Membership state, promotion availability,
dues, and current presentation are server-built dialogue facts. Before asking
for confirmation, the representative names the organization and states the
joining fee and admission requirements, the dues amount, interval, and current
standing, or the current role and every directly reachable role's requirements
as applicable. When a role branches, the authoritative prompt expands into one
stable choice per authored destination role; the selected destination is passed
explicitly to the promotion reducer and revalidated as a direct transition.
Committed prompt answers are retry-safe when a response is lost; a new answer
or an action receipt from a different prompt cannot mutate a closed prompt.
The representative's greeting anchors a highlighted organization-business
topic. Its follow-up links are selected from authoritative membership facts:
nonmembers see joining, suspended members see dues only where dues exist, and
current members can follow a gated chain through dues, promotion, and
presentation without seeing actions unavailable in their current state.

The web conversation surface exposes discovered topics both as highlighted
phrases in NPC dialogue and in four icon tabs: **Quests**, **Lore**, **Recent
Tidings**, and **Of Thee**. Each compiled topic carries a typed presentation
category; the browser never infers it from an ID. The tab list is built only
from owner-scoped authoritative topic-option rows, so it cannot reveal an
undiscovered topic. Recent Tidings is privacy-safe for residents when detailed
source authorization is absent, while Of Thee expresses qualitative
relationship state as spoken questions and answers. While a prompt is open, the shared composer
matches its choices, shows a unique prefix as grey inline completion, and lets
Tab accept it. Multi-select answers use comma-separated choice labels. Other
text continues through the independent free-form chat stream.

Of Thee answers are server-authored observer-safe projections bound to the
selected subject and projection revision. They are appended as transient
contextual exchanges in the same visible message stream and are not durable
free-form chat history. Switching subjects clears them before the next request;
late responses cannot be attributed to the newly selected person. Party
portraits retain ordinary profile selection, while choosing Recent Tidings
deep-loads that subject's authorized social projection into the same functional
chat dock. Self-selection disables free-form posting and exposes reflection in
Recent Tidings.

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

Ordinary social contact also applies to any living, co-present Character,
including quest and random-encounter counterparties. It uses ordinary affinity
and familiarity without broadening the compiled fact/contract dialogue engine.
Random-encounter contact creates mutual awareness and permanently removes that
encounter's opening Sneak option.
