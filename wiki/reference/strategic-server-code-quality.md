# Strategic server code-quality review

This review covers the persistent strategic server in
`crates/adventuresim-stdb-module`, plus pure strategic rules in
`crates/adventuresim-core` where they define the module's domain behavior. It
reviews the `origin/main` snapshot at `7f17681e96a924af239c890e44340a2ce785b2a3`.
It does not review the transient tactical simulation or the strategic web UI.

## Conclusions

1. **Several files should be factored.** The worst files mix schema declarations,
   validation, planning, persistence, projections, reducers, and source-text tests.
   Extraction should follow domain seams, not arbitrary line-count fragments.
2. **Raw primitives cross too many internal layers.** Reducer arguments must use
   SpacetimeDB-compatible wire types, but service keys, IDs, money, quantities, and
   state-dependent strings should be parsed into domain types immediately.
3. **Several product types admit contradictory combinations.** Status enums sit
   beside independent `Option` fields whose legal presence depends on status or
   kind. Internal sum types should make those combinations exhaustive, then be
   converted to storage rows at the persistence boundary.
4. **Macros are not the problem.** There are no custom `macro_rules!` definitions
   in `adventuresim-stdb-module`. Its `#[table]`, `#[reducer]`, `#[view]`, and
   `SpacetimeType` procedural macros define the SpacetimeDB ABI and cannot usefully
   be replaced by traits. Existing core ID-declaration macros create distinct
   nominal types; traits would share behavior but would not generate those types.
5. **The largest quality risks are boundary leakage and weak tests.** Pure rules
   return strings or tuples from reducer files, many module tests inspect source
   text, and broad files make invariants difficult to see and review.

## File-size findings

The following physical line counts were measured before this remediation. Size is
only a signal; the recommended extraction seam is the deciding factor.

| Rank | File | Lines | Cohesive extraction candidates |
| ---: | --- | ---: | --- |
| 1 | `crates/adventuresim-stdb-module/src/relationship.rs` | 4,794 | commitment, courtship, marriage/pregnancy, inheritance, and gateway projections |
| 2 | `crates/adventuresim-stdb-module/src/time.rs` | 4,217 | schedule validation, activity execution, travel synchronization, rest settlement, and source tests |
| 3 | `crates/adventuresim-stdb-module/src/social.rs` | 3,885 | belief projection, action planning, effect application, and presentation views |
| 4 | `crates/adventuresim-stdb-module/src/character.rs` | 3,557 | schema, creation, lifecycle cleanup, inventory/equipment helpers, and source tests |
| 5 | `crates/adventuresim-stdb-module/src/disease.rs` | 2,960 | disease schema/persistence, interval orchestration, treatment, and source tests |
| 6 | `crates/adventuresim-stdb-module/src/strategic/challenges.rs` | 2,689 | challenge schemas, eligibility, resolution, and integration checks |
| 7 | `crates/adventuresim-stdb-module/src/strategic/inventory_trade.rs` | 2,641 | custody, loot, party finance, storefront planning, trade persistence, and party lifecycle |
| 8 | `crates/adventuresim-stdb-module/src/condition.rs` | 2,553 | condition storage, equipment wear, treatment/repair, and projections |
| 9 | `crates/adventuresim-stdb-module/src/strategic/mission_bootstrap.rs` | 2,545 | world seed, mission seed, fixtures, and source tests |

The first split completed here is intentionally vertical:
`adventuresim_core::strategic_inventory` owns pure storefront parsing, payment
planning, and provider-cardinality validation, while `inventory_trade.rs` retains
database lookup and mutation. A mechanical `include!` split would shorten a file
without improving ownership, testability, or dependency direction.

Recommended next factoring order is:

1. extract commitment/courtship transition models from `relationship.rs`;
2. extract schedule parsing and activity/rest plans from `time.rs`;
3. extract observer-safe social projections from `social.rs`;
4. split inventory custody and party lifecycle from storefront persistence;
5. move remaining deterministic disease and challenge calculations into core.

## Stringly typed and raw values

Some raw types are required in public reducer signatures and table columns, but
they should not remain raw after the boundary.

- Storefront routing previously matched raw `service_id: String` values such as
  `"merchants"` and `"weapons"` inside `inventory_trade.rs::merchant_storefront`.
  This PR replaces that internal representation with
  `strategic_inventory::MerchantStorefrontRoute::try_from`.
- `provider_resident_character_id: u64` was selected by
  `unique_default_merchant_provider` as another raw integer. This PR parses the
  unique value into a non-zero `MerchantProviderId`; missing, ambiguous, and zero
  identifiers are distinct typed errors.
- Personal and party-stake payment was an `Option<(u64, u64)>` from
  `personal_storefront_payment`. The tuple did not say which element was which and
  represented an absent source as zero. This PR uses
  `StorefrontPaymentAuthorization`, `CoinAmount`, and the sum type
  `StorefrontPaymentPlan`.
- `authority_model.rs::Contract`, `MissionAuthority`, and many other tables use
  `String` for IDs from different namespaces. Core already demonstrates the
  preferred direction with types such as `CaseSiteId` and `RecruitmentOfferId`.
  Introduce nominal IDs in pure/internal APIs first; change persisted columns only
  in an intentional clean schema revision.
- Durations, absolute strategic minutes, coin, basis points, entropy, counts, and
  row IDs are commonly all `u64`/`u32`. Examples include `CharacterTime::minutes`,
  `Contract::{gold_reward, accepted_at_minute}`, and
  `MissionAuthority::{outcome_entropy, enemy_combat_scale_bps}`. Newtypes would
  prevent unit/namespace swaps and centralize range construction.
- Parallel vectors in `finalize_party_offer` and `finalize_storefront_trade` must
  be non-empty/aligned and contain positive quantities. Generated reducer ABI
  constraints may require the vectors at the wire, but the implementation should
  immediately parse them into `NonEmpty<Vec<TradeLine>>`-like domain values rather
  than repeatedly indexing four arrays.
- Free-form `String` also stores machine state in
  `authority_model.rs::JourneyActivityRun::{selected_choice, run_ineligibility,
  outcome}` and `relationship.rs::LifecycleEventFailure::error`. Human diagnostic
  text is appropriate for the latter, but machine-consumed choices/outcomes should
  be typed discriminants with a separately formatted message.

The rule is not "newtype every integer." Values deserve bespoke types when their
units, namespace, non-zero/range requirement, or legal combinations differ.

## Representable invalid states

These are schema-level products that rely on reducer discipline today:

- `MissionAuthority` combines `MissionAttemptStatus` with optional site, hostile,
  committed resolution, and capture subject fields. This can represent a committed
  mission without a resolution, a bound case mission without its hostile, or a
  non-capture resolution with a capture subject. Model an internal
  `MissionState::{Bound(Binding), Committed(Commitment), Failed, Cancelled}` and a
  nested `Resolution::{Defeated, DrivenOff, Captured { subject, custody_version },
  CaptureTargetKilled { subject }}`.
- `MissionApproachCapability` and `MissionOutcomeCandidate` independently pair
  `HostileResolutionKind` with optional capture subject and custody version. Use the
  same resolution sum type so capture metadata is required only by capture variants.
- `Contract` combines status with `accepted_by`, `accepted_at_minute`, and
  `paid_at_minute`. An internal `ContractState::{Posted, Accepted { party, minute },
  Paid { party, accepted_at, paid_at }, ...}` would prevent partial acceptance and
  payment timestamps. `BackendContract` should project from that state.
- `ExclusiveCommitment` combines status, `resolved_minute`, and `terminal_reason`;
  `CourtshipRecord` additionally combines formal/informal kind, secrecy reason,
  approved father, dowry, and terminal fields. Separate active and terminal variants,
  then nest formal and informal courtship details.
- `BackendCharacterRelationshipStatus` is a product of independent spouse,
  courtship, wedding, and pregnancy options. Bundled projection types such as
  `Option<WeddingSummary>` and `Option<PregnancySummary>` prevent partially populated
  UI states even if the persisted rows stay normalized.
- `ScheduleAllocation` allows organization IDs without corresponding minutes and
  minutes without required organization IDs. Parse its wire form into variants such
  as `ApprenticeshipAllocation::{None, Scheduled { minutes, organization_id }}` and
  validate the 24-hour total once.

Not every table should become one giant enum. The useful pattern is a validated
internal domain state plus an explicit storage mapping, especially where table
indexing requires flattened columns.

## Macro assessment

`rg "macro_rules!" crates/adventuresim-stdb-module` finds no custom declarative
macros. The repeated attributes are SpacetimeDB procedural macros that generate
tables, indices, reducers, views, and ABI types. A trait cannot perform that code
generation or registration. No macro-to-trait rewrite is recommended.

Traits become useful only after multiple concrete implementations share behavior.
For example, a custody trait would currently hide important differences among
personal, party, measured, food-lot, medication, and durable persistence while
having only one database implementation. Prefer explicit plans and functions until
there are genuine interchangeable implementations.

## Additional senior-Rust concerns

- **Dead or non-canonical tests.** The module README explains that native tests
  exclude `adventuresim-stdb-module` because native linking lacks the host ABI.
  Yet many `#[cfg(test)]` modules in the server inspect `include_str!` or
  `STRATEGIC_SOURCE` and assert that source contains tokens in a particular order.
  Those tests are refactor-hostile and do not execute behavior. Move deterministic
  rules to core ordinary unit tests; retain only a small number of deliberate ABI or
  security-guard source checks where no behavioral harness exists. This PR removes
  the source-level provider/payment assertions replaced by core tests.
- **Recoverable panics.** Reducer paths contain `expect` calls after database or
  state lookups, for example the former durable-deposit lookup in
  `inventory_trade.rs::deposit_party_inventory_item` and attribute/rest assumptions
  in `time.rs`. Reducers should return contextual errors for violated durable-state
  assumptions. This PR fixes the durable-deposit panic; audit the remaining runtime
  `expect`/`unwrap` sites separately from test-only uses.
- **`Result<_, String>` everywhere.** `String` is required at the exported reducer
  boundary, not throughout planning. Typed errors preserve failure categories for
  tests and composition. This PR converts `PaymentPlanError`,
  `MerchantProviderError`, and `UnknownMerchantService` to strings only where the
  reducer/persistence layer consumes them.
- **Flat glob API.** `crates/adventuresim-stdb-module/src/lib.rs` publicly glob-
  reexports almost every module. That makes name ownership unclear and increases
  accidental coupling. SpacetimeDB registration requirements should be confirmed,
  then non-ABI helpers should become module-qualified or `pub(crate)`.
- **Query shape hidden in reducers.** `default_merchant_provider` iterates all
  `settlement_resident_profile` rows and filters in Rust before joining presence.
  Similar `.iter().filter(...)` patterns deserve query-plan review. Add/select
  indices for common settlement/service/location lookups and encapsulate queries so
  an innocent helper does not become an unbounded scan.
- **Lint allowances signal missing request objects.** Seven
  `#[allow(clippy::too_many_arguments)]` uses occur in the module. In particular,
  `inventory_trade.rs::finalize_storefront_trade_impl` carries aligned vectors and
  scope flags that want parsed request/line types. Do not silence additional sites
  until a SpacetimeDB reducer signature truly forces it; internal helpers can take a
  request object.
- **Legacy compatibility debt conflicts with current policy.** `time.rs` retains
  `WorldClockSchedule` and `CharacterTrainingSchedule::travel` explicitly for old
  database/client compatibility, while the repository policy says development data
  is disposable and clean schema changes should not carry shims absent an explicitly
  named player-bearing environment. Remove these in a deliberate schema/client
  regeneration change rather than perpetuating them.
- **Validation and mutation are interleaved.** Large reducers often validate some
  facts, mutate, then rely on transactional rollback for later failures. Atomicity
  makes this safe, but a `validate -> plan -> apply` structure is easier to audit,
  especially for trade, mission completion, relationship transitions, and time
  advancement.
- **Large test modules inherit private implementation detail.** The concatenated
  `strategic::STRATEGIC_SOURCE` exists primarily to let tests reach across included
  files. Prefer public/core behavior tests and a narrow SpacetimeDB integration
  fixture over expanding this synthetic source API.

## Remediation in this pull request

This is a schema-safe first slice. It:

- adds `adventuresim_core::strategic_inventory` with typed storefront routes,
  non-zero merchant provider IDs, typed provider errors, payment authorization,
  and a payment-plan sum type;
- adds ordinary native unit tests for canonical and rejected service keys,
  personal-only/stake-only/combined payments, insufficient authorization, zero
  totals, unique/missing/ambiguous providers, and invalid zero provider IDs;
- integrates those types into the existing reducers while keeping reducer
  signatures, tables, generated bindings, and tactical persistence unchanged;
- replaces the durable party-row `expect` with a contextual reducer error; and
- removes duplicate source-level pure assertions now covered by the core suite.

It does **not** claim to repair every raw ID, state product, panic, large file,
query, lint allowance, source test, or legacy field identified above.

## Ranked follow-ups

1. Introduce validated trade request/line types and split inventory custody from
   storefront persistence in `inventory_trade.rs`.
2. Model mission binding/commitment and capture outcomes as internal sum types;
   add round-trip tests against flattened storage rows.
3. Model commitment, courtship, marriage, and pregnancy transitions in core before
   splitting `relationship.rs`.
4. Parse `ScheduleAllocation` into a validated daily plan, then separate rest,
   activity, and synchronization orchestration in `time.rs`.
5. Establish a real disposable SpacetimeDB reducer integration suite and retire
   source-text tests as equivalent behavioral coverage lands.
6. Review indexed query shapes and remove unbounded profile scans from hot paths.
7. Remove confirmed pre-launch compatibility fields in one clean schema revision,
   regenerate bindings, and recreate/reseed the development database.
8. Narrow glob exports and replace avoidable `too_many_arguments` allowances with
   request objects at internal boundaries.
