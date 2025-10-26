# Minimum Viable Product
These features demonstrate everything needed for the basic gameplay loop. It won't necessarily be a very fun game at this point, but gives an idea of the potential once each of these barebones systems are fleshed out more.

1. A dude can fight another dude in standing melee [combat](Combat)
2. Prone/supine [controls](Controls), can be knocked down and get back up
3. Can pick up and fight with different types of melee weapons
4. Ranged combat (server authoritative, no rollback)
5. Advanced movement (climbing, sliding on sloped surfaces, navigating hazardous terrain like fording a river)
6. [Stats](Stats) system (attributes, skills, track damage to different body parts)
7. [Slot](Slots) system
8. Empty world from 1500s geological data
9. Populate world with settlements extrapolated from population data (settlements are just a coordinate, name, and population level for now)
10. Generic humanoid enemy types like orcs, goblins, and bandits
11. [Travel](Travel) system with random hostile encounters
12. Randomly generated [quests](Quests) (see: Battle Brothers for good templates)
13. Rest system for [health](Health) recovery
14. Inventory management (loot/buy/sell items)
15. Food/water/sleep system
# Polished Product
With these features, the game becomes something that we can imagine players actually wanting to pay for. A fun, unique product rather than a mere tech demo.

1. [Procedural modeling plugin](Models)
2. In-game editor for procedural models - design your own clothes or equipment
3. Urban levels (houses, castles, etc and quests that involve them like thievery or assassination)
4. Improved [stealth](Stealth) detection AI (investigate noises, raise alarms, patrol routes?)
5. Level editor - design your own house
6. PVP
7. More humanoid enemy types: beastmen, undead, ogres, and trolls
8. Non-humanoid enemy types like wolves or giant spiders
9. More detailed [downtime](Time) (pick a job to earn a wage at)
10. Elves and the immortal [character](Character) system
	1. Also need to make settlements and assets for them
	2. Character creator
# Simulation
Not required for the basic gameplay loop or polish, but increase the verisimilitude of the world and create opportunities for emergent storytelling.

1. Settlement prosperity that is based on population and reduced by unsolved quests (see: Battle Brothers)
	1. Prosperity affects what items can be bought and how expensive services are
2. Disease system
3. Marry other characters and have children (Mount and Blade II: Bannerlord has a barebones implementation of this)
4. Celebrations like holidays and birthdays - invite friends to hang out at your house
	1. Werewolf/Mafia/SS13/Among Us-style quests where your job is to infiltrate and do something nefarious at a rival's celebration
# Not in roadmap but could be
These are all neat but aren't required for the game to feel complete. However, if someone on the team is very passionate about one of them then we can prioritize it.

1. [Magic](Magic) system, implement on a per-element basis in order of whatever is easiest
	1. Wind magic is probably the easiest, just force fields in Avian that also cause unbalance. Will look much cooler with dust/leaf particles.
	2. Light seems fairly easy, as long as its just for illumination and some kind of blinding effect
	3. Shadow would be invisibility, especially in darkness
	4. A simple version of earth can be just causing tremors that stagger enemies (as opposed to something more complicated like fissures)
	5. Fire requires its own system for fire spreading and temperature
	6. Frost would use the opposite side of the same temperature system
	7. Nature to summon the assistance of wild animals in combat sounds easy, but not anything involving plants
	8. Lightning seems very complicated to do well (Circuits? Nope.)
	9. Water seems nightmarishly complicated to do well (Fluid simulation? Fuck no.)
2. Become a vampire/necromancer/lich
	1. Changes the way a lot of systems for your character work, like food, water, sleep, exhaustion, and health
	2. Become biologically immortal, but still permanently killable unlike an Elf
	3. Don't need the game to give you quests, every night of feeding/graverobbing is essentially a self-driven quest
	4. Game generates PVP quests for other players to destroy you as your infamy increases
3. Player-run factions
	1. Basically a faction board/groupchat + shared asset ownership with quests to antagonize rival factions
4. Factions can manage a settlement as the mayor/governor/etc
	1. Would be like being moderators on a forum, plausibly a semi-democratic election for settlements in Northern Italy. Players in other settlements would need to depose bad mayors by force or get invaded
5. Cultists and demons
	1. See: Space Station 13
	2. Due to the bulletin board social nature of the game, this seems like it might be a significant source of memes.
	3. Probably the best "outlet" for players with... "indecent" inclinations...
		1. If we let players customize characters and their outfits, they are *going* to make ridiculous coomer characters. Moderating this will be an uphill battle.
		2. We can just own it and tell them do whatever they like within their secret cults, and if they expose their degeneracy to anyone else who reports them then the player-run witch hunter/holy paladin faction will be given quests and empowered to come and purge them unannounced during their sex parties
6. Dwarven settlements and assets
	1. Option of using the underway to travel in addition to the overworld and sea. Much faster through difficult terrain, but also much more dangerous
7. Mounts and mounted combat
	1. Basically when you are charging and are holding the attack button, it can be automatically made against any enemy that comes within reach
	2. Horse can trample over smaller enemies
	3. Very high morale effect causes most weaker enemies to route instead of hold their ground
8. Huge monsters that require a more detailed combat system
	1. Dragon, cyclops, giant, griffin, cockatrice, chimera, etc
	2. Dragon's Dogma does a great job of this... but it looks insanely hard to implement well
		1. You can climb onto enemies to attack weak points
		2. Detailed hitboxes for all the different body parts
		3. Lots of variety in the attack animations
	3. Total Warhammer does a workable job of this that looks much easier to implement
		1. They're fast, very tanky, and have sweeping attacks, but their size makes them vulnerable
		2. Poke them with polearms while ranged units shoot overhead
		3. Alternatively, anti-large hero characters are good at dueling them due to being able to reliably dodge their big, telegraphed attacks
		4. Flying units just magically float, their animations give the illusion of more detailed physics
		5. When their huge attacks send characters flying, they aren't real full-body colliders and ragdoll physics, everyone is still a capsule and the animations give the illusion of detailed physics.
9. Poisons, potions, alchemy, herbalism, and gardening
	1. "We want the Stardew Valley audience"
10. Settlements outside of Italy
	1. Assets between European countries are easy to share, need some kind of set of language skills that give you severe trade penalties when no one in your party speaks the local language
		1. Can retroactively apply this to communication with Elves/Dwarves
	2. Arabia, Africa, the East, and the New World need lots of unique assets, don't bother until Europe is very fleshed out (maybe modders will do this?)