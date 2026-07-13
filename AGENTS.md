# Adventure Simulator agent guide

## Orientation

Read these before making a non-trivial change:

- `README.md` for the game vision and product boundaries.
- `docs/ARCHITECTURE.md` for the strategic/tactical split and persistence rules.
- `docs/DEVELOPING.md` and `justfile` for local development commands.
- `docs/llm/PROJECT_MAP.md` for a concise inventory of repository files.
- Relevant pages in `wiki/` for gameplay and design decisions.

This is a Rust workspace. The strategic layer uses SpacetimeDB; the tactical
layer uses Bevy and transient server state. Do not persist tactical tick state
(positions, damage, HP, or enemies) to SpacetimeDB unless the architecture
documentation is intentionally changed as part of the task.

## Working rules

- Keep changes scoped to the requested outcome and preserve unrelated working-tree changes.
- Prefer `just fmt`, `just check`, `just test`, or the narrowest relevant command for verification.
- Treat generated SpacetimeDB client bindings in `crates/adventuresim-stdb-client/src/` as generated output; regenerate them with `just generate-db-client` when changing their source schema.
- Update the relevant README, `docs/`, or `wiki/` page whenever a change affects documented behavior, architecture, or developer workflow.

## Project map maintenance

`docs/llm/PROJECT_MAP.md` is generated from the current source tree. Whenever
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
