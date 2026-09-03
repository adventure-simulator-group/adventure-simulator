# Independent artistic review: weapons and shields

Review scope: the generator's 1544 German equipment vocabulary, construction, surface shading, proportions, and detail across LODs. This document records an independent review; it is not an implementation plan or a claim that every historical object is typical equipment.

## Acceptance criteria

1. **Continuous assembly.** Inspect the default and seeded extreme grip/pommel, guard/blade, shaft/socket, and shield-fitting combinations from front and oblique views. A joint must visibly overlap or meet through a purpose-built neck, collar, or shoulder. A smaller pommel body is permissible if its seating geometry meets the grip; stretching every pommel into an oversized ball is not a satisfactory fix. No floating parts, exposed end caps inside a mating joint, or abrupt unsupported grip overhangs.
2. **Intentional surface shading.** Cylindrical and oval grip sides, round pommel surfaces, curved guard bars, shield bowls, and rounded rims should carry continuous highlights. Preserve crisp blade cutting edges, blade ridge/fullered-section boundaries, ends/caps, and deliberately octagonal pole shafts. Review under the same neutral lighting from at least two angles; a single front view can hide faceting and slab thickness.
3. **Useful shape vocabulary.** A halberd must support an axe with a relatively straight or convex cutting edge, independent upper/lower blade shoulders, a triangular/extended toe, a lower beard, a short downturned rear fluke, and a separately controllable axial spike. Change shape controls independently without detaching the forged root from the socket. Do not make elaborate late parade outlines the default for 1544. Guard variation should change section, taper, terminal swell and curvature, rather than only the span of a uniform tube. Pommel variation should include recognizably different bulb/pear/scent-stopper and flattened cap forms with an explicit grip seat.
4. **Proportion evidence.** Compare whole objects at a consistent metric scale as well as equal-size detail crops. A dense head does not excuse an implausibly thick cutting plate. Overall collection dimensions are useful anchors, not proof of a universal blade thickness. Visually judge tapered cutting edges separately from reinforced sockets and thrusting spikes. For shields, judge hand clearance, fitting placement, rim scale, overall size, and bowl/rib depth together.
5. **LOD preserves construction.** Low, medium and high detail must preserve silhouettes, intentional hard edges, component contacts, blade tips and guard openings. Spend triangles on changing curvature and visible cross-sections. Long straight shafts and flat plates need no artificial uniform subdivision merely to match triangle sizes. A curved shield must not look like a faceted fan while its tiny fittings have lavish radial tessellation. Record triangle counts and equal-camera screenshots for all levels.
6. **Reproducible sampling.** Review a fixed set of default cases, seeded multi-slider perturbations, and explicit adversarial joint/thickness extremes. Save the actual definitions, seed and camera with screenshots. After a fix, rerun identical cases before adding fresh seeds; do not accept a fix solely on newly favorable samples.

## Historical anchors and interpretation

- [Met, probably German halberd, 1525–50, 96.5.23](https://www.metmuseum.org/art/collection/search/25021): 161.3 cm overall, 50.8 cm head, 24.1 cm across, 2.251 kg. This is a direct period silhouette/scale anchor; total surviving shaft length is an individual-object observation.
- [Worcester, Italian halberd, about 1540, 2014.137](https://worcester.emuseum.com/objects/48676/halberd): 226.1 by 21.6 cm; museum describes triangular axe plate, near-vertical edge, cusped upper/lower shoulders, short downcurved fluke, quadrangular spike and octagonal staff. It proves useful adjacent-region shape vocabulary; it does not establish German prevalence. Its shaft and straps are restored.
- [Met, German halberd, late fifteenth century](https://www.metmuseum.org/art/collection/search/25898): older regional reference for the distinct axe, axial spike and rear beak functions. Useful for retained equipment, not a precisely dated 1544 default.
- [Met, German Katzbalger, about 1520, 29.158.707](https://www.metmuseum.org/art/collection/search/33991): 81.3 cm total, 68.2 cm blade, 14 cm across, 1.077 kg. The generator's blade and guard are close; the default 16.5 cm exposed grip is long relative to this particular object's complete non-blade length of 13.1 cm. Do not generalize from one object into rigid universal sliders.
- [KHM, German sword, about 1540, A 1421](https://www.khm.at/kunstwerke/schwert-371801): 123.8 cm overall, 22 cm across and 1.45 kg. A contemporary whole-object scale anchor. Catalog depth includes the hilt; it must not be read as blade thickness.
- [Met, Munich two-handed sword, about 1540, 14.25.950](https://www.metmuseum.org/art/collection/search/27346): 172.7 cm overall, 128.2 cm blade and 41.1 cm across. Supports the existing early Zweihander blade-length scale; its documented 4.451 kg mass should not become the target for every two-handed sword.
- [Met/Dresden exhibition catalog, sword in Landsknecht style, south German about 1530, VIII/32](https://resources.metmuseum.org/resources/metpublications/pdf/The_Splendor_of_Dresden_Five_Centuries_of_Art_Collecting.pdf): figure-eight guard, 102 cm total and 89 cm blade. The museum identifies the elaborate silver hilt/scabbard as apparently a captain's weapon: type evidence, not ordinary-soldier ornament density.
- [Detroit Institute of Arts, German sword pommel, sixteenth century, 69.22](https://dia.org/collection/sword-pommel-25912): 6.6 by 4.2 cm, spherical carved iron form. Broad-century evidence for a distinct rounded option; its carving is not a required baseline surface treatment.
- [British Museum, German pavise, late fifteenth century, 1881,0802.139](https://www.britishmuseum.org/collection/object/H_1881-0802-139): 106.8 by 45 cm, broad raised center rib, wood/canvas/gesso, 4.4 kg. Useful older retained shield construction, materially different from a generic flat steel rectangle.
- [Rijksmuseum, Burgundian pavise, about 1474–75, NG-KOG-2517-C](https://id.rijksmuseum.nl/200416160): 105 by 60.4 cm, 14.2 cm depth and 2 cm thickness; museum connects these ground-standing shields to battlefield crossbow cover. Supports a roughly meter-high pavise option and meaningful curvature; it is older and adjacent-region evidence.
- [Met, tournament shield, German about 1475–1500, 23.261s](https://www.metmuseum.org/art/collection/search/35722): explicitly associated with the Scharfrennen joust. Tournament targes must not be presented as generic 1544 infantry shields.
- [Met, Hungarian-style shield, Eastern European about 1500–1550, 42.50.29](https://www.metmuseum.org/art/collection/search/24698): 127 by 44.5 cm, 22.2 cm deep. Asymmetric raised rear edge protects cavalry head/neck. Strong evidence of a missing shield silhouette, but regional light-cavalry equipment rather than a standard German infantry default.
- [Met, parrying dagger explanatory context](https://www.metmuseum.org/art/collection/search/24882): the museum distinguishes early-sixteenth-century civilian sword/rapier use and buckler/dagger defense. The object itself is 1600–1620 and must not be copied as a 1544 hilt reference.

## Initial code-based concerns, pending screenshots

- Every triangle presently receives its own normal; polished round parts will reveal tessellation regardless of increased radial segments.
- Default crossguards are constant-radius tubular paths with a small center box. They cannot express strong terminal swelling or a tapered/faceted quillon section.
- The halberd axe is a constant-thickness prism (22 mm default) rather than a thin cutting plate that thickens toward its root. Side/oblique inspection is mandatory.
- Longsword/sidearm root thickness defaults (approximately 11–13 mm) deserve visual and mass/proportion review. These sources do not provide measured thickness, so a specific replacement millimeter value would be a modeling judgment rather than a sourced measurement.
- Existing broad blade-length and grip-length slider ranges can combine a dagger-length blade with a large two-hand grip. Such stress models should remain technically coherent, but should not be scored as curated period proportions.
- Kite and Roman tower presets are useful generator studies but should be clearly distinguished from the period baseline.

## Baseline visual review

Inspected `output/playwright/weapon-iteration/before-hilts-heads.png` and `before-shields.png`, batch 0, seed 1544, medium detail, oblique view. The first sheet uses detail framing and the second whole-object framing.

**Decision: changes required.** The baseline establishes reproducible defects rather than acceptable art quality.

1. **Highest priority: forged head shape.** The German halberd axe reads as a thick rectangular paddle with a subtly bulged edge. The upper/lower shoulders and cutting bevel do not communicate a forged blade. Preserve the stout root but visibly thin the edge; expose more useful toe/heel shaping. The rear fluke curls upward in both displayed examples, contradicting the preset's downturned description. Resolve that directional contract before additional ornament.
2. **Highest priority: shading on curved forms.** The zoomed longsword pommel shows individual polygon bands and triangular patches. Both buckler bosses show strong radial stripes and concentric faceting. Smoothed normals are required on rounded surfaces; extra triangles alone will not fix this. Keep flat octagonal shaft sides intentionally distinguishable.
3. **High priority: hilt articulation.** The default longsword cross is a thin bent wire with cut-off ends. A shaped center shoulder, quillon taper and terminal swelling should provide variation in forged structure. The Katzbalger fan cap reads as an angular stone lump; preserve its flattened width while rounding its perimeter and defining a purposeful grip seat. The random longsword image crops away most of the grip/guard; it cannot certify whole-hilt proportions or assembly.
4. **Medium priority: pavise profile.** Both pavises read as broad brown boards; the characteristic raised central rib is effectively invisible. The default's 106.8 cm overall height is a plausible older-object scale, but a visible rib and material-appropriate edge are more consequential than increasing its size. The random pavise also adds an arbitrary small center boss, which should be considered a generator stress case rather than a curated reference shape.
5. **Medium priority: detail distribution and LOD.** The 688-triangle default halberd has underarticulated head structure while the 3,448-triangle Katzbalger and 3,216-triangle buckler still show faceting. Evaluate what each triangle contributes to silhouette and highlights, then verify all three detail levels with fixed framing. Do not subdivide straight poles only to homogenize edge lengths.

The sheets' mass readings are suspicious across unrelated objects (default halberd 14.25 kg, pavise 15.36 kg, round shield 13.65 kg). These numbers are not accepted as physical evidence until the volume and material calculation is audited. The halberd's slab appearance is independently visible, so its geometric thickness remains an artistic defect even if the mass diagnostic is wrong.

## Cycle 1 visual review

Independently inspected `cycle1/medium-oblique-detail-0.png`, `cycle1/medium-oblique-detail-1.png`, and `cycle1/medium-back-whole-1.png` in the same capture directory. Definitions and camera information are retained in `cycle1/fixtures.json` and `cycle1/manifest.json`.

**Decision: substantial visual improvement; two assembly blockers remain.**

- **Pass at the supplied medium view:** rounded grips and pommels now carry continuous highlights; blade sectional creases remain readable. The fan pommel has a deliberate flattened cap perimeter and joined grip seat instead of the stone-lump appearance. Longsword quillons have visible terminal swelling and credible center attachment. The default halberd has a thinner cutting plate, purposeful shoulder/toe, axial spike and correctly downward fluke. The pavise's central rib now reads, and its narrower proportions and dark edge communicate a shield instead of a broad unarticulated board. These improvements do not require further ornamental work for this cycle.
- **Blocker 1, coupled head anatomy:** the random halberd's `Axe side: -1` puts the axe on the same side as its beak. The beak disappears into the cutting plate and a diagonal seam crosses the face. Flipping the axe must also relocate/reorient the opposite fluke, or the public control must be constrained so an authored halberd retains its opposing functions. Acceptance: default and mirrored heads each show an unobstructed axe on one side and an unobstructed beak on the other, with no cross-face intersection seam.
- **Blocker 2, center-grip buckler construction:** back views show a closed plate directly behind the centered handle, with a circular ghost line under the boss on the random case. Source inspection confirms `roundShieldShell` fills the center with front/back triangle fans and `shieldBossMesh` places a hollow dome on top. The dome's cavity is blocked by the underlying plate. A center-grip buckler needs an accessible central bowl: create an aperture with a joined edge or make the central dome part of the body shell. Arm-strapped shields with decorative bosses need not receive the same opening. Acceptance: an oblique back close-up demonstrates a continuous, open hand cavity, joined surfaces and no ghost seam.
- **Pending evidence:** low/high LOD comparisons and adversarial joint/shape extremes. Current screenshots support a medium-view decision only. The random definitions changed as new controls/ranges were introduced, so final acceptance should replay these exact cycle 1 fixtures as well as add fixed adverse cases.

## Final independent acceptance

**Decision: ACCEPT the bounded construction, shading, shape-control and LOD changes. No remaining artistic blockers were found in the inspected fixtures.** This decision does not certify every possible slider combination, exhaustive historical style coverage, or museum-calibrated mass estimates.

Independently inspected these saved captures (paths relative to `output/playwright/weapon-iteration/`):

- `replay/low-oblique-detail-0.png` and `high-oblique-detail-0.png`: exact cycle 1 hilt/head specimens at the lowest and highest detail levels.
- `replay/low-oblique-detail-1.png` and `high-oblique-detail-1.png`: exact cycle 1 shield specimens at those levels.
- `replay/high-back-whole-1.png`: shield rear surfaces and fitting placement.
- `adversarial-final/low-oblique-detail-0.png` and `high-oblique-detail-0.png`: narrow pommel/wide grip, large pommel/short grip, rounded bulb/swept terminals, mirrored cusped halberd, thin buckler/large boss, and deep pavise rib with low authored resolution.
- `adversarial-final/medium-rear-whole-0.png`: the same adverse assemblies from a rear oblique camera.

The exact definitions and camera settings are retained in each directory's `fixtures.json` and `manifest.json`.

### Resolved blockers

The mirrored halberd now preserves an axe and fluke on opposite sides. Both the replay and deliberately cusped adverse head show unobstructed cutting/piercing profiles and no diagonal plate-crossing seam. The head retains its thin cutting edge, reinforced socket and axial point.

The center-gripped buckler now has a recessed opening visible behind the handle. A source check confirms the body shell omits the center fans where `shieldHandAperture` is positive and closes the aperture's thickness edge; the hollow boss interior is reachable through that opening. The circular line is now an actual aperture boundary, not the previous coincident-surface ghost line. Arm-strapped shield bodies remain solid. Rear oblique views show handles/straps anchored to their shield backs.

### LOD and adverse assembly acceptance

The narrow-pommel/wide-grip and large-pommel/short-grip examples maintain a continuous grip seat without exposed overhanging end caps or floating parts. The rounded bulb and swept terminal specimen provides a visibly different coherent hilt silhouette. Smooth curved highlights coexist with blade sectional creases and intentional haft flats.

Low LOD preserves the head silhouette, blade points, grip contacts, cap form, guard openings, shield rim and center rib. Its enlarged detail crops expose some expected polygonality around loop guards, boss perimeters and shield rims; high LOD resolves those contours. The deep narrow pavise rib remains visible even at low authored resolution. There is no contact break or disappearing feature between the inspected LODs.

Triangle counts respond substantially to detail: the replay default longsword moves from 526 to 6,228 triangles, Katzbalger from 1,206 to 14,224, halberd from 420 to 1,688, and buckler from 2,048 to 17,820. High-detail counts should be selected according to viewing distance and use; they are not automatically appropriate for every gameplay asset. The visual review accepts the ability to scale quality and preserve construction, rather than requiring equally sized triangles on flat plates and curved fittings.
