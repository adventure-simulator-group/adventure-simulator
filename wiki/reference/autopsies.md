# Autopsies

Autopsies use durable strategic body state produced by ordinary combat
autoresolve. The corpse boundary stores final regional health, blood loss, and a
bounded sequence of physical injuries. It does not store tactical ticks,
positions, a replay, attacker identity, weapon identity, or a canonical
cause-of-death answer.

## Custody and decomposition

Death time and discovery time are separate. A body's displayed location is
derived dynamically from elapsed time after discovery:

1. at the scene for the first 90 minutes;
2. in local custody (for example, a church or residence) until 24 hours;
3. interred thereafter, unless it has been exhumed.

The party may also bury an accessible body early or rebury an exhumed body.
Burial and exhumation are symmetric handling actions. While a body is interred,
the observer projection conceals its identity, creature kind, source,
decomposition, opened state, and all recorded findings. Only the opaque body
reference, settlement custody, revision, and information needed to attempt
exhumation remain available.

Decomposition is calculated independently from time since death and accumulated
handling damage. Prompt scene examination retains both scene context and
better-preserved anatomy, while a promptly discovered but badly handled body can
still lose information.

Scene portraits are available only while the body is physically at the case
site. Once local people move or inter the body, its portrait and medical windows
move to the owning party's settlement view. Exhumation returns access there; it
does not teleport the body back to the scene.

## Medical workflow

A corpse portrait opens the existing Physiology and Surgery windows. Both
windows support external examination. `Open the body` is an ordinary Surgery
procedure: it is disabled for living patients and, on a corpse, unlocks internal
observations in both windows. Low Surgery produces incision damage and bounded
obscuration; it does not make Surgery interpret physiological effects or name a
culprit.

- Surgery observes wound geometry, tissue damage, and physical instrument
  properties.
- Physiology interprets systemic effects and possible physical mechanisms.
- Bestiary checks interpret already-observed signs through learned creature
  lore. They produce broad candidates from wound morphology and do not read a
  hidden attacker or species identity.

Evidence quality is resolved at the action's completion time, so decomposition,
loss of scene context, and damage from an imprecise opening can suppress or
weaken the result. Only that observer's realized finding is persisted. Another
character cannot obtain a private result merely by subscribing to corpse
authority.

## Permission

Permission is requested through an automatically available, corpse-specific
topic in ordinary dialogue. Explicitly bound family members, a local priest, or
a local secular authority may grant it after a Charm-based social check; refusal
is reported in the dialogue transcript. Examination and exhumation are separate
permissions, and exhumation is substantially harder to secure. Family permission
avoids social penalties. Priest or authority permission prevents settlement infamy, but
bypassing bound family causes a modest family morale and affinity loss.
Proceeding without any permission remains possible after a qualitative warning
and causes a much larger family penalty plus settlement infamy. Receipts make
all actions and consequences retry-safe.

Burning is an irreversible alternative beside burial whenever a body is
accessible. It destroys the body and all remaining evidence and cannot be
authorized by family, clergy, or secular authority. Burning a victim causes
severe settlement infamy and severe affinity loss with every explicitly bound
family member. A corpse produced from an enemy killed by the party in strategic
autoresolve is exempt from those social and reputation penalties, though the
irreversible evidence-loss warning still applies. A buried body must first be
exhumed before it can be burned.

Kinship comes only from explicit family bindings. Generated household labels are
never treated as proof of relation. The current generic character-death producer
calls the binding seam with no relatives because the simulation does not yet
have an authoritative kinship source; future authored or generated victims can
supply specific same-settlement NPC IDs.

## Autoresolve dogfooding

Enemy corpses after a strategic quest battle use the same post-combat-body
materialization function intended for generated victims. Incapacitation alone
does not create a corpse. A reusable bounded seed-search helper can run a
death-required incident through ordinary autoresolve and fail cleanly when the
designated victim survives every attempt.

The present quest-incident generator has no victim-combat producer, so this
issue deliberately adds the reusable helper and materialization seam without
inventing a second wound generator. A later outbreak/threat incident can call
that seam. Tactical combat parity is intentionally deferred while its combat
pipeline is being refactored.

## Development demonstration

The isolated-development **Autopsy demo** loader applies high Surgery,
Physiology, and broad Bestiary training to the selected character and stages a
recent victim, an interred victim, and a party-slain enemy in the character's
current settlement. Each body is produced by the same bounded strategic
autoresolve and post-combat-body materialization used by gameplay. The fixture
authors custody time, identity, and explicit family binding, but never wounds or
findings. See [Development Workflow](developing.md#autopsy-demo).
