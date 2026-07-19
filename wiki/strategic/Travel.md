> TODO: rewrite for HTML-based strategic interface

We technically have an open world, but its not really an open world game in the sense that most people consider it. If you've played the 1.0 version of Mount and Blade (before you could walk around in settlements), Battle Brothers, or early versions of Starsector you should have an idea of what to expect here.

Essentially, the travel map is a minigame where each "party" (players or enemies) has three stats: speed, detection, and stealth.
## Speed
- Your travel speed on the map
- This is not necessarily the same as the speed that a character might run at
	- Humans are unique in the animal kingdom for having very high travel speed due to our ability to sweat and walk upright. A zebra might sprint much faster, but it will quickly become exhausted.
- The larger a party is, the slower your travel speed. Large parties must deal with increased logistical complexity, periodically stopping throughout the day as incidents occur, and take longer to camp and set-off
	- There is also a geometric component to this, a "column" of soldiers is typically one to four men wide (depending on width-of and availability-of roads), so when a huge army set out for the day some of the army may have to wait for hours after the vanguard sets out before they start marching
	- No we are not doing huge armies, that is out of scope for this project (for now 😉)
- On a per-character basis, this is based on your or your mount's endurance versus your weight. It is very exhausting to march all day, especially with a heavy pack.
- On a party basis, this is based on the total endurance against the total weight. This is because characters with high endurance will share the burden of those with low endurance or heavy packs.
## Stealth and Detection
- The radius at which enemies can detect you
- Even if an enemy is faster than you, if they can't see you then you can simply go around them
- Larger parties, especially ones with horses, are easy to spot from a distance
- A party can improve its detection by relying on scouts that stealthily scout ahead of the party to spot potential enemies
- On a per-character basis, your stealth is a function of you or your mount's agility and weight. Your detection is a function of your sight and hearing.
- On a party basis, the average of each character's stealth and detection.

## Interdiction, Ambush, and Rest
- In a perfectly balanced system, the vast majority of parties would ever actually fight. A small group of elite warriors could successfully pursue large groups of weak enemies, but aside from this everyone would just be able to run away from most battles. We don't actually want this.
- At the end of the day, each party needs to set camp for the night
- If you don't, you start becoming exhausted and will eventually lose travel speed
- This means that if you encounter an enemy party at the end of the day, who might ordinarily not be quite as fast as you, its possible that they could chase you to exhaustion.
- This serves the same purpose as Starsector's interdiction system (force a speed penalty on sufficiently close enemies) and Mount & Blade's disorganized penalty (receive a speed penalty when raiding a village or after any battle)
## Planning Your Adventure
- The travel screen is a map that lets you place points to chart your planned journey
- In any given region, you can view a list of what sorts of enemies are in an area along with an estimate of the size of a party of that enemy type from which you could not reliably evade. This is important for planning your journey. If a particular area has parties that are too powerful for you to beat in combat *or* evade (because they are elite warriors), you should find a different route or rethink your party.
- Different sorts of terrain may also have different travel speed multipliers. Traveling on roads is very fast, fording a river is extremely slow. Your route will show the speed multiplier at any given point along its path.
- This isn't necessary for the MVP, but a pathfinding algorithm should be used on the terrain map to find the optimal route between each point. The point of placing points should be more to plan around resources or route around areas with dangerous enemies, not a check to see whether your brain is capable of intuitively computing A*.

## Viabundus road network

The historical world importer represents the Viabundus road system as graph
nodes and edges in the strategic database. The initial 1544 import admits land
roads and ferries; intermediate junctions, bridges, and ferry endpoints remain
in the graph so that a route is not incorrectly collapsed into a direct line
between settlements. Selecting a settlement name opens its overview, which
lists the next connected settlements on the road/ferry graph. Selecting a
destination shows its route distance and an MVP journey-time estimate at an
average walking pace of 5 km/h. Confirming advances the character's strategic
time by that duration and moves them to the destination. Route cost, party
speed, terrain, rest stops, and encounters are subsequent layers on this
graph.

Each edge retains canonical route terrain compiled from GLO-30 and EU-Hydro: a
bounded elevation profile, directional grade, terrain class, landforms,
nearby/crossed water, seasonal risks, and static encounter tags. The separate
Viabundus slope multiplier is only its source travel-cost hint. These values
inform routing and encounter selection without persisting live tactical state.

Travel edges also retain typed bridge and toll infrastructure derived from
active Viabundus nodes. Ferry and land routes are distinct typed variants; land
routes may carry a bridge endpoint. Toll and bridge properties retain whether
the infrastructure lies at the route's `from`, `to`, or both endpoints. This
lets travel events deduplicate a shared node and generate an appropriate
tactical scene without treating infrastructure as a settlement attribute.

Quest travel is deliberately separate from this road graph. A generated quest
stores an off-road point near its posting settlement. The trip to that point and
trips from it to the five nearest settlements use straight-line distance, require
no road connection, and advance strategic time at 1.25 km/h (one quarter of the
MVP's normal walking speed). Both settlement and quest-destination travel use the
shared Map tab: destinations are selected in the left rail while the right rail
sets the fatigue threshold at which the first tiring member makes camp.
Hovering or selecting a destination previews the route above chat with start,
camp, destination, and animated party pins. The preview is calculated from the
same least-rested-member fatigue and Endurance rule that advances travel, so it
does not use a browser-only daily-march approximation. A journey longer than a
leg stops at a persisted camp; the leader may rest the whole party for free and
continue in instantaneous legs. The strategic layer persists the journey's
original endpoints, total duration, actual camp checkpoints, and the remaining
forecast. SSE updates therefore keep every party member's tracker consistent
across camp rests and page navigations. A shorter-than-recommended camp rest
can legitimately add a future projected camp, but camps already reached never
disappear. A party's
active quest destination is added to the settlement Map list only while the
party is at the quest's posting settlement; it carries a red exclamation. From
any other settlement, the Map traces the shortest road/ferry path to the
posting settlement and marks the next available settlement leg red. The current
settlement is a non-traveling Map row; it shows available quests in gold or a
completed active quest ready to report in red, with red taking priority. Once an active objective is
resolved, its route or issuing settlement remains red until turn-in. Quest-offer
dialogue itself never presents a separate travel action.
### Rest Stops
- A point may be made into a rest stop, at which you will rest for the day once you arrive.
- Placing a rest stop at an inn allows you to fully rest faster (no watch schedule or tent pitching) increasing the amount of time available each day for traveling. The inn also has a cost, but this is trivially cheap unless you are an impoverished mendicant.
- The time between each rest stop *should be* 24 hours. Your cursor when placing points displays the expected arrival time, but the longer your characters go without resting the slower their travel speed and worse their combat abilities will be.

### Camping and hourly rest

The party leader chooses the fatigue percentage at which the first tiring party
member should make camp. The map control is a slider and saves when released;
the route preview updates immediately. Camp rest advances every member by the
selected duration and removes their recoverable travel fatigue without
settlement food, water, or a gold cost. The recommended camp duration is the
longest time any party member needs to remove fatigue; it does not extend the
rest to finish healing injuries.

At an inn or church, resting remains personal rather than party-wide. The rest
control can switch between **Hours** and **Days**; its recommendation heals the
active character's injuries first, or removes their fatigue when uninjured.
Recommendations of a day or longer are shown in days.
### Food
- Based on the total duration of the trip (and the return trip, which by default is to just follow the original trip backwards), you will need to bring food.
- We do not actually want players to micromanage their inventory. Your character will automatically purchase sufficient rations for the journey before they set out, displaying the cost of this on the screen before you finalize your travel plan.
- Your rations will be replenished by staying at inns. This will be accounted for when setting out. For example, if you are on a ten-day journey and plan on stopping at an inn on the fifth day, then your character will only set out with five days of rations (+30%) each day.
- Likewise, characters will automatically eat food throughout the day. We do not care to keep track of exactly what kind of food you have (for now), its just an abstract "food" resource.
- Since you can adjust your travel plan as-needed throughout the journey, you may end up taking longer than you initially rationed for. By default you'll bring, say, ~30% extra rations to account for this. But if you run out of rations you don't necessarily starve. It will effectively reduce your travel speed based on how easy it is to forage in a given area. Lush forests therefore are very forgiving, whereas in harsh desert and tundra it may not even be possible to subsist on foraging at any travel speed.
### Water
- You do not normally set out with all the water that you expect to drink in a journey, unless its a very short trip. Water is very heavy and you have to consume a lot of it.
- Fortunately, its easier to obtain water than it is food (in most places)
- You instead bring a certain *capacity* for water, which starts out full, and must periodically stop at places where it can be replenished
- Any source of freshwater as well as settlements and inns (via wells) can provide water. It is normally pretty easy to plan around this, but certain environments (deserts, obviously) make it extremely difficult.

### Current provisioning implementation

Every destination offers **Provision and travel** or **Travel
underprovisioned**. Provisioning purchases each party member's personal
shortfall and fills their waterskins. Settlement journeys plan for the one-way
duration plus 30%. Quest journeys plan for the outbound and expected return
duration plus 30%, because quest locations cannot resupply the party. Existing
rations and waterskins reduce the purchase.

Each party member pays for their own provisions. The reducer validates and
performs every purchase and the travel atomically, so insufficient funds leave
the entire party unchanged. A party member's travel request records whether
provisioning was requested for the captain's eventual approval.

Travel consumes personal rations and carried water automatically. Settlement
arrival immediately clears hunger and thirst and refills owned containers;
quest arrival does not. Foraging, intermediate freshwater stops, weather-based
water use, spoilage, food quality, and manual eating or drinking remain future
layers.
