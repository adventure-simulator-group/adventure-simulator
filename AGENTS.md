# Fabelgeist agent guide

## Architecture and data safety

- Keep tactical tick state, including positions, damage, HP, and enemies, in the
  transient Bevy server. Persisting it to SpacetimeDB would violate the
  strategic/tactical authority boundary.
- Backward compatibility is intentionally unsupported. Always implement the
  clean final schema and API directly; never add migrations, compatibility
  shims or fields, deprecated forwarding APIs, dual paths, compatibility
  aliases, or legacy fallbacks. Remove any such path encountered within the
  work's scope rather than preserving or extending it.
- Disposable data does not make destructive operations generally safe. Reset and
  reseed only through the isolated development workflows; never delete a public
  or player-bearing database without explicit approval.
- Files in `crates/adventuresim-stdb-client/src/` are generated from the
  SpacetimeDB schema. After changing that schema, run `just generate-db-client`
  rather than editing bindings by hand.

## Combat legibility

- Every gameplay-relevant combat factor must be legible to the player or highly
  intuitive from the situation. Combat impairment belongs in the incapacitation
  wheel; do not add hidden parallel penalties or fatigue pools.
- Fatigue is one general value, displayed in black on the incapacitation wheel.
  Weapon work, defenses, dodges, and strenuous movement all contribute to it.
  Do not introduce per-muscle fatigue or a separate oxygen-debt impairment.

## Documentation and interface assets

- `README.md` provides repository orientation; `wiki/index.md` owns the game
  vision and product boundaries.
- Keep documented behavior, architecture, and developer workflow synchronized
  with implementation changes.
- For wiki prose, first follow
  `wiki/contributing/wiki-writing.md`, which defines the project's editorial
  voice.
- Agents may author wiki prose, but only on a dedicated wiki branch created from
  `main`, never on the implementation branch that made the wiki stale. Open a
  separate pull request for the wiki update and request Bruno Segovia's review.
  Agents must not merge wiki prose they authored, and wiki prose must not be
  merged until Bruno approves its exact wording.
- `wiki/SUMMARY.md` is generated from `wiki/navigation.toml`. When adding,
  removing, or moving a wiki page, update the manifest and run
  `python scripts/update_wiki_summary.py`; never edit the summary directly.
- Prefer the vendored Game Icons SVGs in
  `crates/strategic-web/static/icons/game/` when an icon improves the interface.
  If the collection lacks an appropriate icon, add one from Game-Icons.net via
  Iconify and update both the collection's `ATTRIBUTION.md` and the repository's
  `THIRD_PARTY_NOTICES.md`.

## Rust style and maintainability

- Before editing Rust source or Cargo manifests, also follow
  `crates/AGENTS.md`.

## Bounded debugging

- Use one implementation agent for reproduce/fix/test loops until the
  authoritative acceptance test passes or deterministic evidence leaves a
  concrete ambiguity.
- Diagnose the earliest failed contract from bounded evidence. Do not load full
  logs or investigate downstream symptoms while that failure explains the run.
- If the user requests one iteration or testing cycle, perform exactly one
  fix/test cycle and report the agents, tests, and evidence files used.
