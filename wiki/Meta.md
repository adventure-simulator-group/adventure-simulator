# Concept
The goal of this game is to make you feel like an adventurer in a believable world. 
1. Make a character
2. Take a quest
3. Gather a party
4. Prepare for your journey
5. Venture forth
6. Encounter unexpected challenges
7. Finish the job
8. Return for your reward
9. Repeat step 2 until you die, in which case repeat step 1.
We may want some kind of progression between characters later on as well as the ability to create multiple characters and switch between them, leaving each other their estate upon death or something, but this is the basic idea. In the long term, we would love to have this become some large-scale world simulator in the vein of Dorf adventure mode or Mountain Blade, as well as large-scale networking features, but for now the scope will remain limited.

Right now, we aren't too concerned about making the content especially interesting. Only the features. So what content there is can be lazily procedurally generated. But when we establish a fun formula we will eventually either focus on making the procedural generation more interesting or creating tools for players to create their own content. We want there to be a blurry line between developers and players and harness the latent potential of the modding community by offering players/modders an open-source, consistently designed platform.

# Design
## Realistic With Caveats
The underlying game mechanics should always be implemented in a "realistic" manner, but when we encounter a realistic mechanic that isn't fun, we should either account for this in the content or abstract over it.
### Content-based approach
Suppose we are implementing a stealth system and we find realistic eyesight to be unfun (like getting spotted 200ft away under moonlight). Instead of making humans worse at detecting, we simply contrive enemies with worse eyesight. Therefore orcs, goblins, skeletons, and the like have really bad eyesight for some reason. Conversely, we can also simply give the player supernatural stealth methods like magical invisibility or a chameleon cloak.

The goal here is that since the underlying game mechanics are physically based, we have a point of reference to balance the game off of (the real world). This allows different types of gameplay to co-exist. If you want to have a really realistic, punishing time, play as a normal human fighting normal humans. If you want something else, fight enemies that are more fantastical with a character that is more fantastical. This should also help avoid the scenario where developers realize that a gameplay system is poorly balanced, change it, then retroactively break all the old content designed with that poor balance in mind.

### Abstraction-based approach
Suppose we are implementing travel in the overworld and we find realistic travel times / world scales to be unfun (holding W for 10 hours on a road to get to a rest stop). We should instead use the interface to skip over these segments, therefore a fast travel system. It would still actually simulate the travel in a realistic manner though, so you will need to have food and rest and all that. What if micromanaging food is tedious? Same solution, abstract it in the interface. I don't really care whether my character eats his grains or his jerky or his rice. I just want to know how many days worth of food he has, he can automatically eat it as he goes. When I go back to town, I just want to give him a food budget and he chooses whatever level of nutrition is suitable for it (like 10 days on a cheap budget means grains, 10 days on a luxurious budget means jerky).

The abstractions however should be fairly scalable. If you *want* to walk for 10 hours to your destination, you can. If you want to manually pull food out of your inventory and eat it, you can. The lowest level of abstraction should be very precise, down to the level of which hand you are holding something in or which pocket you put it in. We should start with the lowest level of abstraction then work our way up, therefore the first iteration of the game should be more of an immersive sim than adventure sim or RPG.

## World and Lore
### "Kirkland Signature"
When you go to Costco and buy Kirkland Signature peanuts, you do not get xtreme flavor blasted japapeno nacho-cheese chocolate-covered peanuts. You can get salted peanuts or unsalted. Maybe you will like the flavor-blasted peanuts more, but you *know* that the Kirkland Signature ones will at least be solid. Their brand represents that which is generic, but high-quality. These aren't Great Value (Walmart) peanuts, they're Kirkland Signature. That is to say, we will be making our world very generic, but it should still be high-quality. Think Tolkien, or a more toned-down version of Warhammer Fantasy.

### Historical Fantasy
The world is Earth and the year is ~1543, but there are also inexplicable fantasy elements added everywhere. There's elves and goblins in the forests, dwarves and orcs in the mountains, and vampires and undead in the crypts. But the human kingdoms and empires are the same, only now King Henry VIII has dragons flying around his kingdom causing mayhem and he needs YOU, noble knight, to stop them!

In general though, the fantastical elements and historical elements should try to avoid stepping on each other's toes too much. The human kingdoms regard things like griffins and orcs as nuisances, best dealt with by hiring brave adventurers from ahistorical fantasy factions, not as existential threats of relevance to the historical record. Likewise, elves and wizards don't much care which duke is sqabbling over which patch of dirt, they have evil liches to slay and cults to root out.

Therefore, most of our "lore" is just actual history. For the fantasy stuff, there will certainly need to be actual lore written, but that can come later as the game becomes more fleshed out. Its not really necessary to understand the origin of the curse of vampirism in order to be an adventurer who goes on a procedurally generated quest to slay a vampire.

### This sounds too generic...
In being a *platform* for players to create content, eventually we imagine people might use it to make interesting, bizarre worlds. The core gameplay systems that enable this generic fantasy game should also basically work the same in a modern world of firearms and computers or a sci-fi world of spaceships and energy weapons. An AR15 is a very accurate arquebus that fires and reloads extremely quickly. A plasma rifle is an AR15 whose projectiles explode and melt through armor. A car is a faster horse, kindof. A spaceship is a car that moves in three dimensions. There will of course need to be a **lot** of work done to support these systems, but in being open-source we hope that someone who would otherwise make their own sci-fi game from scratch might find it easier to extend ours to support it. But all of this is way, *way* down the line. For now we are making a generic fantasy adventure simulator.
