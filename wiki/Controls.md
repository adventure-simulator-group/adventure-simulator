## Direct
* Left stick/WASD move
* Right stick/mouse to look
* You should be very precise about exactly what your character does and how they do it. Which hand they use, what pocket they put something in.
* We want the detail that SS13 has but to otherwise try and make it fairly convenient and intuitive.
* Intuitiveness is the lowest priority though, the controls should be optimized for a player that has learned them well so that they can press buttons on reflex rather than having to keep track of context-sensitive actions and state
* Try and map each input directly to animations for the sake of feedback.
	* The animation should start as soon as you press the button, not just when you release it or after enough time to know whether you're trying to do a double-tap input
	* If multiple actions have animations that are similar in the beginning then they can be mapped to the same button as long-press/double tap
		* Example: jumping *could* be double-tapping a crouch button, since you have to bend your knees to both jump and crouch
* Each trigger/mouse button is a "stance"
	* A stance on its own just causes your character to put one foot ahead of the other, so left-stance is left foot forward, right foot back.
	* If you have a shield in a hand that's forward, it will also block attacks.
	* When in a stance, pressing and releasing the opposite button causes you to attack (this is called a "stay")
	* Releasing the stance button while pressing the opposite causes a different kind of attack (this is a "switch").
	* The idea is to be very precise about how movement interacts with melee. When you swing a sword diagonally in the real world you naturally try and take a step and switch stance to avoid being thrown off-balance at the end of the swing. If you want to follow-up with an additional swing, now it will be in the opposite direction because you have the opposite stance.
	* Likewise, when you stab you don't need to change stance to avoid being thrown off balance, you just yank the weapon back at the end of the attack. You might still lunge forward to put more weight into it, but your feet end up at the same relative position to each other.
	* This system is also supposed to represent the way you aim and shoot a ranged weapon. When you shoot a rifle or bow, your off-hand and off-foot are forward, in the same position they would be if you were defending with a shield in your off-hand.
	* Mouse and controller inputs are probably mirrored for this, so LT is analogous to RMB
* Space/? - jump
* Hold left stick click/shift to switch from moving to ducking. Now when you move in a given direction, your character stays still but ducks in that direction.
	* This is awkward on a controller, D-pad input can be effectively duck + move
	* Alternatively, if a stance button is held at 100% it could switch to duck mode, you'd be in stance when you duck anyways if you're in the middle of melee combat
		* This could make it awkward to use ranged weapons because now you'll have to keep the trigger partially pressed to move and aim.
		* An advantage of this is that it frees left stick for jump
	* The direction that you are ducking in changes your attack.
		* Forward turns your attack into a shove
		* Backward flips your diagonal switch attack into an uppercut.
		* Stance-side could be a jab/shield-bash/pommel-bash
		* Opposite side is a wide horizontal swing
* LB+RB / Q+E are grab buttons
	* When there is nothing in a hand, pressing grab will grab whatever you're looking at
	* When there is something in your hand, pressing grab drops it
	* When there is nothing in your hand, holding grab then pressing a "slot" button equips whatever is in that slot into that hand
	* When there is something in your hand, holding grab then pressing a slot button places whatever its holding into that slot
	* For keyboard and mouse this is awkward. You can't conveniently press E and 4, because you'd normally use the same finger for these
	* In general its best to keep face buttons/sticks equivalent to keyboard, while triggers/bumpers are equivalent to mouse buttons. But there are only three normal mouse buttons
	* We could keep track of state via the mouse wheel. If you have your weapon raised, LMB/RMB are stance buttons, if you have it lowered, they are grab buttons
	* Alternatively, the hacky solution is that Q and E can be used to drop or grab items from the *world*, but to interact with your slots you first press a slot button then press a corresponding mouse button (whereas with controller, this is the other way around)
	* Another solution is that grabbing is tapping a mouse button without the other button pressed. Stance buttons don't really do anything if they are briefly tapped, in order to actually do something from them you need to hold them down then press the opposite button. So this could maybe work. In this case, you'd still first press and hold a slot button in order to interact between your hand and slot, but you would always use mouse buttons instead of sometimes using Q and E.
* Slot buttons
	* A slot is the combination of a shortcut and a physical location on your character's body that an item can be stored or attached to
	* For example, a slot can be your right hip
	* If you have a belt on, you can place a sheath on your left hip (X)
	* If you have a sheath on your left hip, you can put a sword in it
	* The goal is to avoid the need to navigate menus for most of your inventory. Anything not inside of a bag should be immediately accessible via a slot button
	* On controller at least, slot buttons can overlap with other buttons, because they only function as slot buttons when a grab button is held
	* Map
		* (I did not think very hard about this, please rework. Just try and keep left/right/center consistent and slots that would be pressed a lot should be convenient to press without moving fingers too much)
		* D-pad left/1 - left pocket
		* X/2 - Left hip
		* B/3 - Right hip
		* D-pad right/4 - right pocket
		* Select/Tab - left shoulder
		* Start/r - right shoulder
		* Y/f - front waist
		* A/x - back waist
		* D-pad up/t - head
		* D-pad back-left/Z - back-left pants pocket
		* D-pad back-right/C - back-right pants pocket
		* D-pad down/? - face?
		* ? - glasses?
		* ? - ears?
		* ? - left foot
		* ? - right foot (D-pad down-down-right?)
		* If we run out of buttons, multiple inputs are also an option. So for example face and glasses can both be D-pad up, the former you press once and the latter twice all while holding RB. Only when you release the grab button does it actually perform the action
		* This would also be needed for controllers that only support 4 d-pad directions, the diagonals can just be pressing two directions in either order
* Prone/supine
	* This isn't just for sneaking around, but also because combat will involve getting knocked down and we don't want to just automatically play a stand-up animation for this. You should be able to (poorly) dodge and attack while prone, and even kick or pull your enemy down with you
	* On controller this is pressing left stick. If you just click it you drop down immediately, but you can do it more quietly and slowly by holding it down.
	* If you are moving fast enough and drop prone, you dive
	* When prone, your character automatically switches to supine based on how you look around. Like if you're looking forward you're prone, if you look in the direction of your toes now you're supine
	* If a stance button is held, you try to orient your body towards whatever direction you're looking in. Otherwise your orientation is fixed
	* When in stance if you move laterally your character rolls, which makes for a poor dodge
## Indirect
* You can relinquish detailed control of your character to an AI and direct them with RTS-like controls
* The purpose of this is in case you need to walk somewhere that will be tedious or you just don't like action games
	* Could also support actually controlling multiple characters in the future
* The same system used to direct your character should share most functionality with how NPCs are controlled anyways
* This doesn't have to work especially well on a controller, mainly for mouse and keyboard
* RMB - move to
	* While in combat your character should attack enemies in range of their own volition, so moving to an enemy is just attacking them in melee
* LMB - select
	* Hold to box select
	* Basically irrelevant if you only control one character...
* MMB - hold to rotate camera
* WASD - pan camera