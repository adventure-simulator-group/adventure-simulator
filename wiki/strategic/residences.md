# Residences

Every settlement has three permanent, nonexclusive residence offers: Cheap,
Moderate, and Fancy. Each tier has named, tuneable purchase price, 30-day rent,
owner maintenance, property tax, and Leisure-morale values. Payments use the
settlement's normal inventory currency; the legacy `Character.gold` field is
not an economic authority.

The catalog validator requires exactly `[Cheap, Moderate, Fancy]` in that
order. Purchase price, rent, maintenance, property tax, and Leisure benefit
must each increase strictly from one tier to the next. For every tier,
maintenance plus property tax must remain lower than rent.

A character designates at most one primary residence. Renting pays the first
30-day period up front; buying pays the purchase price up front. Subsequent
bills settle lazily from the character's personal clock in whole 30-day
periods, so advancing a month once has the same result as advancing it in
smaller chunks. A failed rent, tax, or maintenance charge deactivates the
residence without deleting ownership history. Reacquisition is explicit.
Billing iterates due dates chronologically. Each period is an indivisible
charge: partial funds are retained, and the first unpaid date remains the
authoritative failure frontier. A successful period advances the next due date
by exactly `30 × 1,440` minutes.

An active residence is a distinct settlement rest provision. It is never
represented as `at_inn = false`, which remains the temple/religious service
path. Home rest supplies full board without a per-stay inn charge; rent,
maintenance, and property tax remain the recurring cost. A residence supplies
local lodging and tiered Leisure morale only in its own settlement; it neither
makes another settlement free nor replaces travel costs.

Residence and spouse Leisure are each represented by one refreshable morale
source rather than a new stackable event per rest. Residence morale is capped
at 8 points and lasts seven days; spouse Leisure is capped at 12 points and
lasts 30 days. Their combined contribution is capped at 16 points. Refreshing
adds only already-realized morale, clamps the source to its cap, and moves its
expiry from the refresh minute. A zero gain neither creates nor prolongs a
source. Residence comfort earns only the tier multiplier's premium over
already-realized baseline Leisure morale. Spouse Leisure earns two
milli-morale per conserved joint minute before its source cap is applied.
