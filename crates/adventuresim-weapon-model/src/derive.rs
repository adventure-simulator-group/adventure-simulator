use std::collections::HashMap;

use crate::{
    Attachment, ComponentDesign, ComponentRole, ComponentShape, DerivedMaterialMass,
    DerivedProperties, MaterialClass, ValidationError, WeaponDesign, WeaponHolderDesign,
    WeaponHolderKind, validate, validate_holder,
};

fn origin_y<'a>(
    component: &'a ComponentDesign,
    by_id: &HashMap<&'a str, &'a ComponentDesign>,
    cache: &mut HashMap<&'a str, f32>,
) -> f32 {
    if let Some(value) = cache.get(component.id.as_str()) {
        return *value;
    }
    let mut value = component.offset.y as f32 / 1_000.0;
    if let Attachment::TopOf {
        component: parent,
        insertion,
    } = &component.attachment
    {
        let parent = by_id[parent.as_str()];
        value += origin_y(parent, by_id, cache) + parent.shape.axial_length().meters()
            - insertion.meters();
    }
    cache.insert(component.id.as_str(), value);
    value
}

pub fn derive_holder_properties(
    design: &WeaponHolderDesign,
) -> Result<DerivedProperties, Vec<ValidationError>> {
    validate_holder(design)?;
    let weapon = derive_properties(&design.fitted_weapon)?;
    let (mass_kg, length_m) = match design.kind {
        WeaponHolderKind::BladeSheath => {
            let blade_volume = design
                .fitted_weapon
                .components
                .iter()
                .filter(|component| matches!(&component.shape, ComponentShape::Blade(_)))
                .map(|component| volume(&component.shape))
                .sum::<f32>();
            let length = weapon.grip_to_tip_m + design.chape_length.meters() * 0.5;
            let leather = blade_volume * 0.22 * design.body_material.density_kg_m3();
            let fittings = blade_volume * 0.035 * design.fitting_material.density_kg_m3();
            ((leather + fittings).max(0.08), length)
        }
        WeaponHolderKind::HaftLoop => {
            let bar = design.loop_bar_radius.meters();
            let path = std::f32::consts::PI
                * (design.hanger_width.meters() + design.hanger_height.meters())
                + 0.16;
            let mass =
                path * std::f32::consts::PI * bar.powi(2) * design.body_material.density_kg_m3();
            (mass.max(0.04), design.hanger_height.meters())
        }
    };
    Ok(DerivedProperties {
        mass_kg,
        length_m,
        grip_to_tip_m: 0.0,
        center_of_mass_from_grip_m: 0.0,
        moment_of_inertia_kg_m2: 0.0,
        balance: 1.0,
    })
}

fn polygonal_tube(length: f32, radius: f32, segments: u16) -> f32 {
    let n = segments as f32;
    0.5 * n * (std::f32::consts::TAU / n).sin() * radius * radius * length
}
fn polygonal_frustum(length: f32, bottom: f32, top: f32, segments: u16) -> f32 {
    let n = segments as f32;
    0.5 * n
        * (std::f32::consts::TAU / n).sin()
        * length
        * (bottom * bottom + bottom * top + top * top)
        / 3.0
}

fn path_length(points: &[crate::OffsetMm], closed: bool) -> f32 {
    let pairs = points.windows(2).map(|pair| {
        let a = pair[0].meters();
        let b = pair[1].meters();
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
    });
    let mut length: f32 = pairs.sum();
    if closed {
        let a = points[0].meters();
        let b = points[points.len() - 1].meters();
        length += ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    }
    length
}

fn volume(shape: &ComponentShape) -> f32 {
    use ComponentShape::*;
    match shape {
        Cylinder(v) => polygonal_frustum(
            v.length.meters(),
            v.radius.meters() * v.bottom_scale.unit(),
            v.radius.meters() * v.top_scale.unit(),
            v.segments.0,
        ),
        OvalGrip(v) => {
            polygonal_frustum(
                v.length.meters(),
                v.width.meters() * v.bottom_scale.unit() * 0.5,
                v.width.meters() * v.top_scale.unit() * 0.5,
                v.segments.0,
            ) * v.thickness.meters()
                / v.width.meters()
        }
        Socket(v) => {
            let outer = (v.outer_radius.meters() + v.top_radius.meters()) * 0.5;
            let inner = outer - v.wall.meters();
            polygonal_tube(v.length.meters(), outer, v.segments.0)
                - polygonal_tube(v.length.meters(), inner, v.segments.0)
        }
        Blade(v) => {
            let section_factor = match v.section {
                crate::BladeSection::Flat => 0.32,
                crate::BladeSection::Diamond => 0.21,
                crate::BladeSection::Fullered => 0.19,
            };
            v.length.meters() * v.width.meters() * v.thickness.meters() * section_factor
        }
        Guard(v) => v.span.meters() * std::f32::consts::PI * v.radius.meters().powi(2),
        Mace(v) => {
            polygonal_tube(v.length.meters(), v.core_radius.meters(), v.segments.0)
                + v.length.meters()
                    * v.cusp_radius.meters()
                    * v.flange_thickness.meters()
                    * v.flanges as f32
                    * 0.58
        }
        Langet(v) => v.length.meters() * v.width.meters() * v.thickness.meters(),
        Axe(v) => v.reach.meters() * v.height.meters() * v.thickness.meters() * 0.58,
        HammerPoll(v) => v.length.meters() * v.face.meters() * v.thickness.meters() * 0.68,
        CurvedBeak(v) => {
            v.length.meters()
                * (v.root_section.meters() + v.tip_section.meters())
                * 0.5
                * v.thickness.meters()
        }
        FacetedBeak(v) => {
            v.length.meters() * (v.root.meters() + v.tip.meters()) * 0.5 * v.thickness.meters()
        }
        Glaive(v) => v.length.meters() * v.width.meters() * v.thickness.meters() * 0.52,
        Bill(v) => {
            (v.length.meters() * v.width.meters() * 0.42
                + v.hook.meters() * v.width.meters() * 0.55)
                * v.thickness.meters()
        }
        Fork(v) => v.length.meters() * v.width.meters() * v.thickness.meters() * 0.44,
        Partisan(v) => v.length.meters() * v.width.meters() * v.thickness.meters() * 0.46,
        TubePath(v) => {
            path_length(&v.points, v.closed) * std::f32::consts::PI * v.radius.meters().powi(2)
        }
        RingGuard(v) => {
            ((v.arc_end.0 - v.arc_start.0).unsigned_abs() as f32 / 1_000.0)
                * v.radius.meters()
                * std::f32::consts::PI
                * v.bar.meters().powi(2)
        }
        FigureEight(v) => {
            // Ramanujan's ellipse circumference, doubled for the two lobes.
            let a = v.width.meters() * 0.25;
            let b = v.height.meters() * 0.25;
            let h = ((a - b) / (a + b)).powi(2);
            let length = 2.0
                * std::f32::consts::PI
                * (a + b)
                * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
            length * std::f32::consts::PI * v.bar.meters().powi(2)
        }
        FanPommel(v) => v.width.meters() * v.height.meters() * v.thickness.meters() * 0.55,
        Rondel(v) => polygonal_tube(v.thickness.meters(), v.radius.meters(), v.segments.0),
        GothicMace(v) => {
            polygonal_tube(
                v.length.meters() + v.crown_length.meters(),
                v.root_radius.meters(),
                v.radial_segments.0,
            ) + v.length.meters()
                * (v.cusp_radius.meters() - v.root_radius.meters())
                * v.flange_thickness.meters()
                * v.flanges as f32
                * (0.44 + 0.18 * (1.0 - v.concavity.unit()))
        }
        SlabGrip(v) => {
            v.length.meters()
                * v.width.meters()
                * (v.thickness.meters() + v.scale_thickness.meters() * 2.0)
        }
        KnuckleBow(v) => {
            v.length.meters().hypot(v.width.meters() * 2.0)
                * std::f32::consts::PI
                * v.bar.meters().powi(2)
        }
        Collar(v) => polygonal_tube(v.width.meters(), v.radius.meters(), v.segments.0),
        Sleeve(v) => {
            let outer = (v.radius.meters() + v.top_radius.meters()) * 0.5;
            let inner = outer - v.wall.meters();
            std::f32::consts::PI * (outer * outer - inner * inner) * v.length.meters()
        }
        Boss(v) => polygonal_tube(v.thickness.meters(), v.radius.meters(), v.segments.0),
        Spear(v) => v.length.meters() * v.width.meters() * v.thickness.meters() * 0.45,
        ProfiledPommel(v) => v
            .profile
            .windows(2)
            .map(|pair| {
                let h = (pair[1].y.0 - pair[0].y.0) as f32 / 1_000.0;
                let a = pair[0].radius.meters();
                let b = pair[1].radius.meters();
                std::f32::consts::PI * h * (a * a + a * b + b * b) / 3.0
            })
            .sum(),
    }
}

fn central_transverse_inertia_per_kg(shape: &ComponentShape) -> f32 {
    let length = shape.axial_length().meters();
    let axial = length * length / 12.0;
    let lateral = match shape {
        ComponentShape::Cylinder(value) => value.radius.meters().powi(2) / 4.0,
        ComponentShape::OvalGrip(value) => {
            (value.width.meters().powi(2) + value.thickness.meters().powi(2)) / 32.0
        }
        ComponentShape::Blade(value) => {
            (value.width.meters().powi(2) + value.thickness.meters().powi(2)) / 24.0
        }
        ComponentShape::Guard(value) => value.span.meters().powi(2) / 24.0,
        ComponentShape::FanPommel(value) => {
            (value.width.meters().powi(2) + value.thickness.meters().powi(2)) / 24.0
        }
        ComponentShape::Rondel(value) => value.radius.meters().powi(2) / 4.0,
        ComponentShape::ProfiledPommel(value) => {
            value
                .profile
                .iter()
                .map(|point| point.radius.meters())
                .fold(0.0_f32, f32::max)
                .powi(2)
                / 4.0
        }
        _ => 0.0,
    };
    axial + lateral
}

/// Computes gameplay-facing physical properties directly from the quantized recipe.
///
/// This intentionally does not generate vertices or indices. Rendering clients call
/// [`crate::generate`] when they need the full mesh.
pub fn derive_properties(design: &WeaponDesign) -> Result<DerivedProperties, Vec<ValidationError>> {
    validate(design)?;
    let by_id: HashMap<_, _> = design
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    let mut origins = HashMap::new();
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    let mut grip = 0.0;
    let mut mass = 0.0;
    let mut first_moment = 0.0;
    let mut components = Vec::with_capacity(design.components.len());
    for component in &design.components {
        let origin = origin_y(component, &by_id, &mut origins);
        let top = origin + component.shape.axial_length().meters();
        minimum = minimum.min(origin);
        maximum = maximum.max(top);
        if component.role == ComponentRole::Grip {
            grip = (origin + top) * 0.5;
        }
        let component_mass = volume(&component.shape) * component.material.density_kg_m3();
        let center = (origin + top) * 0.5;
        mass += component_mass;
        first_moment += component_mass * center;
        components.push((
            component_mass,
            center,
            central_transverse_inertia_per_kg(&component.shape),
        ));
    }
    let center_of_mass = first_moment / mass;
    let moment_of_inertia = components
        .into_iter()
        .map(|(component_mass, center, central)| {
            component_mass * (central + (center - grip).powi(2))
        })
        .sum::<f32>();
    let grip_to_tip = maximum - grip;
    Ok(DerivedProperties {
        mass_kg: mass,
        length_m: maximum - minimum,
        grip_to_tip_m: grip_to_tip,
        center_of_mass_from_grip_m: center_of_mass - grip,
        moment_of_inertia_kg_m2: moment_of_inertia,
        balance: (moment_of_inertia / mass).sqrt() / grip_to_tip,
    })
}

/// Returns exact per-material mass derived from the same component volumes as
/// the weapon's total physical properties.
pub fn derive_material_masses(
    design: &WeaponDesign,
) -> Result<Vec<DerivedMaterialMass>, Vec<ValidationError>> {
    validate(design)?;
    let classes = [
        MaterialClass::Wood,
        MaterialClass::Leather,
        MaterialClass::DarkLeather,
        MaterialClass::Brass,
        MaterialClass::Steel,
        MaterialClass::DarkSteel,
    ];
    Ok(classes
        .into_iter()
        .filter_map(|material| {
            let mass_kg = design
                .components
                .iter()
                .filter(|component| component.material == material)
                .map(|component| volume(&component.shape) * material.density_kg_m3())
                .sum::<f32>();
            (mass_kg > f32::EPSILON).then_some(DerivedMaterialMass { material, mass_kg })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_material_mass_conserves_total_recipe_mass() {
        for catalog_id in crate::MELEE_CATALOG_IDS {
            let design = crate::default_design(catalog_id).unwrap();
            let total = derive_properties(&design).unwrap().mass_kg;
            let by_material = derive_material_masses(&design)
                .unwrap()
                .into_iter()
                .map(|mass| mass.mass_kg)
                .sum::<f32>();
            assert!((total - by_material).abs() < 0.000_01, "{catalog_id}");
        }
    }
}
