# Item definition authoring

When browser-local developer mode is enabled, expanding a concrete inventory
row shows an **Edit YAML** button that opens the definition at its compiled
file and line in GitHub. Source locations are generated during catalog
compilation; authors should not maintain line numbers or source URLs.

Item metadata is authored in `content/items/*.yaml`. These files use YAML's
strict JSON-compatible subset: quote every mapping key and string, use JSON
arrays/objects, and do not use aliases, tags, implicit scalars, comments, or
duplicate keys. Every document declares `"schema_version": 1`.

## Identity and runtime boundary

`id` is the stable, persisted identity. It is 1--64 lowercase ASCII letters,
digits, or underscores and must never be changed merely to rename an item.
Changing or removing an ID requires recreating/reseeding the disposable
development database. `display_name` and `presentation.icon` are presentation
metadata and may change without changing identity.

The `adventuresim-core` build script reads item files in normalized, sorted
path order, validates them, sorts definitions by stable ID, computes a SHA-256
revision over normalized paths and source bytes, and embeds the compiled JSON.
Production never reads loose YAML. Strategic startup projects typed definitions
into the existing flattened SpacetimeDB `Item` table; that table is a
persistence/client ABI, not an authoring schema. Mutable quantity, owner,
custody, condition, and market state do not belong in definitions.

## Shape

Every item requires `id`, `display_name`, `weight_kg`, `base_value`, `tags`,
`presentation.icon`, and a tagged `kind`. Physical units are part of field
names. Kinds are `simple`, `currency`, `ingredient`, `medication`, `clothing`,
`container`, `shield`, `armor`, `weapon`, and `food`.

Kind payloads contain only compatible fields. Weapons require a slot, explicit
damage types, mode flags, and an explicit finite, non-negative skill
distribution summing to one. Armor and shields require their relevant
slot/stat payload. Repairable kinds require a `durability` capability with
quality 1--5 and explicit physical/handling inputs.

Capabilities compose independently of kind. Garlic remains an `ingredient`
while carrying `capabilities.food`; alcohol remains a simple serving while
carrying `capabilities.alcohol`. Food, alcohol, container, and durability are
the supported capabilities. Executable effects, physiology profile versions,
currency assignment, and other mechanics remain typed Rust and are
cross-validated against catalog membership.

There are no inferred quality, durability, damage types, weapon skills, or
unit conversions. Optional capability sections are absent when inapplicable;
fields within a present section are required unless documented otherwise.
Recipes are outside this catalog.

## Workflow

```powershell
just content-check
just content-check items
```

The default `all` target runs the same build compilers used for item, quest,
organization, and dialogue content. The targeted `items` form runs the item
checker (the core build still validates its other compiled catalogs).

Both commands exercise the production build-time validator. Diagnostics
aggregate independent semantic failures where possible and identify source
file, line, column, item ID, and field path. JSON syntax and duplicate-key
errors use parser coordinates. Validation rejects unsupported schemas, unknown fields,
duplicate IDs, invalid stable IDs, non-finite/out-of-range values,
incompatible slots/stats, malformed weapon skills, and invalid durability,
quality, food, hydration, alcohol, medication, or container metadata.
