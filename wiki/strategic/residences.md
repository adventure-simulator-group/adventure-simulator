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

A legal residence holding is distinct from a designated primary home. A
character may keep multiple purchased houses in multiple settlements but may
designate at most one active holding as home. A new purchase never destroys an
older property. Renting pays the first 30-day period up front and is limited to
one non-relinquished rental; buying pays the purchase price up front. A
specific holding can be sold or relinquished, explicitly removing its
occupants and primary designation while retaining its immutable history. A
dormant purchased holding can be recovered and any eligible active holding can
later be designated.

Subsequent bills settle lazily from the owner's personal clock in whole 30-day
periods, so advancing a month once has the same result as advancing it in
smaller chunks. Billing considers every active holding in chronological
`(due date, holding ID)` order, including maintenance and property tax on
nonprimary purchased property. Each period has separate auditable base
housing, adult necessities, and dependent necessities amounts. The occupant
counts are evaluated from admission/removal history at that due minute, so a
spouse or dependent child increases the household's upkeep while they live
there, and no longer affects later bills after removal, marriage end, or
widowhood. Each period is an indivisible charge: partial funds are retained,
and the first unpaid date remains the authoritative failure frontier. A
successful period advances the next due date by exactly `30 × 1,440` minutes.
An unpaid rental loses occupancy; an unpaid purchased holding becomes dormant
but its ownership history remains.

An active residence is a distinct settlement rest provision. It is never
represented as `at_inn = false`, which remains the temple/religious service
path. Home rest supplies full board without a per-stay inn charge; rent,
maintenance, and property tax remain the recurring cost. A residence supplies
local lodging and tiered Leisure morale only in its own settlement; it neither
makes another settlement free nor replaces travel costs.

The residence ledger is private gateway authority. A resident receives home
rest and comfort through an explicit occupancy edge to the renter or owner,
so spouses and dependent children receive the same tier benefit without
pretending to own the home. Public admission requires living co-located
characters and an active shared household or spouse/parent/child relationship;
an existing place in another home is rejected rather than silently stolen.
Wedding and birth settlement use a private atomic move after establishing the
new household or kinship. A character has one active household membership.
Ending a marriage or widowhood releases the marriage household and guest
occupancy deterministically, while a residence holder keeps their own home.

The web gateway reads only `BackendCharacterResidenceStatus`, a gateway-only
projection of the selected character's active primary or occupied holding.
Browser routes never query legal holdings, primary designations, occupancy
edges, or charge ledgers directly.

Residence and spouse Leisure are each represented by one refreshable morale
source rather than a new stackable event per rest. Residence morale is capped
at 8 points and lasts seven days; spouse Leisure is capped at 12 points and
lasts 30 days. Their combined contribution is capped at 16 points. Refreshing
adds only already-realized morale, clamps the source to its cap, and moves its
expiry from the refresh minute. A zero gain neither creates nor prolongs a
source. Residence comfort earns only the tier multiplier's premium over
already-realized baseline Leisure morale. Spouse Leisure earns two
milli-morale per conserved joint minute before its source cap is applied.
