This page only covers controls exclusively relating to movement, attacking, and blocking/dodging. Hotkeys are described on the [slots](Slots) page, and menus are described in their respective pages:
* [Travel screen](Travel)
* [Inventory](Inventory)
* [Character select](Character)
* [Stats](Stats)
# Goals
In order of importance:
## Unambiguous
Avoid surprising "context-sensitive" inputs, specifically context that depends on analog state like your character's position or what you're looking at.
## Comprehensive
You should be able to make your character do anything that they could physically do which would be situationally advantageous. There are situations where you might want to go prone, but dedicated yoga position buttons are not a priority.
## Immediate
There shouldn't be any invisible timers delaying the start of animations to distinguish between holds/double-taps. That doesn't mean that there can't be any timers, necessarily, because you can have two time-differentiated inputs that share the same starting animation. For example, jump and crouch animations begin the same, so its safe to use a hold/double-tap to differentiate.
## Convenient
Buttons that you press often should be near your fingers. Buttons that you press either all the time, like grabbing an item, or that need to be able to be pressed *immediately*, like a dodge button, should not use the same fingers needed to look and move. Therefore, on a controller, grab and dodge can't use the thumbs.
## Intuitive
All else being equal, its nice if the controls are intuitive to learn, but not a priority. In particular, we would rather follow operating system conventions over videogame conventions, especially for menus and any RTS-like controls. To the extent that controls *are* complex, it would be nice if the complexity were optional, especially at the character-level. For example, playing as an alchemist with a bandolier of different potions and doodads might require you to use more buttons than a naked barbarian with a big stick, and to start out players are encouraged to play such characters.
# Direct
## Camera
The direct controls are designed primarily for first-person, but we can later add a third-person camera option after the MVP.

> I assume that first person is easier because you need to handle a lot of edge cases to make third person cameras not awkward in tight spaces or with thin obstacles like trees, not to mention smoothing it out or how to reconcile the shoulder offset when aiming. But we should implement whatever is easier for the MVP.
## Map
* Left stick/WASD move
* Right stick/mouse to look
* Release Space/Full-LT - dodge/jump
	* A quick tap is more like a hop or a dash. If the button is held for a bit before releasing makes it a full-body jump. You can tell whether you have held long enough for a full jump by the state of your character's animation.
	* LT has to be held all the way, then released, to jump.
* Shift/partial-LT - crouch/duck
	* When crouching and attacked, your character automatically ducks in an appropriate direction, but only if you were not already crouching before the attack began.
	* LT is only held partially here, therefore un-crouching does not cause you to jump 
* L-stick-click/ctrl - prone/stand up
	* Jump while held to dive
	* Crouch/duck to orient yourself in the direction that the camera is facing
	* Once prone/supine, jump causes you to roll if in a lateral direction, scamper otherwise. This is a very mediocre dodge
	* When prone, your character automatically switches to supine based on how you look around. Like if you're looking forward you're prone, if you look in the direction of your toes now you're supine
* R-stick-click/scroll - aim
	* Scrolling is an idempotent aiming input: up puts you in aim, down back to normal
	* Default aim state when equipping something is off.
	* For melee weapons, aiming means stabbing while not aiming is swinging
	* For ranged weapons, aiming means shooting while not aiming is bashing
	* Aiming while pressing a [hand button](Slots) with an item in your hand causes you to throw, when not aiming you simply drop it.
# Indirect
You can relinquish direct control of your character to an AI and direct them with RTS-like controls. This would generally be used to navigate large boring areas, order around multiple characters, or simply because you don't enjoy action games.

We *may* have some version of this in the MVP if only because much of the underlying behavior is shared with how NPCs are controlled by the server. You are essentially giving NPCs orders through the same system that the AI system uses to give them orders.

These controls do give an idea how they might work when controlling an army with a recursive chain-of-command, but we aren't actually doing this for the MVP. Its just here as aspiration or in  case whoever is implementing it is a passionate RTS/RTT enthusiast. The recursive nature of it should mean that the controls still make sense with small parties.
## Map
* RT/RMB - move to
	* While in combat, characters automatically attack enemies in range of their own volition, so moving to an enemy is just attacking them in melee
* RB/LMB - select
	* When pointing at a character, selects them
	* Hold and drag to box select
	* You can select characters controlled by other players and give them orders, this won't take control of their characters but they do see whatever you ordered them
* Left stick/MMB+mouse - Rotate camera
* Right stick/WASD - pan camera
* Hold left stick click+stick up or down/scroll up or down - camera Y level
* LB+RB|group/ctrl+LMB|group - select multiple
	* Everything you select is added to your selection until LB/ctrl is released
	* Press a [group](Slots) button to select whichever party member that you have bound to that slot. You can select multiple before releasing.
		* Pressing an *empty* group button binds the last selected character to that group
	* If you select another character *twice*, it switches to *their* character's slot menu. In a hierarchy, they are forced to have their commander on the same slot that their commander has themselves. Thus, you can use this to traverse the tree without having line of sight to everyone.
		* This is a bit awkward, but you shouldn't normally be skipping the chain of command anyway.
	* When LB/ctrl is held, you see all slotted party members with [incapacitation](Combat) wheels around their portraits. Each portrait is placed on its respective [group](Slots). When a specific character is selected, you see a more detailed summary
		* In a hierarchy, these summaries include aggregated information about everyone under their command. They also get two incapacitation wheels, the outer one is the average for everyone under their command.
		* Ammunition (if ranged)
		* Morale
		* Encumbrance
		* Value of loot
* Shift+LMB - Select everything between current selection and new selection
* LT+RT/Shift+RMB - Place waypoint
* LT+RB/Space+RMB - ping
* LT+click left stick - swoop camera to selected
# Universal
These buttons are not available when a grab or select button is held due to overlapping with slots.

> I have not thought too hard about their mapping, they should be remapped so that the most common buttons to press are the most convenient to move your finger to

* toggle [inventory](Inventory) menu
* toggle logout/[character](Character) menu
* toggle [quest](Quest) menu
* toggle [rest](Rest) menu
* toggle direct/indirect control
* toggle camera
	* Direct - First/third person camera switch
	* Indirect - Tactical/strategic switch
		* Tactical - More like an RTS
		* Strategic - More like a GSG 
			* Similar to the map in Mount & Blade, you see a banner for your party and enemy parties.
			* There is a fog of war, with places that you've been but cannot see being greyed out and places you have not been styled like a more abstract paper map.
* Skip [time](Time)
	* Normally this toggled between real-world time and [sim-time](Time). When camped, it skips straight to the end of your rest. However, this is automatically interrupted if the party spots an enemy or encounters difficult terrain.
	* In the strategic camera you continue to see the map at a consistent speed. In the tactical or direct camera, we can display a cinematic montage of travel or night passing (not in MVP).
# Direct combat Input Terms
This is a hybrid system between a real action game and something that has merely the illusion of an action game. Essentially, we aren't *actually* simulating everything based on hitboxes and projectile trajectories. But we do still want to utilize some of the player's mechanical skills, specifically their reaction time and accuracy. This is trivial to [cheat](Networking), but since combat is still largely based on stats and mediated entirely by the server this isn't a huge deal.

When controlled by NPCs or indirect mode, these are randomized server-side in a standard distribution around arbitrary average values that we pick... [usually](Magic)...
## Precision
is the value from 0 to 1 representing how close the player placed the aiming reticle on the center of any of the target's hitboxes. Its exact formula will have to come about through testing. 
## Reflex
Is the value from 0 to 1 representing how quickly the defender pressed the dodge/parry button after the attack began. Like with precision, we aren't sure exactly what this will be but a value of 1.0 would correspond to pro-gamer reaction time (0.1s) and ~0.75 would correspond to old person reaction time (0.25s).

There is no need to "time" your input to correspond with when an attack will actually hit, as is convention in most action games. As soon as an enemy begins their attack animation you should press the button.