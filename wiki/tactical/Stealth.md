# Strategic

This is the intended strategic/tactical contract. The stealth detection model and its tactical handoff are not yet implemented; [#212](https://github.com/adventure-simulator-group/adventure-simulator/issues/212) tracks the work. Current quest combat is not evidence that stealth scenes already exist.
When traveling, the party has a detection radius and a perception multiplier. The detection radius is the radius at which an enemy with a perception multiplier of 1.0 will detect you.

The detection radius is *mostly* based on party size versus the party member with the highest [stealth score](../shared/Stats.md), who is ostensibly scouting ahead of the rest of the party. But every party member's stealth score does contribute marginally, which could mean multiple scouts if there's multiple with relatively high scores or just ensuring that everyone avoids leaving tracks.

The party perception multiplier should essentially give more weight to the party members with higher stealth, as they are further ahead, and is fundamentally based on their [eyesight attribute](../shared/Stats.md).

If a party encounters an enemy party which does not detect them, they can choose to fight, sneak past, or take the long way around. The long way is guaranteed to succeed, but adds the most travel time. Sneaking past checks the stealth of *all* party members equally, so the weakest link can get you caught. If they choose to fight, then enter a tactical scenario in which the enemy party does not yet detect the players.

Strategic awareness is resolved once from independent deterministic party and
enemy rolls. Party-only awareness offers sneak, detour, or an allies-surprise
attack. Sneaking performs a second domain-separated whole-party check using the
weakest member; failure starts a normal battle. Enemy-only awareness starts an
enemies-surprise attack and bandits may demand surrender. Mutual awareness
offers a normal attack, sustainable-speed run when eligible, and bandit
surrender. Neither-aware results do not interrupt. Autoresolve never rerolls
stealth, so only the authoritative side receives a surprise opener.
# Tactical
The players start positioned relative to their stealth skill, essentially far enough away that enemies do not detect non-scouting party members regardless of line of sight.

For the MVP, we'll put some effort into the detection algorithm, accounting for light level, line of sight, and modify footstep sound by [stealth check](../shared/Stats.md) versus [weight](../shared/Encumbrance.md). But we aren't going to have a super detailed stealth AI. No patrol routes, search pattern logic, footprints, or enemies realizing that their allies are missing. At most, if they see a dead body they get a bonus to their ability to detect enemies due to now being on high alert. But in the future, there will be lots of opportunities to make this system more robust.

When an enemy does not detect you or for a short reaction-time window after they do, they are unable to [dodge](Combat.md). This translates to a significant surplus of [accuracy](Combat.md), allowing you to perform instantaneous kills on less-than-fully armored opponents or bypass the armor of fully-armored opponents.

Even an instantaneous takedown creates *some* noise, so unless nearby enemies are asleep this is typically just going to begin combat rather than allow you to take down an entire enemy camp in stealth.

If you do not manage to kill your target before their flat-footed timer has passed, they will alert nearby allies
