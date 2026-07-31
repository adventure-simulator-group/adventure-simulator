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

## Ordered-sigil puzzle

The first challenge engine orders five distinct sigils. Rules version 2:

1. deterministically shuffles a canonical solution from a private seed;
2. derives the complete pool of true typed `Exact`, `Before`, `Adjacent`, and
   `NotAt` clues;
3. evaluates that pool against all 120 legal permutations;
4. selects a globally minimum-cardinality proof, bounded to four clues, with
   deterministic preferences for fewer `Exact` clues and then fewer `NotAt`
   clues; and
5. replays the stored version and seed before accepting an attempt.

All 120 possible solutions have exhaustive tests for global minimality and the
necessity of every selected clue. Duplicates, omissions, unknown sigils, stale
revisions, and wrong party/case/challenge coordinates are rejected. A wrong
legal answer creates an immutable retryable receipt and advances the revision.
A correct answer closes the challenge and emits the source-idempotent
`ChallengeSolved` fact consumed by `SolveChallenge`.

The observer projection contains only formal clues, allowed sigils, a typed
presenter catalog ID, revision, and safe attempt feedback. It never contains
the private seed or canonical solution.

## Supernatural speech and chat

Every line spoken by the fey presenter is reviewed, authored text selected
from a closed catalog. This includes all 90 reachable clue coordinates. The
complete transcript, clue prompt, feedback, and ordering form appear within
the shared chat visual. Submission is server-rendered POST/redirect/GET and
does not require JavaScript. The former ruin-contraption presentation path is
disabled until physical interaction has its own typed, non-dialogue contract.

## Issuance, road trial, and boon

Only `order_saint_george` has the authored `errantry_issuance` capability.
Production acceptance is a narrow vertical-slice action inside an existing
organization-representative dialogue. The client supplies only its dialogue
session; the reducer re-derives the live NPC, exact chapter, settlement,
location, and organization capability before creating the accepted quest.
Other representatives and services cannot issue it.

The preliminary fey trial is optional. It is available only to the accepted
party at a persisted camp on the active journey whose exact departure,
movement, elapsed, and finale-destination coordinates match private authority.
A pending random encounter hides and blocks trial actions. The trial is not a
`StrategicEncounter` and never blocks **Continue travel**.

Solving awards a durable party inventory item, the **Favor of the Thorn Lady**,
and a typed source-idempotent countermeasure bound to that party, case, finale
site, and hostile group. When the finale mission first binds, its immutable
snapshot records base scale, effective scale, and countermeasure source. The
effective enemy combat scale is reduced by 25%, with a 50% floor; tactical play
and autoresolve consume the same effective snapshot. Existing missions and
hostile-group authority are never mutated.

## Direct development demo

Run `just puzzle-demo`, select a character in a settlement, enable developer
mode, and choose **Puzzle demo**. The development-only reducer creates or
reuses a deterministic Order-sourced case, accepted contract, finale site and
hostile group, active journey, persisted camp, and observer-bound challenge.
The HTTP adapter redirects directly to the chat puzzle, skipping ordinary
dialogue acceptance and travel setup. Solving shows the boon and returns to
camp so **Continue travel** can reach the bound finale.
