use serde::{Deserialize, Serialize};

use crate::body::BodyPart;
use crate::body::BodySide;
use crate::equipment::ArmorSurface;
use crate::item_catalog_schema::{EquipmentAnatomicalRegion, EquipmentPlacement, SurfaceAnchor};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArmorCoverageSpan {
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuthoredArmorCoverage {
    pub span: ArmorCoverageSpan,
    pub region: Option<EquipmentAnatomicalRegion>,
    pub anchor: SurfaceAnchor,
    pub laterality: Option<BodySide>,
    pub surface_index: u8,
}

impl ArmorCoverageSpan {
    #[must_use]
    pub fn centered(coverage: f32) -> Self {
        let coverage = coverage.clamp(0.0, 1.0);
        Self {
            start: (1.0 - coverage) * 0.5,
            end: (1.0 + coverage) * 0.5,
        }
    }

    #[must_use]
    pub fn contains(self, coordinate: f32) -> bool {
        let coordinate = coordinate.clamp(0.0, 1.0 - f32::EPSILON);
        coordinate >= self.start && coordinate < self.end
    }
}

#[must_use]
pub fn authored_armor_coverage_span(
    placement: &EquipmentPlacement,
    part: BodyPart,
    fallback_coverage: f32,
) -> ArmorCoverageSpan {
    authored_armor_coverage(placement, part, fallback_coverage).span
}

#[must_use]
pub fn authored_armor_coverage(
    placement: &EquipmentPlacement,
    part: BodyPart,
    fallback_coverage: f32,
) -> AuthoredArmorCoverage {
    for (surface_index, surface) in placement.surface.iter().enumerate() {
        let count = surface.regions.len().max(1) as f32;
        let coverage = surface.coverage.clamp(0.0, 1.0);
        let retained = match surface.anchor {
            SurfaceAnchor::Proximal => (0.0, coverage),
            SurfaceAnchor::Distal => (1.0 - coverage, 1.0),
            SurfaceAnchor::Center => ((1.0 - coverage) * 0.5, (1.0 + coverage) * 0.5),
        };
        for (index, region) in surface.regions.iter().copied().enumerate() {
            let Some((region_part, body_start, body_end)) = region_body_interval(region) else {
                continue;
            };
            if region_part != part {
                continue;
            }
            let chain_start = index as f32 / count;
            let chain_end = (index as f32 + 1.0) / count;
            let intersection_start = retained.0.max(chain_start);
            let intersection_end = retained.1.min(chain_end);
            if intersection_start >= intersection_end {
                continue;
            }
            let local_start = (intersection_start - chain_start) * count;
            let local_end = (intersection_end - chain_start) * count;
            return AuthoredArmorCoverage {
                span: ArmorCoverageSpan {
                    start: body_start + (body_end - body_start) * local_start,
                    end: body_start + (body_end - body_start) * local_end,
                },
                region: Some(region),
                anchor: surface.anchor,
                laterality: match part {
                    BodyPart::LeftArm | BodyPart::LeftLeg => Some(BodySide::Left),
                    BodyPart::RightArm | BodyPart::RightLeg => Some(BodySide::Right),
                    BodyPart::Chest | BodyPart::Stomach | BodyPart::Head => None,
                },
                surface_index: surface_index.try_into().unwrap_or(u8::MAX),
            };
        }
    }
    AuthoredArmorCoverage {
        span: ArmorCoverageSpan::centered(fallback_coverage),
        region: None,
        anchor: SurfaceAnchor::Center,
        laterality: match part {
            BodyPart::LeftArm | BodyPart::LeftLeg => Some(BodySide::Left),
            BodyPart::RightArm | BodyPart::RightLeg => Some(BodySide::Right),
            BodyPart::Chest | BodyPart::Stomach | BodyPart::Head => None,
        },
        surface_index: u8::MAX,
    }
}

fn region_body_interval(region: EquipmentAnatomicalRegion) -> Option<(BodyPart, f32, f32)> {
    use EquipmentAnatomicalRegion as Region;
    Some(match region {
        // Torso plates and fitted vests occupy the central front/back shell.
        // The final 15% of the chest parameterization names axillary and
        // neckline openings, which are not made into plate merely because a
        // catalog span names the broader `chest` region.
        Region::Chest => (BodyPart::Chest, 0.0, 0.85),
        Region::Stomach => (BodyPart::Stomach, 0.0, 1.0),
        Region::LeftUpperArm => (BodyPart::LeftArm, 0.0, 0.5),
        Region::LeftForearm => (BodyPart::LeftArm, 0.5, 1.0),
        Region::RightUpperArm => (BodyPart::RightArm, 0.0, 0.5),
        Region::RightForearm => (BodyPart::RightArm, 0.5, 1.0),
        Region::LeftThigh => (BodyPart::LeftLeg, 0.0, 0.5),
        Region::LeftLowerLeg => (BodyPart::LeftLeg, 0.5, 1.0),
        Region::RightThigh => (BodyPart::RightLeg, 0.0, 0.5),
        Region::RightLowerLeg => (BodyPart::RightLeg, 0.5, 1.0),
        Region::Neck => (BodyPart::Head, 0.0, 0.25),
        Region::Head => (BodyPart::Head, 0.25, 1.0),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmorImpactOutcome {
    Stopped,
    Deflected,
    Penetrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArmorImpact {
    pub surface: ArmorSurface,
    pub outcome: ArmorImpactOutcome,
    pub resisted_energy_joules: f32,
    pub transmitted_energy_joules: f32,
    pub penetrated_energy_joules: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ContactEnergyResolution {
    pub armor_impact: Option<ArmorImpact>,
    pub cut_energy_joules: f32,
    pub blunt_energy_joules: f32,
}

pub(super) fn effective_edge_resistance(surface: ArmorSurface, penetration: f32) -> f32 {
    // Garment flexibility describes articulation of the whole item. It must
    // not turn an engaged rigid steel plate into compliant leather locally.
    let flexibility = if surface.material.is_some_and(|material| material.is_metal()) {
        0.0
    } else {
        surface.flexibility
    };
    (surface.resistance - flexibility * surface.resistance * penetration).max(0.0)
}

pub(super) fn resolve_contact_energy(
    surface: Option<ArmorSurface>,
    attack: f32,
    incident_energy_joules: f32,
    has_edge: bool,
    has_blunt: bool,
    penetration: f32,
) -> ContactEnergyResolution {
    let incident = incident_energy_joules.max(0.0);
    let active_channels = u8::from(has_edge) + u8::from(has_blunt);
    if active_channels == 0 || incident <= f32::EPSILON {
        return ContactEnergyResolution {
            armor_impact: None,
            cut_energy_joules: 0.0,
            blunt_energy_joules: 0.0,
        };
    }
    let edge_energy = if has_edge {
        incident / f32::from(active_channels)
    } else {
        0.0
    };
    let blunt_energy = if has_blunt {
        incident / f32::from(active_channels)
    } else {
        0.0
    };
    let Some(surface) = surface else {
        return ContactEnergyResolution {
            armor_impact: None,
            cut_energy_joules: edge_energy,
            blunt_energy_joules: blunt_energy,
        };
    };

    let penetrated = (edge_energy - effective_edge_resistance(surface, penetration)).max(0.0);
    let stopped_edge = edge_energy - penetrated;
    // Some stopped edge energy deforms the armor into the body; the remainder
    // is reflected or retained by the armor. Blunt-channel energy starts as a
    // transmission candidate. Padding consumes that shared budget once.
    let edge_transmission_candidate = stopped_edge * 0.5;
    let transmission_candidate = edge_transmission_candidate + blunt_energy;
    let padding_resisted = transmission_candidate.min(surface.padding.max(0.0));
    let transmitted = transmission_candidate - padding_resisted;
    let resisted = stopped_edge - edge_transmission_candidate + padding_resisted;
    debug_assert!((incident - resisted - transmitted - penetrated).abs() < 0.001);
    let impact = ArmorImpact {
        surface,
        outcome: if penetrated > f32::EPSILON {
            ArmorImpactOutcome::Penetrated
        } else if attack < 1.0 {
            ArmorImpactOutcome::Deflected
        } else {
            ArmorImpactOutcome::Stopped
        },
        resisted_energy_joules: resisted,
        transmitted_energy_joules: transmitted,
        penetrated_energy_joules: penetrated,
    };
    ContactEnergyResolution {
        armor_impact: Some(impact),
        cut_energy_joules: penetrated,
        blunt_energy_joules: transmitted,
    }
}

/// Selects the first physical armor layer intersected by one unchanged
/// body-surface coordinate.
#[must_use]
pub fn layered_armor_contact_index(
    sample: f32,
    spans: impl IntoIterator<Item = ArmorCoverageSpan>,
) -> Option<usize> {
    for (index, span) in spans.into_iter().enumerate() {
        if span.contains(sample) {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_catalog_schema::EquipmentMaterial;
    use crate::{body::BodyPart, combat::AnatomicalSubregion};

    #[test]
    fn rigid_steel_surface_does_not_inherit_garment_flexibility() {
        let surface = ArmorSurface {
            inventory_item_id: Some(7),
            material: Some(EquipmentMaterial::RoughSteel),
            resistance: 100.0,
            padding: 5.0,
            flexibility: 0.8,
        };
        assert_eq!(effective_edge_resistance(surface, 1.0), 100.0);
    }

    #[test]
    fn armor_partition_conserves_incident_energy_across_channels() {
        let surface = ArmorSurface {
            inventory_item_id: Some(7),
            material: Some(EquipmentMaterial::RoughSteel),
            resistance: 52.0,
            padding: 18.0,
            flexibility: 0.4,
        };
        for incident in [0.01, 1.0, 20.0, 76.5, 200.0] {
            for channels in [(true, false), (false, true), (true, true)] {
                let resolved = resolve_contact_energy(
                    Some(surface),
                    1.0,
                    incident,
                    channels.0,
                    channels.1,
                    1.0,
                );
                let impact = resolved.armor_impact.unwrap();
                let partition = impact.resisted_energy_joules
                    + impact.transmitted_energy_joules
                    + impact.penetrated_energy_joules;
                assert!((partition - incident).abs() < 0.001);
                assert!(
                    resolved.cut_energy_joules + resolved.blunt_energy_joules <= incident + 0.001
                );
            }
        }
    }

    #[test]
    fn inner_layer_can_cover_a_named_neckline_opening_missed_by_plate() {
        let neckline_sample = 0.93;
        assert_eq!(
            crate::combat::anatomical_subregion(BodyPart::Chest, neckline_sample),
            AnatomicalSubregion::ChestNeckline
        );
        assert_eq!(
            layered_armor_contact_index(
                neckline_sample,
                [
                    ArmorCoverageSpan {
                        start: 0.075,
                        end: 0.925
                    },
                    ArmorCoverageSpan {
                        start: 0.60,
                        end: 1.0
                    },
                ],
            ),
            Some(1)
        );
        assert_eq!(
            layered_armor_contact_index(
                0.05,
                [
                    ArmorCoverageSpan {
                        start: 0.075,
                        end: 0.925
                    },
                    ArmorCoverageSpan {
                        start: 0.60,
                        end: 1.0
                    },
                ],
            ),
            None
        );
    }

    #[test]
    fn authored_centered_torso_layers_share_one_body_surface_point() {
        let breastplate = crate::item_catalog::definition("breastplate").unwrap();
        let doublet = crate::item_catalog::definition("arming_doublet").unwrap();
        let breastplate_placement = &breastplate.equipment.as_ref().unwrap().placements[0];
        let doublet_placement = &doublet.equipment.as_ref().unwrap().placements[0];
        let outer = authored_armor_coverage_span(breastplate_placement, BodyPart::Chest, 0.85);
        let inner = authored_armor_coverage_span(doublet_placement, BodyPart::Chest, 0.60);
        assert!(outer.contains(0.10));
        assert!(!inner.contains(0.10));
        assert_eq!(layered_armor_contact_index(0.10, [outer, inner]), Some(0));
        assert_eq!(layered_armor_contact_index(0.97, [outer, inner]), None);
        assert!(
            !outer.contains(0.88),
            "axillary opening must remain outside plate geometry"
        );
    }

    #[test]
    fn authored_vambrace_geometry_is_laterally_symmetric_and_distal() {
        let vambrace = crate::item_catalog::definition("vambrace").unwrap();
        let placements = &vambrace.equipment.as_ref().unwrap().placements;
        let left = authored_armor_coverage_span(&placements[0], BodyPart::LeftArm, 0.65);
        let right = authored_armor_coverage_span(&placements[1], BodyPart::RightArm, 0.65);
        assert_eq!(left, right);
        assert!(!left.contains(0.5));
        assert!(left.contains(0.9));
    }
}
