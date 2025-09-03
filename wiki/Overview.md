# Adventure Simulator (working title)
* Fantasy immersive sim/RPG
	* Similar works: Kenshi, Mount & Blade, Dwarf Fortress Adventure Mode
* World is mid-1500s Earth, except with inexplicable fantasy elements interspersed throughout
	* Warhammer Fantasy, except more grounded
* Game mechanics are realistic, but...
	* Anything tedious or boring should be automated
		* Fast travel, micromanaging inventory
	* Anything frustrating or unfun should be handled at the content-level
		* For example, most fantasy enemies have poor eyesight to make stealth more fun or players can rely on supernatural luck to not have their characters constantly dying to stray arrows
* Will eventually be multiplayer, and should design architecture accordingly, but can defer this feature to a later milestone
	* We are fine sacrificing latency for simplicity for now, server can mediate all player actions
* [Controls](Controls) are flexible to suit different preferences, you can either control your character directly as you would in an action game, or order them around like a unit in an RTS game
* [Combat](Combat) is a hybrid between action and tab targeting. You actively block/dodge/duck, but we aren't actually doing collision checks. Animations use inverse kinematics to ensure hit/miss.
* [Magic](Magic) is "soft"--unreliable and sometimes difficult to distinguish from coincidence
* You can either create an expensive immortal [character](Character) or pick a cheap randomized mortal one. The former gives you more of a traditional RPG experience, the latter is for more of a roguelike/extraction experience
	* You also are expected to have several characters, which is also helpful in case one gets seriously injured and the realistic healing system means they'll be incapacitated for awhile
* To be [implemented](Implementation) with [Bevy](https:///bevyengine.org), favoring procedurally generated assets and using placeholders until such systems are complete
* No specific timeline for the [roadmap](Roadmap) yet, we want the early phase to be more exploratory
	* This is not a for-profit endeavor expecting an ROI in the near future. We simply want this game to exist, and we want to do it right. We do like timelines and feel that they help get things done, but they need to be negotiated with everyone after we all have a chance to seriously consider the plan.
* This wiki is not a sacred text that must be strictly adhered to, it is in fact deliberately vague about any aspect of the implementation we aren't yet sure about. Use your own judgement when filling in the gaps, though you should understand the [meta-level](Meta) reasoning behind many of these ideas.