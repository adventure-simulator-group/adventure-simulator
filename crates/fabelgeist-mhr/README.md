# fabelgeist-mhr

Native Burn implementation of [MHR](https://github.com/facebookresearch/MHR)
(Momentum Human Rig), Meta's parametric 3D body model.

The crate reads the released assets directly — the binary FBX rig, the momentum
`.model` parameter transform, and the `.npz` pose correctives — and runs the
whole forward pass on Burn tensors. There is no Python, no FBX SDK, no
`pymomentum`, and no C dependency anywhere in the path.

## Model

| Input | Shape | Meaning |
| --- | --- | --- |
| `identity` | `[batch, 45]` | 20 body, 20 head, 5 hand shape coefficients (roughly -3..3) |
| `model_parameters` | `[batch, 204]` | joint angles (radians), translations, scales |
| `expression` | `[batch, 72]` | facial expression coefficients (roughly -1..1) |

| Output | Shape | Meaning |
| --- | --- | --- |
| `vertices` | `[batch, V, 3]` | posed vertices, centimetres |
| `skeleton_state` | `[batch, 127, 8]` | global joint `[tx, ty, tz, qx, qy, qz, qw, s]` |

The pipeline is the reference one: blend shapes give a rest mesh, the parameter
transform maps model parameters to 7 joint channels each, forward kinematics
produces global joint states, a small MLP adds pose-dependent corrective
offsets, and linear blend skinning poses the result.

Vertices per level of detail: 73 639 (LOD 0), 18 439, 10 661, 4 899, 2 461,
971, 595 (LOD 6). The 127 joints, the parameter layout and the 117 blend shapes
are the same at every LOD.

## Assets

Download `assets.zip` from the
[MHR releases](https://github.com/facebookresearch/MHR/releases) and unpack it;
the default path this crate looks in is `D:/AI/Models/mhr`, with or without the
`assets/` subdirectory:

```
compact_v6_1.model                  parameter transform, sets, limits
lod{0..6}.fbx                       rig, mesh, skin weights, 117 blend shapes
corrective_activation.npz           sparse activation layer, shared by all LODs
corrective_blendshapes_lod{0..6}.npz    corrective basis for that LOD
```

`mhr_model.pt` is not used; it is the reference TorchScript build.

## Usage

```rust
use burn::tensor::{Device, Tensor};
use fabelgeist_mhr::{Mhr, MhrConfig, NUM_IDENTITY_BLEND_SHAPES};

let device = Device::default();
let model = Mhr::from_files("D:/AI/Models/mhr", MhrConfig::default(), &device)?;

let identity = Tensor::zeros([1, NUM_IDENTITY_BLEND_SHAPES], &device);
let output = model.forward(identity, model.zero_parameters(1), None)?;
```

Examples:

```bash
cargo run --release -p fabelgeist-mhr --example inspect_assets -- --lod 1
```

## Character creator

`examples/character_creator` is a Bevy front end for the identity and
expression coefficients: 45 body/head/hand sliders, the 72 expression
channels, LOD and corrective switches, and JSON recipes. Bevy is a whole
engine, so it lives behind the off-by-default `character-creator` feature.

```bash
cargo run --release -p fabelgeist-mhr --features character-creator --example character_creator -- --assets D:/AI/Models/mhr
```

Its **Shading normals** selector is the reason it lives here. *Geometric* takes
`MhrOutput.normals` as the forward pass emits them, area-weighted over the
deformed triangles. *Authored* reads the rig's own `ByVertice` normals out of
the FBX — which the model itself never uses, and which disagree with the
geometry at about a quarter of the vertices — encodes each one in a local
surface frame at rest, and rebuilds that frame from the generated vertices, so
authored shading survives identity, expression, skinning and correctives.

## Task integration

`Mhr` implements `burn_tasks::BodyModelTask`, the workspace's task trait for
parametric body models — identity, pose and expression coefficients in, a
skinned mesh plus joint transforms out, with counts and units reported through
`BodyModelLayout` so callers do not hard-code a particular rig.

Every coefficient is described through `ParameterInfo`: MHR reports all 204
pose parameters under the names its `.model` file gives them, bounded by that
file's `[Limits]` section where it declares a bound (198 of 204), grouped for
display (Arms, Legs, Fingers, Torso & head, Scale, Flexible, Root), plus the 45
identity coefficients under their documented 20 body / 20 head / 5 hand split
and the 72 expression coefficients.

The **Body Model** tab of `examples/tasks` drives it: pick the LOD and the
correctives, then move any of the 45 + 204 + 72 sliders — grouped, filterable
by name, and ranged by the rig's own limits — and open the posed mesh in an
interactive wgpu window (drag to orbit, wheel to zoom, `B` for the skeleton,
`R` to reset).

```bash
cargo run --release -p tasks
```

## Memory

The corrective basis dominates: `3000 x V x 3` floats, which is 663 MB at LOD 1
and 2.5 GiB at LOD 0. Set `MhrConfig::pose_correctives` to `false` to skip it
when only the linear model is needed — loading then costs well under a second
and a few tens of MB.

## Accuracy

`tests/parity.rs` checks against values produced by the reference
implementation's own TorchScript model (facebookresearch/MHR v1.0.1, LOD 1) and
is skipped when the assets are absent. Over the whole mesh the largest
per-vertex deviation is 7.7 µm and the mean is 0.6 µm; the rig itself (joint
order, offsets, pre-rotations, mesh topology, UV indices, inverse bind pose,
skin weights, parameter names and limits) matches exactly.

The residual comes from the reference accumulating its kinematic chain in f64
while this port stays in the tensor dtype.

One precision note worth keeping: the joint parameters are computed with a
broadcast multiply and reduction rather than a matmul, because the CUDA backend
promotes f32 matmuls to tf32, and a 10-bit mantissa there costs ~2e-4 relative
accuracy on every joint angle and translation — which the kinematic chain then
amplifies to visible error. The tensor is 204 x 889, so the exact form is free.

## Known gaps

- Locators, collision geometry and FBX animation stacks are skipped; only what
  the body model itself needs is read from the rig.
- Momentum's non-`minmax` parameter limits (`linear`, `ellipsoid`, `halfplane`)
  are ignored. The MHR rig uses none of them.
- Vertex normals are not computed; exports carry positions and topology.
