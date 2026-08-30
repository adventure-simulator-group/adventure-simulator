# Rust agent guide

These instructions apply to hand-written Rust source and Cargo manifests under
`crates/`. Files in `adventuresim-stdb-client/src/` are generated and are
governed by the repository-root generation rule instead.

## Style and design sources

- Use the default [Rust Style Guide](https://doc.rust-lang.org/style-guide/)
  through `rustfmt`; do not maintain a competing hand-formatted style.
- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
  unless a repository-specific contract is more precise.
- For agent-authored Rust, also apply Microsoft's
  [AI Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/ai/):
  prefer idiomatic APIs, strong domain types, testable boundaries, behavioral
  tests, and documentation of the resulting design rather than the design
  process.
- Prefer direct, readable code over cleverness, speculative abstractions,
  compatibility scaffolding, or duplicated paths.

## Semantic values and types

- Treat a literal as magic when it encodes a domain unit, scale, bound,
  probability, tuning decision, protocol or status value, route, query, or
  error classification whose meaning is not intrinsic at the use site.
  Repetition is evidence of a shared concept, but a one-use literal can still
  be magic.
- Put a semantic value in the module that owns its meaning and give it a domain
  name. Reuse an existing owner before creating another constant, type, or
  helper.
- Default to bespoke domain types for domain values, including identifiers,
  units, quantities, states, validated values, and meaningful collections. Use
  enums, newtypes, and validated structs so invalid combinations are rejected
  by the compiler. Keep raw strings, numbers, and containers at explicit system
  boundaries or where the value is genuinely primitive.
- Do not carry a naked primitive through domain logic when a bespoke type can
  express its unit, invariant, authority, or allowed state more precisely.
- Define each domain concept once in its lowest shared owning module. Import and
  reuse that type, its constants, and its conversions; never create equivalent
  bespoke types independently in multiple files.
- Do not create constants whose names merely restate their values. Keep
  intrinsically clear `0` and `1`, small indexes and dimensions, authored
  catalog data, isolated test fixtures, and local format tokens inline when a
  name would only add indirection.
- Do not branch on mutable human-facing prose. Return a stable typed error or
  code from the authoritative boundary and map it to presentation text once.

## Modules and files

- Split modules by responsibility, not to satisfy a mechanical line limit.
  Roughly 400 lines of production code is a healthy target and 500 lines is a
  design smell; exclude unit tests from that judgment.
- Before materially expanding a production file that is already near or above
  that range, extract the responsibility being added into a focused module.
- Split a shorter file when it owns multiple distinct concerns. A longer file
  may remain intact when it is genuinely cohesive and splitting would obscure
  the design.
- Small helper modules, including double-digit-line files, are encouraged when
  they own a clear concern. Do not merge them merely to avoid small files. If a
  helper grows large, check whether it has accumulated multiple concerns.
- Extract stable responsibilities into real modules and move their tests and
  module or type documentation with them. Do not simulate modularity with
  section-banner comments.
- Avoid pass-through modules, services, traits, and wrappers that add no
  policy, invariant, boundary, or reusable abstraction.

## Functions, methods, and constructors

- When an operation has a clear receiver and its main argument is a local
  struct, define it as an inherent method on that struct.
- Define functions that construct a local type as associated constructors.
  Use `new` for the primary construction path, a domain verb when that is
  clearer, and `from_*` for construction from a particular representation.
  Prefer `From` or `TryFrom` when the source type fully determines the
  conversion.
- Keep a free function when an operation is symmetric across multiple types,
  belongs to the module rather than one type, or must satisfy a trait or
  callback signature.
- Prefer self-documenting parameter types over positional booleans, numbers,
  or ambiguous `Option` values.

## Lints, tests, and optimization

- The `fabelgeist-rust-quality check` command enforces the semantic-value
  registry, raw-string branching rules, Cargo lint inheritance, and the
  500-line file and 100-line function no-growth ceilings. `just lint` runs it
  automatically.
- Keep legitimate fixtures, authored catalogs, shader source, and external
  boundary adapters in `rust-quality.toml`. Every scope or exception must be
  narrow and reasoned; the checker rejects entries that no longer match.
- Use the checker's `census` subcommand to review repeated values. Its output is
  advisory: repetition suggests a concept worth reviewing but does not make an
  otherwise ordinary value illegal.
- After splitting an oversized module or migrating a semantic family, run the
  `baseline` subcommand and apply only the reductions relevant to that change.
  Never raise a ceiling to accommodate new production debt.
- Use `#[expect(..., reason = "...")]` for a localized lint exception. Reserve
  `#[allow(...)]` for generated code and macro expansions where `#[expect]`
  cannot be used reliably.
- Test observable behavior, failure paths, and domain invariants. Do not add
  tautological tests that merely repeat constants or mirror the implementation.
- Optimization is not a prototyping goal. Do not optimize without a measured
  problem, an acceptance constraint, or an explicit user request. A
  readability-neutral lint fix is ordinary maintenance, not permission for a
  performance redesign.
- Run `just fmt-check` and `just lint` for Rust changes. Use narrower package
  checks while iterating, then run the applicable final gates.
