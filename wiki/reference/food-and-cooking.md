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
nuts are lower-risk. Temperature, storage, preservation, undercooking, and
burning are deferred.

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

The active character's Cooking skill icon is a raised menu button; flat skill
icons remain informational. Activating Cooking opens a wide, responsive modal
dialog over the unchanged character sheet. It uses the same two-sided inventory
browser as trading and
looting: the cooking pot is on the left, the character's full inventory is on
the right, and transfer arrows stage bounded amounts of food between them. The
amount controls use integer milliunits internally and quarter-unit steps in the
current interface, including a final smaller remainder when necessary. The
center shows a placeholder cooking scene, Cook and Cancel, and a horizontal
icon row for pan-fry, stew, roast/skewer, and bake. Roast is always available.
Pan-fry requires a pan, stew a pot plus water, and bake a portable oven. Stew
draws pooled party water before carried water. Tools are retained. Inns sell
the food ingredients and reusable implements needed by this interface. The
preview reports estimated duration and flavor score and calls out roast calorie
loss, a fatless pan, and stew disposal; the reducer remains authoritative.

Duration is method setup plus the slowest ingredient's safety/doneness time plus
square-root batch scaling. The reducer preflights actor, state, selections,
tools, water, and arithmetic before mutation. It advances neutral strategic
time, and consumes inputs. Pan-fry, roast, and bake create a carried derived
food lot; cooking no longer also eats those meals. Stew is the sole exception:
soup is immediately eaten up to the one-day fullness cap because it cannot be
carried, and any remainder is discarded. Consumed stew water contributes its
milliliters divided by 1,000 to finished mass before flavor scoring and
contamination dilution. Only the registered strategic gateway may
invoke eating or cooking, and tactical actors are rejected. Cooking advances its
safe time prefix before consuming supplies: a terminal interruption commits the
elapsed time and terminal event, leaves ingredients and water untouched, and
creates no meal. Remainders stay as independent lots, and their current lot mass
and value drive encumbrance and merchant quotes. Completing a meal trains the
mental, trained Cooking skill for the elapsed cooking time. A character can
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
