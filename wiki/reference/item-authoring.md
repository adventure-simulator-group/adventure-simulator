# Item definition authoring

An `ingredient` may also author the `food` capability, including zero nutrition
and flavor for a medicine-only substance. This creates a measured lot without
declaring the substance harmless. Solid and liquid containers are selected by
their authored `container.capacity_ml`, not a cooking-vessel ID allowlist.
Grinding tools use the `grinding_tool` tag. Bottle and jar capacity must cover
the authored liquid volume plus direct solid contents.

Every physical definition must author a positive integer
`exterior_volume_ml`; omission, zero, and values above 1,000,000 are rejected.
The `container.capacity_ml` capability is interior inventory capacity, not an
equipment attachment slot count. Provisional cooking capacities are pan 2,500
ml, pot 8,000 ml, and portable oven 12,000 ml. Waterskin capacity remains 4,000
ml. These values are content-owned so they can be tuned without custody-schema
changes.

Bounded herbal grades use the ordinary base ID plus `_poor` and `_fine`
variants tagged `medicinal_herb` and `grade_*`. Concrete remedies use
`herbal_preparation` and `potency_*`; every medication identity needs a
versioned generic physiology intervention profile.
`tincture_spirit` is the single authored alcoholic Herbalism consumable. It is
an ordinary unmeasured Ingredient tagged `alcoholic_solvent` and
`tincture_solvent`, not a potable alcohol serving. Do not generalize those tags
into a solvent subsystem or expose it as a medicinal-herb selection.

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

`presentation.icon` is also the shared item-icon contract. The strategic UI
loads that Game Icons slug from its vendored SVG set, while the tactical client
looks up the same slug in its deterministic sprite atlas. Do not add a
renderer-specific item-to-icon mapping; strategic asset-coverage and tactical
atlas-coverage tests must fail when an authored equipment icon is unavailable
to either renderer.

The `adventuresim-core` build script reads item files in normalized, sorted
path order, validates them, sorts definitions by stable ID, computes a SHA-256
revision over normalized paths and source bytes, and embeds the compiled JSON.
Production never reads loose YAML. Strategic startup projects typed definitions
into the existing flattened SpacetimeDB `Item` table; that table is a
persistence/client ABI, not an authoring schema. Mutable quantity, owner,
custody, condition, and market state do not belong in definitions.

## Shape

Books remain ordinary `kind: simple` items and add `capabilities.book`. The
capability authors a `medium` Written language, exactly one typed leaf target,
a shared `quality` from 1 through 5, and an optional `settlement_allowlist` for
culturally rare stock. Quality is the teaching band: quality 1 teaches rank
0→1, quality 2 teaches 1→2, and so on. Validation rejects aggregate or unknown
targets and quality above the target-family limit. Runtime resolves embedded
metadata by stable item ID rather than widening the persisted inventory row;
the existing flattened item quality field carries the value needed for the
standard five inventory-name colors.

The starter catalog uses real works available by the 1544 setting wherever a
reasonable match exists. Examples include the bilingual *Vocabularius ex quo*,
Hans von Gersdorff's *Feldbuch der Wundarznei* (1517), *Küchenmeisterei*
(1485), Erasmus's *De civilitate morum puerilium* (1530), Vesalius's *De
humani corporis fabrica* (1543), Bock's *New Kreütter Buch* (1539), and the
German *Fechtbuch* tradition. A work's assignment to a single game skill is a
gameplay abstraction rather than a claim that the historical text was a modern
coursebook. Useful collection records include the
[Bodleian copy of *Vocabularius ex quo*](https://textinc7.bodleian.ox.ac.uk/catalog/tiv00363000),
[National Library of Medicine records for Gersdorff](https://www.nlm.nih.gov/exhibition/historicalanatomies/gersdorff_bio.html)
and [Vesalius](https://www.nlm.nih.gov/exhibition/historicalanatomies/vesalius_biblio.html),
the [Bibliothèque nationale de France record for Erasmus](https://catalogue.bnf.fr/ark:/12148/cb30402412n),
the [Metropolitan Museum's Talhoffer *Fechtbuch*](https://www.metmuseum.org/art/collection/search/32426),
and the [Nuremberg Mendel Housebook](https://online-service.nuernberg.de/viewer/!toc/5d64f831-7a9d-47b4-9a01-d6a28f29ad99/307/-/).

Every item requires `id`, `display_name`, `weight_kg`, `base_value`, `tags`,
`presentation.icon`, and a tagged `kind`. Physical units are part of field
names. Kinds are `simple`, `currency`, `ingredient`, `medication`, `clothing`,
`container`, `shield`, `armor`, `weapon`, and `food`.

Kind payloads contain only compatible fields. Weapons require a slot, an
explicit `carry` contract (`sheathable` or `hand_only`), explicit damage types,
mode flags, and an explicit finite, non-negative skill
distribution summing to one. Shields require their relevant slot/stat
payload. Armor and clothing require an `equipment` object, and any other kind
may author one. It contains stable-ID placement alternatives. A root placement
has one or more typed occupancy requirements (`location`, `channel`, and
`order`). A placement may also have a `parents` array of channel/order
requirements. At equip time, every parent requirement selects a concrete
equipped item and attachment point, allowing multi-point attachments and
placements that combine body anchors with parent edges. Armor retains its
combat stats in the kind payload, while clothing may author shared projection
stats in `equipment.protection`. For that shared projection, `coverage` is
required; omitted `padding` and `resistance` default to zero, while omitted
`flexibility` and `range_of_motion` default to one.

Protection targets are an explicit many-to-many list on each placement using
the stable seven-part body vocabulary. Never infer protection from physical
equipment locations: a helmet may occupy head, face, and neck while protecting
only `head`, and a boot sheath protects nothing. Items may expose ordered,
capacity-limited `attachment_points` with optional accepted child tags.
Sheathable weapons must expose the `sheathable_weapon` attachment tag and at
least one placement with exactly one order-zero `containment` parent
requirement; generated carry provisioning selects that compatible placement
rather than an arbitrary parent alternative. Hand-only weapons must expose neither parent placements nor
the sheathable tag, and every placement must be exactly one `held` occupancy at
`left_hand` or `right_hand`. Polearms, zweihanders, bows, firearms, and every
other authored hand-only weapon can only be held or dropped. Catalog validation
rejects a contradictory carry contract, and strategic/tactical authority
independently enforces the same held-root boundary.
Repairable kinds require a `durability` capability with
quality 1--5 and explicit physical/handling inputs.

For example, a sided sleeve authors two alternatives:

```json
"equipment": {
  "placements": [
    {
      "id": "left",
      "occupancy": [
        {"location": "left_arm", "channel": "padding", "order": 0}
      ],
      "protection": ["left_arm"]
    },
    {
      "id": "right",
      "occupancy": [
        {"location": "right_arm", "channel": "padding", "order": 0}
      ],
      "protection": ["right_arm"]
    }
  ]
}
```

A tunic instead authors one atomic placement with four occupancy requirements
and four explicit protection targets. Do not use `any_arm` or `any_leg` to
choose a placement.

Capabilities compose independently of kind. Garlic remains an `ingredient`
while carrying `capabilities.food`; alcohol remains a simple serving while
carrying `capabilities.alcohol`. Food, alcohol, container, and durability are
supported alongside books. Executable effects, physiology profile versions,
currency assignment, and other mechanics remain typed Rust and are
cross-validated against catalog membership.

The `field_shelter` tag identifies a non-consumed item that can satisfy the
typed Tent choice for field rest; the canonical definition is `field_tent`.
It remains `kind: simple`, so a general storefront can sell it, and its
authored positive weight is counted normally in shared party inventory.

There are no inferred quality, durability, damage types, weapon skills, or
unit conversions. Optional capability sections are absent when inapplicable;
fields within a present section are required unless documented otherwise.
Recipes are outside this catalog.

### Tactical placeholder dimensions

Every item with an `equipment` section authors `equipment.physical`. Its
`dimensions_m` is a finite, strictly positive `[width, length, thickness]`
box in local X/Y/Z. `anchor_offset_m` gives the attachment anchor relative to
the ordinary box-centre origin, and `grip_to_tip_m` records gameplay reach from
a weapon's hand anchor without stretching the box. Weapon tips point along
local +Y. The tactical client constrains a held weapon's authored anchor root
to `weapon.L` or `weapon.R`; inspection of the current rig shows those sockets
already use the required direction, so the documented socket correction is
the identity transform.

The same dimensions produce the visible placeholder mesh and dropped-item
collider. Do not infer them from `exterior_volume_ml`, which is container
displacement rather than a useful exterior shape.

For worn equipment, the placement's first body occupancy selects a semantic
humanoid bone and `anchor_offset_m` positions the box centre relative to that
bone. Sided placement alternatives therefore share one mirrored local shape.
For attachment-only placements, the client follows the authored parent chain
until it reaches a body occupancy and uses that body's bone. Keep dimensions
and offsets representative of the specific object; do not copy a generic
weapon box into armor, clothing, shields, or attachment hardware.

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
