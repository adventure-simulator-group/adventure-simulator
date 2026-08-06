# Biped runtime animation assets

```text
biped/
  unarmed/
    base.glb
    idle_relaxed.glb
    walk.glb
    run.glb
    guard_lead_left.glb
    attack_thrust_lead_left_contact.glb
    duck_lead_left_left.glb
    ...
```

`base.glb` is the only spawnable scene and is generated from
`assets_src/base.glb` by `scripts/prepare_rig_base.py`. Every other file is an
exact validated copy of its export under `assets_src/biped/unarmed/`, prepared
by `scripts/prepare_animation_motion.py`, and contains exactly one complete
coherent motion at 30fps. Animation names are ignored;
the code-owned catalog in the tactical client maps semantic anchors to exact
file/frame pairs documented in `wiki/client/animation.md`.

The ergonomic on-disk pack directory is `unarmed`; the runtime semantic pack ID
remains `humanoid_unarmed`. Future specialized pack directories sit beside it
under `biped/<pack-directory>/` and may inherit compatible motions from their
single fallback pack.

Motion GLBs retain their exported scenes and meshes because the runtime loads
only their animation asset. The preparation step does not rewrite them: after
validating the one-animation, duration, and canonical target-path contracts it
copies source bytes exactly, and `--check` verifies the committed result.

Missing motion files are expected while art is in progress. They participate
in fallback independently. For guard, attack, and guard-relative duck pairs,
an exact side wins; otherwise an available same-pack opposite side is mirrored
before the parent pack is consulted. The remaining similar-pose chain ultimately leaves the
authored base rig in its T-pose rather than crashing or hiding the actor.
