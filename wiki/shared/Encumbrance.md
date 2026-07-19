```rs
const LOWER_MUSCLE_MASS_PER_LEG_STRENGTH = 5
const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS = 30
fn encumbrance_term(character):
	average_leg_strength = (character.legs.left.strength + character.legs.right.strength) / 2
	lower_muscle_mass = average_leg_strength * LOWER_MUSCLE_MASS_PER_LEG_STRENGTH
	weight_capacity = WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS * lower_muscle_mass
	return 1 - ((player.calculate_body_weight() + player.inventory.calculate_weight()) / weight_capacity)
```

Encumbrance is linear. The term above is the remaining multiplier, clamped
between 0 and 1; the displayed penalty is `1 - encumbrance_term`. A character
at half capacity therefore has a 50% penalty, and reaching or exceeding
capacity gives the maximum 100% penalty.

The burden includes body weight, carried water at 1 kg per litre, and every
carried inventory row multiplied by its quantity. Equipped items are already
inventory rows and count exactly once. Injury-adjusted capacity uses the
average of left and right leg strength after multiplying each leg by its
current health.

Inventory rails split the summary into equal-width halves. The left half shows
the exact burden and capacity to one decimal place with the exact penalty to
one decimal percent directly below it. The right half is a stable-width
green-to-yellow-to-red meter whose marker follows the same linear penalty.
Merchant Player tabs use the personal summary; merchant
Party tabs use the living-party aggregate. The party chest also shows this
aggregate: all living party members' burdens and capacities are summed, with
the shared chest burden added once. Dead members contribute neither burden nor
capacity, but the shared chest remains part of the aggregate.

The recruitment Athletics tag combines climbing and swimming performance and
uses this same shared encumbrance penalty, so a packed inventory lowers the
recommendation.
