# Trade

For the current strategic prototype, every merchant exposes the same unlimited
catalogue. Items have a base gold value; merchant buy and sell prices are
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
Removing a staged purchase before offering it simply cancels that purchase;
it does not create a sale or apply a merchant fee.
The centered action bar above chat appears only for a pending exchange and
includes **Offer** and **Cancel** controls; Cancel discards the entire draft.

Equipped inventory stacks remain separate from unequipped stacks. Merchant
purchases are always added to an unequipped stack (or a new stack), and the UI
does not offer transfer or sale controls for an equipped stack.
