# Controls
This page only covers controls relating to movement, attacking, and blocking/dodging. Hotkeys are described on the [slots](Slots.md) page, and menus are described in their respective pages:
* [Travel screen](Travel.md)
* [Inventory](Inventory.md)
* [Character select](Character.md)
* [Stats](Stats.md)

At this point in development, much of this page is liable to change in the near future. We assume many developers will be interested in helping design the game, so in this page, we provide first our design goals and then a proposal for a control scheme which meets those goals.

## Goals
Above all else, the game should be playable by someone who's never played a game in his life. This principle goes beyond controls and into most of the game's systems; for instance, we imagine that instead of a difficulty setting, the player can just tweak a knob to abstract away all the technical RPG stuff and controls so most of his actions are CPU-controlled and his character's stats can be generated from "I want to be a big strong guy with a warhammer." But controls are a big part of it. One consequence of this principle is that we'd rather follow operating system conventions than video game conventions wherever we can, especially for menus and any RTS-like controls. Video game conventions are designed for people who've played a lot of video games; OS conventions are designed for people.

Those are some important notes to keep in mind, but they're more *philosophy* than *goals*, which would belie the header we've given this section. More concretely, then, goals: in order of importance, we want controls which are unambiguous, comprhensive, immediate, convenient, and intuitive.

### Unambiguous
We are opposed to "context-sensitive actions" where the response to user input depends on the state of the game, particularly when the state is something continuous like your character's position or what you're looking at.

![A screenshot of *HITMAN 2* (2018) illustrating the game's... colorful use of context-sensitive actions. I say this as a fan.](https://i.redd.it/w3mwmybfsrl21.jpg "I couldn't find the gif of Agent 47 on a balcony trying to press E to dump a body in a container and instead pressing E to dump a body over the railing into the crowd below. This will make do.")

Certainly context-sensitive actions can make your controls "simpler" in the sense that you're using fewer buttons, but it'll also make them a *lot* more cumbersome than they have to be, and your player may end up quite surprised by what his character ends up doing in response to a given input. There are games with "simple" controls, enabled by context-sensitive actions, where no matter how much you play them, you can't perform these actions on instinct because you must always ensure the context is appropriate for them. That's not to mention anything with menus, which are worse for this on an entirely different level.

None of this for us! [*Space Station 13*](https://spacestation13.com/)'s controls are [nowhere close to ideal](https://paradisestation.org/wiki/images/0/04/Keyboard-layout-complete.png), but what they have going for them is that once you get *used* to the controls, they're very good at becoming instinct. This is a quality worth replicating.

### Comprehensive
You should be able to make your character do anything he or she could physically do which would be situationally advantageous.[^1]

[^1]: That's an important caveat. There are situations where you might want to go prone, but dedicated yoga position buttons are not a priority. (This may sound like an argument for context-sensitive actions, and in some cases it may be; as stated above, our primary opposition is to *continuous-state* context-sensitive actions, which leaves things open for contexts depending on discrete states, say *in yoga class* vs. *not in yoga class*.)

### Immediate
In cases where a button does one thing if single-tapped but something else if double-tapped or held, some games will start an invisible timer after a first tap is registered, delaying the onset of the following animation until the player's intention is made clear (with a second tap/hold or lack thereof).

We will not be one of those games. It's a fair enough solution, but we want immediate feedback for the player, and we don't want any invisible timers to delay post-tap animation starts solely for the sake of single-/double-tap differentiation. Note this doesn't preclude *all* timers; we may certainly have two time-differentiated inputs which share the same starting animation. For instance, since jump and crouch animations begin the same way, it would be safe to use a hold or double-tap to differentiate the two.

### Convenient
Buttons that you press often should be near your fingers. Buttons that you press often, like to grab an item, or need to be able to press *immediately*, like to dodge, should not use the same fingers as those needed to look and move.[^2]

[^2]: Thus, grabbing and dodging shouldn't use the thumbs on a controller.

### Intuitive
All else being equal, it'll be nice if the game's controls are intuitive and easy to learn, but this isn't a priority.

To the extent that the game's controls *are* complex, it'll be great if the complexity is optional, especially at the character level. Playing as an alchemist with a bandolier of different potions and doodads might require you to use more buttons than a naked barbarian with a big stick, and starter players may be encouraged to play characters more like the latter.

## Proposal
With our design goals established, we now suggest a tentative outline for the control scheme.

One feature we quite want in the game eventually is a split between *direct* and *indirect* controls. By default, the player has direct control of his character, and the game plays like an action game, but with a toggle, the player may relinquish control to an AI and direct it with RTS-like controls. You'd generally use this feature to navigate large, boring areas, to order around multiple characters, or simply because you don't like action games.

We posit control schemes for both modes below.

### Direct controls
These are designed primarily for the first person, but we can add a third-person camera option after the MVP.

> Halbe: I assume that first person is easier because with third-person cameras, you need to handle a lot of edge cases to avoid awkwardness in tight spaces or near thin obstacles like trees, not to mention smoothing out the motion or reconciling the shoulder offset when aiming. However, if I am mistaken in my assumptions, we should implement whichever is easier for the MVP.

| **M+KB** | **Controller** | **Function** | **Notes** |
|-|-|-|-|
| <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> | Left Stick | Movement. |
| Mouse | Right Stick | Look. |
| LMB | RT | Attack!
| Release <kbd>SPACE</kbd> | Full LT | Dodge or jump. | <ul><li>A quick tap is more like a hop or dash. Holding the button for a bit before releasing makes it a full-body jump. You can tell whether you have held long enough for a full jump by the state of your character's animation.<li>LT has to be held all the way, then released, to jump.
| <kbd>SHIFT</kbd> | Partial LT | Crouch or duck. | <ul><li>When crouching and attacked, your character automatically ducks in an appropriate direction, but only if you were not crouching before the attack began.<li>LT is only held partially here, so releasing it to un-crouch does not cause you to jump.
| <kbd>CTRL</kbd> | Left Stick Click | Prone–standing toggle. | <ul><li>Jump while held to dive.<li>Crouch to orient your character in the direction the camera is facing.<li>When prone, your character automatically switches to supine based on how you look around. Looking forward, you're prone; looking at your toes, you're supine.<li>Once prone/supine, jump causes you to roll if in a lateral direction, scamper otherwise. This is a very mediocre dodge.
| Scroll | Right Stick Click | Aim. | <ul><li>Scrolling is an idempotent aiming input: up puts you in aim, down back to normal.<li>Default aim state when equipping something is off.<li>For melee weapons, stabbing without aiming = swinging.<li>For ranged weapons, shooting without aiming = bashing.<li>Aiming while pressing a [hand button](Slots.md) with an item in your hand causes you to throw. When not aiming, you simply drop it.

This is somewhere between a real action game and an RPG wearing an action game's skin. We aren't actually simulating everything based on hitboxes and projectile trajectories, but we still want to use some of the player's mechanical skills, specifically accuracy and reaction time.[^3]

[^3]: This is trivial to [cheat](Networking), but since combat is still (largely) based on stats and (entirely) mediated by the server, it's not a huge deal.

* **Precision** is the value between 0 and 1 representing how close to the center of any of the target's hitboxes the player placed the aiming reticle. The exact formula for this number will have to come about through testing.
* **Reflex** is the value between 0 and 1 representing how quickly the defender pressed the dodge/parry button after the attack began. Like with precision, we aren't sure exactly how to derive this, but a value of 1.0 would correspond to pro gamer reaction time (0.1s) and ~0.75 would correspond to old person reaction time (0.25s).
  * There is no need to "time" your input to correspond with when an attack will actually hit as is convention in most action games. As soon as an enemy begins its attack animation, you should press the button.

For CPU-controlled characters (NPCs or [indirect mode](#indirect-controls)), the server... [usually](Magic.md)... randomly samples these parameters from a normal distribution with some mean and variance of our choice.

### Indirect controls
We *may* have some version of this concept in the MVP if only because much of the underlying behavior is shared with NPCs controlled by the server. You are essentially giving NPCs orders through the same system that the AI uses to give them orders.

The following outline gives an idea of how things might work when controlling an army with a recursive chain of command, but as we aren't actually doing any RTS stuff for the MVP, it's more here as a distant if (hopefully) attainable aspiration, or in case an implementer is a passionate RTS/RTT enthusiast. The recursive nature of the system means that the controls should still make sense for small parties or individuals.

| **M+KB** | **Controller** | **Function** | **Notes** |
|-|-|-|-|
| RMB | RT | Move to. | While in combat, characters automatically attack enemies in range, so moving to an enemy is just attacking it in melee.
| LMB | RB | Select. | <ul><li>When pointing at a character, select it. <li>Hold and drag to box select. <li>You can select characters controlled by other players and give them orders. This won't take control of their characters, but the players will see whatever you ordered them.
| <kbd>CTRL</kbd>+LMB/[`Group`](Slots.md) | LB+RB/[`Group`](Slots.md) | Select multiple. | <ul><li>Everything you select is added to your selection until the key is released. <li>Press a [group](Slots.md) button to select whichever party member you have bound to that slot. You can select multiple before releasing. Pressing an *empty* group button binds the last selected character to that group. <li>If you select a character *twice*, you switch to that character's slot menu. In a hierarchy, a character is forced to have his commander in the same slot that the commander has *him*. Thus, you can use this to traverse the tree without having line of sight to everyone.[^4] <li>When <kbd>CTRL</kbd>/LB is held, you see all slotted party members, each with an [incapacitation wheel](Combat.md) around its portrait. When a specific character is selected, you see a more detailed summary. <ul><li>In a hierarchy, a selected character's summary includes aggregated information about everyone under his command, and the selected character gets two incapacitation wheels, the outer one being the average for everyone under his command. <li>The stats tracked might include the following: <ul><li>Morale <li>Encumbrance <li>Value of loot <li>Encumbrance (if ranged)
| MMB+Mouse | Left Stick | Rotate camera.
| <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> | Right Stick | Pan camera.
| Scroll | Hold Left Stick Click + Left Stick Up/Down | Adjust camera Y-level.
| <kbd>SHIFT</kbd>+LMB | | Select everything between current selection and new selection.
| <kbd>SHIFT</kbd>+RMB | LT+RT | Place waypoint.
| <kbd>SPACE</kbd>+RMB | LT+RB | Ping.
| | LT+Left Stick Click | Swoop camera to selected character.

[^4]: This is a bit awkward, but you shouldn't normally be skipping the chain of command anyway.
### Universal
These inputs would theoretically find use in both direct and indirect modes.

> Halbe: I have not thought too hard about their mapping. They should be remapped so that the most commonly pressed buttons are the most convenient to move your finger to.
>
> Bruno: It'll likely be the case that these inputs are unavailable when a grab or select button is held due to overlapping with [slots](Slots.md). When you hold RB, X is a slot/group button representing a holster on your left hip; if you aren't holding RB, X can be used for one of the menu inputs below.

* Toggle [inventory](Inventory.md) menu.
* Toggle logout/[character](Character.md) menu.
* Toggle [quest](Quest) menu.
* Toggle [rest](Rest) menu.
* Toggle direct/indirect control.
* Skip [time](Time.md).
	* In normal use, this toggles between real time and [sim-time](Time.md). When camped, it skips straight to the end of your rest. Either way, it's automatically interrupted if the party spots an enemy or encounters difficult terrain.
	* In the strategic layer (GSG mode), you continue to see the map at a consistent speed. In the direct camera or tactical layer (RTS mode), we can display a cinematic montage of travel or night passing. Not in MVP.
