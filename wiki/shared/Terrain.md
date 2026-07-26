# Map

The terrain profile currently affects strategic routing and can select possible scene context. Tactical resolution of hazardous terrain is intended but not yet implemented; [#212](https://github.com/adventure-simulator-group/adventure-simulator/issues/212) tracks the handoff and committed-result contract.
The world map is a grid where each square has a height and an enum for the terrain type. Each of these affect both the speed of travel and the difficulty of climb/swim check to avoid injury. We should not try and create our own, we should be able to find both height and biome data from some open GIS dataset. At minimum, it should be easy to find modern data for these, but there may also be a historical dataset that we can use.

The strategic import records a bounded terrain profile on each road/ferry edge:
elevation, ascent/descent, grade, slope/aspect, roughness, relief, landforms,
water adjacency, and versioned seasonal/encounter tags. Because the historical
road source has endpoint topology but no polyline, these facts explicitly use
straight endpoint geometry. They select strategic travel and possible scenes;
a tactical server still owns all live terrain interaction.

## Height
Traveling from a lower height to a higher one may require the characters to make a climb check (based on upper body strength vs weight) based on the slope, and traveling from higher to lower may require the characters to make an agility check.

## Terrain Type
As this is an enum, we can store extra information in each variant. For example, the "River" variant could include its depth and velocity, both of which would contribute to how hazardous it is to ford.

In addition to the costs imposed to traveling, it may also affect stealth and detection. Forest cover may largely prevent detection from flying enemies but also make it much easier for an enemy to ambush you, presenting a trade-off of risk which you might assess based on which enemies are known to exist in an area and which you are better able to defend against.

## Pathfinding
As described in the [travel](../strategic/Travel.md) page.

# Weather
This probably isn't worth putting in the MVP, especially for such a temperate place like Italy, but eventually there should be a weather map that affects the way that you travel though terrain. Heavy snow would slow down land travel, frozen lakes and rivers become possible to cross without swimming (but also risky if the ice is thin), and rain makes climbing very difficult.

# Points of Interest
For the MVP, there is no need for any point of interest other than enemy camps/lairs/nests relevant to active [quests](../strategic/Quests.md) in an area as well as [settlements](../strategic/Settlement.md). The former are simply placed randomly (though not too close to any other point of interest), the latter should be obtained from a GIS dataset. If we can't find *historical* GIS data on Italian settlements then we can just use modern data and rely on [Cunningham's Law](https://meta.wikimedia.org../Cunningham%27s_Law) to fix it.

# Underground
In our setting, there is ostensibly a vast underground network of caves, crypts, tunnels, Ratling under-cities, Dwarven strongholds, and even antediluvian ruins. But this sounds hard, therefore we shouldn't bother with it for the MVP. All of the quests will conveniently take you to overland locations, which don't even need to have structures. 
> Halbe: You are essentially being hired by local municipalities to clear out homeless encampments. If only we had this in the IRL modern setting...
## Foraging

Personal foraging trains only the Plains, Forest, and Hills leaf skills in the
normalized mixture of the character's current 1 km vicinity. The Terrain
heading remains a presentation aggregate and is never awarded or stored.
Cultivated ground affects legality, not the biome mixture.
