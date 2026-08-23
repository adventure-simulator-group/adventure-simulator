# Biped runtime animation assets

```text
biped/
  unarmed/
    base.glb
    idle_relaxed.glb
    walk.glb
    run.glb
    swing.glb
    thrust.glb
    offhand.glb
    dive.glb
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

Guard movement is procedural around frame 0 of the selected `swing.glb` or
`thrust.glb`. The ordinary raised-guard foot-target planner handles forward,
backward, and lateral movement. There are no authored directional guard-motion,
alternate-stance, or stance-transition files.

Blocking is also procedural. Packs do not provide authored block motions.

`swing.glb` and `thrust.glb` are optional main-hand motions. Frame 0 is guard
and frame 4 is first contact; optional frames 8 and 12 provide recovery into
one buffered continuation and its contact. A pack that supplies either main
motion owns its complete main-hand attack set; another missing family remains
unavailable and is not borrowed from a parent. A pack that supplies neither
inherits the nearest parent's main-hand set. This availability is
gameplay-significant: a character cannot request a strike family with no usable
initial contact pose. Preferred input uses the alternate family when only that
family is available.

`offhand.glb` inherits independently. A single-frame file supplies the release
contact. A two-anchor file uses frame 0 for held preparation and frame 4 for
contact.

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
