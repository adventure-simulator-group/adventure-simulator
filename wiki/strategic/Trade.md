# Trade

The compiler supplies each imported settlement with canonical local production
outputs and a marginal, local, or regional scale. Future trade simulation may
consume these signals, but rules v6 does not create prices, inventory, or
shipping flows.

For the current strategic prototype, every settlement exposes the same unlimited
merchant catalogue, including imported Viabundus settlements. Items have a base gold value; merchant buy and sell prices are
derived from it with shared hidden profit-margin and sales-tax multipliers.
Both the merchant and player inventory tables display each item's per-unit
weight and relevant gold value.
Gold is represented by the `gold_coin` inventory item, in the `Currency`
category, rather than a separate character resource.

The General Market, Weaponsmith, Armourer, and Tailor use the same live trade
interface and transaction reducer. The specialist storefronts filter their
unlimited displayed stock by item category, while all use the same pricing and
buy/sell behavior. Drafting a trade immediately displays the item and gold
quantity changes on both sides; inventory changes only persist after choosing
**Offer**.
Trades are bound to the settlement where the character is currently located;
visiting another settlement's URL does not allow remote trading.
Removing a staged purchase before offering it simply cancels that purchase;
it does not create a sale or apply a merchant fee.
The confirmation popup appears in the center of the view only while an
exchange is pending and includes **Offer** and **Cancel** controls; Cancel
discards the entire draft. Loot, discard, character trade, merchant trade, and
party-inventory transfers all use this same centered confirmation pattern.

Transfer arrows always progress from one to two to three as they point inward
from the source rail. The right rail therefore mirrors their visual order to
three, two, one when read from left to right.

Equipped inventory stacks remain separate from unequipped stacks. Merchant
purchases are always added to an unequipped stack (or a new stack), and the UI
does not offer transfer or sale controls for an equipped stack.

The backpack action beneath the active character portrait opens the inventory
discard view. Discarding follows the same draft-first interaction as trading:
the player stages quantities into the left-side **Discard** list, may cancel the
draft, and must press **Discard** before the server removes anything. Equipped
items are never eligible for deletion.
