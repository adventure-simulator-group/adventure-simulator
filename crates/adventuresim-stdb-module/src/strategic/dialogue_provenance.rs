enum ReferralDeliveryAuthority {
    LocalProblem(Box<crate::local_problem::LocalProblemReceipt>),
    PublicThreat,
}

use crate::relationship::{character_kinship as _, courtship as _};
use crate::social::character_familiarity as _;

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
            Ok(ReferralDeliveryAuthority::LocalProblem(Box::new(
                receipt.ok_or("Classified local-problem receipt disappeared")?,
            )))
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
    current_speaker_resident_character_id: u64,
    delivery: &crate::local_problem::LocalProblemRumorDelivery,
) -> Result<(String, String), String> {
    let facts = dialogue_fact_context(ctx, session, character_id)?;
    let ReferralDeliveryAuthority::LocalProblem(receipt) =
        referral_delivery_authority(ctx, delivery)?
    else {
        return Err("Quest referral is not a local-problem delivery".into());
    };
    let contact = crate::settlement_population::resolve_settlement_resident(
        ctx,
        receipt.contact_resident_character_id,
    )
    .ok_or("Rumor referral contact is unavailable")?;
    let source = crate::settlement_population::resolve_settlement_resident(
        ctx,
        current_speaker_resident_character_id,
    );
    let contact_id = contact.character_id.to_string();
    let source_id = source
        .as_ref()
        .map(|source| source.character_id.to_string());
    if !referral_variant_can_replace_authoritative_wording(
        source
            .as_ref()
            .zip(source_id.as_deref())
            .map(|(source, id)| (id, source.name.as_str())),
        &contact_id,
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
    values.insert("summary".into(), receipt.safe_summary.clone());
    values.insert("contact_name".into(), contact.name.clone());
    values.insert("contact_profession".into(), contact.profession.clone());
    values.insert(
        "contact_location".into(),
        receipt.expected_location_id.clone(),
    );
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
        let source = crate::strategic::STRATEGIC_SOURCE;
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
        assert!(renderer.contains("current_speaker_resident_character_id"));
        assert!(!renderer.contains("receipt.source_resident_character_id"));
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

fn local_service_organization_representative(
    ctx: &ReducerContext,
    settlement_id: &str,
    location_id: &str,
    service_id: &str,
) -> Option<crate::settlement_population::ResolvedSettlementResident> {
    adventuresim_core::organization::organizations_for_chapter(settlement_id)
        .filter(|organization| organization.service_id.as_deref() == Some(service_id))
        .find_map(|organization| {
            let chapter = organization.chapter(settlement_id)?;
            let settlement = ctx.db.settlement().id().find(settlement_id.to_owned())?;
            (adventuresim_core::organization::chapter_effective_location_id(
                organization,
                chapter,
                &settlement.economy,
            ) == location_id)
                .then(|| {
                    adventuresim_core::organization::organization_representative_id(
                        settlement_id,
                        &organization.id,
                    )
                })
                .and_then(|id| crate::settlement_population::resolve_settlement_resident(ctx, id))
                .filter(|npc| {
                    exact_organization_representative(ctx, npc, settlement_id, location_id)
                        .as_deref()
                        == Some(organization.id.as_str())
                        && ctx
                            .db
                            .settlement_resident_presence()
                            .character_id()
                            .find(npc.character_id)
                            .is_some_and(|presence| {
                                presence.settlement_id == settlement_id
                                    && presence.location_id == location_id
                            })
                })
        })
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
        if participant.character_id.is_none()
            && let Ok(npc_character_id) = participant.actor_id.parse::<u64>()
                && let Some(npc) =
                    crate::settlement_population::resolve_settlement_resident(ctx, npc_character_id)
            {
                for organization_role in crate::social_roles::character_roles(ctx, npc.character_id)? {
                    result.facts.insert(
                        FactKey::ParticipantRole {
                            role: participant.role.clone(),
                            profession: organization_role.profession.clone(),
                        },
                        FactValue::Bool(true),
                    );
                }
                if !npc.service_id.is_empty() {
                    result.facts.insert(
                        FactKey::Service {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(npc.service_id.clone()),
                    );
                    result.facts.insert(
                        FactKey::LocalOrganizationRepresentative {
                            role: participant.role.clone(),
                        },
                        FactValue::Bool(
                            local_service_organization_representative(
                                ctx,
                                &session.settlement_id,
                                &session.location_id,
                                &npc.service_id,
                            )
                            .is_some(),
                        ),
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
                if let Some(presence) = ctx
                    .db
                    .settlement_resident_presence()
                    .character_id()
                    .find(npc.character_id)
                {
                    result
                        .facts
                        .insert(FactKey::LocationRole, FactValue::Text(presence.location_id));
                }
            }
        if let Some(id) = participant.character_id {
            for organization_role in crate::social_roles::character_roles(ctx, id)? {
                result.facts.insert(
                    FactKey::ParticipantRole {
                        role: participant.role.clone(),
                        profession: organization_role.profession.clone(),
                    },
                    FactValue::Bool(true),
                );
            }
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
            if let Some(character) = ctx.db.character().id().find(id)
                && let Some(party_id) = character.party_id.as_ref() {
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
            {
                let mut equipped = ctx
                    .db
                    .character_equipped_item()
                    .character_id()
                    .filter(id)
                    .collect::<Vec<_>>();
                equipped.sort_by_key(|row| {
                    let outer = ctx
                        .db
                        .equipment_occupancy()
                        .inventory_item_id()
                        .filter(row.inventory_item_id)
                        .max_by_key(|occupancy| (occupancy.channel.order(), occupancy.order))
                        .map_or((0, 0), |occupancy| {
                            (occupancy.channel.order(), occupancy.order)
                        });
                    (std::cmp::Reverse(outer), row.inventory_item_id)
                });
                let clothing = equipped
                    .into_iter()
                    .filter_map(|row| ctx.db.inventory_item().id().find(row.inventory_item_id))
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
            let Ok(resident_character_id) = representative.actor_id.parse::<u64>() else {
                continue;
            };
            let Some(npc) = ctx
                .db
                .settlement_resident_profile()
                .character_id()
                .find(resident_character_id)
            else {
                continue;
            };
            let Ok(organization_id) = dialogue_organization_id(ctx, session, &npc) else {
                continue;
            };
            let definition = adventuresim_core::organization::organization(&organization_id)
                .ok_or_else(|| {
                    format!("Dialogue organization authority references unknown {organization_id}")
                })?;
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
                        && crate::social_roles::assigned_organization_role(
                            ctx,
                            character_id,
                            &organization_id,
                        )
                        .is_ok_and(|assignment| {
                            definition.promotion_targets(&assignment.role_id).next().is_some()
                        })
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
        let exact_referral = npc
            .actor_id
            .parse::<u64>()
            .ok()
            .map(|resident_character_id| {
                dialogue_referred_witness(ctx, character_id, session, resident_character_id)
            })
            .transpose()?
            .flatten()
            .is_some();
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
    if !service.is_empty()
        && let Some(contract) = ctx
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
    npc: &crate::settlement_population::SettlementResidentProfile,
    bindings: &mut adventuresim_dialogue::RuntimeBindings,
) -> Result<(), String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let organization_id = dialogue_organization_id(ctx, session, npc)?;
    let familiar = dialogue_pair_uses_familiar(ctx, npc.character_id, character_id);
    let possessive = if familiar { "Thy" } else { "Your" };
    let definition = adventuresim_core::organization::organization(&organization_id)
        .ok_or("Organization representative has an unknown organization")?;
    bindings.bind(S::OrganizationName, definition.name.clone());

    let admission_requirements = organization_requirements_summary(
        definition.admission.requirements.iter().chain(
            definition
                .entry_role_ids
                .first()
                .and_then(|role_id| definition.role(role_id))
                .into_iter()
                .flat_map(|role| role.requirements.iter()),
        ),
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
        format!("{fee}; admission requireth {admission_requirements}."),
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
                "{possessive} standing is {standing}. Dues are {} coin{} every {} day{}.",
                dues.amount,
                if dues.amount == 1 { "" } else { "s" },
                dues.interval_days,
                if dues.interval_days == 1 { "" } else { "s" },
            ),
        );
    } else {
        bindings.bind(
            S::OrganizationDuesTerms,
            format!("{possessive} standing is {standing}. This organization chargeth no dues."),
        );
    }

    let role_standing = membership
        .as_ref()
        .and_then(|_| {
            let assignment = crate::social_roles::assigned_organization_role(
                ctx,
                character_id,
                &organization_id,
            ).ok()?;
            let current = definition.role(&assignment.role_id)?;
            let targets = definition.promotion_targets(&assignment.role_id).collect::<Vec<_>>();
            Some(if targets.is_empty() {
                    format!("{possessive} current role is {}. No promotion proceedeth from it.", current.name)
                } else {
                    let choices = targets.iter().map(|role| role.name.as_str()).collect::<Vec<_>>().join(" or ");
                    format!(
                        "{possessive} current role is {}. The available promotion role{} {}: {}.",
                        current.name,
                        if targets.len() == 1 { " is" } else { "s are" },
                        choices,
                        targets.iter().map(|role| format!("{} requireth {}", role.name, organization_requirements_summary(role.requirements.iter()))).collect::<Vec<_>>().join("; "),
                    )
                })
        })
        .unwrap_or_else(|| {
            if familiar {
                "Thou dost not hold a role in this organization.".into()
            } else {
                "You do not hold a role in this organization.".into()
            }
        });
    bindings.bind(S::OrganizationRoleStanding, role_standing);
    Ok(())
}

fn dialogue_actor_id(participant: &DialogueParticipant) -> Option<u64> {
    participant.character_id.or_else(|| participant.actor_id.parse().ok())
}

fn dialogue_public_role(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Option<&'static adventuresim_core::organization::OrganizationRoleDefinition>, String> {
    Ok(crate::social_roles::character_roles(ctx, character_id)?
        .into_iter()
        .filter(|role| role.publicly_recognizable && !role.address_title.is_empty())
        .max_by_key(|role| (role.address_priority, role.id.as_str())))
}

fn dialogue_social_precedence(ctx: &ReducerContext, character_id: u64) -> Result<i16, String> {
    Ok(dialogue_public_role(ctx, character_id)?
        .map(|role| role.social_precedence)
        .unwrap_or_default())
}

fn dialogue_address_title(ctx: &ReducerContext, character_id: u64) -> Result<String, String> {
    if let Some(role) = dialogue_public_role(ctx, character_id)? {
        return Ok(role.address_title.clone());
    }
    if let Some(organization_id) = crate::organization::effective_presented_organization(ctx, character_id)
        && let Some(profession) = adventuresim_core::organization::organization(&organization_id)
            .and_then(|definition| definition.starting_role.as_ref())
    {
        return Ok(profession.profession.label().to_owned());
    }
    Ok(crate::settlement_population::resolve_settlement_resident(ctx, character_id)
        .map(|resident| resident.profession.replace('_', " "))
        .unwrap_or_else(|| "traveler".into()))
}

fn dialogue_pair_is_intimate(ctx: &ReducerContext, left: u64, right: u64) -> bool {
    use crate::relationship::{CourtshipStatus, KinshipKind};
    let immediate_kin = ctx
        .db
        .character_kinship()
        .subject_id()
        .filter(left)
        .any(|edge| {
            edge.related_id == right
                && matches!(
                    edge.kind,
                    KinshipKind::Parent
                        | KinshipKind::Child
                        | KinshipKind::Sibling
                        | KinshipKind::Spouse
                )
        });
    let active_courtship = ctx
        .db
        .courtship()
        .first_character_id()
        .filter(left)
        .chain(ctx.db.courtship().first_character_id().filter(right))
        .any(|courtship| {
        ((courtship.first_character_id == left && courtship.second_character_id == right)
            || (courtship.first_character_id == right && courtship.second_character_id == left))
            && matches!(courtship.status, CourtshipStatus::Active | CourtshipStatus::Exposed)
        });
    let low_id = left.min(right);
    let high_id = left.max(right);
    let familiar = ctx
        .db
        .character_familiarity()
        .low_id()
        .filter(low_id)
        .any(|familiarity| {
        familiarity.high_id == high_id
            && familiarity.shared_minutes >= 40 * 60
        });
    immediate_kin || active_courtship || familiar
}

fn dialogue_pair_uses_familiar(ctx: &ReducerContext, speaker_id: u64, addressee_id: u64) -> bool {
    dialogue_pair_is_intimate(ctx, speaker_id, addressee_id)
        || dialogue_social_precedence(ctx, speaker_id).unwrap_or_default()
            > dialogue_social_precedence(ctx, addressee_id).unwrap_or_default()
}

fn bind_pairwise_address(
    ctx: &ReducerContext,
    session: &DialogueSession,
    speaker: &DialogueParticipant,
    acting_character_id: u64,
    addressee: &adventuresim_dialogue::Addressee,
    bindings: &mut adventuresim_dialogue::RuntimeBindings,
) -> Result<(), String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let participants = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|participant| {
            participant.id != speaker.id
                && participant.role == addressee.role()
                && (!matches!(addressee, adventuresim_dialogue::Addressee::Participant { .. })
                    || dialogue_actor_id(participant) == Some(acting_character_id))
        })
        .collect::<Vec<_>>();
    if participants.is_empty() {
        return Err("Dialogue has no participant bound to the addressed role".into());
    }
    if !addressee.is_group() && participants.len() != 1 {
        return Err("A singular dialogue addressee role must bind exactly one participant".into());
    }
    let addressees = participants
        .iter()
        .filter_map(dialogue_actor_id)
        .collect::<Vec<_>>();
    let singular = !addressee.is_group();
    let speaker_id = dialogue_actor_id(speaker);
    let outranks = singular && speaker_id.is_some_and(|speaker_id| {
        dialogue_social_precedence(ctx, speaker_id).unwrap_or_default()
            > dialogue_social_precedence(ctx, addressees[0]).unwrap_or_default()
    });
    let intimate = singular && speaker_id.is_some_and(|speaker_id| {
        dialogue_pair_is_intimate(ctx, speaker_id, addressees[0])
    });
    let register = adventuresim_core::organization::second_person_register(
        singular, outranks, intimate,
    );
    bindings.bind(S::SecondPersonSubject, register.subject);
    bindings.bind(S::SecondPersonObject, register.object);
    bindings.bind(S::SecondPersonPossessive, register.possessive);
    bindings.bind(S::SecondPersonPossessivePronoun, register.possessive_pronoun);
    bindings.bind(S::SecondPersonReflexive, register.reflexive);
    bindings.bind(S::SecondPersonBe, register.be);
    bindings.bind(S::SecondPersonHave, register.have);
    bindings.bind(S::SecondPersonDo, register.do_word);
    bindings.bind(S::SecondPersonWill, register.will);
    bindings.bind(S::SecondPersonMay, register.may);
    bindings.bind(S::SecondPersonShould, register.should);
    let title = if singular {
        dialogue_address_title(ctx, addressees[0])?
    } else {
        "gentlefolk".into()
    };
    bindings.bind(S::AddresseeTitle, title);
    Ok(())
}

fn dialogue_runtime_bindings(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    speaker_role: &str,
    addressee: Option<&adventuresim_dialogue::Addressee>,
) -> Result<adventuresim_dialogue::RuntimeBindings, String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let participant = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find(|participant| participant.role == speaker_role)
        .ok_or("Dialogue has no bound speaker")?;
    let mut bindings = adventuresim_dialogue::RuntimeBindings::default();
    if let Some(addressee) = addressee {
        bind_pairwise_address(
            ctx,
            session,
            &participant,
            character_id,
            addressee,
            &mut bindings,
        )?;
    }
    let npc_character_id = dialogue_actor_id(&participant).ok_or("Dialogue speaker identity is invalid")?;
    let Some(npc) = crate::settlement_population::resolve_settlement_resident(ctx, npc_character_id) else {
        return Ok(bindings);
    };
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
    if !npc.service_id.is_empty()
        && let Some(representative) = local_service_organization_representative(
            ctx,
            &session.settlement_id,
            &session.location_id,
            &npc.service_id,
        )
    {
        bindings.bind(S::OrganizationRepresentativeName, representative.name);
    }
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
        let contact = crate::settlement_population::resolve_settlement_resident(
            ctx,
            receipt.contact_resident_character_id,
        )
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
                witness.resident_character_id == receipt.contact_resident_character_id
                    && witness.expected_location == receipt.expected_location_id
            })
            .ok_or("Referral contact is not the generated witness")?;
        bindings.bind(S::Symptom, receipt.safe_summary.clone());
        bindings.bind(S::Claim, receipt.safe_summary);
        bindings.bind(S::Uncertainty, "an unconfirmed local account");
        bindings.bind(S::ReferralName, contact.name.clone());
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
        bindings.bind(S::ReferralRole, contact.profession.clone());
        let referral_location =
            adventuresim_core::quest_generation::referral_display_location(witness);
        bindings.bind(S::ReferralLocation, referral_location.to_owned());
        bindings.bind(S::DescribedLocation, referral_location.to_owned());
    }
    if let Some((_, selected_witness)) =
        dialogue_referred_witness(ctx, character_id, session, npc.character_id)?
    {
        let testimony =
            adventuresim_core::quest_generation::initial_testimony_projection(&selected_witness)
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
    Ok(bindings)
}
