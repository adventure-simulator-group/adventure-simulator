# Errantry and modular challenges

Errantry is a quest frame for romantic knightly adventures. It is a sibling of
generated investigation cases, not another `TemplateFamily`. An errantry frame
states a chivalric purpose—such as keeping a vow, seeking a boon, rescuing
someone, making a pilgrimage, proving worth, or reconciling adversaries—and
binds an ordered sequence of trials. A trial may be combat, social interaction,
a puzzle, temptation, or another ordeal.

The frame owns narrative sequence. A challenge owns formal rules. A presenter
owns embodiment and language. This separation lets the same puzzle be asked by
a fey lady, witch, sphinx, magic book, inscription, or physical mechanism
without changing its answer.

## Ordered-sigil puzzle

The first challenge engine orders five distinct sigils. Generation:

1. deterministically shuffles a canonical solution from a private seed and
   version;
2. derives only true bounded `Exact`, `Before`, `Adjacent`, and `NotAt` clues;
3. exhaustively searches all 120 legal permutations after each clue;
4. stops when exactly one solution remains, with a fixed clue bound; and
5. replays from the stored version and seed before accepting an attempt.

Seeds 0 through 999 are covered by an exhaustive core test. Every legal
permutation is typed; duplicates, omissions, unknown sigils, stale revisions,
and wrong party/case/challenge coordinates are rejected. A wrong legal answer
creates an immutable retryable receipt and advances the revision. A correct
answer closes the challenge and emits the single source-idempotent
`ChallengeSolved` fact consumed by `SolveChallenge`.

The observer projection contains the formal clue list, allowed sigils,
presenter, revision, and safe attempt feedback. It never contains the private
seed, canonical assignment, or solution. Strategic challenge authority remains
in SpacetimeDB. No tactical position, damage, HP, or enemy tick state is
persisted.

## Presentation and supernatural speech

Presenter records are selected independently of puzzle generation. The initial
slice provides a spoken fey presenter and a silent ruin contraption. Both render
the same logical projection and accept the same answer.

Whenever a supernatural being speaks, every spoken title, address, instruction,
prompt, and feedback line must be authored as iambic pentameter in
Shakespearean English with modern spelling. Procedural prose must never be
placed in a supernatural speaker's mouth. Use a closed, reviewed line catalog
with bounded authored variants; inscriptions, books, and mechanisms may use a
different register when their text is not spoken by a supernatural being.
Automated syllable counting is not an authoring substitute.

This rule includes procedural puzzle clues. The ordered-sigil fey presenter has
a closed exhaustive catalog covering all 90 reachable sigil, position, order,
and adjacency combinations. The renderer marks those clues as the Lady's
speech. The ruin presenter uses a separate terse inscription catalog; formal
clue truth remains identical.

## Direct development demo

Run `just puzzle-demo`, select or create a character in a settlement, enable
browser-local developer mode, and choose **Puzzle demo**. The development-only
reducer creates or reuses a deterministic real errantry case, accepted
contract, and observer-bound challenge, then the HTTP adapter redirects
directly to the server-rendered puzzle. This skips rumor, travel, and manual
acceptance. Repeated loading reuses the current open demo. Solving immediately
marks its zero-reward contract paid and clears the party's active quest, so the
next load creates a fresh namespaced playable challenge rather than returning
to the solved page. The HTTP adapter derives the redirect from the safe
post-reducer challenge projection. The reducer is unavailable in ordinary
module builds.
