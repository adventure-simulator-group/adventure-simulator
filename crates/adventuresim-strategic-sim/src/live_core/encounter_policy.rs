//! Strategic and narrative encounter choice policy.

use super::*;

pub(super) const RANGED_AMMUNITION_ITEM_ID: &str = "arrow";
/// One ordinary autoresolve can consume several arrows. Twenty leaves a
/// conservative reserve for an encounter plus the disclosed quest fight.
pub(super) const RANGED_AMMUNITION_FLOOR: u32 = 20;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EncounterPolicyChoice {
    pub(super) choice: String,
    pub(super) reason: &'static str,
}

pub(super) fn select_expedition_encounter_choice(
    available_choices: &[String],
    evacuation: bool,
) -> Option<EncounterPolicyChoice> {
    let has = |candidate: &str| available_choices.iter().any(|choice| choice == candidate);
    if has("detour") {
        return Some(EncounterPolicyChoice {
            choice: "detour".into(),
            reason: "guaranteed_party_aware_detour",
        });
    }
    if has("run") {
        return Some(EncounterPolicyChoice {
            choice: "run".into(),
            reason: "public_speed_check_allows_escape",
        });
    }
    if has("surrender") {
        return Some(EncounterPolicyChoice {
            choice: "surrender".into(),
            reason: "bandit_surrender_is_only_protective_choice",
        });
    }
    (!evacuation && has("attack")).then(|| EncounterPolicyChoice {
        choice: "attack".into(),
        reason: "no_protective_response_available",
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NarrativeEncounterPolicyChoice {
    pub(super) choice: String,
    pub(super) reason: &'static str,
    pub(super) visible_alternatives: Vec<String>,
    pub(super) eligible_meaningful_alternatives: Vec<String>,
}

pub(super) fn narrative_axis_fit(
    profile: &AgentProfile,
    development: &adventuresim_core::road_encounter_catalog::PersonalityDevelopment,
) -> i32 {
    use adventuresim_core::road_encounter_catalog::PersonalityAxisId;
    let preferred_sign = match development.axis {
        PersonalityAxisId::Nerve => match profile.personality.nerve {
            Nerve::Brave => 1,
            Nerve::Fearful => -1,
            Nerve::Neutral => 0,
        },
        PersonalityAxisId::Drive => match profile.personality.drive {
            Drive::Ambitious => 1,
            Drive::Content => -1,
            Drive::Neutral => 0,
        },
        PersonalityAxisId::Sociability => match profile.personality.sociability {
            Sociability::Gregarious => 1,
            Sociability::Solitary => -1,
            Sociability::Neutral => 0,
        },
        PersonalityAxisId::Conscience => match profile.personality.conscience {
            Conscience::Compassionate => 1,
            Conscience::Callous | Conscience::Cruel => -1,
            Conscience::Neutral => 0,
        },
        PersonalityAxisId::SelfRegard => match profile.personality.self_regard {
            SelfRegard::Proud => 1,
            SelfRegard::Humble => -1,
            SelfRegard::Neutral => 0,
        },
        PersonalityAxisId::Conviction => match profile.personality.conviction {
            Conviction::Zealous => 1,
            Conviction::Irreverent => -1,
            Conviction::Neutral => 0,
        },
        PersonalityAxisId::Transparency => match profile.personality.transparency {
            Transparency::Open => 1,
            Transparency::Guarded => -1,
            Transparency::Neutral => 0,
        },
        PersonalityAxisId::Courtship => 0,
    };
    preferred_sign * i32::from(development.delta.signum())
}

pub(super) fn select_public_narrative_encounter_choice(
    presentation_json: &str,
    profile: &AgentProfile,
) -> Result<Option<NarrativeEncounterPolicyChoice>, serde_json::Error> {
    let presentation: adventuresim_core::road_encounter_catalog::EncounterPresentation =
        serde_json::from_str(presentation_json)?;
    let mut visible_alternatives = presentation
        .choices
        .iter()
        .filter(|choice| choice.available)
        .map(|choice| choice.id.clone())
        .collect::<Vec<_>>();
    visible_alternatives.sort();
    let ignore = visible_alternatives.iter().any(|choice| choice == "ignore");
    let mut meaningful = presentation
        .choices
        .iter()
        .filter(|choice| choice.available && choice.id != "ignore")
        .filter_map(|presented| {
            let mut authored = adventuresim_core::road_encounter_catalog::definitions()
                .iter()
                .flat_map(|definition| definition.choices.iter())
                .filter(|choice| choice.id == presented.id);
            let choice = authored.next()?;
            if authored.next().is_some()
                || !choice.checks.is_empty()
                || matches!(
                    choice.transition.as_ref(),
                    Some(adventuresim_core::road_encounter_catalog::EncounterTransition::StartCombat { .. })
                )
            {
                return None;
            }
            let personality_fit = choice
                .personality
                .iter()
                .map(|development| narrative_axis_fit(profile, development))
                .sum::<i32>();
            // Public availability and authored requirements prove only that
            // a choice is legal. They do not establish that spending its
            // resources is preferable to continuing safely.
            (personality_fit > 0).then_some((presented.id.clone(), personality_fit))
        })
        .collect::<Vec<_>>();
    meaningful.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let eligible_meaningful_alternatives = meaningful
        .iter()
        .map(|(choice, _)| choice.clone())
        .collect::<Vec<_>>();
    if let Some((choice, _)) = meaningful.into_iter().next() {
        return Ok(Some(NarrativeEncounterPolicyChoice {
            choice,
            reason: "personality_aligned_check_free_noncombat",
            visible_alternatives,
            eligible_meaningful_alternatives,
        }));
    }
    Ok(ignore.then_some(NarrativeEncounterPolicyChoice {
        choice: "ignore".into(),
        reason: "unconditional_check_free_noncombat_fallback",
        visible_alternatives,
        eligible_meaningful_alternatives,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PublicCombatFingerprint {
    pub(super) members: Vec<PublicCombatMemberFingerprint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PublicCombatMemberFingerprint {
    pub(super) character_id: u64,
    pub(super) melee: bool,
    pub(super) ranged: bool,
    pub(super) armored: bool,
    pub(super) precise: bool,
    pub(super) endurance_centipoints: u32,
    pub(super) athletics_centipoints: u32,
    pub(super) weapon_precision_centipoints: u32,
    pub(super) autoresolve_combat_power: u64,
}

#[derive(Clone, Debug)]
pub(super) struct PublicPartyCombatant {
    pub(super) capability: CharacterCapability,
    pub(super) ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicContractAssessment {
    pub(super) eligible: bool,
    pub(super) reason: &'static str,
    pub(super) enemy_count: Option<u32>,
    pub(super) ready_combatants: u32,
    pub(super) party_power_milli: u64,
    pub(super) enemy_power_milli: u64,
}

pub(super) fn public_contract_assessment(
    difficulty: i32,
    opposition_count: u32,
    opposition_combat_power: u64,
    members: &[PublicPartyCombatant],
) -> PublicContractAssessment {
    let Some(enemy_count) = (opposition_count > 0).then_some(opposition_count) else {
        return PublicContractAssessment {
            eligible: false,
            reason: "unknown_public_opposition_count",
            enemy_count: None,
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    };
    if difficulty <= 0 {
        return PublicContractAssessment {
            eligible: false,
            reason: "invalid_public_difficulty",
            enemy_count: Some(enemy_count),
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    }
    if opposition_combat_power == 0 {
        return PublicContractAssessment {
            eligible: false,
            reason: "missing_authoritative_opposition_power",
            enemy_count: Some(enemy_count),
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    }
    let ready = members
        .iter()
        .filter(|member| member.ready && (member.capability.melee || member.capability.ranged))
        .collect::<Vec<_>>();
    let Some(party_power_milli) = ready.iter().try_fold(0u64, |total, member| {
        total.checked_add(member.capability.autoresolve_combat_power)
    }) else {
        return PublicContractAssessment {
            eligible: false,
            reason: "public_party_power_overflow",
            enemy_count: Some(enemy_count),
            ready_combatants: ready.len().min(u32::MAX as usize) as u32,
            party_power_milli: 0,
            enemy_power_milli: opposition_combat_power,
        };
    };
    let enemy_power_milli = opposition_combat_power;
    let margin = adventuresim_core::autoresolve::combat_power_meets_safety_margin(
        party_power_milli,
        enemy_power_milli,
    );
    let eligible = !ready.is_empty() && margin == Some(true);
    PublicContractAssessment {
        eligible,
        reason: if ready.is_empty() {
            "no_ready_public_combatants"
        } else if party_power_milli == 0 {
            "missing_authoritative_party_power"
        } else if margin.is_none() {
            "public_combat_margin_overflow"
        } else if eligible {
            "public_matchup_with_safety_margin"
        } else {
            "public_matchup_below_safety_margin"
        },
        enemy_count: Some(enemy_count),
        ready_combatants: ready.len() as u32,
        party_power_milli,
        enemy_power_milli,
    }
}

pub(super) fn public_combat_fingerprint(
    mut capabilities: Vec<CharacterCapability>,
) -> PublicCombatFingerprint {
    const SKILL_CENTIPOINTS_PER_POINT: f32 = 100.0;

    capabilities.sort_by_key(|row| row.character_id);
    PublicCombatFingerprint {
        members: capabilities
            .into_iter()
            .map(|row| PublicCombatMemberFingerprint {
                character_id: row.character_id,
                melee: row.melee,
                ranged: row.ranged,
                armored: row.heavy || row.half_armor,
                precise: row.precise,
                endurance_centipoints: (row.endurance.max(0.0) * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                athletics_centipoints: (row.athletics.max(0.0) * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                weapon_precision_centipoints: (row.weapon_precision.max(0.0)
                    * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                autoresolve_combat_power: row.autoresolve_combat_power,
            })
            .collect(),
    }
}

pub(super) fn generated_method_skill_fit(profile: &AgentProfile, method: &str) -> u32 {
    let skills = &profile.initial_skills;
    let hours = match method {
        "inspect_site" | "search_area" | "locate_contact" | "watch" | "patrol"
        | "approach_lead" => skills.insight,
        // The public action projection does not expose target terrain.
        "follow_tracks" | "reacquire_tracks" => 0.0,
        "lay_ambush" => (skills.insight + skills.stealth) / 2.0,
        _ => 0.0,
    };
    hours.clamp(0.0, 100_000.0).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedDefeatDecision {
    Proceed,
    SuppressUnchanged,
}

pub(super) fn generated_defeat_decision(
    combat_available: bool,
    previous: Option<&PublicCombatFingerprint>,
    current: &PublicCombatFingerprint,
) -> GeneratedDefeatDecision {
    if combat_available && previous.is_some_and(|previous| previous == current) {
        GeneratedDefeatDecision::SuppressUnchanged
    } else {
        GeneratedDefeatDecision::Proceed
    }
}
