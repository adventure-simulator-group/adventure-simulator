use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::{
    Attachment, ComponentRole, ComponentShape, WeaponDesign, WeaponHolderDesign, WeaponHolderKind,
    recommended_holder,
};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("weapon catalog ID is empty")]
    EmptyCatalogId,
    #[error("weapon has no components")]
    EmptyDesign,
    #[error("weapon exceeds the 64-component generation limit")]
    TooManyComponents,
    #[error("weapon must have exactly one root component")]
    InvalidRootCount,
    #[error("weapon must have exactly one primary grip component")]
    InvalidGripCount,
    #[error("component ID is empty")]
    EmptyComponentId,
    #[error("duplicate component ID `{0}`")]
    DuplicateComponent(String),
    #[error("component `{component}` references missing parent `{parent}`")]
    MissingParent { component: String, parent: String },
    #[error("attachment cycle contains `{0}`")]
    AttachmentCycle(String),
    #[error("component `{0}` has invalid quantized dimensions")]
    InvalidDimensions(String),
    #[error("component `{0}` does not contact its attachment parent")]
    InvalidAttachment(String),
    #[error("resolved weapon axial span exceeds 12 metres")]
    OverallBoundsExceeded,
}

fn footprint(shape: &ComponentShape) -> (u64, u64, u64) {
    let mm = |value: u32| u64::from(value);
    match shape {
        ComponentShape::Cylinder(value) => {
            let radius =
                mm(value.radius.0) * u64::from(value.bottom_scale.0.max(value.top_scale.0)) / 1_000;
            (radius, radius, mm(value.length.0))
        }
        ComponentShape::Blade(value) => (
            mm(value.width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::Guard(value) => (
            mm(value.span.0) / 2,
            mm(value.radius.0),
            mm(value.radius.0) * 2,
        ),
        ComponentShape::Mace(value) => (
            mm(value.cusp_radius.0),
            mm(value.cusp_radius.0),
            mm(value.length.0),
        ),
        ComponentShape::Socket(value) => (
            mm(value.outer_radius.0),
            mm(value.outer_radius.0),
            mm(value.length.0),
        ),
        ComponentShape::Langet(value) => (
            mm(value.width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::SectionBlade(value) => (
            mm(value.width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::Axe(value) => (
            mm(value.reach.0),
            mm(value.thickness.0) / 2,
            mm(value.height.0),
        ),
        ComponentShape::HammerPoll(value) => (
            mm(value.length.0),
            mm(value.face_thickness.0) / 2,
            mm(value.face.0),
        ),
        ComponentShape::CurvedBeak(value) => (
            mm(value.length.0),
            mm(value.thickness.0) / 2,
            mm(value.root_section.0),
        ),
        ComponentShape::FacetedBeak(value) => (
            mm(value.length.0),
            mm(value.thickness.0) / 2,
            mm(value.root.0),
        ),
        ComponentShape::Glaive(value) => (
            mm(value.width.0),
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::Bill(value) => (
            mm(value.width.0) + mm(value.hook.0),
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::Fork(value) => (
            mm(value.width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::Partisan(value) => (
            mm(value.lug_width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.length.0),
        ),
        ComponentShape::TubePath(value) => {
            let x = value
                .points
                .iter()
                .map(|point| point.x.unsigned_abs())
                .max()
                .unwrap_or(0);
            let z = value
                .points
                .iter()
                .map(|point| point.z.unsigned_abs())
                .max()
                .unwrap_or(0);
            let y = value
                .points
                .iter()
                .map(|point| point.y.unsigned_abs())
                .max()
                .unwrap_or(0);
            (
                u64::from(x) + mm(value.radius.0),
                u64::from(z) + mm(value.radius.0),
                u64::from(y) + mm(value.radius.0),
            )
        }
        ComponentShape::RingGuard(value) => (
            mm(value.radius.0) + mm(value.bar.0),
            mm(value.bar.0),
            mm(value.radius.0) * 2,
        ),
        ComponentShape::FigureEight(value) => {
            (mm(value.width.0) / 2, mm(value.bar.0), mm(value.height.0))
        }
        ComponentShape::FanPommel(value) => (
            mm(value.width.0) / 2,
            mm(value.thickness.0) / 2,
            mm(value.height.0),
        ),
        ComponentShape::Rondel(value) => (
            mm(value.radius.0),
            mm(value.radius.0),
            mm(value.thickness.0),
        ),
        ComponentShape::GothicMace(value) => (
            mm(value.cusp_radius.0),
            mm(value.cusp_radius.0),
            mm(value.length.0) + mm(value.crown_length.0),
        ),
        ComponentShape::SlabGrip(v) => (
            mm(v.width.0) / 2,
            mm(v.thickness.0) + mm(v.scale_thickness.0),
            mm(v.length.0),
        ),
        ComponentShape::KnuckleBow(v) => (mm(v.width.0), mm(v.bar.0), mm(v.length.0)),
        ComponentShape::Collar(v) => (mm(v.radius.0), mm(v.radius.0), mm(v.width.0)),
        ComponentShape::Sleeve(v) => (mm(v.radius.0), mm(v.radius.0), mm(v.length.0)),
        ComponentShape::Boss(v) => (mm(v.radius.0), mm(v.thickness.0) / 2, mm(v.radius.0) * 2),
        ComponentShape::Spear(v) => (mm(v.width.0) / 2, mm(v.thickness.0) / 2, mm(v.length.0)),
        ComponentShape::ProfiledPommel(v) => {
            let radius = v
                .profile
                .iter()
                .map(|point| point.radius.0)
                .max()
                .unwrap_or(0);
            let length = v.profile.last().map_or(0, |point| point.y.0);
            (mm(radius), mm(radius), mm(length))
        }
    }
}

pub fn validate(design: &WeaponDesign) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    if design.catalog_id.trim().is_empty() {
        errors.push(ValidationError::EmptyCatalogId);
    }
    if design.components.is_empty() {
        errors.push(ValidationError::EmptyDesign);
    }
    if design.components.len() > 64 {
        errors.push(ValidationError::TooManyComponents);
    }
    if design
        .components
        .iter()
        .filter(|component| component.attachment == Attachment::Root)
        .count()
        != 1
    {
        errors.push(ValidationError::InvalidRootCount);
    }
    if design
        .components
        .iter()
        .filter(|component| component.role == crate::ComponentRole::Grip)
        .count()
        != 1
    {
        errors.push(ValidationError::InvalidGripCount);
    }
    let mut ids = HashSet::new();
    for component in &design.components {
        if component.id.trim().is_empty() {
            errors.push(ValidationError::EmptyComponentId);
        }
        if !ids.insert(component.id.as_str()) {
            errors.push(ValidationError::DuplicateComponent(component.id.clone()));
        }
        let offset = component.offset;
        let offset_valid = [offset.x, offset.y, offset.z]
            .into_iter()
            .all(|value| value.unsigned_abs() <= 6_000);
        let valid = offset_valid
            && match &component.shape {
                ComponentShape::Cylinder(value) => {
                    (1..=6_000).contains(&value.length.0)
                        && (1..=500).contains(&value.radius.0)
                        && (100..=2_000).contains(&value.bottom_scale.0)
                        && (100..=2_000).contains(&value.top_scale.0)
                        && (8..=256).contains(&value.segments.0)
                }
                ComponentShape::Blade(value) => {
                    (1..=3_000).contains(&value.length.0)
                        && (1..=1_000).contains(&value.width.0)
                        && (1..=200).contains(&value.thickness.0)
                        && value.curvature.0.unsigned_abs() <= 2_000
                        && (100..=2_500).contains(&value.taper.0)
                        && value.single_edge.0 <= 1_000
                        && value.belly.0.unsigned_abs() <= 1_000
                }
                ComponentShape::Guard(value) => {
                    value
                        .radius
                        .0
                        .checked_mul(2)
                        .is_some_and(|diameter| value.span.0 > diameter)
                        && value.span.0 <= 1_000
                        && (1..=100).contains(&value.radius.0)
                        && (2..=256).contains(&value.samples.0)
                        && (8..=64).contains(&value.radial_segments.0)
                        && value.sweep.0.unsigned_abs() <= 1_000
                }
                ComponentShape::Mace(value) => {
                    (1..=1_000).contains(&value.length.0)
                        && (1..=200).contains(&value.core_radius.0)
                        && value.cusp_radius.0 > value.core_radius.0
                        && value.cusp_radius.0 <= 500
                        && (3..=32).contains(&value.flanges)
                        && (1..=100).contains(&value.flange_thickness.0)
                        && (8..=256).contains(&value.segments.0)
                        && value.cusp_height.0 > 0
                        && value.cusp_height.0 < 1_000
                }
                ComponentShape::Socket(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && v.outer_radius.0 > v.wall.0
                        && v.outer_radius.0 <= 500
                        && v.top_radius.0 > v.wall.0
                        && v.top_radius.0 <= 500
                        && v.wall.0 > 0
                        && v.wall.0 <= 100
                        && (8..=256).contains(&v.segments.0)
                }
                ComponentShape::Langet(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && (1..=500).contains(&v.width.0)
                        && (1..=200).contains(&v.thickness.0)
                }
                ComponentShape::SectionBlade(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && (1..=1_000).contains(&v.width.0)
                        && (1..=200).contains(&v.thickness.0)
                        && v.curvature.0.unsigned_abs() <= 2_000
                        && (100..=2_500).contains(&v.taper.0)
                        && (4..=256).contains(&v.samples.0)
                }
                ComponentShape::Axe(v) => {
                    (1..=2_000).contains(&v.reach.0)
                        && v.reach.0 > v.root_width.0
                        && (1..=2_000).contains(&v.height.0)
                        && (1..=200).contains(&v.thickness.0)
                        && (1..=500).contains(&v.root_width.0)
                        && v.side.unsigned_abs() == 1
                        && v.beard.0 <= 1_000
                        && v.curvature.0 <= 1_000
                        && v.upper_shoulder.0 <= 1_000
                        && v.lower_shoulder.0 <= 1_000
                        && v.flare.0.unsigned_abs() <= 1_000
                        && v.toe.0.unsigned_abs() <= 1_000
                        && v.heel.0.unsigned_abs() <= 1_000
                        && v.beard_drop.0 <= 1_000
                }
                ComponentShape::HammerPoll(v) => {
                    (1..=2_000).contains(&v.length.0)
                        && v.face.0 > v.neck.0
                        && v.face.0 <= 1_000
                        && (1..=500).contains(&v.neck.0)
                        && (1..=500).contains(&v.thickness.0)
                        && (1..=500).contains(&v.face_thickness.0)
                        && v.direction.unsigned_abs() == 1
                        && v.crown.0 <= 1_000
                        && (50..=880).contains(&v.neck_ratio.0)
                        && v.face_flare.0 <= 1_000
                        && v.crown_length.0 <= 500
                }
                ComponentShape::CurvedBeak(v) => {
                    (1..=2_000).contains(&v.length.0)
                        && v.root_section.0 > v.tip_section.0
                        && v.tip_section.0 > 0
                        && v.root_section.0 <= 500
                        && (1..=200).contains(&v.thickness.0)
                        && v.direction.unsigned_abs() == 1
                        && (4..=256).contains(&v.samples.0)
                        && v.bend_position.0 <= 1_000
                        && v.droop.0.unsigned_abs() <= 1_000
                        && v.curvature.0.unsigned_abs() <= 2_000
                }
                ComponentShape::FacetedBeak(v) => {
                    (1..=2_000).contains(&v.length.0)
                        && v.root.0 > v.tip.0
                        && v.tip.0 > 0
                        && v.root.0 <= 500
                        && (1..=200).contains(&v.thickness.0)
                        && v.direction.unsigned_abs() == 1
                        && v.bend_position.0 <= 1_000
                        && (1..=200).contains(&v.tip_thickness.0)
                        && v.set.0.unsigned_abs() <= 2_000
                }
                ComponentShape::Glaive(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && v.width.0 > v.root.0
                        && v.width.0 <= 1_000
                        && v.root.0 > 0
                        && v.root.0 <= 500
                        && (1..=200).contains(&v.thickness.0)
                        && v.curvature.0.unsigned_abs() <= 2_000
                        && v.edge_curvature.0 <= 1_000
                        && v.spine_curvature.0 <= 1_000
                        && v.point_length.0 < 800
                        && (8..=256).contains(&v.samples.0)
                        && v.belly_position.0 > 0
                        && v.belly_position.0 < 1_000
                        && (1..=500).contains(&v.root_length.0)
                }
                ComponentShape::Bill(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && v.width.0 > v.root.0
                        && v.width.0 <= 1_000
                        && v.root.0 > 0
                        && v.root.0 <= 500
                        && (1..=1_000).contains(&v.hook.0)
                        && (1..=200).contains(&v.thickness.0)
                        && v.hook_depth.0 < 800
                        && v.hook_curvature.0 <= 1_000
                        && (8..=256).contains(&v.samples.0)
                        && v.belly_position.0 > 0
                        && v.belly_position.0 < 1_000
                        && v.point_length.0 > 0
                        && v.point_length.0 < 800
                        && (1..=500).contains(&v.root_length.0)
                }
                ComponentShape::Fork(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && v.width.0 > v.base_width.0
                        && v.width.0 <= 1_000
                        && v.tine_width
                            .0
                            .checked_mul(2)
                            .is_some_and(|twice| v.width.0 > twice)
                        && (1..=200).contains(&v.thickness.0)
                        && v.crotch.0 < 800
                        && v.taper.0 <= 1_000
                        && v.shoulder_blend.0 <= 1_000
                        && v.crotch
                            .0
                            .checked_add(v.crotch_round.0)
                            .is_some_and(|sum| sum < 800)
                }
                ComponentShape::Partisan(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && v.lug_width.0 >= v.width.0
                        && v.lug_width.0 <= 1_500
                        && v.width.0 > v.root_width.0
                        && v.width.0 <= 1_000
                        && (1..=500).contains(&v.root_width.0)
                        && (1..=200).contains(&v.thickness.0)
                        && v.belly.0 < 800
                        && v.lug_drop.0 <= 1_000
                        && v.belly_position.0 > 0
                        && v.belly_position.0 < 1_000
                        && v.lug_sweep.0 <= 1_000
                        && (100..=2_500).contains(&v.acuteness.0)
                }
                ComponentShape::TubePath(v) => {
                    v.points.len() >= 2
                        && v.points.len() <= 256
                        && v.radius.0 > 0
                        && (8..=64).contains(&v.radial_segments.0)
                        && v.radius.0 <= 100
                        && v.points.iter().all(|point| {
                            [point.x, point.y, point.z]
                                .into_iter()
                                .all(|axis| axis.unsigned_abs() <= 6_000)
                        })
                        && v.points.windows(2).all(|pair| pair[0] != pair[1])
                }
                ComponentShape::RingGuard(v) => {
                    v.radius.0 > v.bar.0
                        && v.radius.0 <= 1_000
                        && v.bar.0 <= 100
                        && v.bar.0 > 0
                        && v.arc_end.0 > v.arc_start.0
                        && v.arc_start.0.unsigned_abs() <= 6_500
                        && v.arc_end.0.unsigned_abs() <= 6_500
                        && (8..=256).contains(&v.samples.0)
                        && (8..=64).contains(&v.radial_segments.0)
                }
                ComponentShape::FigureEight(v) => {
                    v.width.0 <= 2_000
                        && v.height.0 <= 2_000
                        && v.bar.0 <= 100
                        && v.bar.0.checked_mul(4).is_some_and(|bar| v.width.0 > bar)
                        && v.bar.0.checked_mul(2).is_some_and(|bar| v.height.0 > bar)
                        && (16..=256).contains(&v.samples.0)
                        && (8..=64).contains(&v.radial_segments.0)
                }
                ComponentShape::FanPommel(v) => {
                    (1..=1_000).contains(&v.width.0)
                        && (1..=1_000).contains(&v.height.0)
                        && (1..=200).contains(&v.thickness.0)
                }
                ComponentShape::Rondel(v) => {
                    (1..=1_000).contains(&v.radius.0)
                        && (1..=200).contains(&v.thickness.0)
                        && (8..=256).contains(&v.segments.0)
                }
                ComponentShape::GothicMace(v) => {
                    (1..=2_000).contains(&v.length.0)
                        && v.crown_length.0 <= 500
                        && v.length
                            .0
                            .checked_add(v.crown_length.0)
                            .is_some_and(|sum| sum <= 2_500)
                        && v.cusp_radius.0 > v.root_radius.0
                        && v.cusp_radius.0 > v.shoulder_radius.0
                        && (1..=200).contains(&v.root_radius.0)
                        && (1..=200).contains(&v.shoulder_radius.0)
                        && v.cusp_radius.0 <= 500
                        && v.flanges >= 3
                        && v.flanges <= 32
                        && (1..=100).contains(&v.flange_thickness.0)
                        && v.cusp_height.0 > 0
                        && v.cusp_height.0 < 1_000
                        && v.concavity.0 <= 1_000
                        && (4..=128).contains(&v.profile_samples.0)
                        && (8..=64).contains(&v.radial_segments.0)
                }
                ComponentShape::SlabGrip(v) => {
                    (1..=1_000).contains(&v.length.0)
                        && (1..=300).contains(&v.width.0)
                        && (1..=100).contains(&v.thickness.0)
                        && v.scale_thickness.0 <= 100
                }
                ComponentShape::KnuckleBow(v) => {
                    (1..=1_000).contains(&v.width.0)
                        && (1..=1_000).contains(&v.length.0)
                        && (1..=100).contains(&v.bar.0)
                        && v.side.unsigned_abs() == 1
                        && v.bulge.0 <= 1_000
                        && (4..=256).contains(&v.samples.0)
                        && (8..=64).contains(&v.radial_segments.0)
                }
                ComponentShape::Collar(v) => {
                    (1..=200).contains(&v.width.0)
                        && (1..=200).contains(&v.radius.0)
                        && (8..=256).contains(&v.segments.0)
                }
                ComponentShape::Sleeve(v) => {
                    (1..=2_000).contains(&v.length.0)
                        && v.radius.0 > v.wall.0
                        && v.radius.0 <= 500
                        && v.top_radius.0 > v.wall.0
                        && v.top_radius.0 <= 500
                        && (1..=100).contains(&v.wall.0)
                        && (8..=256).contains(&v.segments.0)
                }
                ComponentShape::Boss(v) => {
                    (1..=200).contains(&v.radius.0)
                        && (1..=200).contains(&v.thickness.0)
                        && (8..=256).contains(&v.segments.0)
                }
                ComponentShape::Spear(v) => {
                    (1..=3_000).contains(&v.length.0)
                        && (1..=1_000).contains(&v.width.0)
                        && (1..=200).contains(&v.thickness.0)
                        && v.width.0 > v.root_width.0
                        && v.root_width.0 > 0
                        && v.belly_position.0 > 0
                        && v.belly_position.0 < 1_000
                        && (100..=2_500).contains(&v.acuteness.0)
                        && (4..=256).contains(&v.samples.0)
                }
                ComponentShape::ProfiledPommel(v) => {
                    v.profile.len() >= 2
                        && v.profile.len() <= 64
                        && (8..=256).contains(&v.segments.0)
                        && v.profile
                            .iter()
                            .all(|point| point.y.0 <= 1_000 && (1..=500).contains(&point.radius.0))
                        && v.profile.windows(2).all(|pair| pair[0].y.0 < pair[1].y.0)
                }
            };
        if !valid {
            errors.push(ValidationError::InvalidDimensions(component.id.clone()));
        }
    }
    let by_id: HashMap<_, _> = design
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    for component in &design.components {
        if let Attachment::TopOf {
            component: parent,
            insertion,
        } = &component.attachment
        {
            let Some(parent_component) = by_id.get(parent.as_str()) else {
                errors.push(ValidationError::MissingParent {
                    component: component.id.clone(),
                    parent: parent.clone(),
                });
                continue;
            };
            let (parent_x, parent_z, parent_depth) = footprint(&parent_component.shape);
            let (child_x, child_z, child_depth) = footprint(&component.shape);
            let lateral_contact = u64::from(component.offset.x.unsigned_abs())
                <= parent_x.saturating_add(child_x)
                && u64::from(component.offset.z.unsigned_abs()) <= parent_z.saturating_add(child_z);
            let offset_y = i64::from(component.offset.y);
            let insertion_y = i64::from(insertion.0);
            let axial_contact = u64::from(insertion.0) <= parent_depth
                && offset_y <= insertion_y
                && offset_y >= insertion_y - parent_depth as i64 - child_depth as i64;
            if !lateral_contact || !axial_contact {
                errors.push(ValidationError::InvalidAttachment(component.id.clone()));
            }
        }
    }
    if errors.is_empty() {
        let mut origins = HashMap::<&str, i64>::new();
        while origins.len() < design.components.len() {
            let before = origins.len();
            for component in &design.components {
                if origins.contains_key(component.id.as_str()) {
                    continue;
                }
                let origin = match &component.attachment {
                    Attachment::Root => Some(component.offset.y as i64),
                    Attachment::TopOf {
                        component: parent,
                        insertion,
                    } => origins.get(parent.as_str()).map(|parent_origin| {
                        parent_origin + by_id[parent.as_str()].shape.axial_length().0 as i64
                            - insertion.0 as i64
                            + component.offset.y as i64
                    }),
                };
                if let Some(origin) = origin {
                    origins.insert(component.id.as_str(), origin);
                }
            }
            if origins.len() == before {
                break;
            }
        }
        let minimum = origins.values().copied().min().unwrap_or(0);
        let maximum = design
            .components
            .iter()
            .filter_map(|component| {
                origins
                    .get(component.id.as_str())
                    .map(|origin| origin + component.shape.axial_length().0 as i64)
            })
            .max()
            .unwrap_or(0);
        if maximum - minimum > 12_000 {
            errors.push(ValidationError::OverallBoundsExceeded);
        }
    }
    for component in &design.components {
        let mut seen = HashSet::new();
        let mut current = component;
        while let Attachment::TopOf {
            component: parent, ..
        } = &current.attachment
        {
            if !seen.insert(current.id.as_str()) {
                errors.push(ValidationError::AttachmentCycle(component.id.clone()));
                break;
            }
            let Some(next) = by_id.get(parent.as_str()) else {
                break;
            };
            current = next;
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_holder(design: &WeaponHolderDesign) -> Result<(), Vec<ValidationError>> {
    let mut errors = validate(&design.fitted_weapon).err().unwrap_or_default();
    if design.catalog_id.is_empty() {
        errors.push(ValidationError::EmptyCatalogId);
    }
    let expected = recommended_holder(&design.fitted_weapon.catalog_id);
    if expected != Some(design.kind)
        || !matches!(
            (design.kind, design.catalog_id.as_str()),
            (WeaponHolderKind::BladeSheath, "scabbard")
                | (WeaponHolderKind::HaftLoop, "weapon_loop")
        )
    {
        errors.push(ValidationError::InvalidDimensions("holder kind".into()));
    }
    if !(2..=20).contains(&design.clearance.0)
        || !(4..=40).contains(&design.throat_length.0)
        || !(6..=60).contains(&design.chape_length.0)
        || design.loop_position.0 > 1_000
        || !(2..=12).contains(&design.loop_bar_radius.0)
        || !(20..=120).contains(&design.hanger_width.0)
        || !(30..=180).contains(&design.hanger_height.0)
    {
        errors.push(ValidationError::InvalidDimensions(
            "holder parameters".into(),
        ));
    }
    let has_grip = design
        .fitted_weapon
        .components
        .iter()
        .any(|part| part.role == ComponentRole::Grip);
    let has_blade = design.fitted_weapon.components.iter().any(|part| {
        matches!(
            part.shape,
            ComponentShape::Blade(_) | ComponentShape::SectionBlade(_)
        )
    });
    if !has_grip || (design.kind == WeaponHolderKind::BladeSheath && !has_blade) {
        errors.push(ValidationError::InvalidDimensions(
            "holder source geometry".into(),
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
