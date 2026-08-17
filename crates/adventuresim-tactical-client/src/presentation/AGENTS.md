# Tactical procedural presentation

These instructions apply when changing procedural terrain, ground cover,
foliage, trees, debris, rocks, or their capture and presentation systems. More
specific nested `AGENTS.md` files continue to define asset art direction.

## Testing and visual iteration

Use a staged funnel. Expensive stages require the earlier stage to pass.

1. Freeze the fixture, seed, production presentation settings, current
   formulation, expected improvement, and one cheap condition that would
   decisively reject it.
2. Make one coherent formulation change. Keep exploratory shape or material
   work separate from fixture-wide regeneration, manifest expansion,
   documentation polish, and unrelated production hardening.
3. Run the narrow deterministic and topology tests needed to make the candidate
   safe to render. Run one compiler-backed workload at a time.
4. Capture only the cheapest decisive views. Terrain normally starts with a
   feature-framed overhead view and an unobstructed grazing/profile view. Choose
   similarly focused contact and silhouette views for trees, rocks, foliage,
   and debris.
5. The implementer must reject an obviously bad candidate locally. Do not run
   the exhaustive matrix or ask an independent reviewer to confirm a visible
   failure already established by the screening views.
6. After two rejected revisions of one formulation, record what the evidence
   falsified and stop tuning its constants. Change the underlying formulation
   or request an architecture or art-direction decision.
7. For a viable candidate, request a delta-only independent review using raw
   evidence and the frozen rubric. Only after that review passes should the
   workflow run the exhaustive capture matrix, broad tests and benchmarks,
   regenerate all affected fixtures, and finalize documentation.

For long review cycles, keep a lightweight ledger with one row per formulation:
formulation ID, hypothesis, decisive gate, compiler/test runs, captures,
reviewer calls, result, and the specific new information expected from another
iteration. Keep the ledger with ignored capture artifacts unless its contents
become reusable repository policy.
