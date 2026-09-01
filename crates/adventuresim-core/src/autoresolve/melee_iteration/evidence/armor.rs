use super::*;

pub(super) fn forced_armor_contacts(
    attacker: &Combatant,
    defender: &Combatant,
) -> Result<Vec<ForcedArmorContactEvidence>, String> {
    let definition =
        crate::item_catalog::definition("brigandine").ok_or("missing authored brigandine")?;
    let equipment = definition
        .equipment
        .as_ref()
        .ok_or("brigandine has no equipment metadata")?;
    let placement = &equipment.placements[0];
    Ok([("armor_surface", 0.4), ("gap", 0.95)]
        .into_iter()
        .map(|(coverage_contact, coordinate)| {
            let surface = defender
                .equipment
                .armor_surface(BodyPart::Chest, coordinate);
            let geometry = authored_armor_coverage(
                placement,
                BodyPart::Chest,
                defender.equipment.armor[body_part_index(BodyPart::Chest)].coverage,
            );
            ForcedArmorContactEvidence {
                armor: "brigandine",
                coverage_contact,
                body_part: BodyPart::Chest,
                anatomical_subregion: anatomical_subregion(BodyPart::Chest, coordinate),
                contact_surface_coordinate: coordinate,
                armor_layer_chain: vec![ArmorLayerEvidence {
                    item: "brigandine",
                    material: equipment.material,
                    geometry,
                    intersected: geometry.span.contains(coordinate),
                    selected: surface.is_some(),
                }],
                result: forced_melee_contact(attacker, defender, coordinate, surface),
            }
        })
        .collect())
}

pub(super) fn mirrored_vambrace_contacts() -> Result<Vec<MirroredArmorContactEvidence>, String> {
    let definition = crate::item_catalog::definition("vambrace").ok_or("missing vambrace")?;
    let equipment = definition
        .equipment
        .as_ref()
        .ok_or("vambrace has no equipment metadata")?;
    let cases = [
        (BodySide::Left, BodyPart::LeftArm, 0_usize, 0.9_f32),
        (BodySide::Right, BodyPart::RightArm, 1_usize, 0.9_f32),
        (BodySide::Left, BodyPart::LeftArm, 0_usize, 0.4_f32),
        (BodySide::Right, BodyPart::RightArm, 1_usize, 0.4_f32),
    ];
    Ok(cases
        .into_iter()
        .map(|(side, body_part, placement_index, coordinate)| {
            let geometry =
                authored_armor_coverage(&equipment.placements[placement_index], body_part, 0.65);
            let intersected = geometry.span.contains(coordinate);
            MirroredArmorContactEvidence {
                side,
                body_part,
                contact_surface_coordinate: coordinate,
                layer: ArmorLayerEvidence {
                    item: "vambrace",
                    material: equipment.material,
                    geometry,
                    intersected,
                    selected: intersected,
                },
            }
        })
        .collect())
}
