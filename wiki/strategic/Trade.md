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

Herbalists use a narrower authoritative purchase path. They offer unlimited
ingredients plus all eight pre-prepared medication courses, but prepared
medication remains rejected by the generic merchant reducer. Each course costs
more than the normal merchant cost of its recipe ingredients, using shared
pricing helpers on both the server and storefront. Mixed and multiple purchases
are allowed; every medication course enters personal inventory as its own
quantity-one row. The herbalist page deliberately omits party-inventory buying
and explains that restriction so courses cannot become unusable shared stacks.

Weaponsmith and Armourer storefronts also accept individual equipment instances for repair through
separate actions that never enter the sale draft. A smith repairs only condition bins at or below
their independently seeded skill (minimum 3), but may accept an item with additional harder damage
and leave that residual condition untouched. Custody and the quoted ETA persist across travel and
have no collection deadline. The smith quotes the full job when accepting it: the item's base value
multiplied by the share of damage that smith can repair, rounded up to at least one gold. The quote
is stable while the item is in custody and is paid from personal gold when completed work is
retrieved. The custody table shows durability, ETA, and this full-job cost. A row arrow retrieves
that exact quoted order by default; Shift changes it to retrieve up to two matching ready orders,
and Control changes it to retrieve all matching ready orders. The header arrow defaults to two and
Control changes it to all ready work in that shop. Bulk retrieval stops before the first order the
character cannot afford rather than failing already-affordable retrievals.
Removing a staged purchase before offering it simply cancels that purchase;
it does not create a sale or apply a merchant fee.
The confirmation popup appears in the center of the view only while an
exchange is pending and includes **Offer** and **Cancel** controls; Cancel
discards the entire draft. Loot, discard, character trade, merchant trade, and
party-inventory transfers all use this same centered confirmation pattern.

Every inventory action exposes one inward-pointing arrow control. Row controls
default to one arrow, become two while Shift is held, and become three while
Control is held. Header controls default to two arrows and become three while
Control is held. These modifiers apply consistently to merchant, character,
party, loot, discard, and smith-custody inventory views.

Equipped inventory stacks remain separate from unequipped stacks. Merchant
purchases are always added to an unequipped stack (or a new stack), and the UI
does not offer transfer or sale controls for an equipped stack.

The backpack action beneath the active character portrait opens the inventory
discard view. Discarding follows the same draft-first interaction as trading:
the player stages quantities into the left-side **Discard** list, may cancel the
draft, and must press **Discard** before the server removes anything. Equipped
items are never eligible for deletion.
