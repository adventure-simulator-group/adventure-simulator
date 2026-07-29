# Settlement reputation

Fame and Infamy are independent, nonnegative public reputations recorded for
each character and settlement. Fame represents favorable stories; Infamy
represents known crimes, frightening conduct, and public scandals. They do not
erase one another.

The authoritative unit is an integer centipoint. Aggregates saturate at a
fixed cap. Every action creates one private immutable event with a retry-stable
source identity. Its local and road-network contributions are persisted as
idempotent receipts. Only the original action event spreads: aggregate values
and received contributions never propagate, so road cycles cannot create a
feedback loop.

Population has two separate effects. A large origin settlement dilutes the
local change logarithmically, while increasing the event's bounded road
distance. Missing estimates use an explicit settlement-level fallback.
Spillover has a fixed normalized budget, a destination limit, deterministic
distance/ID ordering, and is calculated once using the roads and populations
that existed when the event occurred.

Implemented sources are visible religious professional practice and resolved
cases for Fame; thievery, raiding, discovered illegal foraging, and discovered
activity incidents for Infamy. Ordinary carousing changes no reputation. It
has a bounded chance of a disorder incident, with substantially greater risk
for a Drunkard and lower risk for a Temperate character. Within an activity
interval, retaliation is checked first, theft discovery second, and carousing
disorder third, preserving the one-incident-per-party ordering.

Settlement NPC checks receive a bounded local reputation modifier that falls
to zero as personal familiarity reaches its cap. Personal affinity itself is
not rewritten. When local Infamy exceeds Fame by the arrest threshold, the
watch may create a typed arrest incident. Surrender atomically pays a bounded
fine; insufficient funds leave both money and incident unchanged. Reputation
alone never authorizes execution. All currently implemented discovered
offenses explicitly record that they are not execution-eligible.

The current typed strategic-incident seam interrupts the whole travelling
party even though the offense and fine remain attached only to the instigator.
Companions are not fined and do not receive the instigator's Infamy. A future
character-local custody scene can remove the interruption limitation without
changing offense provenance.
