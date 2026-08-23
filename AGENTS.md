# Fabelgeist agent guide

## Architecture and data safety

- Keep tactical tick state, including positions, damage, HP, and enemies, in the
  transient Bevy server. Persisting it to SpacetimeDB would violate the
  strategic/tactical authority boundary.
- Development schemas and character data are disposable before launch. Implement
  the clean final schema without migrations, compatibility fields, dual paths,
  or legacy fallbacks unless the user identifies a player-bearing environment
  that requires migration.
- Disposable data does not make destructive operations generally safe. Reset and
  reseed only through the isolated development workflows; never delete a public
  or player-bearing database without explicit approval.
- Files in `crates/adventuresim-stdb-client/src/` are generated from the
  SpacetimeDB schema. After changing that schema, run `just generate-db-client`
  rather than editing bindings by hand.

## Documentation and interface assets

- `README.md` provides repository orientation; `wiki/index.md` owns the game
  vision and product boundaries.
- Keep documented behavior, architecture, and developer workflow synchronized
  with implementation changes. Apply public documentation wording that Bruno
  supplies; when wording is missing, identify the required update and ask Bruno
  rather than inventing it.
- For any public-facing prose, first follow
  `wiki/contributing/wiki-writing.md`, which defines the project's editorial
  voice.
- Bruno Segovia writes all public-facing prose, including README and wiki pages
  and public GitHub issue or pull-request titles and descriptions. Agents may
  prepare private research drafts, propose information architecture and
  argument maps, and edit prose Bruno supplies. Do not originate public prose
  unless Bruno explicitly asks for a draft, and never publish a draft until he
  approves its exact wording. In the normal workflow, give Bruno the private
  structure, ask him to write the public section, respond as editor, and apply
  the wording only after he decides it.
- `wiki/SUMMARY.md` is generated from `wiki/navigation.toml`. When adding,
  removing, or moving a wiki page, update the manifest and run
  `python scripts/update_wiki_summary.py`; never edit the summary directly.
- Prefer the vendored Game Icons SVGs in
  `crates/strategic-web/static/icons/game/` when an icon improves the interface.
  If the collection lacks an appropriate icon, add one from Game-Icons.net via
  Iconify and update both the collection's `ATTRIBUTION.md` and the repository's
  `THIRD_PARTY_NOTICES.md`.

## Bounded debugging

- Use one implementation agent for reproduce/fix/test loops until the
  authoritative acceptance test passes or deterministic evidence leaves a
  concrete ambiguity.
- Diagnose the earliest failed contract from bounded evidence. Do not load full
  logs or investigate downstream symptoms while that failure explains the run.
- If the user requests one iteration or testing cycle, perform exactly one
  fix/test cycle and report the agents, tests, and evidence files used.
