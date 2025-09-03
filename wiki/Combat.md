* Whether an attack hits your enemy, and whether an enemy's attack hits you, should be determined by both stats and player skill. 
* Combat should not be unnecessarily complicated. We don't want players to have to memorize a bunch of arbitrary combos. Any complexity that it does have should be on account of it actually working like this in the real world
## Attacking
* Accuracy is determined by how accurate you are--how close do you keep the crosshairs on the enemy for the duration of the attack.
* If its locked-on to the center of their neck for the entire attack then you have a 1.0 multiplier on accuracy
* If you aren't even looking at them its 0.0 multiplier.
## Defending
* Defense is determined by how quickly you react--when an enemy starts their attack, after how long do you press the appropriate defensive button
* If you press it with superhuman reflexes you get a 1.0 multiplier to your character's defensive stat
* If you press it the microinstant before the attack actually hits, or not at all, you get a 0.0 multiplier
* You also may get a penalty if you use the wrong defensive option. If an enemy is doing a R->L diagonal attack, and you duck to the left, you get no defense bonus. If they stab and you duck instead of dodging or blocking you get no defense bonus.
* Blocking might basically always "succeed" but you are staggered somewhat by each attack, especially if they are very heavy. Do not attempt to block a giant's club, for example.
## Poise
* Attacking isn't usually going to cause damage to an enemy, at least if they are wearing armor and/or have a shield. But it can damage their poise.
* Poise can be in four states: Balanced, off-balance, staggered, or knockdown.
* Receiving a good, direct hit can reduce your poise.
* The less poise you have, the less defense you actually get from trying to dodge or block.
* Your poise quickly recovers, so if you knock an enemy off-balance you should exploit the opportunity while you can
* A sufficiently powerful attack can send you straight from balanced to knockdown
* A sufficiently accurate attack (or ineffective dodge) can directly damage an enemy, even if they're armored. A knight that's just standing still is vulnerable to a dagger through the eye-slit, or a supernaturally skilled elven warrior can kill an armored orc without first having to stagger him.
* The goal of poise mainly is to represent a minor cost to failure in combat without having to actually injure characters.
	* We want the game's systems like health and damage to be fairly realistic
	* I fell off a scooter and stubbed my toe recently, I had to limp for a week. I got an appendectomy which put four tiny clean incisions in my belly, I was all-but incapacitated for two days. We need player characters to be able to get hit in combat without actually becoming injured for weeks or months.
	* So the idea is that you're wearing a reasonable amount of armor and its pretty unlikely for an enemy to outright kill or incapacitate you in any given attack, only if you screw up twice or thrice in quick succession.
* Poise also serves a similar role to stamina in "souls" games, except ideally can be represented by the animation system instead of a bar
	* There are four *discrete* states to poise, because if its continuous then players will be unsure exactly what amount they currently have and will therefore want to look at a visual meter. If we don't give them a meter, they will just mod it in.
	* The question is how do you take the inputs to this, which are continuous, and map them to four states. I think we can just do this randomly. If you are staggered by an attack that would put you 0.35 of the way between staggered and knockdown, then it just has a 65% of putting you in stagger and 35% of knockdown.
	* Poise regenerates at a consistent speed so you don't really need a bar for this if you've played a lot. If you were put in stagger half a second ago then your muscle memory knows that you'll be balanced again in a quarter second.
	* If this doesn't work well then it can just be a bar, or perhaps a wheel like the stamina wheel in Breath of the Wild/Tears of the Kingdom.
* Another benefit of poise is that you also don't really have to actively keep track of health. You aren't damaged most of the time, and when you are it should be very obvious because its debilitating. No need to put a health bar on the screen at all times.
	* I like the "lethality" system in newer Total War games. Units in your formation don't have individual health to keep track of, they are simply either dead or alive. This means that an initial volley of arrows doesn't produce zero casualties due to one arrow not being enough to overcome a unit's health, and that you can represent the formation's health with a headcount instead of total health pool which is pretty abstract.
## Stats
* A surgeon has very high dexterity. So does a locksmith. But they don't necessarily know how to do each other's jobs, so you can't just simplify stats down to attributes. However, it would probably be easier for a surgeon to learn lockpicking than it would for most people, so you can't just simplify stats down to skills either.
* But skills that you do not have trained still don't actually have to be listed in the interface. You don't see "Computers: 0" on every character in the 1500s...
* When coming up with attributes, try and keep it based on biology/physics. For example, a character's attack speed is mostly a function of strength, not dexterity, because strength determines how fast you can accelerate your arm or change direction. Dexterity would make your attacks more accurate.
* Skills should account for the fact that they can be related
	* If we want different skills for different melee weapon types, they should have some way to overlap
	* If you spend 10000 hours mastering the blade while your classmates are out partying, you're probably at least decent with an axe even if you've never used one
	* Option A: skills can be "correlated"
		* The sword skill has a 0.5 correlation with the axe skill. Every time you gain one point of sword, you have 0.5 points in axe
		* Keep track of the base values of these to prevent feedback loops. What does not happen is you put 1 point into sword which gives 0.5 in axe which gives 0.25 in sword which gives 0.125 in axe and so on.
	* Option B: skills are broken down further
		* What swords and axes have in common is swinging attacks and keeping the edge pointed in the direction of motion. 
		* Where they're different is that axes are unbalanced and have comparatively short edges.
		* It could be broken up into six skills: swing attacks, thrust attacks, edge alignment, balanced weapons, unbalanced weapons, and lengthwise accuracy. Each weapon involves four of these
		* Simple weapons like clubs and would naturally require fewer skills for mastery
		* "Allistic" people would not care about their character's lengthwise accuracy, so this would probably go hand-in-hand with some abstraction that lets them ignore this
		* "Swords" is a meta-skill which includes all the skills involved in using a sword. If they pick the swords skill, it will just put points into all of them evenly
		* This effectively recreates the correlations from option A, because now if you have the sword skill then you have half the skills that go into axes
		* The abstractions can even have abstractions, "melee" is a meta-skill that includes all the melee skills
		* Abstractions would be done entirely client-side, and we might provide some defaults in the settings analogous to a player selecting their difficulty. Essentially you can choose how complex you want the mechanics to be. More complex = more information and micromanagement
		* This might also be helpful for accounting for the fact that radically different settings still need to share the same underlying skill system. Abstractions could be reworked per-setting, like "alchemy" is pretty similar to "chemistry" but not quite the same.