# Inventory

Medicinal ingredients encode public grade in bounded catalogue identities.
Prepared remedies are concrete quantity-one medication rows, so potency
survives ordinary personal/party transfer, trade, and administration without
linked metadata.

Inventory should preserve the physical and economic consequences of equipment
without turning every expedition into manual spreadsheet work.

Personal inventory represents the contents of a character's pack. Party
inventory is a shared chest with explicit value stakes; it is not equipment
silently distributed among members. Hands, pockets, worn equipment, and other
immediate tactical placement are separate from the backpack abstraction.

## Item identity

Durable equipment is never stacked. Every weapon, shield, and armor piece is a
distinct object whose condition follows it through use, repair, trade, loot,
and personal or party custody.

Ordinary fungible goods may stack. Food, alcohol, and soft soap are
quantity-one measured rows so partial use can leave a meaningful remainder.
Food lots additionally preserve their own preparation, age, nutrition, value,
provenance, and hidden contamination.

See [Measured inventory](measured-inventory.md) for the durable
amount model and [Food and cooking](food-and-cooking.md) for food
lot and cooking behavior.

## Equipment sets

Players should be able to define expected equipment sets containing weapons,
armor, ammunition, medicine, tools, and travel supplies.

At a merchant, a resupply action can purchase missing items for the selected
set. Surplus or inactive-set items can be moved to storage. The interface should
preview what an automated action will buy, move, keep, loot, or sell before it
commits anything.

## Loot and party stakes

Battle loot first enters the shared party inventory. Each participant receives
an equal stake by value, with an indivisible remainder retained by the party.
A character may:

- deposit an item and increase their stake by its value;
- withdraw an item against their stake;
- cover an indivisible difference with personal coin;
- liquidate party-owned goods at a settlement without changing existing
  ownership shares.

Autoloot should respect a configured weight limit and prefer useful
value-to-weight choices. Quest evidence, required trophies, configured
equipment, and explicitly protected items should not be sold automatically.

## Currency

Currency appears as one collapsed **Coin** row. Expanding it reveals the
historical denominations, but search, sorting, transfers, and bulk operations
treat currency as one category.

Coin is not ordinary merchandise and is excluded from automatic sale and
liquidation.

## Provisions

Travel planning should calculate expected food and water needs and let the
party purchase provisions with a safety margin.

Food and water are consumed from shared supplies before personal reserves.
Settlement departure fills owned water capacity. Ordinary settlement water is
consumed automatically during settlement rest. Rest at an inn additionally
includes food; field, private, and camp rest consume the party's own supplies,
while temple rest still consumes the party's food.

Alcohol remains separate from ordinary water. Weak drinks may contribute some
hydration, while strong alcohol is more valuable for disinfection. Desired
quantity targets protect a reserve during settlement downtime but do not make
carried supplies unavailable on the road.

## Measured consumables

Partially consuming food, drink, or soap leaves the remaining fraction in the
same row. Its displayed mass and value fall with the remaining amount.

Current interactions consume validated portions but do not yet support
arbitrary pouring, mixed containers, or partial-row merchant trade. Those are
container-model work rather than reasons to waste the remainder of a unit.

Soft soap supplies 25 cleansing points per full unit. Washing uses only the
needed fraction. Shared soap is allocated deterministically when several
characters need it, with disease and blood exposure taking priority.

## Inventory browser

Two-sided inventory views use the same controls for trade, party transfers,
loot, and cooking:

- independent search on each side;
- sortable columns and optional detail columns;
- expandable item rows;
- staged transfers before confirmation;
- quantity targets for bulk actions;
- keyboard-accessible item actions;
- URL-backed search, sorting, and column preferences.

Quantity and desired-target columns use accessible open-chest and target
icons rather than punctuation. Equipped rows retain the compact QWERTY badges
used by the tactical input map; their shared tooltip spells out each key's
physical location and equipment layer. Non-equippable cells use accessible
negative space instead of a visible placeholder glyph.

Weaponsmith and armorer views can expose relevant combat statistics, while
merchants whose goods do not use those fields retain a simpler table.

Inventory rows use stable monochrome item icons. Unknown or modded items receive
a visible fallback rather than a broken asset.

## Design principle

The underlying model may remain detailed, but repeated chores should be
previewable and automatable. A player who enjoys optimization can inspect every
item; another player should be able to define policy once and trust the
interface to carry it out visibly.
