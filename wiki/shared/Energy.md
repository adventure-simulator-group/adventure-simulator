Energy is an abstraction representing approximately the maximum amount of calories can be used in a day. The lower it gets, the less effective you are, because your body is resorting to more difficult-to-extract energy sources (fat or protein instead of blood sugar, depleted glycogen reserves in muscles, etc). Additionally, your metabolism can only actually absorb so much nutrients in a given day, it takes time to digest food and extract nutrients from it.

The exact biological functions aren't really relevant to gameplay, but they are physical processes to base our equations on.

# Points of reference
- A small, sedentary person uses ~1500 kcal/day
- An average male soldier marching all day uses ~6000 kcal/day
- An Olympic athlete in an endurance event can use as high as ~12000 kcal/day
- Calories metabolized by humans per-gram
	- Fat: 9
	- Protein: 4
	- Uncooked starch: 2
	- Cooked starch: 4
	- Sugars: 4
	- Cellulose: 0.2
	- Alcohol: 7
# Equations

## Current food and water needs

Food and water advance only with a character's authoritative strategic clock.
Settlement life currently assumes that ordinary meals and drinking water are
provided, including lazy catch-up and explicit rest. Entering a settlement
restores one day of short-term food and hydration reserve and refills every
owned waterskin.

Travel uses **6,000 kcal and 4 litres of water per full day**, applied
proportionally for partial days. A travel ration supplies 6,000 kcal. A
waterskin carries 4 litres. Characters automatically eat personal rations and
drink carried water whenever their short-term reserve would otherwise become
negative.

Unsupported hunger reaches full strategic incapacitation after three marching
days beyond the food reserve. Unsupported thirst reaches it after one marching
day beyond the hydration reserve. Both curves are quadratic, begin only below
zero reserve, and combine with pain, blood loss, fear, and fatigue. They do not
currently kill a character.
