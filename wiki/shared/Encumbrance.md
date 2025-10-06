```
const LOWER_MUSCLE_MASS_PER_LEG_STRENGTH = 5
const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS = 30
# todo: should this be linear or nonlinear?
fn encumbrance_term(character):
	average_leg_strength = (character.legs.left.strength + character.legs.right.strength) / 2
	lower_muscle_mass = average_leg_strength * LOWER_MUSCLE_MASS_PER_LEG_STRENGTH
	weight_capacity = WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS * lower_muscle_mass
	return 1 - ((player.calculate_body_weight() + player.equipment.calculate_weight()) / weight_capacity)
```