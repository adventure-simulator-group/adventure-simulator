# Food and cooking

This page describes current production behavior. Food rows now use the initial
integer quantity-plus-measured-state rollout in
[`measured-inventory.md`](measured-inventory.md), while conserved food-lot
mass, nutrition, and value remain floating fields pending the stable
measured-object schema.

Food is authoritative strategic inventory. `ItemKind::Food` identifies ordinary
foods, while edible herbalist ingredients may retain `Ingredient`. Every
acquisition creates one independent quantity-one `food_lot` per purchased or
found unit; food lots never merge merely because item IDs match. The inventory
row identifies the batch, while mass, calories, value, quality, five flavor
potencies, and fractional ingredient provenance live on the lot. Partly eaten
lots retain quantity one and scale every extensive property (including flavor)
together with their fixed-point remaining amount; quality remains unchanged.
Transfers and sales therefore
move a complete remaining batch rather than manufacturing rounded sub-units.
Food item metadata, including dual-purpose ingredient capabilities, is
canonical in the embedded [item definition catalog](item-authoring.md);
spoilage, cooking, and ingestion mechanics remain Rust rules.
Food definitions are validated before either personal or party inventory is
mutated, so an acquisition cannot leave an inedible inventory row without its
lot metadata. Inns sell a standard cooked meal with a fixed lot profile;
player-cooked meals reuse that item ID but retain their derived name, nutrition,
mass, value, contamination, and ingredient provenance on their own lot.
The standard meal provides 3,000 kcal, so two meals cover the ordinary
6,000-kcal daily demand.

The public lot records its inventory link, display name, preparation method,
ingredient provenance, quality, salty/spicy/sweet/sour/savory potency, mass,
useful calories, value, and creation minute. Quality uses the same name colors
as equipment and food tooltips use the plain label `Quality N`. Merchant
catalog quality is copied to every acquired lot. A
separate private row anchors microbial concentration and exponential growth.
Growth is evaluated lazily from strategic time and bounded; there is no spoilage
tick. Initial loads are deterministic server-random log-scale samples. Raw meat
grows fastest, cooked meat is heat-reduced and slower, and intact produce and
nuts are lower-risk. Temperature, storage, and preservation remain deferred;
undercooking and burning are modeled at fireplace retrieval.

Ingestion uses current concentration times consumed mass as a direct dose for
existing Dysentery (`Bloody flux`), whose vector is already food/water. The
exposure identity includes character, lot, and strategic minute. Immunity
applies and an unresolved Dysentery episode prevents duplicate infection.
Travel, camp rest, and non-inn settlement rest apply elapsed nutritional demand
once and then automatically consume the oldest pooled and personal food lots
toward a zero balance. Paid inn rest is full board: its elapsed calories and
ordinary drinking water are covered, any pre-existing food or water deficit is
cleared to zero, and personal and party provisions are preserved. Temple,
private, field, and camp rest provide no food or drinking water.

## Cooking

Cooking is entered through an environmental Fireplace portrait alongside the
people at a journey camp or an actual settlement building. Public Square, maps,
roads, and case sites have no fireplace. The Cooking skill row is informational.
The fireplace opens a trade-style station: personal inventory is the default,
and party inventory is an explicit alternate source and retrieval destination.
Transfer arrows stage bounded measured portions in quarter-unit steps. `Add
Ingredients` warns that committing is irreversible because the ingredients are
immediately diced and consolidated into one generic meal escrow.

Each character has private station contents at each exact fireplace even though
everyone sees the shared environmental portrait. Settlement authority binds the
station to the current settlement and building; camp authority also includes
the party, journey departure minute, and reached movement minute, preventing a
station from leaking between camps or journeys. Tactical actors and stale or
remote contexts are rejected. A station holds at most one dish and one installed
instrument. Trading a pan, pot, or portable oven into an idle station selects
pan-fry, stew, or bake; no instrument selects roast. Replacing or removing a tool
returns it to its recorded personal or party source. If that source is no longer
available, the tool stays installed rather than being lost. Party-sourced tools
remember the exact originating party rather than following the character into a
new party. Equipped tools are ineligible, and instruments cannot change while a
dish occupies the fireplace. A party cannot break its current camp while any
member still has a dish or instrument in that exact camp context; the camp must
be cleared first. A member likewise cannot leave or be removed, and a camped
party cannot disband, while affected private custody remains. Death is the one
deterministic exception: the dead owner's unfinished dish is abandoned, an
installed tool returns to its exact personal or recorded party source when
possible, and an unavailable party return falls back to the dead character's
personal estate inventory. If no character record remains, the tool is
abandoned with the station. Only that owner's private rows are cleaned.

Duration is method setup plus the slowest ingredient's safety/doneness time plus
square-root batch scaling. The reducer atomically preflights actor, exact
location, empty station, unique selection, ownership, measured amounts, tools,
water, and arithmetic before mutation. Commit consumes ingredients immediately
without advancing time and stores one aggregate escrow containing conserved
mass, calories, value, flavors, provenance, and private contamination inputs.
Stew draws pooled party water before carried water and includes that water in
finished mass. Clean cooking water adds no pathogen dose and dilutes the
ingredients' microbial load across that final water-inclusive mass. The dish is
retrievable like every other dish and is never auto-eaten or discarded.
`cooked_meal` cannot be submitted as an ingredient.

Progress is evaluated lazily from the owner's `CharacterTime`, so resting,
travelling, or spending time elsewhere cooks the dish. The fireplace page shows
the contributor, start-relative status, target, and remaining minutes, but never
hidden microbial load. Its convenience rest control uses minutes and defaults
to the remaining target time; once ready it stays visible with a warning that
additional time overcooks. Retrieval may put the meal in personal or current
party inventory, frees the dish, and leaves the tool installed. Early retrieval
reduces quality and interpolates calories from the raw total toward the normal
ready retention. It geometrically interpolates microbial kill from raw load to
the method's complete kill and linearly interpolates microbial growth from the
raw ingredient rate to the cooked rate. This preserves meaningful Dysentery
risk. Exact-target retrieval gives the ordinary cooking result. Late
retrieval reduces calories linearly to zero and lowers quality to tier 1 by one
additional target duration (twice total time); extreme elapsed values are
bounded. Discrete quality penalties use ceiling rounding, so any early or late
retrieval is observably worse. Fireplace waiting and retrieval award no passive
Cooking experience, mastery, or morale. Remainders stay as independent lots,
and their current lot mass and value drive encumbrance and merchant quotes. A character can
also apprentice as a cook through the inn's ordinary profession dialogue;
apprenticeship and later independent practice follow the same progression and
payment rules as the other non-religious settlement professions.

The authoritative Cooking check includes the documented one-pass Knife
transfer after direct Cooking study. Each rank removes 6% of setup and batch
overhead, to a maximum 30%; ingredient safety time is never shortened. Useful
calorie retention rises from 95% at rank zero to 99% at rank five. Roasting
then retains 85% of those calories to represent rendered fat dripping from the
skewer. Baking has 30 minutes of setup, compared with 5 for pan-fry, 12 for
stew, and 7 for roast.

Flavor potency is measured in mass-equivalent kilograms: one gram of salt
contributes enough salty potency for 100 grams of food. The shared objective
for an active flavor is potency equal to finished food mass. Flavors below
target score linearly (`5 * ratio`); excess is punished quadratically
(`5 / ratio²`). Each method has a fixed target mask, and an omitted required
flavor scores zero rather than disappearing from the equal-weight average.
Pan-fry and roast score salty, spicy, and savory flavors; stew also
scores sour. Baking deterministically scores the stronger of sweet and savory,
alongside any salt or spice, allowing both pies and savory bread. These shared
method rules are the future insertion point for character preference modifiers.

The continuous Cooking check first maps to a discrete chef tier:
`floor(check)`, with checks below 1 occupying novice tier 1. Final meal quality
is the lower of that chef tier and floored aggregate flavor score, with tier 1
as the five-tier item-system floor. Pan-frying subtracts one tier, never below
1, unless actually staged ingredients tagged `culinary_fat` comprise at least
2% of selected ingredient mass; merely owning or adding a trace of butter or
lard does not count. Quality tiers multiply derived market
value by 0.80, 0.90, 1.00, 1.15, or 1.35. The catalog includes low-calorie
seasonings (salt, mustard, horseradish, vinegar, garlic, and sage), sweet and
sour ingredients (honey and sour cherries), naturally savory meat and
mushrooms, and calorie-dense butter and lard.
`cooked_meal` is a terminal preparation state and cannot be selected as an
ingredient. This prevents repeated cooking from compounding the value or
nutrition multiplier.
# Foraged food

Raw wild foods gathered through current-vicinity foraging enter personal
inventory through the same validated non-fungible food-lot path as other food.
Watercress and seaweed extend Plants for wet-ground and coast foraging.
Venison is High Game, fowl is Low Game, fish requires wet ground or coast, and
minimal beast meat keeps Harmful Beasts functional. All use authoritative
mass, calories, value, Raw Meat contamination, and cooking definitions.
Foraging does not synthesize processed goods.
