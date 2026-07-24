# Food and cooking

Food is authoritative strategic inventory. `ItemKind::Food` identifies ordinary
foods, while edible herbalist ingredients may retain `Ingredient`. Every
acquisition creates one independent quantity-one `food_lot` per purchased or
found unit; food lots never merge merely because item IDs match. The inventory
row identifies the batch, while mass, calories, value, and fractional ingredient
provenance live on the lot. Partly eaten lots retain quantity one and scale all
four conserved properties together. Transfers, cooking, and sales therefore
move a complete remaining batch rather than manufacturing rounded sub-units.
Food definitions are validated before either personal or party inventory is
mutated, so an acquisition cannot leave an inedible inventory row without its
lot metadata. Inns sell a standard cooked meal with a fixed lot profile;
player-cooked meals reuse that item ID but retain their derived name, nutrition,
mass, value, contamination, and ingredient provenance on their own lot.
The standard meal provides 3,000 kcal, so two meals cover the ordinary
6,000-kcal daily demand.

The public lot records its inventory link, display name, preparation method,
ingredient provenance, mass, useful calories, value, and creation minute. A
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
toward a zero balance. Paid inn rest is full board: its elapsed calories are
covered, any pre-existing food deficit is cleared to zero, and carried food
lots are preserved. Temple, private, field, and camp rest provide no food.

## Cooking

The active character's Cooking skill icon is a raised menu button; flat skill
icons remain informational. Activating Cooking opens a wide, responsive modal
dialog over the unchanged character sheet. It uses the same two-sided inventory
browser as trading and
looting: the cooking pot is on the left, the character's full inventory is on
the right, and transfer arrows stage bounded amounts of food between them. The
center shows a placeholder cooking scene, Cook and Cancel, and a horizontal
icon row for pan-fry, stew, roast/skewer, and bake. Roast is always available.
Pan-fry requires a pan, stew a pot plus water, and bake a portable oven. Stew
draws pooled party water before carried water. Tools are retained. Inns sell
the food ingredients and reusable implements needed by this interface.

Duration is method setup plus the slowest ingredient's safety/doneness time plus
square-root batch scaling. The reducer preflights actor, state, selections,
tools, water, and arithmetic before mutation. It advances neutral strategic
time, consumes inputs, creates a derived meal, and immediately attempts to eat
it up to the one-day fullness cap. Only the registered strategic gateway may
invoke eating or cooking, and tactical actors are rejected. Cooking advances its
safe time prefix before consuming supplies: a terminal interruption commits the
elapsed time and terminal event, leaves ingredients and water untouched, and
creates no meal. Remainders stay as independent lots, and their current lot mass
and value drive encumbrance and merchant quotes. Completing a meal trains the
mental, trained Cooking skill for the elapsed cooking time. A character can
also apprentice as a cook through the inn's ordinary profession dialogue;
apprenticeship and later independent practice follow the same progression and
payment rules as the other non-religious settlement professions.
