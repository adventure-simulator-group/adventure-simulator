# Fabelgeist agent guide

## Orientation

Read these before making a non-trivial change:

- `README.md` for the game vision and product boundaries.
- `wiki/reference/architecture.md` for the strategic/tactical split and persistence rules.
- `wiki/reference/developing.md` and `justfile` for local development commands.
- `wiki/reference/llm/project-map.md` for a concise inventory of repository files.
- Relevant pages in `wiki/` for gameplay, design, and technical decisions.

This is a Rust workspace. The strategic layer uses SpacetimeDB; the tactical
layer uses Bevy and transient server state. Do not persist tactical tick state
(positions, damage, HP, or enemies) to SpacetimeDB unless the architecture
documentation is intentionally changed as part of the task.

## Working rules

- Keep changes scoped to the requested outcome and preserve unrelated working-tree changes.
- Prefer `just fmt`, `just check`, `just test`, or the narrowest relevant command for verification.
- Treat generated SpacetimeDB client bindings in `crates/adventuresim-stdb-client/src/` as generated output; regenerate them with `just generate-db-client` when changing their source schema.
- Update the relevant README or `wiki/` page whenever a change affects documented behavior, architecture, or developer workflow.
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

`wiki/reference/llm/project-map.md` is generated from the current source tree. Whenever
you add, remove, rename, or substantially repurpose a repository file, run:

```powershell
python scripts/update_project_map.py
```

Before finishing a change that affects the map, verify it is current:

```powershell
python scripts/update_project_map.py --check
```

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
