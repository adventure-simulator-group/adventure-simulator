pub(crate) async fn soap_rest_preview(
    state: &AppState,
    members: &[Character],
    party_id: Option<&str>,
) -> SoapRestPreview {
    let (filth, personal, shared, personal_amounts, party_amounts, definitions, personalities) = tokio::join!(
        state
            .db
            .query::<CharacterFilth>("SELECT * FROM character_filth"),
        state
            .db
            .query::<InventoryItem>("SELECT * FROM inventory_item"),
        state
            .db
            .query::<PartyInventoryItem>("SELECT * FROM party_inventory_item"),
        state
            .db
            .query::<InventoryItemAmount>("SELECT * FROM inventory_item_amount"),
        state
            .db
            .query::<PartyItemAmount>("SELECT * FROM party_item_amount"),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        state
            .db
            .query::<CharacterPersonality>("SELECT * FROM backend_character_personalities"),
    );
    let personal = personal.unwrap_or_default();
    let shared = shared.unwrap_or_default();
    let personal_amounts = personal_amounts.unwrap_or_default();
    let party_amounts = party_amounts.unwrap_or_default();
    let mut preview = calculate_soap_rest_preview(
        members,
        &filth.unwrap_or_default(),
        &personal,
        &shared,
        &personal_amounts,
        &party_amounts,
        party_id,
    );
    calculate_rest_supply_availability(
        &mut preview,
        members,
        &personal,
        &shared,
        &personal_amounts,
        &party_amounts,
        &definitions.unwrap_or_default(),
        &personalities.unwrap_or_default(),
        party_id,
    );
    preview
}

pub(super) fn calculate_rest_supply_availability(
    preview: &mut SoapRestPreview,
    members: &[Character],
    personal: &[InventoryItem],
    shared: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
    definitions: &[ItemDefinition],
    personalities: &[CharacterPersonality],
    party_id: Option<&str>,
) {
    const SOAP_ITEM_ID: &str = "soft_soap";
    let living_ids = members
        .iter()
        .filter(|member| member.alive)
        .map(|member| member.id)
        .collect::<std::collections::BTreeSet<_>>();
    let is_temperate = |character_id| {
        personalities
            .iter()
            .find(|personality| personality.character_id == character_id)
            .is_some_and(|personality| {
                personality.temperance == crate::spacetimedb::Temperance::Temperate
            })
    };
    let alcoholic_ids = definitions
        .iter()
        .filter(|item| {
            item.alcohol_potable
                && item.alcohol_serving_ml > 0
                && item.alcohol_abv_basis_points > 0
                && !item.alcohol_disinfectant_focused
        })
        .map(|item| item.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let personal_soap = personal
        .iter()
        .filter(|stack| living_ids.contains(&stack.character_id) && stack.item_id == SOAP_ITEM_ID)
        .map(|stack| {
            personal_amounts
                .iter()
                .find(|state| state.inventory_item_id == stack.id)
                .map_or(0, |state| {
                    state.remaining_milliunits
                        / (1_000_000 / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                })
        })
        .sum::<u32>();
    let shared_soap = party_id.map_or(0, |party_id| {
        shared
            .iter()
            .filter(|stack| stack.party_id == party_id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                party_amounts
                    .iter()
                    .find(|state| state.party_inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum::<u32>()
    });
    preview.available_units = personal_soap.saturating_add(shared_soap);

    let personal_alcohol = personal.iter().any(|stack| {
        living_ids.contains(&stack.character_id)
            && personal_amounts
                .iter()
                .any(|state| state.inventory_item_id == stack.id && state.remaining_milliunits > 0)
            && alcoholic_ids.contains(stack.item_id.as_str())
    });
    let personal_drink = personal.iter().any(|stack| {
        living_ids.contains(&stack.character_id)
            && !is_temperate(stack.character_id)
            && personal_amounts
                .iter()
                .any(|state| state.inventory_item_id == stack.id && state.remaining_milliunits > 0)
            && alcoholic_ids.contains(stack.item_id.as_str())
    });
    let shared_alcohol = party_id.is_some_and(|party_id| {
        shared.iter().any(|stack| {
            stack.party_id == party_id
                && party_amounts.iter().any(|state| {
                    state.party_inventory_item_id == stack.id && state.remaining_milliunits > 0
                })
                && alcoholic_ids.contains(stack.item_id.as_str())
        })
    });
    let has_non_temperate_member = living_ids
        .iter()
        .any(|character_id| !is_temperate(*character_id));
    preview.alcohol_available = personal_alcohol || shared_alcohol;
    preview.alcohol_will_be_consumed =
        personal_drink || (shared_alcohol && has_non_temperate_member);
}

pub(super) fn calculate_soap_rest_preview(
    members: &[Character],
    filth: &[CharacterFilth],
    personal: &[InventoryItem],
    shared: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
    party_id: Option<&str>,
) -> SoapRestPreview {
    const SOAP_ITEM_ID: &str = "soft_soap";
    let mut personal_units = 0_u32;
    let mut need_after_personal = 0_u32;
    for member in members.iter().filter(|member| member.alive) {
        let amount = filth
            .iter()
            .filter(|deposit| deposit.character_id == member.id)
            .map(|deposit| u32::from(deposit.amount))
            .sum::<u32>();
        let needed = amount;
        let available = personal
            .iter()
            .filter(|stack| stack.character_id == member.id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                personal_amounts
                    .iter()
                    .find(|state| state.inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum::<u32>();
        let used = needed.min(available);
        personal_units = personal_units.saturating_add(used);
        need_after_personal = need_after_personal.saturating_add(needed.saturating_sub(used));
    }
    let shared_available = party_id.map_or(0, |party_id| {
        shared
            .iter()
            .filter(|stack| stack.party_id == party_id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                party_amounts
                    .iter()
                    .find(|state| state.party_inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum()
    });
    let shared_units = need_after_personal.min(shared_available);
    SoapRestPreview {
        total_units: personal_units.saturating_add(shared_units),
        personal_units,
        shared_units,
        ..SoapRestPreview::default()
    }
}
