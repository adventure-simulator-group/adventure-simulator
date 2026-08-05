# Strategic action planning

Strategic actions share a pure planning vocabulary in
`adventuresim_core::strategic_action`. The vocabulary standardizes how a
domain names an actor, target, exact place and fixture, physical tools and
their expected custody, requirements, capability snapshots, elapsed time,
interruptions, effects, state revisions, and idempotency provenance.

This is a calculation boundary, not a universal action system. There is no
`perform_action(kind, payload)` reducer, scripting surface, SQL plan, or
client-authored effect payload. Planner types deliberately do not implement
Serde or SpacetimeDB transport traits. Constructing a plan grants no
permission. The reducer that owns an investigation, foraging, cooking, or
preparation fact remains the only code allowed to commit that fact.

## Domain extension contract

Each adopting domain defines closed Rust enums and implements the applicable
marker traits for its target, requirement, capability, interruption, effect,
and public-preview vocabulary. Strings and JSON values do not satisfy these
bounds. `ActionRequirement::Domain` is also the intended seam for a later
typed rights/permission question; the planner does not answer that question or
depend on a rights implementation.

Tool references and custody-transfer effects use checked constructors backed
by `ObjectCustody`; their fields are private, so neither an expected source nor
a destination can encode an object as its own container.

An adapter performs these steps inside its owning reducer boundary:

1. Resolve canonical character, object, custody, place, fixture, and domain
   identities from authoritative state.
2. Snapshot every private prerequisite and capability used by the calculation,
   then assign one revision and digest to that complete input state.
3. Supply private `RequirementCheck` values and one independently chosen,
   player-safe `PublicRejection`. Adding or removing a hidden failed check does
   not alter that public rejection.
4. Call `build_plan`. Its calculation closure receives the canonical
   coordinates and the once-clipped `TimeResolution`; it returns only the
   domain's closed effects and public preview.
5. At commit time, read current state again, rebuild the same pure plan, and
   call `validate_commit` before applying anything. Apply effects in the
   owning reducer's transaction only when it returns `Apply`.
6. Persist an exact `CommitReceipt` with the committed domain facts. An exact
   retry returns `IdempotentReplay`; a request-ID collision, private-binding
   forgery, stale snapshot, changed prerequisite, or calculation mismatch has
   a distinct typed result.

The private authority binding and canonical effects stay server-side. A web
adapter may render `PublicActionPlan` or the sanitized rejection, never the
private requirement list, snapshot digest, authority binding, or effect plan.

## Time and interruption laws

`resolve_time` clips one requested interval against the earliest terminal or
interruption boundary. A boundary exactly at the requested endpoint is still
a boundary outcome, not completion. Boundaries at or before the actor's
current minute produce zero elapsed minutes, and a terminal boundary wins a
tie, avoiding order-dependent results. If the positive duration would reach or
overflow the strategic clock ceiling, `ClockExhausted` clips at `u64::MAX`;
the ceiling is never reported as successful completion. Clipping a duration in partitions reaches
the same end minute and completion-effect eligibility as clipping the combined
duration once. Domains must use `permits_completion_effects` before emitting
completion-only consequences; interrupted, terminal, exhausted-clock, and
zero-elapsed plans remain non-completions.

## Planned integrations

This foundation intentionally converts no reducers. Follow-up stacked changes
will provide independent adapters for:

- investigation inspection, with private case/evidence prerequisites and
  typed journal/evidence effects;
- foraging, with exact vicinity, terrain/capability snapshots, interruption,
  and typed yield/exposure effects;
- cooking and ingredient preparation, with exact fixture/tool custody,
  duration clipping, and typed dish, lot, medicine, and training effects.

Those adapters should remove only the duplicate validation they replace. They
must keep their existing privacy, replay, and transactional domain authority.
