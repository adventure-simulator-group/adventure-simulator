# Local problems

Local problems are persistent strategic conditions which affect settlements
and roads before a character knows their cause. They are independent of legacy
quests, contracts, rewards, and tactical tick state.

## Authority and privacy

`adventuresim-core::local_problem` owns deterministic weighted generation,
absolute-time lifecycle evaluation, stable aggregation, caps, and checked price
adjustment. Relations have separate plausibility and curation weights. Zero is
impossible; rare relations may require a causal bridge, whose key is emitted for
later evidence authoring.

Cause, disease identity, encounter archetype, opaque case reference, weights,
bridges, and generation explanation are private. Public rows contain only
observable symptoms and bounded consequences. Observer-facing code must not
expose private rows through logs, subscriptions, or serialized dialogue.

## Lifecycle and consequences

A problem has an absolute interval, monotonic mitigation, and an optional
earliest resolution minute. At most three active rows contribute to a scope, in
stable ID order, with aggregate caps.

- Trade pressure is applied after base and language pricing by the same checked
  integer basis-point function used for UI quotes and reducer settlement. Food
  lots are included; merchant catalog stock remains infinite.
- Route problems use a canonical sorted endpoint pair and affect only existing
  canonical encounter boundaries. Entropy domains, retries, chunking, and
  impossible habitat weights are preserved.
- Disease pressure reuses `first_eligible_presence_exposure_minute` with the
  stable problem ID as exposure source. Disease identity remains private until
  diagnosed.

The internal outcome seam accepts an idempotent source ID, authoritative minute,
monotonic mitigation, or resolution. It is not a public reducer and cannot
accept contracts, complete objectives, or pay rewards.

## Markerless discovery

Dialogue with an available inn NPC surfaces one unknown unresolved problem.
Overview dialogue does so only where no inn NPC is available. Safe rumors give
the symptom and refer to a persistent NPC by name, visible description,
occupation, and expected location tab. A private per-character receipt records
the source and opaque case reference.

Later conversations with ordinary locals give a short summary and referral
instead of replaying discovery. Discovery does not accept or mutate a quest,
reveal a cause or destination, or create a map marker.
