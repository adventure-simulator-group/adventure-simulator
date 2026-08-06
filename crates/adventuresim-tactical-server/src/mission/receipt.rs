use adventuresim_stdb_client::{
    TacticalCharacterConsequence, TacticalConsequenceReceipt, TacticalEquipmentContact,
    TacticalEquipmentContactRole, TacticalHitInjury, TacticalReceiptBodyPart,
};
use adventuresim_tactical_core::prelude::BodyPart;

use crate::combat::TacticalConsequenceAccumulator;

fn receipt_body_part(body_part: BodyPart) -> TacticalReceiptBodyPart {
    match body_part {
        BodyPart::LeftArm => TacticalReceiptBodyPart::LeftArm,
        BodyPart::RightArm => TacticalReceiptBodyPart::RightArm,
        BodyPart::LeftLeg => TacticalReceiptBodyPart::LeftLeg,
        BodyPart::RightLeg => TacticalReceiptBodyPart::RightLeg,
        BodyPart::Chest => TacticalReceiptBodyPart::Chest,
        BodyPart::Stomach => TacticalReceiptBodyPart::Stomach,
        BodyPart::Head => TacticalReceiptBodyPart::Head,
    }
}

pub(super) fn tactical_consequence_receipt(
    accumulated: &TacticalConsequenceAccumulator,
) -> TacticalConsequenceReceipt {
    let mut party: Vec<_> = accumulated
        .party
        .iter()
        .map(|(character_id, consequence)| TacticalCharacterConsequence {
            character_id: character_id.0,
            injuries: consequence
                .injuries
                .iter()
                .map(|injury| TacticalHitInjury {
                    body_part: receipt_body_part(injury.body_part),
                    cut_damage: injury.cut_damage,
                    blunt_damage: injury.blunt_damage,
                    max_single_hit_blunt_damage: injury.max_single_hit_blunt_damage,
                })
                .collect(),
            blood_loss_fraction: consequence.blood_loss_fraction,
            ammunition_used: consequence.ammunition_used,
        })
        .collect();
    for contact in &accumulated.equipment_contacts {
        if !party
            .iter()
            .any(|consequence| consequence.character_id == contact.character_id.0)
        {
            party.push(TacticalCharacterConsequence {
                character_id: contact.character_id.0,
                injuries: Vec::new(),
                blood_loss_fraction: 0.0,
                ammunition_used: 0,
            });
        }
    }
    party.sort_by_key(|consequence| consequence.character_id);
    party.truncate(adventuresim_core::mission::MAX_TACTICAL_RECEIPT_PARTICIPANTS);
    TacticalConsequenceReceipt {
        party,
        equipment_contacts: accumulated
            .equipment_contacts
            .iter()
            .map(|contact| TacticalEquipmentContact {
                character_id: contact.character_id.0,
                inventory_item_id: contact.inventory_item_id,
                contact_stress: contact.contact_stress,
                role: if contact.defender_equipment {
                    TacticalEquipmentContactRole::DefenderEquipment
                } else {
                    TacticalEquipmentContactRole::AttackerWeapon
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use adventuresim_tactical_core::prelude::CharacterId;

    use super::*;
    use crate::combat::AccumulatedEquipmentContact;

    #[test]
    fn weapon_contact_owner_is_included_without_an_injury() {
        let mut accumulated = TacticalConsequenceAccumulator::default();
        accumulated
            .equipment_contacts
            .push(AccumulatedEquipmentContact {
                character_id: CharacterId(7),
                inventory_item_id: 99,
                contact_stress: 12.0,
                defender_equipment: false,
            });

        let receipt = tactical_consequence_receipt(&accumulated);
        assert_eq!(receipt.party.len(), 1);
        assert_eq!(receipt.party[0].character_id, 7);
        assert!(receipt.party[0].injuries.is_empty());
        assert_eq!(receipt.equipment_contacts.len(), 1);
    }
}
