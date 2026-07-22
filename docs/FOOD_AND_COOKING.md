# Food and cooking

Food is authoritative strategic inventory. `ItemKind::Food` identifies ordinary
foods, while edible herbalist ingredients may retain `Ingredient`. Every
acquisition creates an independent `food_lot`; food lots never merge merely
because item IDs match.

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

## Cooking

The character page accepts `?activity=cooking`. Its left rail selects pan-fry,
stew, roast/skewer, or bake; its right rail stages arbitrary positive bounded
amounts from owned food lots. Roast is always available. Pan-fry requires a pan,
stew a pot plus water, and bake a portable oven. Stew draws pooled party water
before carried water. Tools are retained.

Duration is method setup plus the slowest ingredient's safety/doneness time plus
square-root batch scaling. The reducer preflights actor, state, selections,
tools, water, and arithmetic before mutation. It consumes inputs, advances
neutral strategic time, creates a derived meal, and immediately attempts to eat
it up to the one-day fullness cap. Remainders stay as independent lots. Cooking
trains no skill.
