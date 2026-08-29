// Owns trusted-gateway relationship and courtship-discovery projections.
/// A deliberately actor-scoped summary for the trusted strategic gateway.
/// The underlying relationship, kinship, commitment, and pregnancy tables
/// remain private: the gateway filters this projection to the signed-in
/// character before presenting it to the browser.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterRelationshipStatus {
    pub character_id: u64,
    pub spouse_id: Option<u64>,
    pub courtship_partner_id: Option<u64>,
    pub courtship_kind: Option<CourtshipKind>,
    pub courtship_exposed: bool,
    pub wedding_commitment_id: Option<String>,
    pub wedding_partner_id: Option<u64>,
    pub wedding_effective_minute: Option<u64>,
    pub wedding_settlement_id: Option<String>,
    pub pregnancy_due_minute: Option<u64>,
    pub pregnancy_child_id: Option<u64>,
}

/// Observer-scoped knowledge of a discovered facade. The gateway may return a
/// row only to `observer_character_id`; partners and unrelated characters do
/// not learn which family member discovered the relationship.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCourtshipDiscoveryStatus {
    pub observer_character_id: u64,
    pub first_character_id: u64,
    pub second_character_id: u64,
    pub discovered_minute: u64,
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

/// Do not make the private relationship tables public merely for UI work.
/// The web gateway is the trust boundary and asks for the active character's
/// one-row summary; direct clients receive no rows at all.
#[view(accessor = backend_character_relationship_statuses, public)]
pub fn backend_character_relationship_statuses(
    ctx: &ViewContext,
) -> Vec<BackendCharacterRelationshipStatus> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    let mut character_ids = BTreeSet::new();
    for edge in ctx.db.character_kinship().subject_id().filter(0u64..) {
        character_ids.insert(edge.subject_id);
        character_ids.insert(edge.related_id);
    }
    for courtship in ctx.db.courtship().first_character_id().filter(0u64..) {
        character_ids.insert(courtship.first_character_id);
        character_ids.insert(courtship.second_character_id);
    }
    for pregnancy in ctx.db.pregnancy().father_id().filter(0u64..) {
        character_ids.insert(pregnancy.mother_id);
        character_ids.insert(pregnancy.father_id);
    }
    character_ids
        .into_iter()
        .filter_map(|character_id| {
            ctx.db.character().id().find(character_id).map(|character| {
                let observer_minute = ctx
                    .db
                    .character_time()
                    .character_id()
                    .find(character.id)
                    .map_or(0, |time| time.minutes);
                let spouse_id = ctx
                    .db
                    .marriage()
                    .first_character_id()
                    .filter(character.id)
                    .chain(ctx.db.marriage().second_character_id().filter(character.id))
                    .find(|marriage| {
                        (marriage.first_character_id == character.id
                            || marriage.second_character_id == character.id)
                            && marriage.married_minute <= observer_minute
                            && marriage
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    })
                    .map(|marriage| {
                        if marriage.first_character_id == character.id {
                            marriage.second_character_id
                        } else {
                            marriage.first_character_id
                        }
                    });
                let courtship = ctx
                    .db
                    .courtship()
                    .first_character_id()
                    .filter(character.id)
                    .find(|row| {
                        row.started_minute <= observer_minute
                            && row
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    })
                    .or_else(|| {
                        ctx.db
                            .courtship()
                            .second_character_id()
                            .filter(character.id)
                            .find(|row| {
                                row.started_minute <= observer_minute
                                    && row
                                        .resolved_minute
                                        .is_none_or(|resolved| resolved > observer_minute)
                            })
                    });
                let (courtship_partner_id, courtship_kind, courtship_exposed) =
                    courtship.map_or((None, None, false), |row| {
                        let exposed = ctx
                            .db
                            .courtship_discovery()
                            .courtship_id()
                            .filter(&row.id)
                            .any(|receipt| {
                                receipt.succeeded && receipt.attempted_minute <= observer_minute
                            });
                        (
                            Some(if row.first_character_id == character.id {
                                row.second_character_id
                            } else {
                                row.first_character_id
                            }),
                            Some(row.kind),
                            exposed,
                        )
                    });
                let active_pregnancy = ctx
                    .db
                    .pregnancy()
                    .mother_id()
                    .filter(character.id)
                    .chain(ctx.db.pregnancy().father_id().filter(character.id))
                    .find(|row| {
                        (row.mother_id == character.id || row.father_id == character.id)
                            && row.conceived_minute <= observer_minute
                            && row
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    });
                let born_child_id = ctx
                    .db
                    .pregnancy()
                    .mother_id()
                    .filter(character.id)
                    .chain(ctx.db.pregnancy().father_id().filter(character.id))
                    .filter(|row| {
                        (row.mother_id == character.id || row.father_id == character.id)
                            && row.status == PregnancyStatus::Born
                            && row.due_minute <= observer_minute
                    })
                    .max_by_key(|row| (row.due_minute, row.id.clone()))
                    .and_then(|row| row.birth_character_id);
                let wedding = ctx
                    .db
                    .exclusive_commitment_participant()
                    .character_id()
                    .find(character.id)
                    .and_then(|participant| {
                        ctx.db
                            .exclusive_commitment()
                            .id()
                            .find(&participant.commitment_id)
                    })
                    .filter(|commitment| {
                        commitment.created_minute <= observer_minute
                            && commitment
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                            && (commitment.first_character_id == character.id
                                || commitment.second_character_id == character.id)
                    });
                BackendCharacterRelationshipStatus {
                    character_id: character.id,
                    spouse_id,
                    courtship_partner_id,
                    courtship_kind,
                    courtship_exposed,
                    wedding_commitment_id: wedding.as_ref().map(|row| row.id.clone()),
                    wedding_partner_id: wedding.as_ref().map(|row| {
                        if row.first_character_id == character.id {
                            row.second_character_id
                        } else {
                            row.first_character_id
                        }
                    }),
                    wedding_effective_minute: wedding.as_ref().map(|row| row.effective_minute),
                    wedding_settlement_id: wedding
                        .as_ref()
                        .map(|row| row.ceremony_settlement_id.clone()),
                    pregnancy_due_minute: active_pregnancy.map(|row| row.due_minute),
                    pregnancy_child_id: born_child_id,
                }
            })
        })
        .collect()
}

/// Pairwise courtship lookup for other gateway projections. Keeping this
/// beside the private table avoids leaking rows or creating an accessor-name
/// collision in consumers that also read relationship views.
pub(crate) fn active_courtship_between_view(ctx: &ViewContext, left: u64, right: u64) -> bool {
    ctx.db
        .courtship()
        .first_character_id()
        .filter(left)
        .chain(ctx.db.courtship().first_character_id().filter(right))
        .any(|courtship| {
            ((courtship.first_character_id == left && courtship.second_character_id == right)
                || (courtship.first_character_id == right && courtship.second_character_id == left))
                && matches!(
                    courtship.status,
                    CourtshipStatus::Active | CourtshipStatus::Exposed
                )
        })
}

#[view(accessor = backend_courtship_discoveries, public)]
pub fn backend_courtship_discoveries(ctx: &ViewContext) -> Vec<BackendCourtshipDiscoveryStatus> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .courtship_discovery()
        .observer_id()
        .filter(0u64..)
        .filter(|receipt| receipt.succeeded)
        .filter_map(|receipt| {
            let observer_minute = ctx
                .db
                .character_time()
                .character_id()
                .find(receipt.observer_id)
                .map_or(0, |time| time.minutes);
            (receipt.attempted_minute <= observer_minute)
                .then(|| ctx.db.courtship().id().find(&receipt.courtship_id))
                .flatten()
                .map(|courtship| BackendCourtshipDiscoveryStatus {
                    observer_character_id: receipt.observer_id,
                    first_character_id: courtship.first_character_id,
                    second_character_id: courtship.second_character_id,
                    discovered_minute: receipt.attempted_minute,
                })
        })
        .collect()
}
