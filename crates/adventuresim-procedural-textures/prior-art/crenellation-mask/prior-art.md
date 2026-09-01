# Crenellation mask prior art

## Scope

This report concerns the `CrenellationMask` recipe only: the periodic opacity
mask used to turn the top edge of a distant shell-LOD wall strip into alternating
merlons and crenels. It does not propose replacing close crenellations, gameplay
collision, or the building generator's architectural dimensions with texture
data.

The useful prior art comes mostly from alpha-tested foliage rather than castle
walls. That is not an aesthetic analogy; it is the same sampling problem. In
both cases a binary architectural silhouette is filtered into fractional alpha,
then thresholded back into visible or discarded fragments while its projected
texel footprint changes.

## Repository facts and constraints

The following are repository constraints, not claims from the sources below.

- `crenellation_mask.rs` defines a deterministic analytic pattern. One U repeat
  is one merlon-plus-crenel pitch; V spans the entire crown strip.
- The current authored proportions are a 0.60 merlon duty cycle and a continuous
  breastwork occupying 5/9 of the strip height.
- The 256 by 256 source is generated with 4 by 4 sub-texel coverage samples and
  supplied with nine mips. U repeats, V clamps, filtering is linear, anisotropy
  is 8, and the material alpha cutoff is 0.5.
- Shell LOD owns this render-only silhouette. Close, playable crenellations and
  all crenellation collision remain geometry.
- A production capture must therefore judge the crown itself under continuous
  camera motion and distance change. Whole-building coverage or isolated still
  endpoints cannot establish that teeth are temporally stable.

## What practitioners have done

### Build an architectural rule, then instance or repeat it

**Evidence.** Houdini artists generally preserve a small semantic construction
rule rather than generate an arbitrary bitmap. In a SideFX forum example, a
procedural castle starts from an orthogonal L-system and places reusable parts on
points of square polygons; the author exposes module count and a low/high switch
for complexity. The same author notes that some components are prebuilt
subnetworks or HDAs while shape-dependent pieces consume input geometry. This is
useful evidence for separating an authored pitch/profile from its placement and
LOD representation, although that particular castle generator's all-90-degree
limitation is also explicitly acknowledged ([SideFX forum: Procedural generator
of random castles](https://www.sidefx.com/forum/topic/72064/?page=1)).

SideFX's castle-wall tutorial similarly treats the wall as a reusable procedural
asset exported to a game engine, and explicitly adds the inner wall, colliders,
and materials as distinct concerns ([SideFX: Creating a Castle
Wall](https://www.sidefx.com/tutorials/castle-wall-part-1/)). SideFX's broader
game-asset guidance builds kits of repeated parts, bakes high-resolution detail
to low-resolution representations, and then assembles those parts procedurally
([SideFX: Dungeon Props](https://www.sidefx.com/tutorials/dungeon-props/)).

**Inference for this recipe.** Keep the mask analytic and normalized to one
architectural pitch. The mesh UV generator, not the texture, should decide how
many complete pitches fit a wall segment. That prevents the last merlon from
being stretched and makes adjacent strips testably phase-aligned. Random noise,
Voronoi breakup, or per-building raster painting would weaken a silhouette whose
primary information is a regular built form.

### Ordinary mip averaging does not preserve an alpha-tested silhouette

**Evidence.** Ignacio Castaño describes the production failure directly:
ordinary mip generation changes the fraction of texels passing the alpha test,
so alpha-tested leaves thin and eventually disappear. His remedy measures base
coverage at the render cutoff, builds each lower mip, and rescales its alpha so
the proportion of texels passing that same cutoff approximates the base
coverage ([Ignacio Castaño: Computing Alpha
Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/)).

Guerrilla used a closely related offline solution for *Horizon Zero Dawn*.
Their GDC presentation describes calculating source coverage, generating a
regular mip chain, bilinearly upsampling each mip for a histogram, finding the
histogram point corresponding to original coverage, and scaling it to the
runtime threshold of 0.5. The work moved distance correction out of the shader
and into the texture sample ([GDC Vault: Between Tech and Art: The Vegetation of
Horizon Zero Dawn](https://www.gdcvault.com/play/1025530/), [Guerrilla GDC 2018
slides](https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

An NVIDIA Texture Tools engineer also cautions that coverage scaling can make a
coarse mip substantially more opaque, especially for thin features and grazing
views; preserving aggregate coverage can reveal the underlying card shape in
ways the original nearly invisible mip did not ([NVIDIA Developer Forums:
Scaling alpha for mipmaps](https://forums.developer.nvidia.com/t/nvidia-texture-tools-exporter-scaling-alpha-for-mipmaps-grass/155854)).

**Inference for this recipe.** Box-filtering alpha preserves integrated alpha,
but that is not identical to preserving occupancy after a 0.5 alpha test. The
current tests correctly distinguish mean alpha from threshold coverage, but the
algorithm should be evaluated against the actual projected crown, not accepted
solely because early texture mips remain within a scalar tolerance. Because a
crenellation pitch is comparatively broad, coverage remapping may be needed
only for mips actually selected before the shell becomes sub-pixel; applying an
aggressive correction through the 1 by 1 tail could turn the whole crown card
opaque.

### A signed-distance representation is useful for hard silhouettes, but is
not sufficient by itself under minification

**Evidence.** Valve generated a low-resolution signed-distance field from a
high-resolution binary image, stored signed distance in an 8-bit channel, and
thresholded at 0.5. Bilinear interpolation then reconstructs the boundary more
faithfully than interpolating binary coverage under magnification. Valve calls
out alpha-tested impostors as an intended use, but also documents topology and
spread limitations ([Valve: Improved Alpha-Tested Magnification for Vector
Textures and Special Effects](https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf)).

Guerrilla's GDC slides are an important qualification: their foliage used
signed-distance alpha and still required a custom coverage-preserving mip chain.
An independent practitioner experiment likewise found that pure SDF mipmapping
could shrink thin shapes; one useful hybrid was to preserve source coverage at
mip zero and use downsampled distance information for lower levels
([Lisyarus: Exploring ways to mipmap alpha-tested
textures](https://lisyarus.github.io/blog/posts/exploring-ways-to-mipmap-alpha-tested-textures.html)).

**Inference for this recipe.** An SDF is an optional future improvement if close
shell views show stair-stepped or wobbly vertical merlon edges. It is not the
first fix for distant tooth loss. The first fix is coverage-correct mip behavior
at the exact runtime cutoff. If an SDF is adopted, derive it from the same
analytic boundary, keep the 0.5 zero-crossing contract, and regenerate
coverage-aware mips rather than applying an ordinary SDF box chain.

### Stable stills do not establish stable motion

**Evidence.** NVIDIA's SpeedTree chapter observes that hard alpha-cutout edges
scintillate during animation and reports alpha-to-coverage as substantially
reducing that artifact while remaining order-independent. It also notes that
alpha cutouts were used for LOD fading, which makes sampling behavior part of
the LOD system rather than merely a texture concern ([GPU Gems 3: Next-Generation
SpeedTree Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)).

Wyman and McGuire show that fixed-threshold alpha testing loses aggregate
appearance when coarse mips are selected. Their hashed threshold is spatially
and temporally stable compared with stochastic testing, but they still note
unexplored hash choices and recommend temporal supersampling for high quality
([Wyman and McGuire: Hashed Alpha
Testing](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)).

Guerrilla also states that its small alpha textures and custom mip chain need a
good anti-aliasing solution. That matters here because changing the mask alone
cannot guarantee temporal quality if the renderer's alpha-test edge has no
appropriate spatial or temporal antialiasing.

**Inference for this recipe.** The acceptance harness should capture
consecutive frames during fine sub-pixel camera translation and gradual
distance change, not only stationary endpoints. It should isolate a narrow
crown region, then measure at least:

1. thresholded crown occupancy per frame;
2. count and spacing of connected merlon runs where screen resolution permits;
3. frame-to-frame occupancy delta after compensating for the known camera
   motion;
4. premature conversion into a solid unbroken parapet;
5. premature loss of all merlons; and
6. visible phase jumps as the sampled mip changes.

The image sequence remains the visual authority. Metrics should fail closed on
obvious tooth loss or card-shaped fill, but should not claim perceptual
acceptance by themselves.

## Recommended construction for this module

The cheapest robust path is incremental:

1. **Retain the analytic one-pitch source.** Construct the breastwork union
   merlon rectangle in continuous normalized coordinates. Keep sub-texel
   integration at mip zero. Ensure U=0 and U=1 are the same periodic boundary,
   and clamp V so filtering cannot wrap the breastwork into the sky edge.
2. **Generate mips against the runtime decision rule.** For every relevant mip,
   downsample, evaluate bilinearly reconstructed coverage at cutoff 0.5, and
   solve a monotonic alpha scale (or equivalent threshold remap) that best
   preserves the source occupancy. Record the residual error. Do not blindly
   force the terminal 1 by 1 mip to the same binary coverage when a single texel
   cannot represent the topology.
3. **Constrain the useful mip range by projected pitch.** Once a whole
   merlon-plus-crenel pitch projects below a defensible screen-space width,
   neither binary alpha nor an SDF can reproduce alternating teeth honestly.
   At that point use the intended far-LOD silhouette policy: a deliberately
   simplified crown, a controlled fade, or no crown. Do not allow accidental
   filtering to make that design decision.
4. **Consider an SDF only if magnification warrants it.** A narrow signed
   distance band can improve the rectilinear boundary under bilinear sampling,
   but it adds recipe and shader semantics. Adopt it only after captures show a
   real close-shell edge defect that coverage-correct mips do not address.
5. **Do not add stochastic/hashed alpha casually.** Hashed alpha is compelling
   for extremely fine, irregular foliage. Regular architectural teeth are
   expected to read as orderly solids. Noise in that silhouette may look like
   broken masonry, and without a proven temporal reconstruction path it can
   shimmer. It is a fallback for genuinely sub-pixel coverage, not the default.
6. **Keep collision and shadows explicit.** The mask must never become the
   collision shape. If the shell casts shadows, its shadow pass must sample the
   same opacity texture, mip semantics, and cutoff; otherwise a stable visible
   crown can still cast a thinning or solid-card shadow.

## Deterministic checks worth keeping or adding

- Generation is byte-for-byte deterministic and the mip chain is complete.
- The base top-row occupancy matches the declared 0.60 duty cycle within the
  supersampling tolerance.
- U-periodicity is exact at the seam; V uses clamp addressing.
- For each render-relevant mip, both integrated alpha and thresholded coverage
  are reported. Coverage error is tested against the actual 0.5 material cutoff.
- A synthetic 2 by 2 tiling shows no seam, doubled tooth, or narrowed tooth at
  tile boundaries.
- UV tests prove that every wall span contains an integer or explicitly clipped
  number of architectural pitches, with constant metres per pitch across
  differently sized buildings.
- A production Shell-LOD traversal records consecutive frames across mip
  transitions at several azimuths, including grazing views. It saves both the
  normal frame and a crown-only diagnostic mask.
- The crown diagnostic rejects all-clear readbacks, all-filled card regions,
  abrupt tooth-count changes where projected resolution is adequate, and
  unbounded frame-to-frame occupancy oscillation.
- The same traversal is repeated at the actual game anti-aliasing settings. A
  texture-lab still is useful for construction review but cannot substitute for
  this motion evidence.

## Pitfalls to avoid

- Equating average alpha with post-threshold visible area.
- Judging only mip-zero or stationary screenshots.
- Lowering the shader cutoff with distance without measuring the resulting card
  fill and grazing-angle behavior.
- Disabling mipmaps; this trades disappearance for aliasing, cache cost, and
  shimmer.
- Preserving coverage so aggressively that the smallest mips become solid
  rectangles.
- Packing irregular damage or masonry noise into the silhouette mask. Those
  belong in surface textures or geometry and can destabilize a strong regular
  outline.
- Letting U repeat count vary implicitly with wall dimensions. Architectural
  pitch is a world-scale rule, not a texture-density accident.
- Assuming an SDF automatically solves minification or temporal stability.
- Using whole-frame pixel counts as evidence for a feature occupying only a
  narrow crown band.

## Bottom line

The present analytic rectangle-union is an appropriate source representation
for a cheap shell-LOD crenellation. The highest-value improvement is not more
surface detail: it is a mip generator and capture gate defined in the same
terms as the renderer's 0.5 alpha test. Preserve crown occupancy only while the
projected pitch can still represent teeth, inspect consecutive moving frames,
and make the transition to an intentionally simpler far silhouette explicit.
SDF or hashed-alpha techniques should remain measured alternatives for specific
observed failures, not assumptions baked into the first implementation.
