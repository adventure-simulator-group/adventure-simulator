# Herbalism

Herbalism is the trained, Intelligence-governed strategic skill for turning
named biological ingredients into concrete medicines. It is separate from
Physiology, which observes patients and administers preparations, and Cooking,
which prepares food.

## Professional boundary

The Fellowship of Herbalists is institutionally separate from the College of
Physicians and the Surgeons' Guild. It is an informal, fee-free network based
in workshops and meeting houses rather than a university. Learners enter
through practice and mentorship without university enrollment.
Its curriculum trains Herbalism only, and advancement runs from learner through
herbalist to elder herbalist.

Physicians instead train Physiology for patient assessment, fallible
differentials, and non-operative treatment.
Surgeons train Surgery for operative procedures. Knife and Tailoring have
modest symmetric correlations with Surgery, but are not direct procedure inputs.

## Bounded preparation model

The playable loop is deliberately small:

1. Obtain willow bark, comfrey, poppy, or sage by foraging or trade.
2. Open the raised Herbalism action on the character's skill rail.
3. Select one personal ingredient row and one authored method.
4. Review the deterministic output, time, material requirement, effect, and
   risk, then prepare it.
5. Transfer, trade, or administer the resulting quantity-one medicine through
   the existing inventory and medical controls.

The three method families are **dry and grind**, **infuse or decoct**, and
**tincture**. Each recipe uses one ingredient and one method. Methods carry
authored heat rather than exposing temperature or duration controls. Tincture
is the one bounded exception to the one-input rule: it additionally consumes
one fungible unit of alcoholic tincture spirit. The spirit is ordinary
herbalist stock but is not itself selectable as a medicinal herb.

Ingredients have public **Poor**, **Ordinary**, and **Fine** grades represented
by bounded catalogue identities. Ordinary herbs retain their established IDs;
poor and fine variants use shared suffixes and tags. This preserves stack,
trade, and transfer behavior without a lot or provenance table.

Outcome is a pure function of ingredient identity, public grade, method, and
Herbalism capability. Every recipe consumes one whole herb; low capability
takes longer, produces lower potency, and provides coarser hazard clarity.
There is no random craft failure and the UI never auto-selects a method.

Risk awareness is also deterministic. Novices receive a coarse but explicit
safety warning, intermediate herbalists see the affected physiological
systems, and skilled herbalists see the exact authored hazard range. Greater
knowledge never removes or softens the safety warning.

Comfrey decocted with excessive heat becomes visible spent-herb waste. Poppy
tincture provides a strong neurologic benefit with meaningful oxygenation and
renal-clearance hazards. These outcomes are authored and previewed.

## Authority and physiology boundary

The browser submits only the selected inventory-row ID and closed method. The
gateway binds the selected session character. The reducer verifies the
registered gateway, living strategic actor, personal ingredient ownership,
catalogue kind, quantity, recipe, output, and arithmetic before advancing
time. Unresolved strategic encounters block preparation. A terminal safe-prefix
interruption consumes neither herb nor tincture spirit, produces nothing, and
grants no training. Success consumes the exact authored inputs, creates one
concrete output, and trains Herbalism for elapsed craft time.

Medicines use versioned generic `InterventionProfile` meter effects. Recipes
never inspect or name a disease. Administration continues to resolve intrinsic
route and course semantics from the trusted preparation profile. No tactical
tick state is persisted.

## Explicitly deferred

Ingredient condition/spoilage, arbitrary mixing, exact heat/duration/dose,
hidden constituents, assay, generalized lots/provenance, and cross-training
with Alchemy are outside this model. Herbalism issue #214 no longer blocks
Alchemy issue #215; Alchemy may define its own material model independently.
