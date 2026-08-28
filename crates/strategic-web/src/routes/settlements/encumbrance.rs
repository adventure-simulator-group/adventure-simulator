#[derive(Default)]
pub(super) struct EncumbranceRows {
    attributes: Vec<CharacterAttributes>,
    limbs: Vec<CharacterLimbs>,
    conditions: Vec<CharacterCondition>,
    objects: Vec<InventoryObject>,
    containment: Vec<InventoryContainment>,
    liquids: Vec<ContainerLiquid>,
}

pub(super) const ENCUMBRANCE_QUERY_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InventoryEncumbranceSummaries {
    pub(super) personal: EncumbranceSummary,
    pub(super) party: EncumbranceSummary,
}

pub(super) fn encumbrance_query_ids(
    members: &[Character],
    active_character_id: u64,
) -> (Vec<u64>, Vec<u64>) {
    let living_ids: std::collections::BTreeSet<u64> = members
        .iter()
        .filter(|member| member.alive)
        .map(|member| member.id)
        .collect();
    let mut row_ids = living_ids.clone();
    row_ids.insert(active_character_id);
    (
        living_ids.into_iter().collect(),
        row_ids.into_iter().collect(),
    )
}

pub(super) async fn inventory_encumbrance_summaries(
    state: &AppState,
    active_character: &Character,
    active_inventory: &[InventoryItem],
    members: &[Character],
    pooled: &[PartyInventoryItem],
    items: &[ItemDefinition],
    include_party: bool,
) -> InventoryEncumbranceSummaries {
    let aggregate_members = if include_party {
        members
    } else {
        Default::default()
    };
    let (member_ids, encumbrance_ids) =
        encumbrance_query_ids(aggregate_members, active_character.id);
    let all_inventories = stream::iter(member_ids)
        .map(|member_id| async move {
            if member_id == active_character.id {
                active_inventory.to_vec()
            } else {
                state
                    .db
                    .query::<InventoryItem>(&format!(
                        "SELECT * FROM inventory_item WHERE character_id = {member_id}"
                    ))
                    .await
                    .unwrap_or_default()
            }
        })
        .buffer_unordered(ENCUMBRANCE_QUERY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let rows = EncumbranceRows::query(state, &encumbrance_ids).await;
    let food_lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    InventoryEncumbranceSummaries {
        personal: personal_encumbrance(
            active_character.id,
            active_inventory,
            items,
            &food_lots,
            &rows,
        ),
        party: if include_party {
            party_encumbrance(members, &all_inventories, pooled, items, &food_lots, &rows)
        } else {
            Default::default()
        },
    }
}

impl EncumbranceRows {
    pub(super) async fn query(state: &AppState, character_ids: &[u64]) -> Self {
        let unique_ids: std::collections::BTreeSet<u64> = character_ids.iter().copied().collect();
        let lookups = stream::iter(unique_ids)
            .map(|character_id| async move {
                // Keep each member's lookups sequential so the outer
                // buffer is a bound on actual in-flight database calls.
                let attributes = query_single::<CharacterAttributes>(
                    state,
                    "backend_character_attributes",
                    character_id,
                )
                .await;
                let limbs =
                    query_single::<CharacterLimbs>(state, "backend_character_limbs", character_id)
                        .await;
                let condition = query_single::<CharacterCondition>(
                    state,
                    "backend_character_conditions",
                    character_id,
                )
                .await;
                (attributes, limbs, condition)
            })
            .buffer_unordered(ENCUMBRANCE_QUERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut rows = Self::default();
        for (attributes, limbs, condition) in lookups {
            rows.attributes.extend(attributes);
            rows.limbs.extend(limbs);
            rows.conditions.extend(condition);
        }
        let (objects, containment, liquids) = tokio::join!(
            state
                .db
                .query::<InventoryObject>("SELECT * FROM inventory_object"),
            state
                .db
                .query::<InventoryContainment>("SELECT * FROM inventory_containment"),
            state
                .db
                .query::<ContainerLiquid>("SELECT * FROM container_liquid"),
        );
        rows.objects = objects.unwrap_or_default();
        rows.containment = containment.unwrap_or_default();
        rows.liquids = liquids.unwrap_or_default();
        rows
    }
}

pub(super) fn item_stack_weight_kg(item_id: &str, quantity: u32, items: &[ItemDefinition]) -> f32 {
    items
        .iter()
        .find(|definition| definition.id == item_id)
        .map_or(0.0, |definition| {
            definition.weight.max(0.0) * quantity as f32
        })
}

pub(super) fn personal_encumbrance(
    character_id: u64,
    inventory: &[InventoryItem],
    items: &[ItemDefinition],
    food_lots: &[FoodLot],
    rows: &EncumbranceRows,
) -> EncumbranceSummary {
    let body_weight = rows
        .conditions
        .iter()
        .find(|row| row.character_id == character_id)
        .map_or(0.0, |row| {
            adventuresim_core::physiology::BodyMassKg::try_new(row.body_weight_kg)
                .expect("persisted character body mass must be valid")
                .kilograms()
        });
    let water_weight = adventuresim_core::physical_object::OperationalCustody::character(
        character_id,
    )
    .map_or(0.0, |custody| {
        super::contained_water_ml_for_custody(
            &rows.objects,
            &rows.containment,
            &rows.liquids,
            &custody,
        ) as f32
            / 1_000.0
    });
    let inventory_weight = inventory
        .iter()
        .filter(|row| row.character_id == character_id)
        .map(|row| {
            food_lots
                .iter()
                .find(|lot| lot.inventory_item_id == Some(row.id))
                .map_or_else(
                    || item_stack_weight_kg(&row.item_id, row.quantity, items),
                    |lot| lot.mass_kg.max(0.0),
                )
        })
        .sum::<f32>();
    let capacity = rows
        .attributes
        .iter()
        .find(|row| row.character_id == character_id)
        .zip(
            rows.limbs
                .iter()
                .find(|row| row.character_id == character_id),
        )
        .map_or(0.0, |(attributes, limbs)| {
            encumbrance_capacity_kg(
                (attributes.left_leg_strength * limbs.left_leg_health.clamp(0.0, 1.0)
                    + attributes.right_leg_strength * limbs.right_leg_health.clamp(0.0, 1.0))
                    / 2.0,
            )
        });

    EncumbranceSummary::new(body_weight + water_weight + inventory_weight, capacity)
}

pub(super) fn party_encumbrance(
    members: &[Character],
    inventories: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    items: &[ItemDefinition],
    food_lots: &[FoodLot],
    rows: &EncumbranceRows,
) -> EncumbranceSummary {
    let member_summary = members.iter().filter(|member| member.alive).fold(
        EncumbranceSummary::default(),
        |summary, member| {
            summary.combined(personal_encumbrance(
                member.id,
                inventories,
                items,
                food_lots,
                rows,
            ))
        },
    );
    let pooled_weight = pooled
        .iter()
        .map(|row| {
            food_lots
                .iter()
                .find(|lot| lot.party_inventory_item_id == Some(row.id))
                .map_or_else(
                    || item_stack_weight_kg(&row.item_id, row.quantity, items),
                    |lot| lot.mass_kg.max(0.0),
                )
        })
        .sum::<f32>();
    let water_weight = members
        .iter()
        .find_map(|member| member.party_id.as_deref())
        .and_then(|party_id| {
            adventuresim_core::physical_object::OperationalCustody::party(party_id).ok()
        })
        .map_or(0.0, |custody| {
            super::contained_water_ml_for_custody(
                &rows.objects,
                &rows.containment,
                &rows.liquids,
                &custody,
            ) as f32
                / 1_000.0
        });
    member_summary.combined(EncumbranceSummary::new(pooled_weight + water_weight, 0.0))
}

pub(super) async fn get_active_character(
    state: &AppState,
    character_id: Option<u64>,
) -> Option<(Character, Vec<InventoryItem>)> {
    let character_id = character_id?;
    let inventory_sql = format!("SELECT * FROM inventory_item WHERE character_id = {character_id}");
    let (character, inventory) = tokio::join!(
        super::super::data::character_as_observed(state, character_id, character_id),
        state.db.query::<InventoryItem>(&inventory_sql),
    );
    let character = character.ok().flatten()?;
    let inventory = inventory.unwrap_or_default();
    Some((character, inventory))
}

pub(super) fn camp_entry_redirect(has_party: bool, has_camp: bool) -> Option<&'static str> {
    (!has_party || !has_camp).then_some("/")
}

#[cfg(test)]
mod camp_page_model_tests {
    use super::camp_entry_redirect;

    #[test]
    fn camp_page_model_requires_selected_party_and_camp_projection() {
        assert_eq!(camp_entry_redirect(false, false), Some("/"));
        assert_eq!(camp_entry_redirect(true, false), Some("/"));
        assert_eq!(camp_entry_redirect(true, true), None);
    }
}

pub(super) async fn get_character_capability(
    state: &AppState,
    character_id: u64,
) -> Option<CharacterCapability> {
    let _ = state
        .db
        .call("refresh_capabilities", &[json!(character_id)])
        .await;
    state
        .db
        .query(&format!(
            "SELECT * FROM backend_character_capabilities WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

pub(crate) async fn get_combat_training_profile(
    state: &AppState,
    character_id: u64,
) -> CombatTrainingProfile {
    let occupancies = state
        .db
        .query::<EquipmentOccupancy>(&format!(
            "SELECT * FROM equipment_occupancy WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let mut hands = Vec::new();
    for inventory_id in occupancies
        .iter()
        .filter(|row| row.channel == adventuresim_stdb_client::EquipmentChannel::Held)
        .map(|row| row.inventory_item_id)
    {
        let inventory = state
            .db
            .query_one::<InventoryItem>(&format!(
                "SELECT * FROM inventory_item WHERE id = {inventory_id}"
            ))
            .await
            .ok()
            .flatten();
        let Some(inventory) = inventory else { continue };
        let definition = state
            .db
            .query_one::<ItemDefinition>(&format!(
                "SELECT * FROM item WHERE id = {}",
                sql_string_literal(&inventory.item_id)
            ))
            .await
            .ok()
            .flatten();
        if let Some(item) = definition {
            hands.push(EquippedCombatItem {
                weapons: item.weapon_skills.core(),
                shield: item.kind == ItemKind::Shield,
                balance: item.balance,
            });
        }
    }
    CombatTrainingProfile::from_equipped_hands(hands)
}

pub(crate) async fn get_active_party_members(
    state: &AppState,
    active_character: Option<&Character>,
) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships_sql = format!(
        "SELECT * FROM party_member WHERE party_id = {}",
        sql_string_literal(party_id)
    );
    let party_sql = crate::spacetimedb::party_by_id(party_id);
    let (memberships, party) = tokio::join!(
        state.db.query::<PartyMember>(&memberships_sql),
        state.db.query::<Party>(&party_sql),
    );
    let memberships = memberships.unwrap_or_default();
    let leader_id = party
        .unwrap_or_default()
        .first()
        .map(|party| party.leader_id);
    let lookups = memberships.into_iter().map(|membership| async move {
        state
            .db
            .query::<Character>(&format!(
                "SELECT * FROM backend_characters WHERE id = {}",
                membership.character_id
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    });
    let mut members: Vec<Character> = join_all(lookups).await.into_iter().flatten().collect();
    if let Some(actor) = active_character {
        // Party membership and Character are mutable current projections. A
        // member whose personal frontier is ahead of the viewer would expose
        // future location, party, progression, and wealth through this model,
        // so omit that row until the viewer catches up.
        let visibility = join_all(members.iter().map(|member| async move {
            (
                member.id,
                super::super::data::character_not_ahead_of_observer(state, member.id, actor.id)
                    .await
                    .unwrap_or(false),
            )
        }))
        .await;
        let visible_ids = visibility
            .into_iter()
            .filter_map(|(id, visible)| visible.then_some(id))
            .collect::<HashSet<_>>();
        members.retain(|member| visible_ids.contains(&member.id));
        if let Err(error) =
            super::super::data::project_alive_as_observed(state, actor.id, &mut members).await
        {
            // A failed chronology read must not disclose or act on broad
            // current death state from beyond the selected character's date.
            tracing::warn!(%error, "could not project party life state at observer date");
            for member in members.iter_mut().filter(|member| !member.alive) {
                member.alive = true;
            }
        }
        let addresses_sql = format!(
            "SELECT * FROM backend_social_addresses WHERE actor_id = {}",
            actor.id
        );
        let automatic_sql = format!(
            "SELECT * FROM backend_automatic_social_chats WHERE actor_id = {}",
            actor.id
        );
        let source_lookups = members.iter().map(|member| async move {
            state
                .db
                .query::<CharacterMoraleSource>(&format!(
                    "SELECT * FROM backend_character_morale_sources WHERE character_id = {}",
                    member.id
                ))
                .await
                .unwrap_or_default()
        });
        let (source_groups, addresses, automatic_chats) = tokio::join!(
            join_all(source_lookups),
            state.db.query::<SocialAddress>(&addresses_sql),
            state.db.query::<AutomaticSocialChat>(&automatic_sql),
        );
        let sources: Vec<_> = source_groups.into_iter().flatten().collect();
        let successful = addresses.unwrap_or_default();
        let automatic_targets: HashSet<u64> = automatic_chats
            .unwrap_or_default()
            .into_iter()
            .filter(|preference| preference.enabled && preference.actor_id == actor.id)
            .map(|preference| preference.target_id)
            .collect();
        for member in &mut members {
            let colocated = member.id == actor.id
                || (member.current_settlement_id == actor.current_settlement_id
                    && member.current_case_site_id == actor.current_case_site_id);
            if !member.alive || !actor.alive || !colocated {
                continue;
            }
            member.social_notification_count =
                adventuresim_core::social::unaddressed_social_source_count(
                    actor.id,
                    member.id,
                    sources
                        .iter()
                        .filter(|source| source.character_id == member.id)
                        .map(|source| (source.id.as_str(), source.kind.as_str(), source.magnitude)),
                    successful.iter().map(|address| {
                        (
                            address.actor_id,
                            address.target_id,
                            address.source_id.as_str(),
                            true,
                        )
                    }),
                );
            member.automatic_social_chat_enabled = automatic_targets.contains(&member.id);
        }
    }
    members.sort_by_key(|member| (Some(member.id) != leader_id, member.id));
    members
}
