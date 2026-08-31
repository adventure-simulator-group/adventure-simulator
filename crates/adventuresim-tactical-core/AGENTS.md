# Tactical gameplay mechanics guide

## Authored runtime parameters

`content/tactical/combat.yaml` is the canonical authored source for tactical
combat and animation tuning. Always place a new combat, movement, targeting,
impact-response, or animation tuning parameter in that YAML and represent it
as a typed field in `src/combat_config.rs`. Validate its physical range, keep
the Rust default exactly synchronized with the committed YAML, and carry the
server-loaded snapshot to every runtime consumer that needs the value.

Do not introduce a production numeric, boolean, duration, threshold, scale, or
mode constant as a substitute for a combat-YAML field. Compile-time structural
invariants, protocol limits, and mathematical identities may remain in code.
When shared core combat or autoresolve needs a configured value, pass a typed
projection into the shared calculation rather than duplicating a default or
reading mutable global state.
