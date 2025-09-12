# Grab
**LB+RB / MMB+RMB**
* When there is nothing in a hand, pressing grab will grab whatever you're looking at
* When there is something in your hand, pressing grab drops it
* When there is nothing in your hand, holding grab then pressing a "slot" button equips whatever is in that slot into that hand
* When there is something in your hand, holding grab then pressing a slot button places whatever its holding into that slot
# Slots
A "slot" is the combination of a shortcut and a physical location on your character's body that an item can be stored or attached to. 
For example, a slot can be your right hip
* If you have a belt on, you can place a sheath on your left hip (X)
* If you have a sheath on your left hip, you can put a sword in it
The goal is to avoid the need to navigate menus for most of your inventory. Anything not inside of a bag should be immediately accessible via a slot button
Slot buttons can overlap with other buttons, because they only function as slot buttons when a grab button is held
## Layers
When you press a slot button once, it selects the outer layer of that slot. If you press it again without releasing the grab button, it will select one layer deeper. For example, press it once to draw your sword from your sheath, twice to remove the sheath itself, and three times to remove your belt.
## Multi-slot items
Many items, generally clothing and armor occupy multiple slots. A belt occupies all four belt slot buttons. It can be equipped and removed using any of these buttons.
## Slot restrictions
Many items can only occupy specific slots. When such an item is held in your hand and you hold the corresponding grab button, those buttons are not available.
## Map
To make them more intuitive, the location of a given slot button on your controller/keyboard should somewhat correspond to its physical location. A button on the left should be used for a slot on the left side of your body, and we should attempt to group them so that adjacent slots have adjacent buttons.

On the controller, there aren't quit enough buttons to give every slot its own button. But the D-pad slots are laid out in a navigable grid. So for example, face and glasses can both be D-pad up, the former you press once and the latter twice all while holding RB. Only when you release the grab button does it actually perform the action

This would also be needed for controllers that only support 4 d-pad directions, the diagonals can just be pressing two directions in either order

Pressing a slot 
* X/q - Left belt
* B/e - Right belt
* Y/f - front belt
* A/x - back belt
* Select/Tab - left shoulder
* Start/r - right shoulder
* D-pad left/2 - left pocket
* D-pad right/3 - right pocket
* D-pad down-left/1 - back-left pocket
* D-pad down-right/4 - back-right pocket
* D-pad up/t - head
* D-pad down/? - face?
* D-pad up-left/\` - left arm
* D-pad up-right/5 - right arm
* ? - glasses?
* ? - ears?
* D-pad down-down-left/z - left foot
* D-pad down-down-right/c - right foot
## GUI
Normally, there is no indicator on the screen of what is in your slots or hands. However, when you hold down a grab button you will see a map of all your slots, which includes icons for their buttons. This map approximately corresponds to your controller/keyboard, the relative position of each slot should be based on the relative position of each button.

When holding an item, any slot that it can be placed in is white, otherwise they are greyed out. If your hand is empty, then slots that have items in them are white, while empty ones are greyed out.

Each layer of item in a slot is visible in this interface. Layers for items that occupy multiple slots also contiguously span all relevant slots.
# Bags
Your entire inventory will not necessarily fit onto this system. For items that you do not need readily accessible, you can put them in a bag. These still occupy a slot, but can hold multiple items. Generally this would be a backpack, which is slung over your shoulder(s). To access the items in your backpack, you must grab it into one of your hands. When you are holding a bag, the grab button for the hand opposite of the hand that's holding the bag is used to grab/place into it. The hand that is holding it functions normally, meaning that if you simply press it then you will drop the bag and if you hold it and press a slot then it will put the bag in that slot.
# Alternative Controls
The goal of the slot system is to make most aspects of inventory management more painless, as it usually skips the need for menus. I think that it will be hard to learn, but ultimately speed up gameplay for experienced users because its extremely consistent. But if this is not the case, we can also try more of a middle-ground with conventional systems. Essentially, all slot buttons simply become hotkeys that have nothing to do with a specific physical location on your body. You still place items onto these hotkeys to equip them, but it doesn't matter which button you press. This would make layering and multi-slot buttons a mess, so clothing and armor would just have to be done through a normal inventory menu. As for sheaths and holsters, these would be handled like clothing in that you'd have to equip them from a menu, and you would simply be forbidden from hotkeying a weapon for it if you have no sheath to put it in.