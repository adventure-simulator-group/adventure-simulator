# Adventure Simulator Tactical Client

The authored humanoid scene is loaded from
`assets/animations/biped/unarmed/base.glb` for both the client-controlled
character and replicated remote characters. Keep the skinned character mesh in
the default scene when exporting the base rig; only authoring helpers such as
the placeholder weapon cylinder are stripped by `scripts/prepare_rig_base.py`.

Each motion is exported as a separate GLB with the same skeleton, hierarchy,
bone names, bind pose, and neutral root transform as the base rig. Until the
base scene is available, every character receives a generated T-pose fallback.
Missing motion files are resolved through the semantic fallback chain and
ultimately display the authored bind pose (or the generated T-pose if the base
scene itself is unavailable).
