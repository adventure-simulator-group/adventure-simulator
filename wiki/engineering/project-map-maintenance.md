# LLM-oriented project documentation

This directory contains compact, repository-specific orientation material for
coding agents and other tools. It is part of the unified `wiki/` documentation
tree and complements the root `README.md`.

- `project-map.md` is a generated inventory of source, configuration,
  documentation, and asset files.

## Keeping the map current

After adding, deleting, renaming, or substantially repurposing a tracked file,
regenerate the map from the repository root:

```powershell
python scripts/update_project_map.py
```

Use the check mode in reviews or before completing a task:

```powershell
python scripts/update_project_map.py --check
```

The generator deliberately excludes Git internals, Cargo build output,
third-party dependency directories, and generated browser artifacts.
