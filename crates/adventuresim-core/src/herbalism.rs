//! Small, authored herbalism rules.
//!
//! Herbalism deliberately models public ingredient grades and named
//! preparations, not hidden chemistry, lots, arbitrary mixtures, or disease
//! cures.  The result is pure and deterministic so the browser can preview
//! exactly the same decision that the authoritative reducer commits.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngredientGrade {
    Poor,
    Ordinary,
    Fine,
}

impl IngredientGrade {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Poor => 0,
            Self::Ordinary => 1,
            Self::Fine => 2,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Poor => "Poor",
            Self::Ordinary => "Ordinary",
            Self::Fine => "Fine",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparationMethod {
    DryGrind,
    InfuseDecoct,
    Tincture,
}

impl PreparationMethod {
    pub const ALL: [Self; 3] = [Self::DryGrind, Self::InfuseDecoct, Self::Tincture];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::DryGrind => "dry_grind",
            Self::InfuseDecoct => "infuse_decoct",
            Self::Tincture => "tincture",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::DryGrind => "Dry and grind",
            Self::InfuseDecoct => "Infuse or decoct",
            Self::Tincture => "Tincture",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::DryGrind => "Air-dry the herb, then grind it for a topical preparation.",
            Self::InfuseDecoct => {
                "Use an authored gentle infusion or stronger heated decoction, as the herb requires."
            }
            Self::Tincture => {
                "Steep the herb in one unit of tincture spirit to extract a concentrated remedy."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftOutcome {
    Medication(&'static str),
    DegradedWaste(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequiredConsumable {
    pub item_id: &'static str,
    pub units: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskAwareness {
    Coarse,
    Intermediate,
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CraftPreview {
    pub base_ingredient_id: &'static str,
    pub grade: IngredientGrade,
    pub method: PreparationMethod,
    pub input_units: u32,
    pub required_consumable: Option<RequiredConsumable>,
    pub duration_minutes: u32,
    pub potency_tier: u8,
    pub outcome: CraftOutcome,
    pub expected_effect: &'static str,
    pub risk: &'static str,
    pub risk_awareness: RiskAwareness,
}

fn risk_projection(base: &str, skill: f32) -> (&'static str, RiskAwareness) {
    let awareness = if skill < 1.5 {
        RiskAwareness::Coarse
    } else if skill < 3.5 {
        RiskAwareness::Intermediate
    } else {
        RiskAwareness::Exact
    };
    let risk = match (base, awareness) {
        ("willow_bark", RiskAwareness::Coarse) => {
            "Safety warning: may dangerously affect bleeding; exact severity is unclear"
        }
        ("willow_bark", RiskAwareness::Intermediate) => {
            "Safety warning: may meaningfully impair coagulation"
        }
        ("willow_bark", RiskAwareness::Exact) => {
            "Safety warning: coagulation hazard rises from +0.02 to +0.07 by potency"
        }
        ("comfrey", RiskAwareness::Coarse) => {
            "Safety warning: do not swallow; exact topical safety is unclear"
        }
        ("comfrey", RiskAwareness::Intermediate | RiskAwareness::Exact) => {
            "Safety warning: topical use only"
        }
        ("poppy", RiskAwareness::Coarse) => {
            "Safety warning: dangerous breathing and kidney effects; exact severity is unclear"
        }
        ("poppy", RiskAwareness::Intermediate) => {
            "Safety warning: meaningful oxygenation and renal-clearance hazard"
        }
        ("poppy", RiskAwareness::Exact) => {
            "Safety warning: oxygenation +0.04–0.16 and renal clearance +0.03–0.11 by potency"
        }
        ("sage", RiskAwareness::Coarse) => {
            "Safety warning: may worsen fluid balance; exact severity is unclear"
        }
        ("sage", RiskAwareness::Intermediate) => "Safety warning: may increase dehydration",
        ("sage", RiskAwareness::Exact) => {
            "Safety warning: hydration hazard rises from +0.02 to +0.06 by potency"
        }
        _ => "Safety warning: preparation risk is uncertain",
    };
    (risk, awareness)
}

pub const SUPPORTED_BASE_INGREDIENTS: [&str; 4] = ["willow_bark", "comfrey", "poppy", "sage"];

/// Decode a bounded catalogue identity into its medicinal base and public
/// grade. Ordinary ingredients retain their established IDs.
pub fn normalize_ingredient(id: &str) -> Option<(&'static str, IngredientGrade)> {
    for base in SUPPORTED_BASE_INGREDIENTS {
        if id == base {
            return Some((base, IngredientGrade::Ordinary));
        }
        if id.strip_suffix("_poor") == Some(base) {
            return Some((base, IngredientGrade::Poor));
        }
        if id.strip_suffix("_fine") == Some(base) {
            return Some((base, IngredientGrade::Fine));
        }
    }
    None
}

fn tier(grade: IngredientGrade, capability: f32) -> u8 {
    let skill = if capability.is_finite() {
        capability.clamp(0.0, 5.0)
    } else {
        0.0
    };
    // Grade supplies the first two steps; practiced hands can raise it by one.
    (1 + grade.rank() + u8::from(skill >= 2.5)).min(3)
}

fn medication(prefix: &str, tier: u8) -> &'static str {
    match (prefix, tier) {
        ("willow", 1) => "weak_willow_decoction",
        ("willow", 2) => "cooling_willow_draught",
        ("willow", 3) => "strong_willow_decoction",
        ("comfrey", 1) => "weak_comfrey_poultice",
        ("comfrey", 2) => "comfrey_poultice",
        ("comfrey", 3) => "fine_comfrey_poultice",
        ("poppy", 1) => "weak_poppy_tincture",
        ("poppy", 2) => "poppy_tincture",
        ("poppy", 3) => "strong_poppy_tincture",
        ("sage", 1) => "weak_sage_infusion",
        ("sage", 2) => "sage_infusion",
        ("sage", 3) => "fine_sage_infusion",
        _ => unreachable!("authored herbal preparation"),
    }
}

/// Preview an authored one-ingredient preparation.
pub fn preview(
    ingredient_id: &str,
    method: PreparationMethod,
    capability: f32,
) -> Option<CraftPreview> {
    let (base, grade) = normalize_ingredient(ingredient_id)?;
    let potency_tier = tier(grade, capability);
    let skill = if capability.is_finite() {
        capability.clamp(0.0, 5.0)
    } else {
        0.0
    };
    let input_units = 1;
    let base_minutes = match method {
        PreparationMethod::DryGrind => 120,
        PreparationMethod::InfuseDecoct => 180,
        PreparationMethod::Tincture => 360,
    };
    let duration_minutes = ((base_minutes as f32) * (1.25 - skill * 0.05)).round() as u32;
    let (risk, risk_awareness) = risk_projection(base, skill);
    let required_consumable =
        (method == PreparationMethod::Tincture).then_some(RequiredConsumable {
            item_id: "tincture_spirit",
            units: 1,
        });
    let (outcome, expected_effect) = match (base, method) {
        ("willow_bark", PreparationMethod::InfuseDecoct) => (
            CraftOutcome::Medication(medication("willow", potency_tier)),
            "Reduces temperature and inflammation",
        ),
        ("comfrey", PreparationMethod::DryGrind) => (
            CraftOutcome::Medication(medication("comfrey", potency_tier)),
            "Supports tissue integrity and reduces inflammation",
        ),
        ("poppy", PreparationMethod::Tincture) => (
            CraftOutcome::Medication(medication("poppy", potency_tier)),
            "Strongly relieves pain and stress",
        ),
        ("sage", PreparationMethod::InfuseDecoct) => (
            CraftOutcome::Medication(medication("sage", potency_tier)),
            "Mildly reduces inflammation and stress",
        ),
        // Comfrey's useful constituents are authored as heat-sensitive. This
        // is a deliberate, visible degradation outcome rather than a roll.
        ("comfrey", PreparationMethod::InfuseDecoct) => (
            CraftOutcome::DegradedWaste("spent_herb_waste"),
            "No medicinal effect; excessive heat destroys the useful preparation",
        ),
        _ => return None,
    };
    Some(CraftPreview {
        base_ingredient_id: base,
        grade,
        method,
        input_units,
        required_consumable,
        duration_minutes,
        potency_tier,
        outcome,
        expected_effect,
        risk,
        risk_awareness,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_public_grade_method_matrix_is_deterministic() {
        for base in SUPPORTED_BASE_INGREDIENTS {
            for id in [format!("{base}_poor"), base.into(), format!("{base}_fine")] {
                assert!(normalize_ingredient(&id).is_some());
                for method in PreparationMethod::ALL {
                    assert_eq!(preview(&id, method, 2.0), preview(&id, method, 2.0));
                }
            }
        }
        assert!(normalize_ingredient("mystery_lot").is_none());
        assert!(normalize_ingredient("tincture_spirit").is_none());
        assert!(preview("sage", PreparationMethod::DryGrind, 5.0).is_none());
    }

    #[test]
    fn every_bounded_identity_is_authored_in_the_item_catalogue() {
        for base in SUPPORTED_BASE_INGREDIENTS {
            for id in [format!("{base}_poor"), base.into(), format!("{base}_fine")] {
                assert!(crate::item_catalog::definition(&id).is_some(), "{id}");
            }
        }
        for base in SUPPORTED_BASE_INGREDIENTS {
            for method in PreparationMethod::ALL {
                if let Some(preview) = preview(base, method, 2.0) {
                    let output = match preview.outcome {
                        CraftOutcome::Medication(id) | CraftOutcome::DegradedWaste(id) => id,
                    };
                    assert!(
                        crate::item_catalog::definition(output).is_some(),
                        "{output}"
                    );
                }
            }
        }
    }

    #[test]
    fn grade_and_skill_improve_tier_while_skill_reduces_time() {
        let poor = preview("willow_bark_poor", PreparationMethod::InfuseDecoct, 0.0).unwrap();
        let ordinary = preview("willow_bark", PreparationMethod::InfuseDecoct, 1.0).unwrap();
        let fine = preview("willow_bark_fine", PreparationMethod::InfuseDecoct, 3.0).unwrap();
        assert!(poor.potency_tier < ordinary.potency_tier);
        assert!(ordinary.potency_tier < fine.potency_tier);
        assert!(poor.duration_minutes > fine.duration_minutes);
        assert_eq!(poor.input_units, 1);
        assert_eq!(fine.input_units, 1);
    }

    #[test]
    fn heat_degradation_and_toxic_tincture_are_explicit() {
        let degraded = preview("comfrey", PreparationMethod::InfuseDecoct, 3.0).unwrap();
        assert_eq!(
            degraded.outcome,
            CraftOutcome::DegradedWaste("spent_herb_waste")
        );
        assert!(degraded.expected_effect.contains("excessive heat"));
        let poppy = preview("poppy_fine", PreparationMethod::Tincture, 5.0).unwrap();
        assert!(poppy.risk.contains("oxygenation"));
        assert_eq!(
            poppy.required_consumable,
            Some(RequiredConsumable {
                item_id: "tincture_spirit",
                units: 1
            })
        );
        assert_eq!(poppy.potency_tier, 3);
        let weak = crate::physiology::current_intervention_profile("weak_poppy_tincture").unwrap();
        let strong =
            crate::physiology::current_intervention_profile("strong_poppy_tincture").unwrap();
        assert!(
            strong
                .adverse_delta_per_unit
                .get(crate::physiology::Meter::Oxygenation)
                > weak
                    .adverse_delta_per_unit
                    .get(crate::physiology::Meter::Oxygenation)
        );
        assert!(strong.route == crate::physiology::InterventionRoute::Oral);
    }

    #[test]
    fn risk_awareness_improves_but_never_removes_the_warning() {
        let novice = preview("poppy", PreparationMethod::Tincture, 0.0).unwrap();
        let intermediate = preview("poppy", PreparationMethod::Tincture, 2.0).unwrap();
        let master = preview("poppy", PreparationMethod::Tincture, 5.0).unwrap();
        assert_eq!(novice.risk_awareness, RiskAwareness::Coarse);
        assert_eq!(intermediate.risk_awareness, RiskAwareness::Intermediate);
        assert_eq!(master.risk_awareness, RiskAwareness::Exact);
        assert_ne!(novice.risk, master.risk);
        for risk in [novice.risk, intermediate.risk, master.risk] {
            assert!(risk.starts_with("Safety warning:"));
        }
    }

    #[test]
    fn only_tincture_requires_the_bounded_alcoholic_consumable() {
        assert!(
            preview("comfrey", PreparationMethod::DryGrind, 2.0)
                .unwrap()
                .required_consumable
                .is_none()
        );
        assert!(
            preview("willow_bark", PreparationMethod::InfuseDecoct, 2.0)
                .unwrap()
                .required_consumable
                .is_none()
        );
        assert_eq!(
            preview("poppy", PreparationMethod::Tincture, 2.0)
                .unwrap()
                .required_consumable
                .unwrap()
                .item_id,
            "tincture_spirit"
        );
        for method in PreparationMethod::ALL {
            assert!(!method.description().is_empty());
        }
    }
}
