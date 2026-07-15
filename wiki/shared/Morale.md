Morale is a stat which defaults to zero, meaning no penalties. There are no benefits to morale above zero, except that it is a buffer against receiving morale penalties. The penalty for negative morale applies to [incapacitation](../tactical/Combat.md).

To determine a characters' morale, all of the positive and negative factors are first separately consolidated via a function that adds them with diminishing returns. The implemented function sorts the effects from highest to lowest, then adds each subsequent effect at a reduced harmonic rank:

```rs
let mut positive_effects: Vec<f32> = ...;
positive_effects.sort().reverse();
let mut negative_effects: Vec<f32> = ...;
negative_effects.sort().reverse();

fn cumulative_morale(effects: &[f32]) -> f32 {
	effects.iter()
		.enumerate()
		.fold(0., |acc, (effect, i)| 
			acc + effect / (i + 1) as f32
		)
}

let cumulative_positive = cumulative_morale(&positive_effects);
let cumulative_negative = cumulative_morale(&negative_effects);
let final_morale = cumulative_positive - cumulative_negative / character.skill_check(will);
```

# Positive Morale Effects
- [Charisma skill check](Stats.md) from each party member
	- Multiplied by mutual faith of the given party member and that of oneself
	- Conflicting faith turns the conviction-weighted portion into a negative morale source
- Food quality
	- Higher quality food (like well-seasoned) = more expensive
- Recent successes
	- Each enemy routed/killed at the tactical layer, each encounter won at the strategic layer
- Allied power / enemy power
	- If the total strength of your force is greater than the enemy, apply the difference as positive morale
# Negative Morale Effects
- [Injuries](Health.md)
- Disease
- Recent defeats
	- Huge penalty when seeing an ally die or flee
- Enemy power / allied power
	- Multiplied by the "fear multiplier" of the enemy. For most this would be 1.0, but we would give undead high multipliers and demons a huge multiplier.
	- This should be one of the reasons that player characters are better at dealing with fantasy enemies than knights and soldiers. Adventurers (especially when accompanied by bards and/or clerics) should have much higher morale bonuses than normal characters.

# Current strategic implementation

Strategic morale is derived on the server from injuries, party Charisma and Faith, recent encounter results, and the allied/enemy power difference at a quest location. Food and disease contribute zero until those systems are implemented. Undead currently use a 1.5 fear multiplier and demons use 3.0; other enemies use 1.0.

Recent morale events decay linearly to zero over seven days of the affected character's strategic time. A shared faith can increase an ally's Charisma contribution up to 2x at maximum mutual Faith. When both characters have different religions, conviction instead turns up to the full Charisma contribution into a negative source. A character with no selected religion receives the neutral Charisma contribution and causes no religious conflict.

The strategic condition projection records the positive and negative subtotals, final morale, and resulting fear. Negative morale converts to fear at one percentage point of incapacitation per morale point. The projection is a refreshable cache; the durable state is the character's condition and time-stamped morale events.
