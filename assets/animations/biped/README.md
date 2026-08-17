# Biped runtime animation assets

```text
biped/
  unarmed/
    base.glb
    idle_relaxed.glb
    walk.glb
    run.glb
    guard.glb
    swing.glb
    swing_follow.glb
    thrust.glb
    duck_forward.glb
    duck_backward.glb
    duck_left.glb
    duck_right.glb
    ...
```

`base.glb` is the only spawnable scene and is generated from
`assets_src/base.glb` by `scripts/prepare_rig_base.py`. Every other file is an
exact validated copy of its export under `assets_src/biped/unarmed/`, prepared
by `scripts/prepare_animation_motion.py`, and contains one coherent motion or
single pose. Animation names inside the GLB are ignored; the code-owned catalog
maps semantic poses to exact file/frame pairs.

The on-disk pack directory is `unarmed`; its semantic pack ID is
`humanoid_unarmed`. Future compatible packs may sit beside it and declare one
parent pack.

Guard movement is procedural. `guard.glb` supplies one static whole-body guard,
while the ordinary raised-guard foot-target planner handles forward, backward,
and lateral movement. There are no authored directional guard-motion,
alternate-stance, or stance-transition files.

`swing.glb`, `swing_follow.glb`, and `thrust.glb` are optional single contact
poses. A pack that supplies any of these three owns its complete attack set;
another missing attack remains unavailable and is not borrowed from a parent.
A pack that supplies none inherits the nearest parent's attack set. This
availability is gameplay-significant: a character cannot request a strike
family with no usable initial contact pose. Preferred input uses the alternate
family when only that family is available.

Attacks do not own foot targets. Their authored full-body rotations may bend
knees and pivot feet, but live guard locomotion and terrain IK continue to plan
the same feet they would have planned without the attack.

Motion GLBs retain their exported scenes and meshes because the runtime loads
only their animation asset. Preparation validates the one-animation, duration,
and canonical target-path contracts, then copies source bytes exactly. Missing
non-attack motions use the deterministic semantic and parent fallback chains;
if nothing resolves, the runtime keeps the authored bind pose rather than
crashing or hiding the actor.

The runtime can evaluate these files through either Bevy graph playback or the
pose-buffer backend. Pose-buffer playback bakes curves to 30 Hz in memory,
substitutes bind transforms for missing or invalid tracks, and removes root-bone
translation. Neither backend grants an authored root track authority over the
gameplay entity.
