# Character creator

Native, non-authoritative character design tool backed by `fabelgeist-mhr`. It loads
Meta's Momentum Human Rig assets locally, exposes its 45 identity coefficients
and 72 expression coefficients, and previews the generated mesh in Bevy.

```powershell
cargo run --manifest-path crates/adventuresim-character-creator/Cargo.toml --release -- --assets D:/AI/Models/mhr
```

Use the left panel to edit, randomize, reset, import, and export a versioned
JSON recipe. Drag the viewport to orbit and use the mouse wheel to zoom. The
tool defaults to MHR LOD 1 with pose correctives disabled, preserving facial
and finger topology while keeping edits interactive. The **Pose-corrective
model** checkbox reloads the selected LOD with or without MHR's corrective
network for direct comparison. Recipes contain model coordinates, not authoritative character
state, and must be regenerated and validated when connected to game creation.

The preview reads each LOD's authored `ByVertice/Direct` normals from its MHR
FBX. It stores those normals in local rest-surface frames and reconstructs the
frames from the final generated vertices, so authored shading follows identity,
expression, skinning, and optional pose-corrective displacement. Triangle-only
normal reconstruction is retained internally only to define those frames; it is
not sent to Bevy as the character's shading normal.

## Animation integration

The preview keeps body identity separate from animation. Prism's retargeting
pipeline establishes the intended boundary: import a clip into an engine
skeleton, retarget model-space deltas through semantic rig profiles, then
encode the resulting MHR joint pose into MHR's 204 model parameters. Identity
remains this recipe's 45 coefficients, so one retargeted clip works for every
generated body. The creator currently shows a neutral pose; clip playback
should reuse Prism's `Retargeter`, `MhrRig`, and `MhrPoseEncoder`, including its
T-pose reference and hinge correction, rather than copying local rotations.
