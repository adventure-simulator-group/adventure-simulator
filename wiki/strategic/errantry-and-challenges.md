# Errantry and modular challenges

Errantry is a quest frame for romantic knightly adventures. It is a sibling of
generated investigation cases rather than another investigation
`TemplateFamily`. A frame states a chivalric purpose and binds an ordered
sequence of combat, social, puzzle, temptation, or ordeal trials.

The frame owns narrative sequence and a challenge owns formal rules. The
initial presentation boundary is deliberately chat-only: it identifies the
Lady Beneath the Thorn with a typed catalog ID, and the server derives every
utterance from that closed catalog. No arbitrary persisted supernatural prose
reaches the client. Future speakers or physical presenters can be new typed
catalogs without changing puzzle truth.

## Puzzle authority and projections

Puzzle challenges use a tagged, presenter-independent envelope with five
variants: ordered sigils, truthful witnesses, rune transformation,
multi-category logic grids, and resource allocation. Private
authority contains the versioned seed and canonical truth. The observer-safe
projection contains only formal observations and the legal answer domain. A
typed submission must match the projected engine before the authoritative
reducer will check it. The shared revision and immutable attempt receipt make
wrong answers retryable and exact lost-response retries idempotent for every
engine.

The mechanics live in the dependency-light `adventuresim-puzzles` crate and
are re-exported by `adventuresim-core::errantry`. Each private authority also
retains a validated generation specification, allowing the same engine to be
parameterized and replayed exactly in the `puzzle-lab` CLI without importing
quest presentation or persistence. See [Puzzle laboratory](../engineering/puzzle-lab.md).

The presenter catalog is a separate boundary. The Lady Beneath the Thorn has
closed introduction, instruction, failure, and success verse for every engine.
Formal testimony and transformation examples remain data owned by the puzzle
engine; changing the presenter cannot alter them. All of the Lady's spoken
lines use authored modern-spelling Shakespearean English in iambic pentameter.

## Ordered-sigil puzzle

The first challenge engine orders five distinct sigils. Rules version 3:

1. deterministically shuffles a canonical solution from a private seed;
2. derives the complete pool of true typed `Exact`, `Before`, `Adjacent`, and
   `NotAt` clues;
3. evaluates that pool against all 120 legal permutations;
4. selects a globally minimum-cardinality proof, bounded to four clues, with
   deterministic preferences for fewer `Exact` clues and then fewer `NotAt`
   clues; and
5. replays the stored version and seed before accepting an attempt.

The generation specification can enable or disable each clue family and set
the clue ceiling. Production uses the standard specification above; laboratory
runs can test other formal grammars while retaining the same minimizer and
validator.

All 120 possible solutions have exhaustive tests for global minimality and the
necessity of every selected clue. Duplicates, omissions, unknown sigils, stale
revisions, and wrong party/case/challenge coordinates are rejected. A wrong
legal answer creates an immutable retryable receipt and advances the revision.
A correct answer closes the challenge and emits the source-idempotent
`ChallengeSolved` fact consumed by `SolveChallenge`.

The observer projection contains only formal clues, allowed sigils, a typed
presenter catalog ID, revision, and safe attempt feedback. It never contains
the private seed or canonical solution.

## Truthful-witness puzzle

Three badge-named witnesses each make one statement compiled from formal
predicates about the safe path or another witness's truthfulness. Exactly one
witness lies. Generation chooses private canonical safe-path and liar values,
derives only statements whose truth values agree with that authority, and then
exhaustively evaluates all nine legal `(safe path, liar)` assignments. A puzzle
is accepted only when every remaining assignment proves the same safe path.

Every statement is necessary: removing any one must cease to prove a unique
safe path. The player answers the meaningful conclusion—which road is safe—
rather than being required to identify a hidden liar when that identity is not
needed. The public projection omits both the canonical path and liar.

## Rune-transformation puzzle

The standard rune engine gives each of three named gates one of five typed exchange
operations over the same bounded five-sigil domain. Each law exchanges one
adjacent pair in a closed ring and leaves the other three unchanged. It shows two input/output
examples for each gate, then asks which sigil emerges after a new input passes
through all three gates in a generated order.

The public prompt names the five sigils and enumerates all five candidate
exchange rules. Each gate independently selects one
rule; rules are never combined within a gate, although the final route composes
the three inferred gate operations. Examples are evidence selecting among that
closed rule family, not an invitation to guess an arbitrary mathematical
function.

Generation exhaustively compares every operation consistent with each gate's
examples. Each individual example is deliberately ambiguous, while the pair
must identify exactly one gate operation; consequently every displayed fact is
necessary to infer all three laws. The projection contains examples, route,
query, and answer choices but not the operations or result. Icons may later
decorate the sigils, but mechanically relevant names are always present as
text.

Its generation specification can vary the active gate count, route length,
examples per gate, minimum ambiguity of a single example, operation reuse, and
route-gate reuse. Validation always requires the displayed examples jointly to
prove every active gate law and the final submitted result.

## Logic-grid puzzle

The logic-grid engine matches three or four travelers across two additional
one-to-one categories: carried tokens and chosen roads. Typed positive and
negative clues can relate traveler to token, traveler to road, or token to
road. The engine enumerates every legal pair of permutations, selects clues by
bounded deterministic information gain, proves one complete assignment, and
then removes every clue that is not necessary. The chat form presents ordinary
select controls for every traveler; no world-space manipulation or hidden
visual distinction is required.

## Resource-allocation puzzle

The allocation engine presents a finite provision catalog, a carrying limit,
and named journey hazards. Every provision has explicit integer weight,
readiness, and hazard coverage. The player chooses a pack that fits the limit
and answers every hazard, then applies the stated lexicographic objective:
greatest readiness, lowest weight, and fewest items. The engine exhaustively
evaluates every subset and rejects generation unless exactly one pack is
optimal. Standard instances also require every hazard and the capacity itself
to change that optimum. Puzzle choices are hypothetical and never consume
real inventory.

## Supernatural speech and chat

Every line spoken by the fey presenter is reviewed, authored text selected
from a closed catalog. This includes all 90 reachable clue coordinates. The
complete transcript, clue prompt, submitted answer, feedback, and ordering
form appear within the shared chat visual. A challenge conversation opts out
of location-driven live navigation, so background world updates cannot clear
an answer being composed or dismiss a solved exchange. The submitted answer
and response remain visible until the player explicitly returns to camp.
Submission is server-rendered POST/redirect/GET and does not require JavaScript.
The former ruin-contraption presentation path is disabled until physical
interaction has its own typed, non-dialogue contract.

## Issuance, road trials, and preparation

Only `order_saint_george` has the authored `errantry_issuance` capability.
Production acceptance is a narrow vertical-slice action inside an existing
organization-representative dialogue. The client supplies its dialogue session
and an idempotency action ID; the reducer re-derives the live NPC, exact
chapter, settlement, location, and organization capability before creating the
accepted quest. A durable acceptance receipt makes an exact retry return the
original case and contract before the now-consumed dialogue presence is
checked. Other representatives and services cannot issue it.

Production acceptance leaves the party in its settlement and creates the
ordinary accepted journey destination. The party departs through the normal
travel flow. The preliminary trials are initially unbound and attach to the
first persisted camp on that exact journey; stale URLs from an earlier camp no
longer project or accept them. Only the development demo fabricates a midpoint
camp and moves the party there immediately.

The preliminary fey trial is optional. It is available only to the accepted
party at a persisted camp on the active journey whose exact departure,
movement, elapsed, and finale-destination coordinates match private authority.
A pending random encounter hides and blocks trial actions. The trial is not a
`StrategicEncounter` and never blocks **Continue travel**.

Road encounter choices carry a closed audit vocabulary. Empty
`shared_systems` claims are invalid, combat and ignore choices must name their
matching shared action, and interactions that still require negotiation,
crime, or ownership authority remain explicitly blocked on their follow-up
issue. If a party talks before choosing combat, the exact road-cast Character
IDs carry into the strategic encounter and eventual corpse flow; mutual
awareness removes the opening sneak option.

After at least one hour of rest at that same camp, an optional wounded courier
can interrupt the rest. The same build-compiled, goal-neutral scene may also
occur by chance during ordinary travel or wilderness rest. Its public
projection does not reveal which origin selected it. This is a mortal,
Shakespearean-English conversation inside the camp chat window. The deed, not
a virtue-labelled button, determines
which virtue the outcome recognizes. Binding his wound is always available,
exemplifies Mercy, and yields the physical captured dispatch. A trained
Command route rallies him and commits the knight to escorting him back through
the threatened ford, exemplifies Courage, and yields his physical Order oath
token. Direct study of his Roman Catholic tradition opens
a Religion route that consecrates his oath, exemplifies Faith, and yields a
blessed sword-knot. Each successful source-idempotent resolution raises only
the acting knight's matching Conscience, Nerve, or Conviction score by up to
6,000; the requested change is clamped at the score boundary.
Robbing him yields more coin while reducing Conscience. Ignoring him is an
explicit, uniformly available resolution and does not mutate inventory or
personality. Neither
preliminary challenge is a quest objective: defeating the finale's four armed
retainers resolves the errantry normally.

Solving the fey puzzle grants no token, magical buff, enemy-scale reduction, or
capability multiplier. It reveals tactical knowledge derived from fields the
combat system already consumes. The current finale's armed retainers have no
ranged attack and must close to melee range, so the retained transcript and
camp preparation notice recommend bows and arrows. The solved challenge does
not mutate their count, attributes, skills, equipment, or combat profile.

The courier's dispatch, oath token, sword-knot, or stolen currency is ordinary
inventory. No route writes generic enemy-scale reductions, capability
multipliers, or magical buffs. An errantry-only overlay may append practical
information after resolution, but a chance occurrence never mutates a quest.
Applying that information through physical tactical preparation is deferred to
the corresponding finale implementation; it may not weaken the enemy merely
because a content tag was earned.

The shared virtue vocabulary now also names Justice, Courtesy, Loyalty,
Prudence, and Honesty. Personality changes still target the existing mutable
continuous axes and must be paired with an authored deed, rather than serving
as decorative bookkeeping.

## Direct development demo

Run `just puzzle-demo`, select a character in a settlement, enable developer
mode, and choose **Sigil puzzle**, **Witness puzzle**, **Rune puzzle**,
**Logic-grid puzzle**, or **Provision puzzle**. The development-only reducer creates or
reuses a deterministic Order-sourced case, accepted contract, finale site and
hostile group, active journey, persisted camp, and observer-bound challenge.
The isolated development bootstrap seeds an authored Order chapter and its
canonical organization representative so a fresh demo profile has a valid
Order issuer even though the selected Social Demo character begins in
Riverdale.
The HTTP adapter redirects directly to the chat puzzle, skipping ordinary
dialogue acceptance and travel setup. Solving shows the modeled tactical
insight, which remains visible after returning to camp so equipment can be
prepared accordingly. Rest at least one hour to exercise the wounded-courier interruption, or
use **Continue travel** to bypass either preliminary challenge and reach the
bound finale.

Wounded couriers and knights are durable Characters with an actual untreated
limb wound and treatment consent. Their authored outcome observes an ordinary
successful bandage rather than consuming a bandage through a bespoke effect.
The camp projects every active Character-backed cast member in authored order.
Each can be contacted through the ordinary contextual social reducer, and a
consenting wounded Patient exposes the ordinary bandage action. Resolving or
ignoring the occurrence deactivates only context membership; it never deletes
the Characters or their relationship history.
