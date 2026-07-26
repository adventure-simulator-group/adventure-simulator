# Historical equipment catalog

This document records the first equipment slice for issue #65: weapons,
shields, and armor intended for northern Germany in approximately 1544. The
definitions are the typed `WEAPONS`, `SHIELDS`, and `ARMOR` arrays in
`crates/adventuresim-stdb-module/src/item.rs`. The strategic game and the
autoresolver consume the same persisted `Item` records; there are no special
NPC-only item rules.

## Scope and historical basis

The core professional infantry set is pike-and-shot equipment with halberds,
sidearms, and armor. The German History in Documents and Images description of
an approximately 1532--42 Landsknecht procession identifies the pike as the
typical weapon. The Musée Lorrain's description of imperial Landsknecht
equipment identifies pikes and halberds as the main bodies, supplemented by
crossbowmen and arquebusiers, and notes Nuremberg mass production of infantry
armor from the 1540s. The Met describes the halberd as especially associated
with German Landsknechts, while noting the pike's role in massed sixteenth
century formations.

The catalog also deliberately includes usable older stock: the baselard,
barbute, sallet, visored sallet, mail garments, heater shield, and longbow.
These are not presented as the fashionable or preferred 1544 battlefield kit;
they are plausible inherited, stored, civilian, militia, or second-hand goods.
No later sixteenth-century weapons or armor types are included simply for
variety.

## Inventory

| Category | Intended entries |
| --- | --- |
| Civilian and militia weapons | club, walking staff, hand axe, flanged mace, war hammer, utility knife, baselard, Bauernwehr, hunting spear, self bow |
| Daggers and swords | rondel dagger, misericorde, Katzbalger, arming sword, longsword, messer, Kriegsmesser, rapier, Zweihänder |
| Formation and ranged weapons | military pike, halberd, longbow, light crossbow, heavy crossbow, matchlock arquebus, hooked arquebus |
| Shields | buckler, targe, heater shield, round shield, pavise |
| Helmets | arming cap, mail coif, kettle hat, barbute, sallet, visored sallet, burgonet, close helmet |
| Arm defenses | quilted sleeve, mail sleeve, vambrace |
| Leg defenses | padded chausses, mail chausses, greave |
| Torso defenses | arming doublet, jack of plates, brigandine, mail shirt, breastplate, cuirass |
| Waist and upper-leg defenses | padded skirt, mail skirt, fauld, tassets |

The catalog intentionally has more weapons than armor entries (26 weapons to
24 armor pieces). Helmets receive eight entries because they were independently
useful, highly varied, and more likely than a complete suit to persist in an
armory.

Shield statistics preserve an actual handling tradeoff rather than making one
catalog entry a strict upgrade. The round shield weighs 3.0 kg and provides 3.0
block, while the heater shield weighs 3.5 kg and provides 3.5 block. A player
therefore chooses between lower burden and greater protection.

## Representation and gameplay inference

The current item schema has one armor item per existing body slot, with no
layering field. A `mail_shirt`, for example, is a chest-slot alternative to a
brigandine rather than a simultaneous underlayer. This is an explicit temporary
representation constraint, not a claim that period armor was worn in one
layer. Adding layering, ammunition-specific projectile behavior, or rust
belongs to later corrosion work.

Weapons carry an explicit distribution across Polearm, Axe, Bludgeon, Sword,
Knife, Bow, Crossbow, Firearm, and Throw. Hybrid weapons split equally among
their applicable tags: a halberd is Polearm/Axe/Bludgeon, a glaive is
Polearm/Sword, and a hand axe is Axe/Knife. Combat averages the complete leaf
checks by these weights. Knife is the short-weapon category, not a literal item
name test.

## Condition and repair

Weapons, shields, armor, and clothing are individual inventory instances. Their condition is a continuous
five-bin bar: tier one is yellow-green, tier two yellow, tier three orange, tier four red, and tier
five violet. Bins one and two are field-repairable, while bins three through five require a
settlement craftsperson. Repairing bin *n* requires Smithing skill *n* for weapons, shields, and
armor, or Tailoring skill *n* for clothing; field work is capped at bin two.
Equipment quality is also its required maintenance skill, so a lesser smith can improve a
masterwork item without restoring it completely. Damage can never occupy a tier above the item's
quality: only quality-5 equipment can acquire violet tier-5 damage.

Clothing condition and Tailor repair are authoritative for damaged clothing
instances, including seeded and imported damage. Ordinary clothing wear is not
yet generated because clothing has no equipped/worn slot in the current item
model; carried inventory is deliberately not worn down as if it were being
worn. Routine wear should begin when clothing becomes equippable and can
participate in the same contact and use paths as other equipment.

Quality uses the same 1--5 scale and is shown by the item name using the corresponding condition
color, adjusted toward the fixed light interface text color for readability. Quality 3 is ordinary
munition-grade work, quality 4 is the sort of commission a knight might order, and quality 5 is
work for royalty or an esteemed hero. Munition grade is the neutral durability baseline. Quality
1--5 multiplies physical durability by 0.65, 0.80, 1.00, 1.25, and 1.60 respectively: the multiplier
raises yield and fracture stress and inversely scales ordinary wear. Outside durability and its
maintenance requirements, quality does not currently change combat statistics, coverage, handling,
price, or any other item property.

The local catalog assigns several starter and demo items across all five qualities so the Wounded
Demo fixture exercises each color and repair ceiling.

Equippable personal-inventory rows expose a checkbox backed by the item's catalog slot. Checking
it equips that exact inventory instance, displacing any item already occupying the selected slot;
unchecking it unequips the instance. Non-equipment rows keep a disabled checkbox.

Smithing uses the shared trained-skill curve: 5,000 invested hours is rank 2.5. Database upgrades
split any legacy durable stack into quantity-one instances while retaining the original row ID for
one piece, preserving equipped references; pooled party equipment is migrated the same way.

Rest resolves health first, then automatic field maintenance, then scheduled downtime. Field
maintenance uses Tailoring for clothing and Smithing for other durable equipment. Settlement rest
recommendations also include unfinished local repair orders. Services have independently seeded
Weaponsmith, Armourer, and Tailor ratings of 3--5. Repair orders escrow the exact item instance, have an ETA,
retain damage beyond the smith's skill, and never expire. A job's stable quote is
`ceil(base_value * repairable_damage)`, with a minimum of one gold; only bins within that smith's
skill contribute. The quote is charged atomically from personal gold when the repaired item is
retrieved. Bulk collection is deterministic: orders are considered by submission time and ID, and
the affordable prefix is retrieved without skipping an earlier unaffordable job.

Impact damage uses each item's explicit yield, fracture, wear, and failure-share values. Ductile
armor yields and dents readily but resists catastrophic fracture; stiff weapons resist ordinary
wear but fail more sharply under a sufficiently large impact. Failure share models construction:
one failed plate in segmented armor contributes less total damage than failure of a monolithic
breastplate. Wear continuously reduces weapon precision and armor/shield handling, without reducing
coverage for the sake of a single local hole.

Weights are kilograms. The documented object weights below anchor the scale;
the other weights are bounded gameplay estimates for ordinary serviceable
examples, rather than claimed measurements of a particular surviving object.
`base_value` is a relative gameplay value that represents material and skilled
labor. It is not a historical price series.

Combat-facing fields retain the meanings in [Combat](../tactical/combat.md):

- `accuracy` is the weapon precision multiplier; values use the documented
  0.5 club/hammer, 1.0 axe, 1.5 sword/spear, and 2.0 purpose-built precision
  calibration.
- `penetration` is its armor-resistance coefficient; blunt weapons use 0.1--0.5,
  ordinary edged weapons 1.0, spear and broadhead-like points 2.0, and narrow
  armor-seeking points 4.0.
- `reach` is metres. The present schema uses it for melee reach on melee items
  and autoresolve range on ranged items.
- `resistance` and `padding` are joules; `coverage`, `flexibility`, and
  `range_of_motion` are 0--1. Plate favors resistance and coverage, mail loses
  resistance under penetrating attacks through its high flexibility, and padded
  clothing favors padding and mobility.

These are deterministic gameplay inferences from the documented combat model
and physical construction, not claims that period sources supplied those exact
numbers. The catalog test rejects duplicate IDs, placeholder `bot_` entries,
impossible slots, absent damage types, out-of-range armor values, and a
weapons-to-armor ratio contrary to the intended inventory.

## Sources

- [German History in Documents and Images: marching Landsknechts, c. 1532--42](https://germanhistorydocs.org/de/von-den-reformationen-bis-zum-dreissigjaehrigen-krieg-1500-1648/marschierende-landsknechte-1-haelfte-des-16-jahrhunderts)
- [Musée Lorrain: equipment of two imperial Landsknechts](https://musee-lorrain.nancy.fr/les-collections/catalogues-numeriques/la-lorraine-pour-horizon/une-premiere-lorraine-francaise/equipement-de-deux-lansquenets-imperiaux)
- [The Met: German halberd](https://www.metmuseum.org/art/collection/search/25898)
- [The Met: German breastplate, dated 1540, with documented 3.515 kg weight](https://www.metmuseum.org/art/collection/search/22292)
- [The Met: German Augsburg burgonet, c. 1525--30, with documented 2.332 kg weight](https://www.metmuseum.org/art/collection/search/685229)
- [The Met: Augsburg breastplate with tassets, c. 1530](https://www.metmuseum.org/art/collection/search/35917)
- [The Met: German helmet, c. 1535](https://www.metmuseum.org/art/collection/search/24667)
- [Wallace Collection: German sallet, c. 1515, explicitly a declining form](https://wallacelive.wallacecollection.org/eMP/eMuseumPlus?module=collection&objectId=60574&service=ExternalInterface)
