# Tactical combat server guide

Combat timing and outcome tuning is authored in `content/tactical/combat.yaml`
through the typed schema owned by `adventuresim-tactical-core::combat_config`.
Do not add a server-local tuning constant or fallback value; thread the
validated `TacticalCombatConfig` field through the authoritative resolution
path.
