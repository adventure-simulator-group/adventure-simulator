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

## Representation and gameplay inference

The current item schema has one armor item per existing body slot, with no
layering field. A `mail_shirt`, for example, is a chest-slot alternative to a
brigandine rather than a simultaneous underlayer. This is an explicit temporary
representation constraint, not a claim that period armor was worn in one
layer. Adding layering, ammunition-specific projectile behavior, condition, or
rust belongs to later schema and durability work.

Weights are kilograms. The documented object weights below anchor the scale;
the other weights are bounded gameplay estimates for ordinary serviceable
examples, rather than claimed measurements of a particular surviving object.
`base_value` is a relative gameplay value that represents material and skilled
labor. It is not a historical price series.

Combat-facing fields retain the meanings in [Combat](../wiki/tactical/Combat.md):

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
