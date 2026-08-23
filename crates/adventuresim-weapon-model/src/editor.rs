use serde::Serialize;

use crate::{ComponentShape, WeaponDesign};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NumericEditorField {
    pub path: String,
    pub min: i32,
    pub max: i32,
    pub step: u32,
}

pub fn numeric_editor_fields(design: &WeaponDesign) -> Vec<NumericEditorField> {
    let mut fields = Vec::new();
    for (index, component) in design.components.iter().enumerate() {
        let prefix = format!("components.{index}.shape");
        macro_rules! field {
            ($shape:literal, $name:literal, $min:expr, $max:expr, $step:expr) => {
                fields.push(NumericEditorField {
                    path: format!("{prefix}.{}.{}", $shape, $name),
                    min: $min,
                    max: $max,
                    step: $step,
                });
            };
        }
        match &component.shape {
            ComponentShape::Cylinder(_) => {
                field!("Cylinder", "length", 20, 4_000, 5);
                field!("Cylinder", "radius", 5, 80, 1);
                field!("Cylinder", "bottom_scale", 500, 1_500, 25);
                field!("Cylinder", "top_scale", 500, 1_500, 25);
            }
            ComponentShape::Blade(_) => {
                field!("Blade", "length", 100, 1_800, 5);
                field!("Blade", "width", 15, 160, 1);
                field!("Blade", "thickness", 3, 24, 1);
                field!("Blade", "curvature", -300, 300, 5);
                field!("Blade", "taper", 300, 1_000, 25);
                field!("Blade", "single_edge", 0, 1_000, 50);
                field!("Blade", "belly", -300, 500, 25);
                field!("Blade", "ricasso", 0, 500, 10);
            }
            ComponentShape::Guard(_) => {
                field!("Guard", "span", 40, 600, 5);
                field!("Guard", "radius", 3, 20, 1);
                field!("Guard", "sweep", -100, 100, 5);
            }
            ComponentShape::Mace(_) => {
                field!("Mace", "length", 60, 400, 5);
                field!("Mace", "core_radius", 5, 60, 1);
                field!("Mace", "cusp_radius", 15, 120, 1);
                field!("Mace", "flanges", 4, 12, 1);
                field!("Mace", "cusp_height", 100, 900, 25);
            }
            ComponentShape::Socket(_) => {
                field!("Socket", "length", 40, 500, 5);
                field!("Socket", "outer_radius", 10, 60, 1);
                field!("Socket", "top_radius", 10, 60, 1);
                field!("Socket", "wall", 1, 10, 1);
            }
            ComponentShape::Langet(_) => {
                field!("Langet", "length", 40, 800, 5);
                field!("Langet", "width", 5, 80, 1);
                field!("Langet", "thickness", 1, 12, 1);
            }
            ComponentShape::Axe(_) => {
                field!("Axe", "reach", 50, 350, 5);
                field!("Axe", "height", 60, 500, 5);
                field!("Axe", "thickness", 5, 50, 1);
                field!("Axe", "beard", 0, 1_000, 25);
                field!("Axe", "curvature", 0, 1_000, 25);
            }
            ComponentShape::HammerPoll(_) => {
                field!("HammerPoll", "length", 40, 250, 5);
                field!("HammerPoll", "face", 20, 180, 5);
                field!("HammerPoll", "neck", 10, 120, 5);
                field!("HammerPoll", "crown", 0, 500, 25);
            }
            ComponentShape::CurvedBeak(_) => {
                field!("CurvedBeak", "length", 50, 300, 5);
                field!("CurvedBeak", "curvature", -200, 200, 5);
                field!("CurvedBeak", "droop", -100, 100, 5);
            }
            ComponentShape::FacetedBeak(_) => {
                field!("FacetedBeak", "length", 40, 250, 5);
                field!("FacetedBeak", "set", -100, 100, 5);
            }
            ComponentShape::Glaive(_) => {
                field!("Glaive", "length", 200, 800, 10);
                field!("Glaive", "width", 40, 180, 5);
                field!("Glaive", "curvature", -250, 250, 5);
            }
            ComponentShape::Bill(_) => {
                field!("Bill", "length", 180, 650, 10);
                field!("Bill", "width", 40, 180, 5);
                field!("Bill", "hook", 20, 180, 5);
            }
            ComponentShape::Fork(_) => {
                field!("Fork", "length", 180, 650, 10);
                field!("Fork", "width", 60, 240, 5);
                field!("Fork", "crotch", 100, 800, 25);
            }
            ComponentShape::Partisan(_) => {
                field!("Partisan", "length", 180, 650, 10);
                field!("Partisan", "width", 50, 220, 5);
                field!("Partisan", "lug_width", 60, 260, 5);
            }
            ComponentShape::TubePath(_) => {
                field!("TubePath", "radius", 2, 16, 1);
            }
            ComponentShape::RingGuard(_) => {
                field!("RingGuard", "radius", 25, 100, 1);
                field!("RingGuard", "bar", 3, 14, 1);
            }
            ComponentShape::FigureEight(_) => {
                field!("FigureEight", "width", 60, 240, 5);
                field!("FigureEight", "height", 20, 100, 2);
                field!("FigureEight", "bar", 3, 16, 1);
            }
            ComponentShape::FanPommel(_) => {
                field!("FanPommel", "width", 25, 100, 1);
                field!("FanPommel", "height", 20, 100, 1);
                field!("FanPommel", "thickness", 5, 30, 1);
            }
            ComponentShape::Rondel(_) => {
                field!("Rondel", "radius", 20, 80, 1);
                field!("Rondel", "thickness", 5, 30, 1);
            }
            ComponentShape::GothicMace(_) => {
                field!("GothicMace", "length", 80, 400, 5);
                field!("GothicMace", "cusp_radius", 25, 100, 1);
                field!("GothicMace", "concavity", 0, 1_000, 25);
                field!("GothicMace", "flanges", 4, 12, 1);
            }
            ComponentShape::SlabGrip(_) => {
                field!("SlabGrip", "length", 80, 400, 5);
                field!("SlabGrip", "width", 20, 70, 1);
                field!("SlabGrip", "scale_thickness", 3, 20, 1);
            }
            ComponentShape::KnuckleBow(_) => {
                field!("KnuckleBow", "width", 40, 180, 5);
                field!("KnuckleBow", "length", 80, 300, 5);
                field!("KnuckleBow", "bulge", 0, 1_000, 25);
            }
            ComponentShape::Collar(_) => {
                field!("Collar", "width", 5, 50, 1);
                field!("Collar", "radius", 10, 50, 1);
            }
            ComponentShape::Sleeve(_) => {
                field!("Sleeve", "length", 40, 300, 5);
                field!("Sleeve", "radius", 8, 50, 1);
                field!("Sleeve", "top_radius", 8, 50, 1);
            }
            ComponentShape::Boss(_) => {
                field!("Boss", "radius", 4, 30, 1);
                field!("Boss", "thickness", 4, 60, 1);
            }
            ComponentShape::Spear(_) => {
                field!("Spear", "length", 120, 650, 5);
                field!("Spear", "width", 25, 160, 5);
                field!("Spear", "belly_position", 100, 700, 25);
                field!("Spear", "acuteness", 300, 1_500, 25);
            }
            ComponentShape::ProfiledPommel(_) => {}
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MELEE_CATALOG_IDS, default_design};

    #[test]
    fn catalog_editor_fields_are_bounded_and_include_blade_ricassos() {
        for id in MELEE_CATALOG_IDS {
            let design = default_design(id).unwrap();
            let value = serde_json::to_value(&design).unwrap();
            let fields = numeric_editor_fields(&design);
            assert!(
                fields
                    .iter()
                    .all(|field| field.min < field.max && field.step > 0),
                "{id}"
            );
            for field in &fields {
                let pointer = format!("/{}", field.path.replace('.', "/"));
                let current = value
                    .pointer(&pointer)
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or_else(|| panic!("missing {id} {}", field.path));
                assert!(
                    (i64::from(field.min)..=i64::from(field.max)).contains(&current),
                    "{id} {}={current} outside {}..={}",
                    field.path,
                    field.min,
                    field.max
                );
            }
        }
        let fields = numeric_editor_fields(&default_design("zweihander").unwrap());
        assert!(
            fields
                .iter()
                .any(|field| field.path.ends_with("Blade.ricasso"))
        );
        assert!(
            fields
                .iter()
                .all(|field| !field.path.ends_with("samples") && !field.path.ends_with("segments"))
        );
    }
}
