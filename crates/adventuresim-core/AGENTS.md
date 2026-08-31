# Shared game-mechanics guide

## Tactical combat tuning

When changing `src/combat.rs`, `src/combat/`, or tactical combat behavior in
`src/autoresolve.rs`, follow the runtime-parameter contract in
`../adventuresim-tactical-core/AGENTS.md`. New combat tuning belongs in
`content/tactical/combat.yaml` and must be passed into shared calculations as a
typed input; do not add a hard-coded production tuning constant here.
