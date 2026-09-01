# Hawthorn leaf procedural prior art

Internal research draft. This is not public-facing documentation and does not authorize source implementation by itself.

## Practitioner sources inspected

### SideFX forum: art-directable plant hierarchy

- Source: SideFX Forums, “about modeling plants and vegetations,” replies by `aeaeaeae` and `Digipiction`, 2023: https://www.sidefx.com/forum/topic/92910/
- Direct observation: practitioners warn that a general L-system can be hard to art-direct, and describe a controllable hierarchy of noised trunk/stalk lines, scattered branch lines, scattered twig lines, and leaves copied to twig points.
- Transferable method: represent related biological responsibilities as separate deterministic stages with explicit controls. A leaf silhouette generator should not also own shrub placement or crown topology.
- Repository constraint: this task is only `HawthornLeaf` texture generation. The existing woody-plant skeleton, instance distribution, and leaf-card geometry remain authoritative and out of scope.

### SideFX Labs: profile and deformation as independent controls

- Sources: SideFX Houdini 22 documentation, “Labs Tree Simple Leaf” and “Labs Tree Leaf Generator”: https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_simple_leaf.html and https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html
- Direct observation: SideFX exposes leaf profile, segmentation, fold, bend, point jitter, placement, orientation, pruning, seeded variation, variant selection, and instancing as independent parameters.
- Transferable method: keep outline, relief/fold, material channels, and plant placement separable. For the HawthornLeaf texture, derive opacity from an explicit profile and normals from a separate relief field rather than allowing generic noise to define both.
- Repository constraint: no Houdini dependency, new mesh segmentation, stochastic point jitter, or atlas variant is needed. The Rust generator must remain deterministic, and the current task produces one 256px seven-channel card.

### GDC 2019: mip-aware card padding

- Source: Sean Feeley, Sony Santa Monica Studio, “Interactive Wind and Vegetation in *God of War*,” GDC 2019: https://gdcvault.com/play/1026036/Interactive-Wind-and-Vegetation-in
- Direct observation from the public session overview: the foliage system includes leaf/card-cluster LODs and fast texture flood filling through mip chains to address UV seams and matting.
- Transferable method: treat opacity coverage and color outside the cutout as distance behavior, not merely export details. Validate consecutive frames through minification and card/LOD transitions.
- Repository constraint: reuse `leaf_mipped_image` semantic reduction. Do not add a new cluster LOD, flood-fill runtime pass, shader architecture, or renderer-wide temporal method. Because project albedo outside the cutout is transparent black today, candidate evidence must specifically check halos/matting before any padding change is proposed.

### GDC 2015: alpha-tested foliage with scale-specific representation

- Source: Stephen McAuley, Ubisoft Montreal, “Rendering the World of *Far Cry 4*,” GDC 2015: https://gdcvault.com/play/1022235/Rendering-the-World-of-Far
- Direct observation from the public slides/session summary: vegetation uses alpha-tested leaf clusters rather than alpha blending, with individual leaves nearby and displaced billboard representation farther away; the wider presentation also uses temporal stabilization.
- Transferable method: preserve a hard, stable silhouette for cutout foliage and judge it at intended scales rather than only on a 256px plate.
- Repository constraint: keep the production `TacticalTreeLeafCardMaterial` and alpha cutoff 0.31. The task may improve semantic mips and the analytic mask, but it may not replace alpha testing, add displaced billboards, or redesign temporal AA.

### GDC 2023: nondestructive foliage-library iteration

- Source: Ehsan Ebrahimzadeh, Arkane Austin, “Fall Foliage in *Redfall*: Creating the Essence of Northeast America,” GDC 2023: https://gdcvault.com/play/1029061/Fall-Foliage-in-Redfall-Creating
- Direct observation from the public overview: Arkane combined hand-authored art with procedural, nondestructive methods so a large foliage library could be iterated quickly from design and art-direction feedback.
- Transferable method: one shared texture API plus small species modules and repeatable evidence makes iteration inexpensive while keeping review targets species-specific.
- Repository constraint: a dedicated `hawthorn_leaf.rs` may reuse shared `LeafTextureSet` and mip helpers, but it must not duplicate those APIs or alter unrelated species.

### Procedural environment-art breakdown: mask first, channels from structure

- Source: Bogodar Havrylyuk, “Procedural Vegetation Materials Breakdown,” 80 Level, 2017: https://80.lv/articles/procedural-vegetation-materials-breakdown
- Direct observation: the artist decomposes plants into reusable singular components; difficult outlines may begin as alpha/SVG masks; the central vein is assembled from simple shapes, transforms, levels, and symmetry; bumps and veins become surface detail; outputs are later composed into an atlas.
- Transferable method: construct the hawthorn blade as explicit lobe/sinus masks, construct midrib and lobe-directed secondaries separately, then derive opacity, height, normal, AO, and bounded palette regions from those shared fields.
- Repository constraint: do not copy the breakdown's gradients, sharpen-as-detail shortcut, random deformation, dirt, damage, or color variation. Project albedo must remain a few solid pigment regions with hard boundaries, and normals must come from a stable physical-style relief field.

## Frozen method synthesis for this repository

1. Author one analytic five-lobed `Crataegus monogyna` profile from explicit lobe and sinus landmarks. Botanical sources, not practitioner examples, define those landmarks.
2. Generate one midrib and one secondary direction per major lobe. Use those structural fields for vein palette selection and bounded relief.
3. Derive each channel deliberately: binary conservative opacity; separate solid front/back pigment palettes; height from blade dome/fold/veins; normals from height gradients; AO/roughness from bounded material fields.
4. Reuse shared semantic mip generation and prove minification with 128/64/32px proxies plus consecutive production readbacks. Plate beauty alone is insufficient.
5. Keep shrub assembly, card geometry, alpha cutoff, leaf scale, LOD architecture, and atlas variation unchanged unless later evidence opens a separate task.

## Rejected transfers

- Unconstrained L-systems or plant hierarchy changes: useful at plant scale, outside a texture-only task.
- Random point jitter or noise-defined outline: weakens deterministic species landmarks.
- Continuous pigment gradients, edge highlights, dirt, damage, water drops, or baked shadow: violates the molded solid-palette material direction.
- A new Houdini/Substance authoring dependency: unnecessary because the repository requires deterministic code-native textures.
- Multi-leaf atlas variants: botanically desirable eventually, but an architecture expansion that requires a separate gate.
- Renderer-wide alpha, billboard, deferred-texturing, or temporal-AA redesign: production talks establish concerns, not permission to change unrelated architecture.

## Evidence limitations

- GDC sources were inspected through public session overviews and available public slides; they do not provide comparable performance ceilings for this workload.
- The SideFX forum is practitioner testimony, not controlled research or botanical evidence.
- No prior-art result is accepted as visual proof for HawthornLeaf. Baseline and candidate captures must use the repository's deterministic lab and `understory-common-hawthorn` production fixture.
