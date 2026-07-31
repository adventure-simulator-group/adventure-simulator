# Residences

Every settlement has three permanent, nonexclusive residence offers: Cheap,
Moderate, and Fancy. Each tier has named, tuneable purchase price, 30-day rent,
owner maintenance, property tax, and Leisure-morale values. Payments use the
settlement's normal inventory currency; the legacy `Character.gold` field is
not an economic authority.

A character designates at most one primary residence. Renting pays the first
30-day period up front; buying pays the purchase price up front. Subsequent
bills settle lazily from the character's personal clock in whole 30-day
periods, so advancing a month once has the same result as advancing it in
smaller chunks. A failed rent, tax, or maintenance charge deactivates the
residence without deleting ownership history. Reacquisition is explicit.

An active residence is a distinct settlement rest provision. It is never
represented as `at_inn = false`, which remains the temple/religious service
path. Home rest supplies full board without a per-stay inn charge; rent,
maintenance, and property tax remain the recurring cost. A residence supplies
local lodging and tiered Leisure morale only in its own settlement; it neither
makes another settlement free nor replaces travel costs.
