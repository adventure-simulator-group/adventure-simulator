// Owns gateway-only preparation plans and fireplace station/dish projections.
/// Gateway-only public projection of the exact reducer tuple and preview. The
/// reducer still rebuilds and revalidates the private strategic/material plan
/// in its transaction; this view prevents the browser from inventing object
/// identity, revision, request identity, or duration.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendIngredientPreparationPlan {
    pub actor_character_id: u64,
    pub inventory_scope: String,
    pub inventory_item_id: u64,
    pub food_lot_id: u64,
    pub material_object_id: u64,
    pub request_id: String,
    pub expected_revision: u64,
    pub attempt_generation: u64,
    pub action: IngredientPreparationAction,
    pub duration_minutes: u32,
    pub next_display_name: String,
}

fn view_object_for_row(
    ctx: &ViewContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Option<crate::InventoryObject> {
    let mut matches = ctx
        .db
        .inventory_object()
        .item_id()
        .filter(""..)
        .filter(|object| match (&object.location, scope) {
            (InventoryLocation::Personal(location), CarriedInventoryScope::Personal) => {
                location.row_id == row_id
            }
            (InventoryLocation::Party(location), CarriedInventoryScope::Party) => {
                location.row_id == row_id
            }
            _ => false,
        });
    let object = matches.next()?;
    matches.next().is_none().then_some(object)
}

fn view_ancestry_reaches_fireplace(ctx: &ViewContext, object_id: u64) -> bool {
    let mut cursor = Some(object_id);
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let Some(id) = cursor else { return false };
        let Some(object) = ctx.db.inventory_object().id().find(id) else {
            return true;
        };
        if object.location.is_fireplace() {
            return true;
        }
        cursor = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(id)
            .map(|edge| edge.parent_object_id);
    }
    true
}

fn view_carried_item_rows(
    ctx: &ViewContext,
    actor: &crate::Character,
) -> Vec<(CarriedInventoryScope, u64, String)> {
    let mut rows = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(actor.id)
        .filter(|row| {
            view_object_for_row(ctx, CarriedInventoryScope::Personal, row.id).is_some_and(
                |object| {
                    !view_ancestry_reaches_fireplace(ctx, object.id)
                        && view_carried_custody_is_fully_resolved(
                            ctx,
                            actor,
                            CarriedInventoryScope::Personal,
                            &object,
                        )
                },
            )
        })
        .map(|row| (CarriedInventoryScope::Personal, row.id, row.item_id))
        .collect::<Vec<_>>();
    if let Some(party_id) = actor.party_id.as_deref() {
        rows.extend(
            ctx.db
                .party_inventory_item()
                .party_id()
                .filter(party_id)
                .filter(|row| {
                    view_object_for_row(ctx, CarriedInventoryScope::Party, row.id).is_some_and(
                        |object| {
                            !view_ancestry_reaches_fireplace(ctx, object.id)
                                && view_carried_custody_is_fully_resolved(
                                    ctx,
                                    actor,
                                    CarriedInventoryScope::Party,
                                    &object,
                                )
                        },
                    )
                })
                .map(|row| (CarriedInventoryScope::Party, row.id, row.item_id)),
        );
    }
    rows
}

fn view_cutting_weapon_binding(ctx: &ViewContext, actor: &crate::Character) -> Option<String> {
    view_carried_item_rows(ctx, actor)
        .into_iter()
        .filter_map(|(scope, row_id, item_id)| {
            let item = ctx.db.item().id().find(item_id)?;
            if !item.slash || item.accuracy < 0.5 {
                return None;
            }
            let damage = if scope == CarriedInventoryScope::Personal {
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .find(row_id)
                    .map(|condition| condition.bins())
            } else {
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(row_id)
                    .map(|condition| {
                        DamageBins([
                            condition.tier_1,
                            condition.tier_2,
                            condition.tier_3,
                            condition.tier_4,
                            condition.tier_5,
                        ])
                        .normalized()
                    })
            }
            .unwrap_or_default();
            (effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity) >= 0.5).then(
                || {
                    cutting_weapon_binding(
                        scope,
                        row_id,
                        &item.id,
                        item.accuracy,
                        item.edge_sensitivity,
                        damage,
                    )
                },
            )
        })
        .min()
}

fn view_preparation_skill_check(ctx: &ViewContext, character_id: u64, skill: Skill) -> Option<f32> {
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)?;
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)?;
    Some(skill.capped_training_rank(skills.effective_skill_hours(skill), &attributes))
}

fn view_carried_custody_is_fully_resolved(
    ctx: &ViewContext,
    actor: &crate::Character,
    scope: CarriedInventoryScope,
    object: &crate::InventoryObject,
) -> bool {
    let mut cursor = object.clone();
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let row_id = match (&cursor.location, scope) {
            (InventoryLocation::Personal(location), CarriedInventoryScope::Personal)
                if location.character_id == actor.id =>
            {
                location.row_id
            }
            (InventoryLocation::Party(location), CarriedInventoryScope::Party)
                if actor.party_id.as_deref() == Some(location.party_id.as_str()) =>
            {
                location.row_id
            }
            _ => return false,
        };
        if !view_object_for_row(ctx, scope, row_id).is_some_and(|unique| unique.id == cursor.id) {
            return false;
        }
        let row_matches = match scope {
            CarriedInventoryScope::Personal => ctx
                .db
                .inventory_item()
                .id()
                .find(row_id)
                .is_some_and(|row| {
                    row.character_id == actor.id
                        && row.item_id == cursor.item_id
                        && row.quantity == 1
                }),
            CarriedInventoryScope::Party => ctx
                .db
                .party_inventory_item()
                .id()
                .find(row_id)
                .is_some_and(|row| {
                    actor.party_id.as_deref() == Some(row.party_id.as_str())
                        && row.item_id == cursor.item_id
                        && row.quantity == 1
                }),
        };
        if !row_matches {
            return false;
        }
        let parent = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(cursor.id)
            .map(|edge| edge.parent_object_id);
        let Some(parent_id) = parent else { return true };
        let Some(parent) = ctx.db.inventory_object().id().find(parent_id) else {
            return false;
        };
        cursor = parent;
    }
    false
}

fn view_direct_custody(
    ctx: &ViewContext,
    actor: &crate::Character,
    scope: CarriedInventoryScope,
    object: &crate::InventoryObject,
) -> Option<OperationalCustody> {
    if let Some(edge) = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(object.id)
    {
        return PhysicalObjectId::try_new(edge.parent_object_id)
            .ok()
            .map(OperationalCustody::Container);
    }
    match scope {
        CarriedInventoryScope::Personal => OperationalCustody::character(actor.id).ok(),
        CarriedInventoryScope::Party => OperationalCustody::party(actor.party_id.clone()?).ok(),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the projection mirrors the authoritative preparation identity coordinates"
)]
fn view_next_preparation_generation(
    ctx: &ViewContext,
    actor_id: u64,
    scope: &str,
    row_id: u64,
    lot_id: u64,
    object_id: u64,
    revision: u64,
    action: IngredientPreparationAction,
) -> Option<u64> {
    let key =
        preparation_attempt_state_key(actor_id, scope, row_id, lot_id, object_id, revision, action);
    match ctx
        .db
        .ingredient_preparation_attempt_state()
        .key()
        .find(&key)
    {
        Some(state) if state.completed => None,
        Some(state) => Some(state.next_generation),
        None => Some(0),
    }
}

#[view(accessor = backend_ingredient_preparation_plans, public)]
pub fn backend_ingredient_preparation_plans(
    ctx: &ViewContext,
) -> Vec<BackendIngredientPreparationPlan> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let actors = ctx
        .db
        .character()
        .scan_id()
        .filter(0u64..)
        .filter(|actor| {
            actor.alive
                && !actor.in_server
                && actor.current_settlement_id.is_some()
                && !actor.party_id.as_deref().is_some_and(|party_id| {
                    ctx.db
                        .strategic_encounter()
                        .party_id()
                        .find(party_id.to_string())
                        .is_some_and(|encounter| {
                            encounter.status == StrategicEncounterStatus::AwaitingChoice
                        })
                        || ctx
                            .db
                            .party_authority()
                            .id()
                            .find(party_id.to_string())
                            .is_some_and(|party| {
                                ctx.db
                                    .road_challenge_authority()
                                    .party_id()
                                    .filter(&party_id.to_string())
                                    .any(|challenge| {
                                        challenge.open
                                            && crate::strategic::party_at_bound_road_challenge_view(
                                                ctx, &party, &challenge,
                                            )
                                    })
                            })
                })
        })
        .collect::<Vec<_>>();
    let mut plans = Vec::new();
    for actor in actors {
        let Some(_) = ctx
            .db
            .character_time()
            .character_id()
            .find(actor.id)
            .map(|time| time.minutes)
        else {
            continue;
        };
        let Some(place) = actor.current_settlement_id.as_deref().and_then(|id| {
            adventuresim_core::strategic_place::StrategicPlaceId::settlement(id).ok()
        }) else {
            continue;
        };
        let carried = view_carried_item_rows(ctx, &actor);
        let cutting_weapon = view_cutting_weapon_binding(ctx, &actor);
        let grinding_tool = carried
            .iter()
            .filter(|(_, _, item_id)| item_id == "mortar_and_pestle")
            .map(|(scope, row_id, item_id)| format!("{}|{row_id}|{item_id}", scope.as_str()))
            .min()
            .unwrap_or_else(|| "hands".into());
        for lot in ctx
            .db
            .food_lot()
            .material_revision()
            .filter(1u64..)
            .filter(|lot| lot.material_revision > 0)
        {
            let row = if let Some(row_id) = lot.inventory_item_id
                && ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(row_id)
                    .is_some_and(|row| row.character_id == actor.id && row.quantity == 1)
            {
                Some((CarriedInventoryScope::Personal, row_id))
            } else if let Some(row_id) = lot.party_inventory_item_id
                && actor.party_id.as_deref().is_some_and(|party_id| {
                    ctx.db
                        .party_inventory_item()
                        .id()
                        .find(row_id)
                        .is_some_and(|row| row.party_id == party_id && row.quantity == 1)
                })
            {
                Some((CarriedInventoryScope::Party, row_id))
            } else {
                None
            };
            let Some((scope, row_id)) = row else { continue };
            let Some(object) = view_object_for_row(ctx, scope, row_id) else {
                continue;
            };
            if view_ancestry_reaches_fireplace(ctx, object.id)
                || !view_carried_custody_is_fully_resolved(ctx, &actor, scope, &object)
            {
                continue;
            }
            let Some(direct_custody) = view_direct_custody(ctx, &actor, scope, &object) else {
                continue;
            };
            let custody_binding = crate::object_custody::canonical_custody_binding(&direct_custody);
            let actions = match lot.preparation {
                FoodPreparation::Raw => [
                    cutting_weapon.clone().map(|tool_binding| {
                        (
                            IngredientPreparationAction::Cut,
                            Skill::Knife,
                            herbalism::PhysicalPreparation::Cut,
                            "Cut",
                            tool_binding,
                        )
                    }),
                    Some((
                        IngredientPreparationAction::Grind,
                        Skill::Bludgeon,
                        herbalism::PhysicalPreparation::Ground,
                        "Ground",
                        grinding_tool.clone(),
                    )),
                ],
                FoodPreparation::Cut => [
                    None,
                    Some((
                        IngredientPreparationAction::Grind,
                        Skill::Bludgeon,
                        herbalism::PhysicalPreparation::Ground,
                        "Ground",
                        grinding_tool.clone(),
                    )),
                ],
                _ => [None, None],
            };
            for (action, skill, physical, prefix, tool_binding) in actions.into_iter().flatten() {
                let Some(check) = view_preparation_skill_check(ctx, actor.id, skill) else {
                    continue;
                };
                let base_name = lot
                    .display_name
                    .trim_start_matches("Cut ")
                    .trim_start_matches("Ground ");
                let Some(attempt_generation) = view_next_preparation_generation(
                    ctx,
                    actor.id,
                    scope.as_str(),
                    row_id,
                    lot.id,
                    object.id,
                    lot.material_revision,
                    action,
                ) else {
                    continue;
                };
                let duration = herbalism::physical_preparation_minutes(
                    physical,
                    check,
                    tool_binding != "hands",
                );
                plans.push(BackendIngredientPreparationPlan {
                    actor_character_id: actor.id,
                    inventory_scope: scope.as_str().into(),
                    inventory_item_id: row_id,
                    food_lot_id: lot.id,
                    material_object_id: object.id,
                    request_id: preparation_request_id(
                        actor.id,
                        scope.as_str(),
                        row_id,
                        lot.id,
                        object.id,
                        lot.material_revision,
                        action,
                        attempt_generation,
                        &place.to_string(),
                        &custody_binding,
                    ),
                    expected_revision: lot.material_revision,
                    attempt_generation,
                    action,
                    duration_minutes: duration,
                    next_display_name: format!("{prefix} {base_name}"),
                });
            }
        }
    }
    plans
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceStation {
    pub key: String,
    pub character_id: u64,
    pub fireplace_fixture_id: String,
    pub instrument_item_id: Option<String>,
    pub instrument_object_id: Option<u64>,
}

#[view(accessor = backend_fireplace_stations, public)]
pub fn backend_fireplace_stations(ctx: &ViewContext) -> Vec<BackendFireplaceStation> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_station()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceStation {
            key: row.key,
            character_id: row.character_id,
            fireplace_fixture_id: row.fireplace_fixture_id,
            instrument_item_id: row.instrument_item_id,
            instrument_object_id: row.instrument_object_id,
        })
        .collect()
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceDish {
    pub station_key: String,
    pub character_id: u64,
    pub fireplace_fixture_id: String,
    pub contributor_name: String,
    pub method: CookingMethod,
    pub started_at_minute: u64,
    pub target_minutes: u32,
    pub display_name: String,
}

#[view(accessor = backend_fireplace_dishes, public)]
pub fn backend_fireplace_dishes(ctx: &ViewContext) -> Vec<BackendFireplaceDish> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_dish()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceDish {
            station_key: row.station_key,
            character_id: row.character_id,
            fireplace_fixture_id: row.fireplace_fixture_id,
            contributor_name: row.contributor_name,
            method: row.method,
            started_at_minute: row.started_at_minute,
            target_minutes: row.target_minutes,
            display_name: row.display_name,
        })
        .collect()
}
