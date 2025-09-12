# Overview
Our network will facilitate the bare minimum of what can be considered a single-world MMO. This does not mean that there is one server handling all player interactions, however, or that we have some kind of complicated mesh network. The only thing that actually needs to be synchronized between everyone is their [character](Character) data so that you can form (small) parties with any player. Each party is assigned its own virtual server, and all gameplay is simulated off of that server. Even [travel](Travel) in the world map is instanced, so you can't actually encounter other parties out in the wilderness or see the same groups of enemies as them.

# World Server
The World Server stores your character's information when you are logged out. It tracks your stats, equipment, and which settlement you logged out in. When you log in, your information is handed off to a settlement server governing the settlement that you logged out in.
# Settlement Server
The purpose of this server is to generate [quests](Quests), handle [trading](Trade), and serve as a register for parties. You must have a party to set out on a quest or travel to another settlement.
# Party Servers
Each party of characters (~1-12 players/NPCs) is provisioned a party server. A party server runs a headless version of the gameplay and all interactions are mediated through it, there are no peer-to-peer connections (for simplicity). Everything about [real-time gameplay](Combat) and [travel](Travel) except player input is simulated on the party server then synchronized to each client. The only gameplay-relevant events that originate from client servers are input. Clients do not use a rollback system or anything to anticipate the future state of the server and process gameplay independently, they just have to wait for double ping-time to see the effects of their inputs (for simplicity). This means that there *would* be a very noticeable amount of input lag, but the [secondary animation system](Animation) which exclusively runs client-side can mitigate this by beginning animations immediately and extending their duration. There is no PvP in the MVP, but we will eventually want to support it.
## Logging Out
If you log out while other characters in your party are still logged in, then your character becomes an NPC until they return to a settlement or log out themselves. If you are not in a settlement when everyone in a party server logs out, then your party server remains active long enough to control your character(s) back to the nearest settlement. You are warned about this behavior when you log out.
## PvE Combat
### Network Rundown
1. Attacking Client declares an attack and sends input to server.
2. Server receives attack input and immediately decides what the enemy's reaction time is, informing all clients about the attack, whether it will actually hit, where it will hit, how much damage will it do, and when it will hit
3. Attacking client updates animation to account for the expected outcome, other clients are able to begin the attack and dodge animations at the same time (but may choose to render an artificial reaction time on the dodge)
You cannot cancel attacks, or at least if you can then it has to be done double pingtime before when it would canonically hit so that there's no need for rollback 

## PvP Melee Combat (not in MVP)
### Cheating
We do not care about detecting or preventing cheating. Having a pro-gamer level reaction time is less of a factor in [combat](Combat) than having good stats, so there's an upper-limit on how much of a problem cheaters are.
### Network Rundown
1. Attacking Client declares an attack and sends input to server
2. Server forwards attack to all clients, which includes a **tentative** speculation of whether it will actually hit, where it will hit, how much damage it will do, and when it will hit **based on the assumption that the defender will either not attempt to dodge, or will not dodge quickly enough**
	1. If it takes the defender longer than X milliseconds to respond then they'll receive a [direct/critical](Combat) hit *anyways* so past that point it doesn't matter if you even try to dodge, the other clients can just assume that you didn't
3. Defender receives attack and may or may not respond with a dodge input in time
4. (optional) Server forwards dodge response to all clients
5. Clients receive dodge response and can now be certain of what will happen, updating their animations accordingly
## PvP Ranged Combat
### Option A: Similar to melee
Ranged combat can be treated similar to melee combat, where the duration of the attack is a function of both how long it takes to aim the weapon and how long it would take the projectile to reach its target. This should be good enough for bows, slings, and throwing weapons, which don't allow you to instantly loose a projectile (holding a bow drawn is exhausting) so its not that bad to say that pointing at an enemy and shooting is something that can plausibly take >0.4s. You **cannot** draw a bow first, *then* choose an enemy, and like with melee you either can't cancel attacks or can't cancel by the time that other clients believe that an arrow has been loosed. This will work poorly with crossbows and especially with firearms, both of which don't have to be loaded immediately before firing and the latter has projectile speeds that are too fast to work well here. They can still be functionally implemented though, as the attack duration just represents your character taking the time to aim at a particular enemy.
### Option B: No dodging
To speed up ping duration, we don't assume that the defender can plausibly dodge ranged attacks (at least from loaded crossbows/firearms). Thus the outcome of the attack can be canonically decided as soon as the server receives it. This will be very frustrating for players (though realistic), but it can be mitigated via [luck](Magic).
### Option C: Simulate projectiles
Avoiding projectiles now has nothing to do with a defense calculation and is entirely simulated via physics. The accuracy of the projectile doesn't determine whether it will hit, but rather whether it will hit given that the defender does not adjust their movement trajectory or [primary animation](Animation). This will make long-distance combat better, as otherwise projectiles will have to track characters that change their course if the server has decided that they will hit. But it is more complicated, therefore not planned for the MVP