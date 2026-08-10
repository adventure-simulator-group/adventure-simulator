# Slots
A **slot** is a physical location on your character's body which an item can be stored in, or attached to, for quick access via hotkey.

The persisted equipment model separates this fine-grained topology from the
seven combat/health body parts. Equipment locations include head, face, neck,
chest, stomach, back, paired shoulders, arms, hands, legs, and feet. The typed
vocabulary also reserves the four belt and four pocket access points below.
Combat continues to target only left/right arm, left/right leg, head, chest,
and stomach; worn protection is projected back onto those stable regions.

For example, the right hip can be a slot:
* If you have a belt on, you can place a sheath on your left hip with <kbd>Q</kbd>.
* If you have a sheath on your left hip, you can put a sword in it.

The goal is for slots to replace menus to access most of your inventory. Anything not inside a bag should be immediately accessible with a **slot button**. Slot buttons can overlap with other buttons because they only function as slot buttons when a [grab button](controls.md#direct-controls) is held.
## Keybindings
Ideally, the location of each slot button should correspond roughly to the slot's physical location. A button on the left should be used for a slot on the left side of the body, and we should attempt to group the buttons so that adjacent slots have adjacent buttons.

| **M+KB** | **Controller** | **Slot** |
|-|-|-|
| <kbd>Q</kbd> | X | Left belt.
| <kbd>E</kbd> | Q | Right belt.
| <kbd>F</kbd> | Y | Front belt.
| <kbd>X</kbd> | A | Back belt.
| <kbd>Tab</kbd> | Select | Left shoulder.
| <kbd>R</kbd> | Start | Right shoulder.
| <kbd>G</kbd> | | Chest.
| <kbd>Y</kbd> | | Stomach.
| <kbd>H</kbd> | | Back.
| <kbd>2</kbd> | ⇐ | Left pocket.
| <kbd>3</kbd> | ⇒ | Right pocket.
| <kbd>1</kbd> | ⇓⇐ | Back-left pocket.
| <kbd>4</kbd> | ⇓⇒ | Back-right pocket.
| <kbd>T</kbd> | ⇑ | Head.
| | ⇓ | Face?
| <kbd>`</kbd> | ⇑⇐ | Left arm.
| <kbd>5</kbd> | ⇑⇒ | Right arm.
| | | Glasses?
| | | Ears?
| <kbd>V</kbd> | | Left leg.
| <kbd>B</kbd> | | Right leg.
| <kbd>Z</kbd> | ⇓⇓⇐ | Left foot.
| <kbd>C</kbd> | ⇓⇓⇒ | Right foot.

The body-slot map deliberately leaves <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd>
unbound so tactical movement remains available while donning, doffing, or
accessing equipment.

The controller doesn't have quite enough buttons to give every slot its own button. To get around this, we can assign certain slots to *combinations* of buttons; because slot inputs require the grab button be held to initiate them, and we don't execute any action until the grab button is released, no ambiguity is possible. (For instance, while holding the grab button, the face slot can be ⇑ and glasses can be ⇑⇑; only when we release the grab button does it actually perform the action.) This is also helpful for controllers that only support four D-pad directions; the diagonals can just be pressing two directions in either order. (That is, "down-left" can be "down and then left.")

In the map proposed above, we rely on button combinations for directly adjacent slots, which we imagine as lying on a navigable grid navigated by the D-pad.
## Layers
Pressing a slot button once selects the outer layer of that slot. Pressing it again -- without releasing the grab button -- selects one layer deeper. For example, press <kbd>Q</kbd> once to draw your sword from your sheath, twice to remove the sheath itself, and three times to remove your belt.

Equipment uses explicit occupancy channels ordered from inside to outside:
held, base clothing, padding/under-armor, flexible armor, rigid armor,
outerwear, accessory, mount, and containment. Channel plus authored order
forms an occupancy cell, so a cloak can coexist with armor and a sheath can
coexist with clothing. Held, base-clothing, padding, flexible-armor,
rigid-armor, and outerwear channels are singleton at each location; authored
order only distinguishes repeatable accessory, mount, and containment cells.
The same item may atomically occupy several locations, and sided garments
author explicit left and right alternatives.

The persisted equipment graph is rooted at the character body. Equipped items
may provide ordered, capacity-limited attachment points, so a belt may parent a
sheath or bag, a sheath may parent a weapon, and a bag may parent contents. A
single placement may require several points (the catalog sword sheath uses two
belt mounts), producing a DAG rather than a single-parent tree.
Reparenting validates the complete destination before mutation. Player-facing
removal and reparenting reject items with children, preventing orphaned graph
rows. Repeated slot input walks body channels outside-to-inside and then child
attachment points in authored order.
## Multi-slot items
Many items, generally clothing and armor, occupy multiple slots. A belt occupies all four belt slot buttons. It can be equipped and removed using any of these buttons.
## Slot restrictions
Many items may only occupy specific slots. When such an item is held in your hand and you hold the corresponding grab button, all buttons not corresponding to those slots are unavailable.
## GUI
The screen normally gives no indicator for what is in your slots or your hands. However, holding down any grab button brings up a "map" of your slots with a few properties:
* This map includes icons for each button and approximately corresponds to the keyboard/controller; the relative position of each slot should be based on the relative position of each button.
* When holding an item, any slot it may be placed in is white, and all others are grayed out; if your hand is empty, slots with items in them are white, and empty ones are greyed out.
* Each layer of item in a slot is visible in this interface. Layers for items that occupy multiple slots contiguously span all relevant slots.

The strategic inventory uses a compact version of this map. An equipped row
shows every applicable QWERTY key; lighter key text is nearer the surface and
darker text is farther underneath. Clicking an equipped row removes it
directly. Clicking an unequipped row opens the QWERTY map. Invalid keys are
dimmed and flash red when pressed. Eligible occupied slots show their current
item icon and swap that occupant out when selected; eligible empty slots keep
the icon area as empty negative space. A valid key can be clicked or pressed,
and selects the outermost compatible placement or attachment target reachable
through that slot. Hovering over any equippable row control previews the map
without a modal backdrop and accepts a slot key immediately, including moving
an already equipped item to another compatible placement. Reaching the control
with keyboard Tab navigation shows the same preview while leaving Tab
navigation active; press Space to open the modal, then choose a slot key or
press Escape to close it. An equipped item's current placement is highlighted
and carries its item icon in either map. Clicking an equipped control still
unequips it.

## Tactical implementation

In direct tactical control, holding LMB opens the right-hand egui map and
holding MMB opens the left-hand map. RMB-held aim/attack takes precedence and
prevents a new grab. If both grab buttons compete, the first active grab owns
the interaction until release. Slot input is preview-only while held; one
ordered request commits on release. Repeating a key walks that key's authored
location alternatives and then deeper layers deterministically. WASD is never
consumed by the map.

An empty hand may draw an occupied reachable layer. A full hand may place into
a compatible empty destination or atomically swap an occupied destination
into the hand. The HUD dims invalid choices and flashes rejected input without
sending a mutation. Releasing without a selection drops a held item; releasing
an empty hand is a no-op. The opposite-hand and pointed-scene destinations are
explicit HUD choices.

The tactical server re-resolves every selection against mapped ECS entities
and validates control, action sequence/revision, expected source/destination,
compatibility, every parent requirement, attachment tags/capacity, cycles,
children, and pickup range/line of sight before applying a batch. Ordered DAG
traversal exposes occupied children and empty authored attachment capacities;
multi-parent placement reserves every required edge atomically. These
changes exist only in the mission ECS snapshot and never replace the durable
strategic equipment graph.

The HUD renders each reachable layer outside-to-inside and then through child
attachment points. Near layers use lighter labels, the chosen depth is
highlighted, catalog Game Icons are drawn from a deterministic tactical sprite
atlas, and contiguous mapped cells are connected into one visual span for
multi-location equipment. Occupied destinations state that their item will be
swapped into the active hand.

## Bags
Your entire inventory won't necessarily fit into the slot system, which is fine. The slot system is intended not to replace "standard inventory management" altogether but to make a *significant subset* of your inventory more manageable, that being the subset of items that you need readily accessible. If you don't need a given item readily accessible, you can put it in a bag.

A bag still occupies a slot, but it can hold multiple items. For example, a backpack is a bag which is slung over your shoulder(s).[^1] To access a bag's internal contents, you must grab the bag into one of your hands; when you are holding the bag, the grab button for the hand opposite the hand holding the bag is used to grab/place into it, and the hand holding the bag functions normally: if you simply press the associated grab button, you will drop the bag, and if you hold it and press a slot, you will place the bag in that slot.

[^1]: In the real world, carrying a backpack on one shoulder can lead to strain, pain, and posture problems. It is always recommended to use both shoulder straps.
## Alternative controls
The goal of the slot system is to obviate the need for menus in inventory management, for the most part, and thereby simplify most aspects of inventory management. Following the philosophy laid out in the [Controls](controls.md) page, it will likely be hard to learn but ultimately speed up gameplay for experienced users on account of its consistency and unambiguity.

If this is not the case, we can also try more of a middle ground with conventional systems. For instance, we could turn all slot buttons into hotkeys untethered to any physical locations on the body. Players would still place items onto these hotkeys to equip them, but it wouldn't matter which button they pressed. This would make layering and multi-slot buttons a mess, so clothing and armor would just have to be done through a normal inventory menu. Sheaths and holsters would be handled like clothing; players would have to equip them from a menu and be forbidden from hotkeying a weapon without having equipped a sheath to put it in.
