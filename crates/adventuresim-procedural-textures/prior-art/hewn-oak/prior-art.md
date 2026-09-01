# Hewn oak procedural texture prior art

## Scope

This report concerns exactly the `HewnOak` procedural surface used for exposed
structural timber, especially fachwerk posts, rails, plates, braces, lintels,
joists, and door framing. It covers oak anatomy, longitudinal and end grain,
hewing marks, checks and weathering, directional reflection, physical scale,
member-local UVs, repetition and mip behavior, and the requirements imposed by
half-timbered construction.

It does not prescribe the fachwerk structural layout, joinery geometry, paint
colors, or placement of dirt over a whole building. Those are consumers of the
base wood material and require object- or world-space information that a
repeatable surface tile cannot recover.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `HewnOak` produces albedo, OpenGL normal, height, and ARM maps. Its 512 by 512
  tile represents 2 metres square, about 3.906 mm per source texel, with a
  declared 9 mm full height range.
- The current generator makes longitudinal bands from periodic functions of
  U, broad grain modulation, a 7 by 10 cellular adze field, sparse elongated
  checks, and exactly one fixed knot. Grain and checks therefore repeat with a
  fixed orientation and the knot repeats at the same place every tile.
- Current roughness is high and bounded (0.68–0.94), is varied by grain, tool
  recesses, checks, and the knot, and metalness is zero. AO is a shallow local
  formula rather than a multi-radius visibility estimate.
- Tests prove determinism, approximate tile-edge continuity, expected physical
  mark dimensions, sparse checks and knot coverage, nonmetallic channel
  packing, map variation, and a complete mip chain.
- The shared mip helper averages encoded bytes. It therefore does not average
  sRGB albedo in linear light, decode and renormalize normals, or convert
  unresolved normal variance into additional roughness.
- High-detail building cuboids generate face UVs at a 2 metre repeat. Side
  faces use a local horizontal coordinate for U and local Y for V; top and end
  faces use local X/Z. There is no explicit per-member longitudinal tangent,
  log centre, growth-ring coordinate, or end-grain material selection.
- The same broad `Timber` material role covers vertical and horizontal frame
  members, diagonal braces, roof framing, joists, stairs, closures, gates, and
  doors. One flat longitudinal tile cannot be anatomically correct on all of
  those faces without member semantics.
- Tactical buildings currently use palette-colored checker textures for that
  role. The generated `HewnOak` maps are not yet bound to the visible building
  material, so improving this recipe alone will not alter tactical-city
  screenshots.
- The distant building shell has a separate fachwerk-baked path. Near timber,
  middle LOD timber, and baked far fachwerk must ultimately agree in average
  color, member scale, and major grain/tool-mark character.

The first implementation problem is consequently an interface problem: every
timber solid needs a stable longitudinal axis and distinguishable end faces.
The texture synthesis can become more realistic only after the mesh/material
path can preserve those meanings.

## Practitioner and material evidence

### Wood is a directional solid, not a decorated plane

The SideFX material-based destruction documentation assigns wood grain from a
piece's longest direction and treats grain-aligned splits differently from
cross-grain cuts. It exposes separate controls for grain spacing, offsets,
jaggedness, detail, and splinter length
([SideFX, *Working with wood*](https://www.sidefx.com/docs/houdini/destruction/wood.html)).
The companion tutorial warns that orientation must be established per piece:
connected studs can retain correct vertical grain, whereas treating a larger
assembly as one object can assign an obviously wrong horizontal direction
([SideFX, *Introduction to material-based destruction*](https://www.sidefx.com/docs/houdini/destruction/tutorials/intro_to_mbd_2.html)).

That is fracture documentation rather than a surface-authoring recipe, but its
transferable point is strong: timber direction is semantic data attached to
each member. A UV convention inferred merely from world axes will fail on
braces, rafters, rotated doors, and horizontal rails.

A SideFX forum discussion of realistic procedural wood explicitly separates
face grain from end grain and proposes triplanar or multi-material placement to
put the appropriate pattern on the appropriate faces
([SideFX forum, “Procedural realistic wood texture”](https://www.sidefx.com/forum/post/233892/)).
This is practitioner discussion, not authoritative documentation, but it
matches the solid-wood premise demonstrated by the Cornell wood appearance
model: wood texture is generated as a three-dimensional structure and its
reflection is anisotropic
([Marschner et al., *Modeling and Rendering for Realistic Wood*](https://www.cs.cornell.edu/projects/wood/simulating_the_structure_and_texture_of_solid_wood.pdf)).

**Inference for `HewnOak`:** the robust target is one coherent member-local
wood volume—or a 2D approximation derived from the same growth-ring field—not
independent noise painted on six faces. At minimum, longitudinal faces and end
caps need different mappings.

### White oak has identifiable anatomical structure

White oak is commonly straight grained with a coarse, uneven texture. Its end
grain is ring porous: earlywood has large pores, latewood pores are much
smaller and commonly arranged radially or dendritically, growth-ring
boundaries are distinct, and wide rays are conspicuous. Quartersawn surfaces
show prominent ray fleck
([The Wood Database, *White Oak*](https://www.wood-database.com/white-oak/)).
The Wood Database anatomy guide illustrates why rays read radially on end
grain and differently on longitudinal cuts
([*Hardwood Anatomy*](https://www.wood-database.com/wood-articles/hardwood-anatomy/)).
Purdue's white-oak guide similarly distinguishes straight rift grain from the
ray fleck exposed by quartersawing
([Purdue Extension, *White Oak*](https://www.extension.purdue.edu/extmedia/FNR/FNR-292-W.pdf)).

**Inference:** a plausible texture should begin with an offset ring field.
End grain samples that field in cross-section, showing ring arcs, rays, and an
earlywood band. Tangential and radial faces slice it in different directions,
producing cathedral-like or straighter grain and different ray visibility.
The current parallel U bands can serve as a distant longitudinal cue but do not
explain either end grain or radial fleck.

SideFX's Copernicus wood tutorial is useful procedural precedent for deriving
cathedral grain rather than layering unrelated noises
([SideFX, *Creating Wood Patterns (Cathedral Grain) with Copernicus*](https://www.sidefx.com/tutorials/creating-wood-patternscathedral-grain-with-copernicus/)).
Additional knot-oriented procedural research shows that knots and surrounding
grain can be modeled together rather than stamping a circular decal onto an
otherwise undisturbed field
([Liu et al., *Procedural Modeling of Knots in Wood*](https://www.ma-la.com/procedural_knots/Procedural_Knots_2022.pdf)).

At the repository's 3.9 mm source texel, broad earlywood/latewood rhythm, rays,
tool scars, and checks are representable. Individual sub-millimetre vessels
are not reliable height features; they should contribute mostly to restrained
albedo and roughness statistics, then merge smoothly in mips.

### Grain, damage, and tool marks should share a cause

Saki Jung's aged-wood Substance breakdown establishes a fiber direction first,
then uses dynamic gradients, warps, and directional noise; dents are warped to
follow the grain and small imperfections are added after the larger structure
([Jung, *Procedural Aged Wood Material Breakdown*](https://www.artstation.com/blogs/sakijung/dzBaR/procedural-aged-wood-material-breakdown-substance-designer)).
The Not Lonely procedural wood breakdown similarly separates fibers, aging
masks, color, normals, and roughness instead of using one noise field for every
channel
([Not Lonely, *Procedural Wood in Substance Designer*](https://www.not-lonely.com/blog/tutorials/procedural-wood-substance-designer/)).
Mark Foreman's Substance tips use anisotropic noise for grain, soften and
combine it with rings, deliberately restrain grain/ring contrast, derive
roughness bands from the same structure, and add dirt/weathering later
([Adobe, *Mark Foreman’s Substance 3D Designer tips and tricks*](https://www.adobe.com/learn/substance-3d-designer/web/mark-foreman-s-substance-3d-designer-tips-and-tricks)).

Conservation guidance provides a useful distinction between construction
marks and later surface damage. In historic timber, axe-hewn surfaces and
saw-cut faces retain different conversion marks; tool evidence can identify
how a timber was produced
([Building Conservation, *Timber-framed buildings*](https://www.buildingconservation.com/articles/timberframedbuildings/timberframedbuildings.htm);
[Historic New England, *Documenting changes in wood*](https://www.historicnewengland.org/news-and-notes-for-homeowners-documenting-changes-in-wood/)).

**Inference:** the present cellular “adze” relief risks reading as stone facets
or generic Voronoi noise. A hewn face should instead contain overlapping,
directional axe/adze scallops: coherent passes, shallow scoops, occasional
ridges left between passes, and a work direction consistent over part of a
beam. Sawed faces, if needed later, should be a separate surface family rather
than another random layer.

The USDA wood handbook notes that oak's prominent rays influence drying and
that checking is a characteristic drying defect to control
([USDA Forest Products Laboratory, *Wood Handbook, Chapter 1*](https://www.fpl.fs.usda.gov/documnts/usda/ah188/chapter01.pdf)).
Checks should therefore run with the grain, widen at exposed ends, and react to
knots or joints. Large end-origin checks cannot be convincingly placed by an
infinitely repeating base tile. They belong in a per-member or trim/detail
mask; the base texture should contain only sparse fine checking.

### Roughness and normals need directional, physically restrained behavior

Wood is a dielectric, so metalness remains zero. OpenPBR represents directional
highlights with anisotropy and a tangent direction rather than with metallic
response
([Adobe, *OpenPBR overview*](https://experienceleague.adobe.com/en/docs/substance-3d/general-knowledge/openpbr/openpbr-overview)).
The Cornell model likewise treats wood reflection as anisotropic because the
material's fibrous structure has a direction.

**Inference:** if the current Bevy material cannot consume anisotropy and a
member tangent, `HewnOak` should not counterfeit the effect with large normal
grooves or glossy dark bands. Use subtle longitudinal normal relief and
roughness correlation: freshly exposed/tool-burnished ridges may be slightly
smoother; porous earlywood, degraded fibers, and check interiors rougher.
Albedo, height, and roughness should share the ring/fiber/tool causes, but not
be simple copies. The declared 9 mm relief range should be reserved for deep
checks and pronounced hewing recesses; normal grain should occupy only a small
fraction of it.

Rain darkening, biological growth, soot, and sun bleaching should not be baked
uniformly into the tile. Their distribution depends on façade orientation,
eaves, ground distance, joints, and water paths. Preserve a relatively clean
base wood and apply those as building- or world-space masks.

## UV and member-space contract for fachwerk

A practical implementation does not require triplanar projection, but it does
require explicit timber coordinates.

1. Give every structural timber a member-local longitudinal tangent, with a
   deterministic sign convention.
2. Map longitudinal V along that tangent in physical metres. Map U across the
   face width, not from a global wall axis. Diagonal braces must therefore have
   diagonal grain in world space; horizontal rails horizontal grain; posts
   vertical grain.
3. Mark the two member ends and select an end-grain atlas region/material for
   them. End caps should share ring phase/log-centre parameters with the side
   faces when possible.
4. Distinguish radial and tangential longitudinal faces if the mesh data can
   afford it. A cheaper first version may alternate two deterministic face
   variants, but should not show cathedral grain on every side.
5. Assign a stable per-member seed for log-centre offset, ring spacing, limited
   knot occurrence, and tool-pass phase. Do not randomize per face, because
   seams at corners would reveal six unrelated boards.
6. Preserve the 2 metre world scale across members. UV islands may be packed,
   but their material coordinates must not be normalized to each member's
   bounds; otherwise a short brace and tall post display the same number of
   rings and scars.

This contract also helps the LOD system. LOD0 can show end grain, tool relief,
and object-level checks. LOD1 can retain correctly oriented face grain with
reduced geometry. LOD2 may bake fachwerk color and the strongest directional
features into its façade sheet while suppressing fine normals. All levels
should converge toward the same spatially averaged color and coverage to avoid
visible LOD pops.

## Repetition, filtering, and distance behavior

The fixed knot is the highest-risk repetition cue: a 2 metre repeat puts the
same knot at regular intervals on every member. Replace it with multiple
deterministic, tileable base variants or make knots a sparse per-member overlay
that bends the surrounding grain. Tool-pass phase and check masks also need a
small deterministic variant set. Variation should be seeded by building and
member identity, never by frame time or camera distance.

Mip generation should be semantic:

- downsample albedo in linear light and encode the result as sRGB;
- decode normal texels, filter vectors, renormalize, and encode again;
- carry unresolved normal variance into roughness so distant timber does not
  become implausibly glossy;
- filter height as height, while ensuring isolated deep checks do not bias an
  entire coarse texel;
- filter AO conservatively and avoid letting a few dark crack interiors turn
  the whole distant beam black.

The normal-variance point follows a general GDC material-filtering lesson:
normal maps contain subpixel slope variation that ordinary color mips discard,
so roughness must account for the lost variance
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).
This is not wood-specific, but it directly applies to narrow grain and hewing
relief.

Fine vessels and hairline fibers must fade before they approach a pixel; they
should not survive as high-contrast albedo stripes. Broad ring rhythm, hewing
undulation, and member silhouettes remain legible longer. Deterministic mip
selection plus temporally stable coordinates is preferable to stochastic
detail that changes as the camera moves.

## Recommended implementation sequence

### 1. Fix the consuming interface before polishing the recipe

- Add a member-local timber-axis/end-face contract to generated building
  solids.
- Bind `HewnOak` to an isolated procedural-texture lab fixture and, later, to a
  tactical timber material while preserving the building palette as a tint or
  low-frequency color parameter.
- Establish how near timber feeds LOD1 and the fachwerk-baked LOD2 result.

Without this step, visual iteration will optimize a texture that the city does
not display and cannot orient correctly.

### 2. Rebuild the causal wood fields

- Generate a tileable, offset growth-ring field with controlled ring-width
  variation.
- Derive tangential/radial longitudinal grain and end grain from that common
  field.
- Add wide rays at species-appropriate low frequency; represent fine pores
  mostly in albedo/roughness at this resolution.
- Couple knots to grain deflection and make them sparse per member.

### 3. Replace generic facets with hewing passes

- Create directional overlapping scallops at real-world axe/adze scale.
- Reserve deep height for checks and the strongest cuts; keep fiber relief
  restrained.
- Separate tool scars from later weathering and from cut/sawed variants.

### 4. Make channels and mips semantic

- Derive albedo, height, normal, and roughness from shared causes with bounded,
  non-identical responses.
- Replace byte-averaged albedo and normals with color-space- and
  vector-correct mips; add normal-variance roughness compensation.
- Confirm that the far result preserves average tone rather than becoming a
  dark, noisy stripe.

## Acceptance and regression tests

### Deterministic numeric tests

- Assert the declared metres-per-tile and millimetres-per-texel.
- Measure ring, ray, hewing-scallop, check, and knot feature-size distributions
  in metres, not only pixel counts.
- Prove seam continuity for each longitudinal and end-grain variant.
- Prove metalness is zero and roughness stays bounded.
- Verify albedo mips against a linear-light reference and normal mips against a
  decoded/filter/renormalized reference.
- Verify deterministic per-member variants and that different seeds change
  feature placement without changing scale or palette bounds.
- Verify all LOD reductions remain stable under small camera-distance changes.

### Visual fixtures

Use one neutral-lit timber fixture containing:

- a vertical post, horizontal rail, diagonal brace, rafter, and door member;
- exposed end caps beside longitudinal faces;
- a three-member corner/joint to reveal discontinuous face seeds;
- a 2 by 2 tiled plane to expose periodic seams;
- many adjacent members to expose repeated knots and tool patterns;
- dry and wet/grazing-light views, without building-scale weathering mixed in;
- matching LOD0, LOD1, and LOD2 views at transition distances.

The independent visual reviewer should reject any result where grain is not
parallel to the member, end caps show side grain, every face has the same
cathedral pattern, knots repeat in a grid, tool marks resemble Voronoi stone,
fine grain shimmers, distant wood becomes glossy or black, or LOD fachwerk
changes average color abruptly.

## Evidence, inference, and project decisions

- **Evidence:** oak anatomy is ring porous with conspicuous rays; procedural
  artists establish directional fibers/rings before defects; wood reflection
  is anisotropic; hewing and sawing leave distinguishable tool evidence;
  ordinary normal-map mips lose subpixel slope information.
- **Inference:** the best affordable fachwerk material uses one coherent ring
  field, member-local longitudinal mapping, explicit end grain, directional
  hewing passes, restrained pores, and per-member overlays for knots/checks.
  These are recommendations derived from the evidence, not claims that all
  surviving 1544 German timbers had the same appearance.
- **Repository decisions still required:** whether to add true anisotropic
  shading, how timber roles carry member tangents and end-face tags, whether
  the runtime uses an atlas or material variants, how palette tint combines
  with procedural albedo, and how the same statistics are baked into LOD2.

The minimum credible next milestone is therefore not “add more grain noise.”
It is a neutral-lit lab artifact proving that a post, rail, brace, and exposed
end all show the correct anatomical orientation at a fixed physical scale,
with stable semantic mips and no repeated-knot grid.
