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
- The current strategic implementation uses bounded deterministic A* over the installed 30 m terrain pack. Future route waypoints should be about planning around resources and threats, not manually approximating the shortest path.

## Viabundus road network

The historical world importer represents the Viabundus road system as graph
nodes and edges in the strategic database. The initial 1544 import admits land
roads and ferries; intermediate junctions, bridges, and ferry endpoints remain
in the graph so that a route is not incorrectly collapsed into a direct line
between settlements. Selecting a settlement name opens its overview, which
lists the next connected settlements on the road/ferry graph. Selecting a
destination asks the optional native terrain pack for the fastest bounded
route. Road movement is 5 km/h; open ground, sparse woods, and deep woods use
progressively slower base rates, and climbing adds a directional uphill
penalty. Water blocks movement unless road infrastructure identifies a
crossing. Each cell mixes Plains, Forest, and Hills from independent canopy and
hill coverage. Urban expertise is available on characters but is not inferred
from roads and has no route weight until built-up coverage is sourced.
The bounded party Terrain check for that mixture provides a 1.0–1.5 speed
multiplier and participates directly in A*, so different parties can prefer
different paths. Confirming recomputes the route at execution time, uses its aggregate
distance and terrain-weighted duration, and persists its package digest,
bounded polyline, ordered terrain spans, skill mixture, check, and exposure
discount for the active party journey.
Missing or incomplete terrain data falls back to the former straight-line
estimate and is explicitly labelled as such.

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

Quest travel is deliberately separate from the settlement-connectivity rule. A generated quest
stores an off-road point near its posting settlement. Terrain A* may follow a
road initially, leave it across open ground, and pass through woods to reach
that point; the return projection reverses the same terrain course. Both
settlement and quest-destination travel use the
shared Map tab. The leader configures walking time with a 0-24 hours-per-day
slider (eight by default) and chooses whether the party travels by day or by night. Day
travel centers the walking window on solar noon. Night travel centers its
contiguous walking window on midnight, which equivalently centers the camp and
downtime interval on noon. A sun/moon switch in the travel configuration saves
this choice with the party and immediately recomputes the remaining forecast.
These travel preferences remain available on the right side of the Map while
the party is at a settlement, camp, or quest destination. Provision purchasing
is shown only at settlements, where a market can actually fulfill it.

Actual movement accrues exactly 8 dirt per 1,440 travel minutes. A private
per-character remainder carries fractional progress across travel legs, and long
advances split at each dirt boundary so wound risk changes at the same minute
regardless of travel chunking. Camp/settlement rest, medical procedures, and
medication crafting advance strategic time without adding travel dirt.
Every minute outside the walking window is camp/downtime, so a full day's camp
interval is 24 hours minus the configured walking hours. A
member who cannot clear their fatigue in that interval carries it into the next
day. The reducer and preview use the same itinerary function, including partial
first and final walking days.

The runner-track preview contains exactly five compact vertical rails: Food,
Water, Fatigue, Terrain, and Day/night. The Terrain rail appears between Fatigue
and Day/night and shows road, open ground, sparse woods, deep woods, and stopped
camp intervals in journey order. Camp brackets span elapsed rest time while their
markers retain movement coordinates, and white progress advances through both
walking and rest. The fatigue rail shows party average, range, highest member,
and a warning at 100%. The Day/night rail follows absolute party time; midnight
ticks protrude right and show accessible lunar phases from the canonical
42,524-minute cycle, beginning with a new moon on Day 1. A journey longer than
a walking window stops at a persisted camp. The strategic layer persists the journey's
original endpoints, total duration, actual camp checkpoints, remaining
forecast, and the validated terrain route. SSE updates therefore keep every party member's tracker consistent
across camp rests and page navigations. A shorter-than-recommended camp rest
can legitimately add a future projected camp, but camps already reached never
disappear. While camped, the left rail keeps the journey's settlement endpoints
available, so the leader can redirect the remaining journey or turn back before
continuing. Choosing a different endpoint only changes the plan; the party can
rest before attempting the new leg. A party's
active quest destination is added to the settlement Map list only while the
party is at the quest's posting settlement; it carries a red exclamation. From
any other settlement, the Map traces the shortest road/ferry path to the
posting settlement and marks the next available settlement leg red. The current
settlement is a non-traveling Map row; it shows available quests in gold or a
completed active quest ready to report in red, with red taking priority. Once an active objective is
resolved, its route or issuing settlement remains red until turn-in. Quest-offer
dialogue itself never presents a separate travel action.

Journey rows carry an itinerary plan version. A pre-version active journey is
conservatively reconstructed from the party's current synchronized minute minus
its completed movement, then upgraded before travel continues. This may omit
unknown historical rest from an old row, but it never silently renders the
journey against Day 1 celestial chronology.
### Rest Stops
- A point may be made into a rest stop, at which you will rest for the day once you arrive.
- Placing a rest stop at an inn allows you to fully rest faster (no watch schedule or tent pitching) increasing the amount of time available each day for traveling. The inn also has a cost, but this is trivially cheap unless you are an impoverished mendicant.
- The time between each rest stop *should be* 24 hours. Your cursor when placing points displays the expected arrival time, but the longer your characters go without resting the slower their travel speed and worse their combat abilities will be.

### Camping, destination downtime, and hourly rest

Camp rest advances every living member by one common interval and consumes
provisions without settlement refill. Shared rations and water are used before
personal supplies. Each member rests until their own fatigue reaches zero; the
remaining interval applies safe saved downtime proportionally. Labor,
Thievery, and Raiding, including their rewards and incidents, are suppressed,
while healing and field repair retain their priority. Disease boundaries clip
the party to one common safe interval. Manual rest remains pinned to the bottom
of the Map's left sidebar at a settlement, an en-route camp, and a quest
destination, so a leader can clear fatigue or wait until a chosen time before
departing or beginning combat. Field rest is free. It reuses the wake-time
control from settlement rest, but permits an exact sub-24-hour interval;
choosing the current clock time means the next day's occurrence.
At an en-route camp, the recommended duration always targets the next absolute
start of the configured walking window rather than adding a fixed interval to
the camp's arrival time. If another system advances party time, the
recommendation shrinks toward that same scheduled wake time. Continue travel is
disabled outside the walking window and becomes available once it opens.

Only actual movement trains Terrain. Every living participant receives the
same conserved movement exposure divided across the persisted terrain mixture;
camp time is excluded. Road movement trains the underlying terrain at 25% for
open ground, 20% for sparse woods, or 15% for deep woods. Persisted span
overlap makes the result identical across multi-day chunks and offline
continuation.

The en-route header keeps its single Camp tab and existing rest and continue
actions. The raster scene combines a cut-paper tent with a small stone-and-log
firepit, while flame and smoke remain independent decorative SVG layers. A
newly reached camp shows flame, rising yellow-to-orange-to-red-to-grey fire
particles, and a denser smoke column that rises through the full height of the
location header; after rest is recorded at that movement checkpoint, the
flame and fire particles are omitted and only smoke remains. Reaching a later
checkpoint shows the fire again because the latest recorded rest no longer
matches the journey's current movement minute. Reduced-motion clients receive
static effects.

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

Travel has one **Begin journey** action. The route preview aggregates the
physiological reserves and provisions of every living member together with the
shared party inventory. Food and water rails show where each aggregate supply
runs out. Settlement journeys end at the settlement; quest journeys place the
quest destination partway down the rails and extend the estimate through the
return journey home, including return-leg camp estimates because a quest
location cannot resupply the party.

The leader can set a transient target surplus, including a negative target, in
Travel configuration. Its value uses the shared floating numeric editor also
used by daily skill allocations: click or focus it to type, use the arrow
buttons, keyboard arrows, or mouse wheel, then save or cancel. **Buy** opens the current settlement's General Market,
selects Party inventory, and stages the exact whole rations and waterskins
needed to reach that target. The transparent target rails are removed after
departure, when the settlement merchant is no longer available. It does not
submit the offer. Party gold pays when
the leader accepts the normal merchant offer; the former fixed 30% buffer and
automatic provisioning purchase no longer apply.

Travel consumes shared party rations before personal rations and shared
waterskin water before personal carried water. Every personal and party-held
waterskin fills freely immediately before departure from a settlement. Quest
locations do not refill water. Settlement arrival continues to clear hunger
and thirst and refill personal containers. Foraging, intermediate freshwater
stops, weather-based water use, spoilage, food quality, and manual eating or
drinking remain future layers.

# Emergency alcohol hydration

Movement consumes pooled water and then personal carried water before touching
alcohol. If a character still has a hydration deficit, travel may consume
ordinary potable alcohol whose explicit net-hydration value is positive;
medical-only/non-potable preparations and non-hydrating strong spirits are
never used. Multi-day movement is split at absolute evening boundaries for
needs processing, and each whole serving's ethanol is recorded in the nightly
history where its hydration deficit arose. Long reducer intervals and bounded
travel chunks therefore produce the same history. Generic waits do not invoke
this fallback.

The journey forecast uses the same item metadata and ordinary-alcohol
eligibility rules. It first reserves the whole servings expected to satisfy
Temperance-driven morale drinking only during projected camp intervals;
movement-only elapsed time creates no drinking opportunity. New journeys use
their calculated camp segments, while active journeys use persisted future
camp intervals clipped to the remaining elapsed span. Concrete units are
rounded separately and allocated in runtime order by absolute evening and then
character ID before remaining net hydration is counted. The planner presents ordinary water
and emergency alcohol separately, while its overall water-sufficiency verdict
uses their sum. Provisioning still stages waterskins only; it does not disguise
alcohol as water or automatically purchase it as a water container.
