#![cfg(feature = "spacetimedb")]

use std::fmt::Debug;

use adventuresim_core::{
    attribute::PlayerAttributeValues,
    capability::RoleRequirements,
    equipment::WeaponSkillDistribution,
    investigation_action::{InvestigationActionAvailability, InvestigationActionUnavailableReason},
    item_catalog_schema::{
        EquipmentChannel, EquipmentLocation, OccupancyRequirement, ParentRequirement,
    },
    morale::{MoraleEventKind, MoraleSourceKind},
    personality::{
        Conscience, Conviction, Courtship, Drive, Hygiene, Inclination, Mirth, Nerve, Outlook,
        Presentation, SelfKnowledge, SelfRegard, Sex, Sociability, Temperance, Transparency,
    },
    strategic_place::CaseSiteId,
};
use spacetimedb_sats::serde::SerdeWrapper;

fn sats_json_roundtrip<T>(value: T) -> String
where
    T: spacetimedb_sats::Serialize
        + for<'de> spacetimedb_sats::Deserialize<'de>
        + Debug
        + PartialEq,
{
    let json = serde_json::to_string(SerdeWrapper::from_ref(&value)).unwrap();
    let SerdeWrapper(decoded) =
        serde_json::from_str::<SerdeWrapper<T>>(&json).expect("SATS JSON should decode");
    assert_eq!(decoded, value);
    json
}

fn roundtrip_all<T>(values: impl IntoIterator<Item = T>)
where
    T: spacetimedb_sats::Serialize
        + for<'de> spacetimedb_sats::Deserialize<'de>
        + Debug
        + PartialEq,
{
    for value in values {
        sats_json_roundtrip(value);
    }
}

#[test]
fn shared_struct_boundaries_roundtrip_through_sats_serde() {
    sats_json_roundtrip(PlayerAttributeValues {
        endurance: 1.0,
        immunity: 2.0,
        gut: 3.0,
        intelligence: 4.0,
        instinct: 5.0,
        eyesight: 6.0,
        hearing: 7.0,
        left_arm_strength: 8.0,
        right_arm_strength: 9.0,
        left_leg_strength: 10.0,
        right_leg_strength: 11.0,
        left_arm_agility: 12.0,
        right_arm_agility: 13.0,
        left_leg_agility: 14.0,
        right_leg_agility: 15.0,
    });
    let weapon_json = sats_json_roundtrip(WeaponSkillDistribution {
        throw: 9.0,
        ..WeaponSkillDistribution::default()
    });
    assert!(weapon_json.contains("\"throw\":9.0"));
    sats_json_roundtrip(RoleRequirements {
        melee: true,
        weapon_precision: 1.5,
        athletics: 3,
        ..RoleRequirements::default()
    });
    sats_json_roundtrip(OccupancyRequirement {
        location: EquipmentLocation::Head,
        channel: EquipmentChannel::RigidArmor,
        order: 7,
    });
    sats_json_roundtrip(ParentRequirement {
        channel: EquipmentChannel::Containment,
        order: 11,
    });
    let site_json = sats_json_roundtrip(CaseSiteId::try_new("site:test").unwrap());
    assert_eq!(site_json, r#"{"value":"site:test"}"#);
}

#[test]
fn every_personality_variant_roundtrips_through_sats_serde() {
    roundtrip_all([Nerve::Neutral, Nerve::Brave, Nerve::Fearful]);
    roundtrip_all([Drive::Neutral, Drive::Ambitious, Drive::Content]);
    roundtrip_all([Outlook::Neutral, Outlook::Sanguine, Outlook::Brooding]);
    roundtrip_all([
        Sociability::Neutral,
        Sociability::Gregarious,
        Sociability::Solitary,
    ]);
    roundtrip_all([
        Conscience::Neutral,
        Conscience::Compassionate,
        Conscience::Callous,
        Conscience::Cruel,
    ]);
    roundtrip_all([SelfRegard::Neutral, SelfRegard::Proud, SelfRegard::Humble]);
    roundtrip_all([
        Conviction::Neutral,
        Conviction::Zealous,
        Conviction::Irreverent,
    ]);
    roundtrip_all([Hygiene::Neutral, Hygiene::Slovenly, Hygiene::Cleanly]);
    roundtrip_all([
        Temperance::Neutral,
        Temperance::Temperate,
        Temperance::Drunkard,
    ]);
    roundtrip_all([Mirth::Neutral, Mirth::Merry, Mirth::Grave]);
    roundtrip_all([Courtship::Neutral, Courtship::Amorous, Courtship::Proper]);
    roundtrip_all([
        Transparency::Neutral,
        Transparency::Open,
        Transparency::Guarded,
    ]);
    roundtrip_all([
        SelfKnowledge::Neutral,
        SelfKnowledge::Introspective,
        SelfKnowledge::SelfDeceiving,
    ]);
    roundtrip_all([
        Inclination::Men,
        Inclination::Either,
        Inclination::Women,
        Inclination::Neither,
    ]);
    roundtrip_all([
        Presentation::Man,
        Presentation::Ambiguous,
        Presentation::Woman,
    ]);
    roundtrip_all([Sex::Female, Sex::Male]);
}

#[test]
fn typed_availability_and_morale_vocabularies_roundtrip_through_sats_serde() {
    sats_json_roundtrip(InvestigationActionAvailability::Available);
    let unavailable_json = sats_json_roundtrip(InvestigationActionAvailability::unavailable(
        InvestigationActionUnavailableReason::TravelRequired,
        true,
        37,
    ));
    assert_eq!(
        unavailable_json,
        r#"{"Unavailable":{"reason":{"TravelRequired":[]},"can_travel_to_required_site":true,"wait_minutes":37}}"#
    );
    for reason in [
        InvestigationActionUnavailableReason::PartyNotReady,
        InvestigationActionUnavailableReason::TravelRequired,
        InvestigationActionUnavailableReason::NightWindow,
        InvestigationActionUnavailableReason::TargetChanged,
        InvestigationActionUnavailableReason::ContactScheduleWindow,
        InvestigationActionUnavailableReason::ContactNotPresent,
        InvestigationActionUnavailableReason::CharacterUnavailable,
        InvestigationActionUnavailableReason::PartyRequired,
    ] {
        sats_json_roundtrip(InvestigationActionAvailability::unavailable(
            reason, true, 37,
        ));
    }
    roundtrip_all([
        MoraleEventKind::CorpseHandling,
        MoraleEventKind::SocialInteraction,
        MoraleEventKind::WitnessCharm,
        MoraleEventKind::WitnessCommand,
        MoraleEventKind::WitnessBluff,
        MoraleEventKind::Victory,
        MoraleEventKind::Defeat,
        MoraleEventKind::Injury,
        MoraleEventKind::MasteryEnjoyment,
        MoraleEventKind::ReligiousObservanceNeglected,
        MoraleEventKind::HolyDayObserved,
        MoraleEventKind::Prayer,
        MoraleEventKind::Meditation,
        MoraleEventKind::TravelPrayerNeglected,
        MoraleEventKind::SpouseLeisure,
        MoraleEventKind::Carousing,
        MoraleEventKind::AlcoholSatisfied,
        MoraleEventKind::AlcoholUnsatisfied,
        MoraleEventKind::ResidenceLeisure,
        MoraleEventKind::Leisure,
    ]);
    roundtrip_all([
        MoraleSourceKind::Injury,
        MoraleSourceKind::Cleanliness,
        MoraleSourceKind::Religion,
        MoraleSourceKind::ReligiousDiscord,
        MoraleSourceKind::Prayer,
        MoraleSourceKind::Meditation,
        MoraleSourceKind::Power,
        MoraleSourceKind::Ally,
        MoraleSourceKind::CorpseHandling,
        MoraleSourceKind::SocialInteraction,
        MoraleSourceKind::WitnessCharm,
        MoraleSourceKind::WitnessCommand,
        MoraleSourceKind::WitnessBluff,
        MoraleSourceKind::Victory,
        MoraleSourceKind::Defeat,
        MoraleSourceKind::MasteryEnjoyment,
        MoraleSourceKind::ReligiousObservanceNeglected,
        MoraleSourceKind::HolyDayObserved,
        MoraleSourceKind::TravelPrayerNeglected,
        MoraleSourceKind::SpouseLeisure,
        MoraleSourceKind::Carousing,
        MoraleSourceKind::AlcoholSatisfied,
        MoraleSourceKind::AlcoholUnsatisfied,
        MoraleSourceKind::ResidenceLeisure,
        MoraleSourceKind::Leisure,
    ]);
}
