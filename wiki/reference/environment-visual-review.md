# Tactical environment visual review

This review loop evaluates only terrain, environment lighting, rocks, and
foliage. Characters are absent from the capture harness and must not be scored.
Do not propose or assess work on volumetric clouds, caves, weather, or water.
Prefer physically based material, geometry, lighting, and density changes over
per-asset lighting compensation.

## Capture contract

Run `just tactical-environment-review` into a fresh directory. The compact
matrix deliberately crosses three representative environments with morning,
grazing, and moonlit times, then adds one isolated plate each for the Sun,
twilight, Moon, and stars. Each child manifest must pass semantic and exact-view
gates, the aggregate `manifest.json` must have `passed: true`, and neither the
aggregate nor any child may contain `failure.txt`. A missing, unreadable,
misframed, or irrelevant plate is a coverage gap, not evidence of good quality.
Branch-junction diagnostics suppress only production leaf render entities, and
terrain-grazing diagnostics suppress only production grass render entities;
their manifests record and gate those suppressions so production subject
geometry and material remain visible without occlusion.

The `moonlit` slot is a separate verified lunar instant. Its manifest must show
the Sun below -12 degrees, Moon above 20 degrees, and lunar illumination above
0.9. Sky children record canonical time, camera, dimensions, pixel-content
metrics, revision, and source identity; black or subject-free plates fail.

The ordinary `tactical-scene-capture` recipe remains the exhaustive 23-image
semantic profile. The compact profile is for repeated art review; it does not
replace the exhaustive profile before publication.

## Reviewer prompt and rubric

Give the reviewer the aggregate `index.html`, all referenced PNGs, its
`manifest.json`, and this prompt:

> Independently review these deterministic Fabelgeist environment captures.
> Evaluate only terrain, physically based environment lighting, trees and
> forest-floor debris, rocks, grass/foliage, the Sun, Moon, twilight, and stars.
> Exclude characters, volumetric clouds, caves, weather, and water. Inspect
> every relevant close-up and time-of-day context plate. Record one ledger item
> per distinct visual defect. Use severity 0-5 below. If evidence cannot show a
> criterion, record UNASSESSABLE with the missing fixture/time/view and do not
> treat it as severity 0. Compare reassessments against the same deterministic
> camera and capture provenance. Do not suggest arbitrary per-asset shader
> brightening/darkening; favor physically based geometry, textures, normals,
> roughness, density, distribution, and shared lighting behavior.

Severity measures visible harm, not implementation difficulty:

- **0 — resolved:** no material defect remains at the reviewed scale.
- **1 — negligible:** detectable under close inspection, unlikely to justify work.
- **2 — minor:** locally distracting but the environment still reads correctly.
- **3 — material:** repeatedly visible or meaningfully harms realism.
- **4 — major:** dominates a subject or breaks scene credibility.
- **5 — critical:** pervasive failure that invalidates the environment presentation.
- **UNASSESSABLE:** required evidence is absent, obscured, stale, or misframed.

At minimum, explicitly assess trunk bark/model complexity, branch-to-trunk
junction width, root uniformity, ground normal/height detail, leaf and twig
debris at tree bases, rock surface material, grass patch seams, grass density
and height variety, and the Sun/Moon presentation. If none of these can be seen,
the corresponding item must be UNASSESSABLE and the capture prompt/profile must
be corrected before triage.

## Triage and reassessment

For every severity 2-5 item, the planner records at least one candidate solution
and independently scores implementation complexity and steady-state performance
cost from 0 (negligible) to 5 (very high). Record expected severity reduction,
confidence, dependencies, physically based rationale, and affected shared
systems. Benefit is `severity * expected_severity_reduction * confidence` where
confidence is 0-1. Cost is `1 + implementation_complexity + performance_cost`.
The ledger stores the resulting benefit/cost ratio, but judgment—not a numeric
threshold alone—selects work.

Triage highest-confidence material defects with strong benefit/cost and a
bounded shared-system fix. Implement one coherent fix on one stacked branch,
recapture the identical affected matrix cells plus required sky evidence, then
have an independent reviewer reassess without seeing the implementer's desired
score. Accept the fix only when semantic gates still pass and measured severity
decreases. Continue while a worthwhile item remains.

Stop and request user input when every remaining severity 2-5 item either has
implementation complexity 4-5, performance cost 4-5, requires an aesthetic or
architecture choice, depends on unavailable evidence/assets, or has insufficient
expected benefit to justify its cost. Severity 0-1 items are normally closed as
not worth further iteration. UNASSESSABLE items block conclusions until coverage
is repaired; they are never silently closed.

The canonical machine-readable fields and allowed values are defined by
`assets/tactical-scenes/environment-review-ledger.schema.json`; copy
`environment-review-ledger.template.json` for each review cycle.
After JSON Schema validation, run `just tactical-environment-review-ledger`
against the populated ledger. It verifies evidence and coverage state,
severity-2+ alternatives, exact benefit/cost arithmetic, and stop invariants.
