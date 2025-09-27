This page only covers controls exclusively relating to movement, attacking, and blocking/dodging. Hotkeys are described on the [slots](Slots) page, and menus are described in their respective pages:
* [Travel screen](Travel)
* [Inventory](Inventory)
* [Character select](Character)
* [Stats](Stats)
## Direct
### Camera
The direct controls are designed primarily for first-person, but we can later add a third-person camera option after the MVP. In fact, we aren't especially opinionated about which to use for the MVP, so if third person seems easier we can do that first instead.
> I assume that first person is easier because you need to handle a lot of edge cases to make third person cameras not awkward in tight spaces or with thin obstacles like trees, not to mention smoothing it out or how to reconcile the shoulder offset when aiming.
### Map
* Left stick/WASD move
* Right stick/mouse to look
* Release Space/Full-LT - dodge/jump
	* A quick tap is more like a hop or a dash. If the button is held for a bit before releasing makes it a full-body jump. You can tell whether you have held long enough for a full jump by the state of your character's animation.
	* LT has to be held all the way, then released, to jump.
* Shift/partial-LT - crouch/duck
	* When crouching and attacked, your character automatically ducks in an appropriate direction, but only if you were not already crouching before the attack began.
	* LT is only held partially here, therefore un-crouching does not cause you to jump 
* Ctrl/L - prone/stand up
	* Jump while held to dive
	* Crouch/duck to orient yourself in the direction that the camera is facing
	* Once prone/supine, jump causes you to roll if in a lateral direction, scamper otherwise. This is a very mediocre dodge
	* When prone, your character automatically switches to supine based on how you look around. Like if you're looking forward you're prone, if you look in the direction of your toes now you're supine
* Scroll/R - aim
	* Scrolling is an idempotent aiming input: up puts you in aim, down back to normal
	* Default aim state when equipping something is off.
	* For melee weapons, aiming means stabbing while not aiming is swinging
	* For ranged weapons, aiming means shooting while not aiming is bashing
	* Aiming while pressing a [hand button](Slots) with an item in your hand causes you to throw, when not aiming you simply drop it.
## Indirect (not necessarily in MVP)
You can relinquish detailed control of your character to an AI and direct them with RTS-like controls. This would generally be used to navigate large boring areas, order around multiple characters, or simply because you don't enjoy action games.
We *may* have some version of this in the MVP if only because much of the underlying behavior is shared with how NPCs are controlled by the server. You are essentially giving NPCs orders through the same system that the AI system gives them orders.

This doesn't have to work especially well on a controller, it can mainly be intended for mouse and keyboard.
### Map
* RMB - move to
	* While in combat your character should attack enemies in range of their own volition, so moving to an enemy is just attacking them in melee
* LMB - select
	* Hold to box select
	* Basically irrelevant if you only control one character...
* MMB - hold to rotate camera
* WASD - pan camera
# Combat Input Terms
This is a hybrid system between a real action game and something that has merely the illusion of an action game. Essentially, we aren't *actually* simulating everything based on hitboxes and projectile trajectories. But we do still want to utilize some of the player's mechanical skills, specifically their reaction time and accuracy. This is trivial to [cheat](Networking), but since combat is still largely based on stats and mediated entirely by the server this isn't a huge deal.

When controlled by NPCs or indirect mode, these are randomized server-side in a standard distribution around arbitrary average values that we pick... [usually](Magic)...
## Precision
is the value from 0 to 1 representing how close the player placed the aiming reticle on the center of any of the target's hitboxes. Its exact formula will have to come about through testing. 
## Reflex
Is the value from 0 to 1 representing how quickly the defender pressed the dodge/parry button after the attack began. Like with precision, we aren't sure exactly what this will be but a value of 1.0 would correspond to pro-gamer reaction time (0.1s) and ~0.75 would correspond to old person reaction time (0.25s).

There is no need to "time" your input to correspond with when an attack will actually hit, as is convention in most action games. As soon as an enemy begins their attack animation you should press the button.