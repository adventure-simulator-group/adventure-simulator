# Fabelgeist agent guide

## Orientation

Read these before making a non-trivial change:

- `README.md` for repository orientation and `wiki/index.md` for the game vision
  and product boundaries.
- `wiki/engineering/architecture.md` for the strategic/tactical split and persistence rules.
- `wiki/engineering/developing.md` and `justfile` for local development commands.
- `wiki/generated/project-map.md` for a concise inventory of repository files.
- Relevant pages in `wiki/` for gameplay, design, and technical decisions.

For any public-facing prose, first read
`wiki/contributing/wiki-writing.md`.

This is a Rust workspace. The strategic layer uses SpacetimeDB; the tactical
layer uses Bevy and transient server state. Do not persist tactical tick state
(positions, damage, HP, or enemies) to SpacetimeDB unless the architecture
documentation is intentionally changed as part of the task.

## Working rules

- Keep changes scoped to the requested outcome and preserve unrelated working-tree changes.
- Prefer `just fmt`, `just check`, `just test`, or the narrowest relevant command for verification.
- Treat generated SpacetimeDB client bindings in `crates/adventuresim-stdb-client/src/` as generated output; regenerate them with `just generate-db-client` when changing their source schema.
- Keep documented behavior, architecture, and developer workflow synchronized
  with implementation changes. Apply public documentation wording that Bruno
  supplies; when wording is missing, identify the required update and ask Bruno
  rather than inventing it.
- Bruno Segovia writes all public-facing prose, including README and wiki pages
  and public GitHub issue or pull-request titles and descriptions. Agents may
  prepare private research drafts, propose information architecture and
  argument maps, and edit prose Bruno supplies. Do not originate public prose
  unless Bruno explicitly asks for a draft, and never publish a draft until he
  approves its exact wording. In the normal workflow, give Bruno the private
  structure, ask him to write the public section, respond as editor, and apply
  the wording only after he decides it.
- Use icons where they improve the clarity or usability of the interface. Prefer the
  locally vendored Game Icons SVGs in `crates/strategic-web/static/icons/game/`;
  when a suitable icon is missing, source it from the same Game-Icons.net collection
  through Iconify (`@iconify-json/game-icons`) and update both that directory's
  `ATTRIBUTION.md` and `THIRD_PARTY_NOTICES.md`.

## Database schema evolution

This project is pre-launch. During feature development, existing database and
character data is disposable. Implement the clean final schema for the feature
and recreate/reseed the development database whenever the schema changes.

- Do not preserve backward compatibility with an earlier development schema.
- Do not create schema/data migrations, compatibility shims, legacy fields,
  dual-read/dual-write paths, or transitional fallbacks for existing data.
- Do not complicate a feature merely to retain current local characters or
  other development data. Losing that data and recreating the database is
  always an acceptable outcome while iterating on a feature.
- Only implement a migration or compatibility path when the user explicitly
  requests one for a specifically identified player-bearing environment.
- This policy governs implementation choices; it does not authorize deleting a
  public or player-bearing database. Use the repository's isolated development
  workflows for destructive reset and reseed operations.

## Project map maintenance

`wiki/generated/project-map.md` is generated from the current source tree. Whenever
you add, remove, rename, or substantially repurpose a repository file, run:

```powershell
python scripts/update_project_map.py
```

Before finishing a change that affects the map, verify it is current:

```powershell
python scripts/update_project_map.py --check
```

## Wiki maintenance

`wiki/SUMMARY.md` is generated from the human-authored `wiki/navigation.toml`.
Do not edit `SUMMARY.md` directly. When adding, removing, or moving a wiki page,
update the manifest and run:

```powershell
python scripts/update_wiki_summary.py
```

Before finishing a wiki change, format changed prose with
`just wiki-format path/to/page.md` and run `just wiki-check`.

## Completion policy

- Continue working until the requested outcome is implemented and relevant verification has run.
- Do not end a turn solely to give a progress report. Progress notes, when useful, are not a terminal response.
- For user-visible changes, initialize or restart the relevant local server and demonstrate the result when the environment permits it.
- Stop only when the task is complete and verified, or when a real blocker requires a user decision, permission, or unavailable external state.
- Before completing, compare the working tree and verification results against every explicit acceptance criterion in the request.

## Codex stop hook

This repository includes `.codex/hooks.json`, which runs a bounded Stop hook.
It asks Codex for one more pass only when the final message looks like an
intermediate progress report. Review and trust it with `/hooks` before relying
on it; project-local hooks run only in trusted projects.
