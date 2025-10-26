Morale is a stat which defaults to zero, meaning no penalties. There are no benefits to morale above zero, except that it is a buffer against receiving morale penalties. The penalty for negative morale applies to [incapacitation](Combat).

To determine a characters' morale, all of the positive and negative factors are first separately consolidated via a function that adds them with diminishing returns. Essentially this function sorts the effects from highest to lowest, then iterates through them, adding each subsequent effect at a reduced penalty. It may look something like this:

```rust
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

// or maybe this
fn cumulative_morale_alt(effects: &[f32]) -> f32 {
	effects.iter()
		.fold(0., |acc, effect| 
			acc + effect / acc
		)
}

let cumulative_positive = cumulative_morale(&positive_effects);
let cumulative_negative = cumulative_morale(&negative_effects);
let final_morale = cumulative_positive - cumulative_negative / character.skill_check(will);
```

# Positive Morale Effects
- [Charisma skill check](Stats) from each party member
	- Multiplied by mutual faith of the given party member and that of oneself
	- Divided by total conflicting faith of the pair (could alternatively be negative)
- Food quality
	- Higher quality food (like well-seasoned) = more expensive
- Recent successes
	- Each enemy routed/killed at the tactical layer, each encounter won at the strategic layer
- Allied power / enemy power
	- If the total strength of your force is greater than the enemy, apply the difference as positive morale
# Negative Morale Effects
- [Injuries](Health)
- Disease
- Recent defeats
	- Huge penalty when seeing an ally die or flee
- Enemy power / allied power
	- Multiplied by the "fear multiplier" of the enemy. For most this would be 1.0, but we would give undead high multipliers and demons a huge multiplier. 