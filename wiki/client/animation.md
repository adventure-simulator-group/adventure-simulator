# State
There may be four layers to the state of a character:
1. Their input/NPC AI (what action are they attempting to do?)
2. What action are they actually doing? Let's call this "skeleton state".
3. The forward/inverse kinematic animation playing
4. Secondary animation (physics)

## Input and NPC AI
When you press W, there is an input system that determines that you are trying to move forward. When an NPC has a goal to move to a position that is in front of it, it has a pathfinding system that tells it to move forward. Both of these set the same "skeleton state", which is a component that keeps track of the fact that this character is trying to move forward.

## Skeleton State
The state of the skeleton tells you everything that you _need_ to know about what a character is doing this frame. Are they moving? Attacking? Both? Grabbing an item? Staggered? Jumping? Being knocked down? This is the minimum information necessary to reproduce the current animation for your character, and thus is what would be synchronized over the network. From a client's perspective, it is not (necessarily) clear whether another character is an NPC or a player, at least were it not for the existence of voice/text chat, because the only thing your client sees is their skeleton state, not input or AI.

The skeleton state may also be more complicated than what the controls imply due to needing to include information like "which direction are they ducking in?" or "when in a combat stance, which side is currently facing the enemy?" (when you slash with a weapon, this switches since you take a single step).

## Forward and Inverse Kinematics
There is a system which recieves the skeleton state and actually animates the bones. It may not actually be strictly necessary to run this on the server, its possible that only clients need to see this since the only thing that the location of your individual bones affects is your [input precision](../client/controls.md). We may want to use [this](https://www.youtube.com/watch?v=LNidsMesxSE) as a reference for producing these animations.

> Halbe: There might also be a layer before this for an animation graph, if the person implementing this feels that such an abstraction would be helpful

## Secondary Animation
Clients may also apply animations via joint motors to make them more realistic, as each bone can now have inertia which may be affected by movement or collisions with the environment or weapons. This is not in-scope for the MVP though.

# Stylistic Principles
We want to try and keep animations realistic, in accordance with the [meta-level heuristics](../meta.md) that govern our decisionmaking for this project. This means, for example, that melee attack animations should generally be inspired by [HEMA](https://en.wikipedia.org../Historical_European_martial_arts). In general this means that there is a lot less "leading" or "anticipation" to attacks than you typically see in action game or movie choreography, at least for professional characters. But beyond the MVP, we can make multiple animation sets for different skill levels of characters. Untrained characters like goblins, for example, can have lots of anticipation in their attacks which is what makes them relatively easy to dodge.

Due to the fact that this is an online game, we also have to account for the fact that animations will have different speeds depending on perspective. Since all [networking](../networking.md) will be mediated by the server, we want to be able to extend animations by the duration of a network round-trip so that they can start immediately when you press the button but end at the same time for all players. This is a pretty good excuse to add some otherwise unnecessary leading to the _player character's_ animations, which may look better than simply slowing the attack animation down.
