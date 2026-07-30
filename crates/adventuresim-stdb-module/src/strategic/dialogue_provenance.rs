enum ReferralDeliveryAuthority {
    LocalProblem(crate::local_problem::LocalProblemReceipt),
    PublicThreat,
}

fn referral_delivery_authority(
    ctx: &ReducerContext,
    delivery: &crate::local_problem::LocalProblemRumorDelivery,
) -> Result<ReferralDeliveryAuthority, String> {
    let receipt = ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&delivery.receipt_id);
    let public_disclosure = ctx
        .db
        .public_threat_disclosure()
        .id()
        .find(&delivery.receipt_id);
    match adventuresim_core::threat_escalation::referral_delivery_authority_kind(
        receipt.is_some(),
        public_disclosure.is_some(),
    ) {
        adventuresim_core::threat_escalation::ReferralDeliveryAuthorityKind::LocalProblem => {
            Ok(ReferralDeliveryAuthority::LocalProblem(
                receipt.expect("classified local-problem receipt"),
            ))
        }
        adventuresim_core::threat_escalation::ReferralDeliveryAuthorityKind::PublicThreat => {
            Ok(ReferralDeliveryAuthority::PublicThreat)
        }
        adventuresim_core::threat_escalation::ReferralDeliveryAuthorityKind::Missing => {
            Err("Rumor delivery authority disappeared".into())
        }
        adventuresim_core::threat_escalation::ReferralDeliveryAuthorityKind::Conflict => {
            Err("Rumor delivery authority conflicts".into())
        }
    }
}

/// Select presentation-only quest referral prose after the same authoritative
/// session facts used by ordinary dialogue have been revalidated. The receipt,
/// contact, and generated case remain the authority; variants cannot affect
/// any of them.
fn referral_variant_can_replace_authoritative_wording(
    source: Option<(&str, &str)>,
    contact_id: &str,
    contact_name: &str,
) -> bool {
    source.is_some_and(|(source_id, source_name)| {
        source_id != contact_id && !source_name.eq_ignore_ascii_case(contact_name)
    })
}

fn render_quest_referral_variant(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    current_speaker_npc_id: &str,
    delivery: &crate::local_problem::LocalProblemRumorDelivery,
) -> Result<(String, String), String> {
    let facts = dialogue_fact_context(ctx, session, character_id)?;
    let ReferralDeliveryAuthority::LocalProblem(receipt) =
        referral_delivery_authority(ctx, delivery)?
    else {
        return Err("Quest referral is not a local-problem delivery".into());
    };
    let contact = ctx
        .db
        .settlement_npc()
        .id()
        .find(&receipt.contact_npc_id)
        .ok_or("Rumor referral contact is unavailable")?;
    let source = ctx
        .db
        .settlement_npc()
        .id()
        .find(&current_speaker_npc_id.to_owned());
    if !referral_variant_can_replace_authoritative_wording(
        source
            .as_ref()
            .map(|source| (source.id.as_str(), source.name.as_str())),
        &contact.id,
        &contact.name,
    ) {
        // The authoritative presentation distinguishes self-referrals and
        // same-named people, and may expose the referred-testimony topic.
        // Generic flavor variants cannot safely preserve those semantics.
        return Ok((delivery.fragments_json.clone(), "[]".into()));
    }
    let catalog = adventuresim_core::quest_catalog::catalog();
    let Some(variant) = catalog.dialogue_variant(
        adventuresim_core::quest_catalog::QuestDialogueVariantKind::Referral,
        &facts,
    )?
    else {
        return Ok((delivery.fragments_json.clone(), "[]".into()));
    };
    let mut values = std::collections::BTreeMap::new();
    values.insert("summary".into(), receipt.safe_summary);
    values.insert("contact_name".into(), contact.name);
    values.insert("contact_profession".into(), contact.profession);
    values.insert("contact_location".into(), receipt.expected_location_id);
    let text = variant.render(&values)?;
    let fragments = vec![adventuresim_dialogue::Fragment::Text { value: text }];
    let json = serde_json::to_string(&fragments)
        .map_err(|_| "Could not encode generated referral dialogue")?;
    let source = catalog
        .dialogue_variant_source(variant)
        .ok_or("Quest dialogue variant has no compiler source mapping")?;
    let sources = serde_json::to_string(&vec![source])
        .map_err(|_| "Could not encode generated referral source")?;
    Ok((json, sources))
}

#[cfg(test)]
mod referral_variant_tests {
    use super::referral_variant_can_replace_authoritative_wording;

    #[test]
    fn generic_variants_never_erase_referral_identity_disambiguation() {
        assert!(!referral_variant_can_replace_authoritative_wording(
            Some(("npc:church:0", "Marta Hartmann")),
            "npc:church:0",
            "Marta Hartmann",
        ));
        assert!(!referral_variant_can_replace_authoritative_wording(
            Some(("npc:inn:0", "Marta Hartmann")),
            "npc:church:0",
            "Marta Hartmann",
        ));
        assert!(!referral_variant_can_replace_authoritative_wording(
            None,
            "npc:church:0",
            "Marta Hartmann",
        ));
        assert!(referral_variant_can_replace_authoritative_wording(
            Some(("npc:inn:0", "Anna Kramer")),
            "npc:church:0",
            "Marta Hartmann",
        ));
    }

    #[test]
    fn referral_renderer_uses_the_current_speaker_not_discovery_provenance() {
        let source = STRATEGIC_SOURCE;
        let start = source
            .split("pub fn start_dialogue")
            .nth(1)
            .and_then(|tail| tail.split("pub fn join_dialogue_session").next())
            .expect("dialogue startup");
        assert!(start.contains("&npc_actor_id,\n            &delivery"));

        let renderer = source
            .split("fn render_quest_referral_variant")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("referral renderer");
        assert!(renderer.contains("current_speaker_npc_id"));
        assert!(!renderer.contains("receipt.source_npc_id"));
    }
}

#[reducer]
pub fn join_dialogue_session(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    role: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    crate::character::require_living_character(ctx, character_id)?;
    validate_dialogue_action_id(&action_id)?;
    let action_row_id = format!("{session_id}:{action_id}");
    let mut session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&session_id)
        .ok_or("Dialogue session not found")?;
    if session.catalog_revision != catalog_revision || session.state != "active" {
        return Err("Dialogue session is stale or closed".into());
    }
    require_live_dialogue_presence(ctx, &session, character_id)?;
    if ctx.db.dialogue_action().id().find(&action_row_id).is_some() {
        return Ok(());
    }
    if session.revision != expected_revision {
        return Err("Dialogue join used a stale session revision".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(session.settlement_id.as_str()) {
        return Err("Dialogue participants must share a location".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let specification = conversation
        .roles
        .get(&role)
        .filter(|role| role.kind == adventuresim_dialogue::ParticipantKind::Player)
        .ok_or("Unknown player dialogue role")?;
    let count = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session_id)
        .filter(|participant| participant.role == role)
        .count();
    let id = format!("{session_id}:character:{character_id}");
    if ctx.db.dialogue_participant().id().find(&id).is_some() {
        return Ok(());
    }
    if count >= usize::from(specification.max) {
        return Err("Dialogue role is full".into());
    }
    ctx.db.dialogue_participant().insert(DialogueParticipant {
        id,
        gateway_bucket: 0,
        session_id: session_id.clone(),
        role,
        character_id: Some(character_id),
        actor_id: format!("character:{character_id}"),
        display_name: character.name,
    });
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id,
        action_id,
        action_kind: "join".into(),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

pub(crate) fn require_session_member(
    ctx: &ReducerContext,
    session_id: &str,
    character_id: u64,
) -> Result<DialogueSession, String> {
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(session_id.to_owned())
        .ok_or("Dialogue session not found")?;
    if session.state != "active" {
        return Err("Dialogue session is closed".into());
    }
    let member = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(session_id)
        .any(|p| p.character_id == Some(character_id));
    if !member {
        return Err("Character is not a dialogue participant".into());
    }
    require_live_dialogue_presence(ctx, &session, character_id)?;
    Ok(session)
}

fn validate_dialogue_action_id(action_id: &str) -> Result<(), String> {
    if action_id.is_empty()
        || action_id.len() > 100
        || action_id.chars().any(|c| c.is_control() || c == ':')
    {
        Err("Invalid dialogue action ID".into())
    } else {
        Ok(())
    }
}

fn validate_dialogue_cardinality(
    ctx: &ReducerContext,
    session: &DialogueSession,
    conversation: &adventuresim_dialogue::Conversation,
) -> Result<(), String> {
    let participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .collect();
    for (role_name, role) in &conversation.roles {
        let count = participants
            .iter()
            .filter(|participant| participant.role == *role_name)
            .count();
        if count < usize::from(role.min) || count > usize::from(role.max) {
            return Err(format!(
                "Dialogue role {role_name} does not meet its cardinality"
            ));
        }
    }
    Ok(())
}

fn dialogue_fact_context(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<adventuresim_dialogue::FactContext, String> {
    use adventuresim_dialogue::{FactKey, FactValue};
    let mut result = adventuresim_dialogue::FactContext::default();
    result.facts.insert(
        FactKey::Location,
        FactValue::Text(session.settlement_id.clone()),
    );
    let participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .collect();
    for participant in &participants {
        result.facts.insert(
            FactKey::ParticipantPresent {
                role: participant.role.clone(),
            },
            FactValue::Bool(true),
        );
        result.facts.insert(
            FactKey::ParticipantCount {
                role: participant.role.clone(),
            },
            FactValue::Integer(
                participants
                    .iter()
                    .filter(|other| other.role == participant.role)
                    .count() as i64,
            ),
        );
        if participant.character_id.is_none() {
            if let Some(npc) = ctx.db.settlement_npc().id().find(&participant.actor_id) {
                let estate = crate::social_estate::settlement_npc_estate(ctx, &npc.id)?;
                result.facts.insert(
                    FactKey::ParticipantEstate {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(estate.id().into()),
                );
                if !npc.service_id.is_empty() {
                    result.facts.insert(
                        FactKey::Service {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(npc.service_id.clone()),
                    );
                }
                result.facts.insert(
                    FactKey::ParticipantProfession {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(npc.profession.clone()),
                );
                result.facts.insert(
                    FactKey::ParticipantAgeBand {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(format!("{:?}", npc.age_band).to_lowercase()),
                );
                result.facts.insert(
                    FactKey::ParticipantSex {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(format!("{:?}", npc.sex).to_lowercase()),
                );
                result.facts.insert(
                    FactKey::ParticipantLocalRole {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(npc.local_role.clone()),
                );
                result.facts.insert(
                    FactKey::LocalCircumstance,
                    FactValue::Text(
                        if session.location_id == "residences" {
                            "household errand"
                        } else {
                            npc.local_role.as_str()
                        }
                        .into(),
                    ),
                );
                if let Some(presence) = ctx.db.settlement_npc_presence().npc_id().find(&npc.id) {
                    result
                        .facts
                        .insert(FactKey::LocationRole, FactValue::Text(presence.location_id));
                }
            }
        }
        if let Some(id) = participant.character_id {
            let estate = crate::social_estate::character_estate(ctx, id)?;
            result.facts.insert(
                FactKey::ParticipantEstate {
                    role: participant.role.clone(),
                },
                FactValue::Text(estate.id().into()),
            );
            if let Some(character) = ctx.db.character().id().find(id) {
                let age = match character.age_years {
                    0..=12 => "child",
                    13..=17 => "adolescent",
                    60.. => "elder",
                    _ => "adult",
                };
                result.facts.insert(
                    FactKey::ParticipantAgeBand {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(age.into()),
                );
            }
            if let Some(organization_id) =
                crate::organization::effective_presented_organization(ctx, id)
            {
                result.facts.insert(
                    FactKey::ParticipantOrganization {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(organization_id.clone()),
                );
                // Presentation is current and locally recognized before it
                // reaches this branch, so neither identity fact can leak a
                // hidden, expired, or unrecognized membership.
                if let Some(profession) =
                    adventuresim_core::organization::organization(&organization_id)
                        .and_then(|organization| organization.starting_role.as_ref())
                        .map(|role| role.profession.id().to_owned())
                {
                    result.facts.insert(
                        FactKey::ParticipantProfession {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(profession),
                    );
                }
            }
            if let Some(religion) = ctx
                .db
                .character_condition()
                .character_id()
                .find(id)
                .and_then(|condition| condition.religion_id)
            {
                result.facts.insert(
                    FactKey::ParticipantReligion {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(religion),
                );
            }
            if let Some(character) = ctx.db.character().id().find(id) {
                if let Some(party_id) = character.party_id.as_ref() {
                    let leader = ctx
                        .db
                        .party_authority()
                        .id()
                        .find(party_id)
                        .is_some_and(|party| party.leader_id == id);
                    result.facts.insert(
                        FactKey::PartyLeader {
                            role: participant.role.clone(),
                        },
                        FactValue::Bool(leader),
                    );
                    result.facts.insert(
                        FactKey::ParticipantStatus {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(
                            if leader {
                                "party_leader"
                            } else {
                                "party_member"
                            }
                            .into(),
                        ),
                    );
                }
            }
            if let Some(equipment) = ctx.db.character_equip().character_id().find(id) {
                let equipped = [
                    equipment.left_hand_item_id,
                    equipment.right_hand_item_id,
                    equipment.left_arm_armor_id,
                    equipment.right_arm_armor_id,
                    equipment.left_leg_armor_id,
                    equipment.right_leg_armor_id,
                    equipment.head_armor_id,
                    equipment.chest_armor_id,
                    equipment.stomach_armor_id,
                ];
                let clothing = equipped
                    .into_iter()
                    .flatten()
                    .filter_map(|inventory_id| ctx.db.inventory_item().id().find(inventory_id))
                    .filter_map(|inventory| ctx.db.item().id().find(&inventory.item_id))
                    .find(|item| item.kind == crate::item::ItemKind::Clothing);
                if let Some(item) = clothing {
                    result.facts.insert(
                        FactKey::ParticipantClothingCategory {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(item.id),
                    );
                    result.facts.insert(
                        FactKey::ParticipantHasVisibleClothing {
                            role: participant.role.clone(),
                        },
                        FactValue::Bool(true),
                    );
                }
            }
        }
    }
    if let Some(player) = participants
        .iter()
        .find(|participant| participant.character_id == Some(character_id))
    {
        for representative in participants
            .iter()
            .filter(|participant| participant.character_id.is_none())
        {
            let Some(npc) = ctx.db.settlement_npc().id().find(&representative.actor_id) else {
                continue;
            };
            let Ok(organization_id) = dialogue_organization_id(session, &npc) else {
                continue;
            };
            let definition = adventuresim_core::organization::organization(&organization_id)
                .expect("bound organization was resolved");
            let membership = crate::organization::membership(ctx, character_id, &organization_id);
            let minute = ctx
                .db
                .character_time()
                .character_id()
                .find(character_id)
                .map_or(0, |time| time.minutes);
            let membership_state = membership.as_ref().map_or("none", |row| {
                if crate::organization::membership_is_current(row, minute) {
                    "current"
                } else {
                    "suspended"
                }
            });
            result.facts.insert(
                FactKey::OrganizationMembership {
                    player: player.role.clone(),
                    representative: representative.role.clone(),
                },
                FactValue::Text(membership_state.into()),
            );
            result.facts.insert(
                FactKey::OrganizationPromotionAvailable {
                    player: player.role.clone(),
                    representative: representative.role.clone(),
                },
                FactValue::Bool(membership.as_ref().is_some_and(|row| {
                    crate::organization::membership_is_current(row, minute)
                        && definition.next_rank(&row.rank_id).is_some()
                })),
            );
            result.facts.insert(
                FactKey::OrganizationPresentation {
                    player: player.role.clone(),
                    representative: representative.role.clone(),
                },
                FactValue::Bool(
                    ctx.db
                        .organization_presentation()
                        .character_id()
                        .find(character_id)
                        .is_some_and(|row| row.organization_id == organization_id),
                ),
            );
            result.facts.insert(
                FactKey::OrganizationDuesRequired {
                    representative: representative.role.clone(),
                },
                FactValue::Bool(definition.dues.is_some()),
            );
        }
    }
    if let Some(npc) = participants.iter().find(|p| p.character_id.is_none()) {
        let delivery_authority = ctx
            .db
            .local_problem_rumor_delivery()
            .session_id()
            .filter(&session.id)
            .find(|delivery| delivery.character_id == character_id)
            .map(|delivery| referral_delivery_authority(ctx, &delivery))
            .transpose()?;
        let delivery_receipt = match delivery_authority.as_ref() {
            Some(ReferralDeliveryAuthority::LocalProblem(receipt)) => Some(receipt),
            _ => None,
        };
        let has_public_delivery = matches!(
            delivery_authority,
            Some(ReferralDeliveryAuthority::PublicThreat)
        );
        result.facts.insert(
            FactKey::ParticipantRumorCase {
                role: npc.role.clone(),
            },
            FactValue::Bool(delivery_receipt.is_some() || has_public_delivery),
        );
        let exact_referral = if let Some(receipt) = delivery_receipt.as_ref() {
            referred_generated_witness(
                ctx,
                character_id,
                &receipt.opaque_case_ref,
                &npc.actor_id,
                &session.settlement_id,
                &session.location_id,
            )?
            .is_some()
        } else {
            false
        };
        result.facts.insert(
            FactKey::ParticipantReferralContact {
                role: npc.role.clone(),
            },
            FactValue::Bool(exact_referral),
        );
    }
    if let (Some(player), Some(npc)) = (
        participants
            .iter()
            .find(|p| p.character_id == Some(character_id)),
        participants.iter().find(|p| p.character_id.is_none()),
    ) {
        let prior_sessions: HashSet<_> = ctx
            .db
            .dialogue_participant()
            .actor_id()
            .filter(&npc.actor_id)
            .filter(|other| other.session_id != session.id)
            .map(|other| other.session_id)
            .collect();
        let prior = prior_sessions.iter().any(|prior_session| {
            ctx.db
                .dialogue_participant()
                .session_id()
                .filter(prior_session)
                .any(|other| other.character_id == Some(character_id))
        });
        result.facts.insert(
            FactKey::ParticipantPriorInteraction {
                left: player.role.clone(),
                right: npc.role.clone(),
            },
            FactValue::Bool(prior),
        );
        result.facts.insert(
            FactKey::PriorQuestioning {
                role: npc.role.clone(),
            },
            FactValue::Bool(prior),
        );
        if let (Some(skills), Some(attributes), Some(settlement)) = (
            ctx.db.character_skills().character_id().find(character_id),
            ctx.db
                .character_attributes()
                .character_id()
                .find(character_id),
            ctx.db.settlement().id().find(&session.settlement_id),
        ) {
            let effective_hours = skills
                .oral_languages
                .effective(settlement.languages.dominant_german());
            let (compatibility, passes_check) =
                participant_language_compatibility(effective_hours, attributes.instinct);
            result.facts.insert(
                FactKey::ParticipantLanguageCompatibility {
                    left: player.role.clone(),
                    right: npc.role.clone(),
                },
                FactValue::Text(compatibility.into()),
            );
            result.facts.insert(
                FactKey::LanguageCheck {
                    left: player.role.clone(),
                    right: npc.role.clone(),
                },
                FactValue::Bool(passes_check),
            );
        }
    }
    let beliefs: Vec<_> = ctx
        .db
        .investigation_belief()
        .owner_character_id()
        .filter(character_id)
        .collect();
    result
        .facts
        .insert(FactKey::KnownClaim, FactValue::Bool(!beliefs.is_empty()));
    result.facts.insert(
        FactKey::Confidence,
        FactValue::Integer(
            beliefs
                .iter()
                .map(|belief| i64::from(belief.confidence_bps))
                .max()
                .unwrap_or(0),
        ),
    );
    result.facts.insert(
        FactKey::KnownLead,
        FactValue::Bool(
            ctx.db
                .investigation_lead()
                .owner_character_id()
                .filter(character_id)
                .next()
                .is_some(),
        ),
    );
    result
        .facts
        .insert(FactKey::SocialCheck, FactValue::Bool(false));
    if let Some(time) = ctx.db.character_time().character_id().find(character_id) {
        let period = match time.minutes % 1440 {
            300..720 => "morning",
            720..1020 => "afternoon",
            1020..1260 => "evening",
            _ => "night",
        };
        result
            .facts
            .insert(FactKey::TimePeriod, FactValue::Text(period.into()));
    }
    let service = dialogue_service_id(ctx, session)?;
    if !service.is_empty() {
        if let Some(contract) = ctx
            .db
            .contract_authority()
            .service_id()
            .filter(&service)
            .find(|contract| contract.settlement_id == session.settlement_id)
        {
            result.facts.insert(
                FactKey::ContractState {
                    contract: "selected-service-contract".into(),
                },
                FactValue::Text(format!("{:?}", contract.status).to_lowercase()),
            );
        }
    }
    Ok(result)
}

fn participant_language_compatibility(
    target_effective_hours: f32,
    instinct: f32,
) -> (&'static str, bool) {
    let effective = if target_effective_hours.is_finite() {
        target_effective_hours.max(0.0)
    } else {
        0.0
    };
    let aptitude = if instinct.is_finite() {
        instinct.clamp(0.0, 5.0)
    } else {
        0.0
    };
    let coefficient =
        effective.min(aptitude * 1_000.0) / adventuresim_world_schema::ORAL_FLUENCY_HOURS;
    if coefficient >= 0.75 {
        ("fluent", true)
    } else if coefficient >= 0.35 {
        ("limited", true)
    } else {
        ("poor", false)
    }
}

#[cfg(test)]
mod participant_language_compatibility_tests {
    use super::participant_language_compatibility;

    #[test]
    fn instinct_cap_controls_dialogue_language_thresholds() {
        assert_eq!(
            participant_language_compatibility(5_000.0, 1.749),
            ("poor", false)
        );
        assert_eq!(
            participant_language_compatibility(5_000.0, 1.75),
            ("limited", true)
        );
        assert_eq!(
            participant_language_compatibility(5_000.0, 3.749),
            ("limited", true)
        );
        assert_eq!(
            participant_language_compatibility(5_000.0, 3.75),
            ("fluent", true)
        );
        assert_eq!(
            participant_language_compatibility(1_749.0, 5.0),
            ("poor", false)
        );
    }
}

fn organization_requirement_label(
    requirement: &adventuresim_core::organization::Requirement,
) -> String {
    use adventuresim_core::organization::Requirement;
    match requirement {
        Requirement::SkillRating {
            skill,
            minimum,
            leaf,
        } => format!(
            "{}{} rating {:.1}",
            skill.replace('_', " "),
            leaf.as_ref().map_or(String::new(), |leaf| format!(
                " ({})",
                leaf.replace('_', " ")
            )),
            minimum
        ),
        Requirement::ProfessedReligion { religion } => {
            format!("profession of {}", religion.replace('_', " "))
        }
    }
}

fn organization_requirements_summary<'a>(
    requirements: impl Iterator<Item = &'a adventuresim_core::organization::Requirement>,
) -> String {
    let requirements = requirements
        .map(organization_requirement_label)
        .collect::<BTreeSet<_>>();
    if requirements.is_empty() {
        "no additional requirements".into()
    } else {
        requirements.into_iter().collect::<Vec<_>>().join(", ")
    }
}

fn bind_organization_business_terms(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    npc: &crate::settlement_population::SettlementNpc,
    bindings: &mut adventuresim_dialogue::RuntimeBindings,
) -> Result<(), String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let organization_id = dialogue_organization_id(session, npc)?;
    let definition = adventuresim_core::organization::organization(&organization_id)
        .ok_or("Organization representative has an unknown organization")?;
    bindings.bind(S::OrganizationName, definition.name.clone());

    let admission_requirements = organization_requirements_summary(
        definition
            .admission
            .requirements
            .iter()
            .chain(definition.ranks[0].requirements.iter()),
    );
    let fee = if definition.admission.joining_fee == 0 {
        "There is no joining fee".to_owned()
    } else {
        format!(
            "The joining fee is {} coin{}",
            definition.admission.joining_fee,
            if definition.admission.joining_fee == 1 {
                ""
            } else {
                "s"
            }
        )
    };
    bindings.bind(
        S::OrganizationAdmissionTerms,
        format!("{fee}; admission requires {admission_requirements}."),
    );

    let membership = crate::organization::membership(ctx, character_id, &organization_id);
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let standing = membership.as_ref().map_or("not enrolled", |membership| {
        if crate::organization::membership_is_current(membership, minute) {
            "current"
        } else {
            "suspended"
        }
    });
    if let Some(dues) = &definition.dues {
        bindings.bind(
            S::OrganizationDuesTerms,
            format!(
                "Your standing is {standing}. Dues are {} coin{} every {} day{}.",
                dues.amount,
                if dues.amount == 1 { "" } else { "s" },
                dues.interval_days,
                if dues.interval_days == 1 { "" } else { "s" },
            ),
        );
    } else {
        bindings.bind(
            S::OrganizationDuesTerms,
            format!("Your standing is {standing}. This organization charges no dues."),
        );
    }

    let rank_standing = membership
        .as_ref()
        .and_then(|membership| {
            let current = definition.rank(&membership.rank_id)?;
            Some(
                if let Some(next) = definition.next_rank(&membership.rank_id) {
                    format!(
                        "Your current rank is {}. The next rank is {}; it requires {}.",
                        current.name,
                        next.name,
                        organization_requirements_summary(next.requirements.iter()),
                    )
                } else {
                    format!(
                        "Your current rank is {}, the organization's highest rank.",
                        current.name
                    )
                },
            )
        })
        .unwrap_or_else(|| "You do not hold a rank in this organization.".into());
    bindings.bind(S::OrganizationRankStanding, rank_standing);
    Ok(())
}

fn dialogue_runtime_bindings(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    speaker_role: &str,
) -> Result<adventuresim_dialogue::RuntimeBindings, String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let participant = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find(|participant| participant.character_id.is_none() && participant.role == speaker_role)
        .ok_or("Dialogue has no NPC speaker")?;
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&participant.actor_id)
        .ok_or("Dialogue NPC is no longer authoritative")?;
    let mut bindings = adventuresim_dialogue::RuntimeBindings::default();
    bindings.bind(S::SpeakerName, npc.name.clone());
    bindings.bind(
        S::SpeakerDescription,
        format!(
            "{}, {}, {}, with {} and {}",
            npc.height, npc.build, npc.complexion, npc.hair, npc.visible_features
        ),
    );
    bindings.bind(S::Settlement, session.settlement_id.clone());
    bindings.bind(S::Location, session.location_id.clone());
    if !npc.organization_id.is_empty() {
        bind_organization_business_terms(ctx, session, character_id, &npc, &mut bindings)?;
    }
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    bindings.bind(
        S::TimeWindow,
        match minute % 1_440 {
            300..720 => "in the morning",
            720..1_020 => "in the afternoon",
            1_020..1_260 => "in the evening",
            _ => "at night",
        },
    );
    if let Some(lead) = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(character_id)
        .next()
    {
        bindings.bind(S::Landmark, lead.directions.clone());
        bindings.bind(S::DescribedLocation, lead.directions);
    }
    let beliefs: Vec<_> = ctx
        .db
        .investigation_belief()
        .owner_character_id()
        .filter(character_id)
        .collect();
    if let Some(belief) = beliefs.last() {
        bindings.bind(S::Testimony, belief.statement.clone());
        bindings.bind(S::Claim, belief.statement.clone());
    }
    if let Some(circumstance) = beliefs
        .iter()
        .find(|belief| belief.proposition_id.contains("circumstance"))
    {
        bindings.bind(S::WitnessCircumstance, circumstance.statement.clone());
    }
    if let Some(evidence) = ctx
        .db
        .investigation_evidence_knowledge()
        .owner_character_id()
        .filter(character_id)
        .next()
    {
        bindings.bind(S::Evidence, format!("evidence {}", evidence.evidence_id));
        bindings.bind(S::Proof, format!("proof {}", evidence.evidence_id));
    }
    if let Some(character) = ctx.db.character().id().find(character_id)
        && let Some(contract) = character
            .party_id
            .and_then(|party_id| ctx.db.party_authority().id().find(&party_id))
            .and_then(|party| party.active_contract_id)
            .and_then(|contract_id| ctx.db.contract_authority().id().find(&contract_id))
    {
        bindings.bind(
            S::ContractTerms,
            format!(
                "{} gold and {} experience",
                contract.gold_reward, contract.xp_reward
            ),
        );
    }
    if let Some(delivery) = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|row| row.character_id == character_id)
        && let ReferralDeliveryAuthority::LocalProblem(receipt) =
            referral_delivery_authority(ctx, &delivery)?
    {
        let contact = ctx
            .db
            .settlement_npc()
            .id()
            .find(&receipt.contact_npc_id)
            .ok_or("Rumor referral contact is unavailable")?;
        let authority = ctx
            .db
            .quest_generation_authority()
            .case_id()
            .find(&receipt.opaque_case_ref)
            .ok_or("Rumor is not backed by a generated case")?;
        let generated = validate_quest_generation_authority(&authority)?.manifest;
        let witness = generated
            .witnesses
            .iter()
            .find(|witness| {
                witness.npc_id == receipt.contact_npc_id
                    && witness.expected_location == receipt.expected_location_id
            })
            .ok_or("Referral contact is not the generated witness")?;
        bindings.bind(S::Symptom, receipt.safe_summary.clone());
        bindings.bind(S::Claim, receipt.safe_summary);
        bindings.bind(S::Uncertainty, "an unconfirmed local account");
        bindings.bind(S::ReferralName, contact.name);
        bindings.bind(
            S::ReferralDescription,
            format!(
                "a {} {}, {} build, with {} and {}",
                format!("{:?}", contact.age_band).to_lowercase(),
                format!("{:?}", contact.sex).to_lowercase(),
                contact.build,
                contact.hair,
                contact.visible_features
            ),
        );
        bindings.bind(S::ReferralRole, contact.profession);
        let referral_location =
            adventuresim_core::quest_generation::referral_display_location(witness);
        bindings.bind(S::ReferralLocation, referral_location.to_owned());
        bindings.bind(S::DescribedLocation, referral_location.to_owned());
        if let Some((_, selected_witness)) = referred_generated_witness(
            ctx,
            character_id,
            &receipt.opaque_case_ref,
            &npc.id,
            &session.settlement_id,
            &session.location_id,
        )? {
            let testimony = adventuresim_core::quest_generation::initial_testimony_projection(
                &selected_witness,
            )
            .into_iter()
            .map(|(_, draft)| adventuresim_dialogue::TestimonyLine {
                spoken_text: draft.spoken_text.trim().to_owned(),
                claim_text: draft.challenge_text.clone(),
            })
            .filter(|line| !line.spoken_text.is_empty())
            .collect::<Vec<_>>();
            if testimony.is_empty() {
                return Err("Generated witness testimony is unavailable or too large".into());
            }
            bindings.bind_testimony(testimony);
        }
    }
    Ok(bindings)
}
