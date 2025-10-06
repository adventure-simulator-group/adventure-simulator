# Strategic
When traveling, the party has a detection radius and a perception multiplier. The detection radius is the radius at which an enemy with a perception multiplier of 1.0 will detect you.

The detection radius is *mostly* based on party size versus the party member with the highest [stealth score](Stats), who is ostensibly scouting ahead of the rest of the party. But every party member's stealth score does contribute marginally, which could mean multiple scouts if there's multiple with relatively high scores or just ensuring that everyone avoids leaving tracks.

The party perception multiplier should essentially give more weight to the party members with higher stealth, as they are further ahead, and is fundamentally based on their [eyesight attribute](Stats).

If a party encounters an enemy party which does not detect them, they can choose to fight, sneak past, or take the long way around. The long way is guaranteed to succeed, but adds the most travel time. Sneaking past checks the stealth of *all* party members equally, so the weakest link can get you caught. If they choose to fight, then enter a tactical scenario in which the enemy party does not yet detect the players.
# Tactical
The players start positioned relative to their stealth skill, essentially far enough away that enemies do not detect non-scouting party members regardless of line of sight.

For the MVP, we'll put some effort into the detection algorithm, accounting for light level, line of sight, and modify footstep sound by [stealth check](Stats) versus [weight](Encumbrance). But we aren't going to have a super detailed stealth AI. No patrol routes, search pattern logic, footprints, or enemies realizing that their allies are missing. At most, if they see a dead body they get a bonus to their ability to detect enemies due to now being on high alert. But in the future, there will be lots of opportunities to make this system more robust.

When an enemy does not detect you or for a short reaction-time window after they do, they are unable to [dodge](Combat). This translates to a significant surplus of [accuracy](Combat), allowing you to perform instantaneous kills on less-than-fully armored opponents or bypass the armor of fully-armored opponents.

Even an instantaneous takedown creates *some* noise, so unless nearby enemies are asleep this is typically just going to begin combat rather than allow you to take down an entire enemy camp in stealth.

If you do not manage to kill your target before their flat-footed timer has passed, they will alert nearby allies