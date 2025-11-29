# Management
Inventory management should NOT be a full-time job. When players come home from laboring at the spreadsheet mines all day they should not *have* to toil more in what is ostensibly their reprieve from such work. But this is adventure _simulator_, not adventure _handwaver_, so we need to use the interface to abstract over all of the things that normally make inventory management tedious while still preserving the underlying depth.

## Automation
Most aspects of resupplying, looting, selling, and stocking rations for an upcoming adventure can be effectively automated.

### Resupplying
Players can define "sets" for their character. This is their weapons, armor, ammunition, potions, and tools (like pack, rope, torch, firestarting kit, and bedroll) which they expect to have on them when they set out. When in a trade menu, you can press a "resupply" button to automatically purchase any equipment that you need for your currently selected "set" that you don't already have. The button can be blue, and any items which _would_ be purchased, were you to press it, can be highlighted as blue.

### Looting and Selling
A party can configure a weight limit using a slider, which shows them what the total party [travel](../strategic/Travel.md) speed would be at a given limit presuming that the load is optimally distributed (such that all characters can maintain the same pace). When a tactical simulation ends, the party sees a screen that displays all of the loot available to collect. They _could_ loot each individual item and decide whose inventories they are going into, _or_ they could just press the "autoloot" button which will loot items in order of their value to weight ratio until the weight limit is reached. Like with resupplying, you can anticipate this behavior because any items to be looted will have a gold highlight which matches the gold loot button. When in a town, the loot button becomes a sell button, and any items not in any of your equipment sets will be sold.

Some items might also automatically be flagged as "keep" (perhaps a checkbox next to them in the inventory menu), such as goblin ears if you're on a quest to kill goblins and this is the required trophy to turn in for the reward.

### Storage
Any items which _are_ in your equipment sets, but not in the currently configured one (or which you have a surplus of, for example if you have 50 arrows and your set only calls for 30), can be automatically deposited in storage with a grey storage button.

### Rations
The trade/loot page can also have a red button+highlight for rations. When pressed, you will purchase however many rations would be required for the currently planned journey. It may also have a slider to increase/decrease the number of expected days in case you want some safety margin or to eat something at your destination (warning: orc and goblin meat is nasty and unsanitary).
