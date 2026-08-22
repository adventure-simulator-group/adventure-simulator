# Character creator

Native, non-authoritative character design tool backed by `fabelgeist-mhr`. It loads
Meta's Momentum Human Rig assets locally, exposes its 45 identity coefficients
and 72 expression coefficients, and previews the generated mesh in Bevy.

Install the pinned upstream assets into the ignored local authoring cache, then
start the creator from the repository root:

```powershell
just init-mhr-assets
just character-creator
```

The importer verifies Meta's MHR v1.0.1 release by size and SHA-256 and installs
the FBX rigs and model definition under `target/mhr-assets/v1.0.1/assets`.
That default cache is about 50 MB after extraction. Run
`just init-mhr-lod1-correctives` only when comparing the optional LOD 1
pose-corrective network; installing every corrective basis is an explicit
`scripts/init_mhr_assets.py --all-correctives` operation and consumes about
4 GB. Override the location with `--assets` or `MHR_ASSETS` when needed. The
downloaded archive and extracted source assets are not committed; deliberately
exported game and Cascadeur artifacts are tracked separately.

The default project is John Fabelgeist. **Save recipe** writes his versioned
parameters to `assets_src/characters/john_fabelgeist.json`; **Export rigged
GLB** writes the Cascadeur source to `assets_src/biped/unarmed/base.glb`.
The export is a zero-animation, identity-shaped T-pose containing MHR's 127
joints plus the three Fabelgeist animation attachments, both sets of skinning
influences, and inverse bind
matrices computed for the generated body. Run `just prepare-john-rig` after
saving to regenerate that source GLB and its validated spawnable copy at
`assets/animations/biped/unarmed/base.glb` without opening the studio.

The zero-weight attachment joints follow MHR's side-prefix naming convention:
`l_weapon` is parented to `l_wrist`, `r_weapon` to `r_wrist`, and `c_camera`
to `c_head`. Each weapon joint is positioned halfway from its wrist toward the
corresponding `*_middle1` knuckle, placing it in the generated palm. The camera
joint is positioned at the midpoint of the generated eye joints. Their
rotations inherit the wrist or head without mirrored negative scale.

Use the left panel to edit, randomize, reset, save, load, and export. Drag the
viewport to orbit and use the mouse wheel to zoom. The
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

The exported base establishes MHR's stable bone names and hierarchy as the
animation-pack contract. The preview keeps body identity separate from animation. Prism's retargeting
pipeline establishes the intended boundary: import a clip into an engine
skeleton, retarget model-space deltas through semantic rig profiles, then
encode the resulting MHR joint pose into MHR's 204 model parameters. Identity
remains this recipe's 45 coefficients, so one retargeted clip works for every
generated body. The creator currently shows a neutral pose; clip playback
should reuse Prism's `Retargeter`, `MhrRig`, and `MhrPoseEncoder`, including its
T-pose reference and hinge correction, rather than copying local rotations.
