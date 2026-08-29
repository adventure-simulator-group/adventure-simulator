// Owns kinship, household membership, birth records, and effective age.
fn kinship_id(subject_id: u64, related_id: u64, kind: KinshipKind) -> String {
    format!("kinship:{subject_id}:{related_id}:{}", kind.stable_id())
}

fn ensure_kinship(
    ctx: &ReducerContext,
    subject_id: u64,
    related_id: u64,
    kind: KinshipKind,
    minute: u64,
) {
    let id = kinship_id(subject_id, related_id, kind);
    if ctx.db.character_kinship().id().find(&id).is_none() {
        ctx.db.character_kinship().insert(CharacterKinship {
            id,
            subject_id,
            related_id,
            kind,
            established_minute: minute,
        });
    }
}

fn leave_household(ctx: &ReducerContext, character_id: u64) {
    if let Some(member) = ctx.db.household_member().character_id().find(character_id) {
        ctx.db.household_member().id().delete(&member.id);
    }
}

fn join_household(
    ctx: &ReducerContext,
    household_id: &str,
    character_id: u64,
    minute: u64,
    role: HouseholdRole,
) {
    if ctx
        .db
        .household_member()
        .character_id()
        .find(character_id)
        .is_some_and(|member| member.household_id == household_id)
    {
        return;
    }
    leave_household(ctx, character_id);
    ctx.db.household_member().insert(HouseholdMember {
        id: format!("household:{household_id}:{character_id}"),
        household_id: household_id.to_owned(),
        character_id,
        joined_minute: minute,
        role,
    });
}

pub fn record_character_birth(ctx: &ReducerContext, character_id: u64, birth_minute: i64) {
    if ctx
        .db
        .character_birth()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_birth().insert(CharacterBirth {
            character_id,
            birth_minute,
        });
    }
}

pub fn effective_age_years(ctx: &ReducerContext, character_id: u64, minute: u64) -> Option<u16> {
    let character = ctx.db.character().id().find(character_id)?;
    let Some(birth) = ctx.db.character_birth().character_id().find(character_id) else {
        return Some(character.age_years);
    };
    let elapsed = i128::from(minute).saturating_sub(i128::from(birth.birth_minute));
    Some((elapsed.max(0) as u128 / u128::from(MINUTES_PER_YEAR)).min(u128::from(u16::MAX)) as u16)
}

/// Refresh the cached display age from the authoritative birth coordinate.
/// Calling this at every lifecycle boundary naturally promotes dependents at
/// their yearly boundary without granting newborn starter equipment.
pub fn settle_character_age(ctx: &ReducerContext, character_id: u64, minute: u64) {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return;
    };
    let Some(age_years) = effective_age_years(ctx, character_id, minute) else {
        return;
    };
    if character.age_years != age_years {
        character.age_years = age_years;
        ctx.db.character().id().update(character);
    }
}

/// Turn the deterministic resident roster into coherent authoritative family
/// units. Each complete cohort is father, mother, adult daughter, adult son;
/// incomplete tails still receive one household and unique roles, but no
/// fabricated identities or kinship edges.
pub fn ensure_seeded_family_households(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    let mut residents: Vec<_> = ctx
        .db
        .npc_policy()
        .iter()
        .filter(|policy| policy.home_settlement_id == settlement_id)
        .map(|policy| policy.character_id)
        .collect();
    residents.sort_unstable();
    for (cohort, family) in residents.chunks(4).enumerate() {
        let household_id = format!("household:seeded:{settlement_id}:{cohort}");
        if ctx.db.household().id().find(&household_id).is_none() {
            ctx.db.household().insert(Household {
                id: household_id.clone(),
                home_settlement_id: settlement_id.to_owned(),
                created_minute: 0,
            });
        }
        let roles = [
            HouseholdRole::Head,
            HouseholdRole::Spouse,
            HouseholdRole::AdultChild,
            HouseholdRole::AdultChild,
        ];
        for (index, character_id) in family.iter().copied().enumerate() {
            join_household(ctx, &household_id, character_id, 0, roles[index]);
        }
        let family_key = format!("seeded:{settlement_id}:{cohort}");
        let noble = family.iter().copied().any(|character_id| {
            crate::social_roles::character_has_profession(ctx, character_id, "noble")
                .unwrap_or(false)
        });
        for character_id in family.iter().copied() {
            crate::social_roles::ensure_character_family_role(
                ctx,
                character_id,
                &family_key,
                noble,
            )?;
        }
        if family.len() < 4 {
            continue;
        }
        let assigned = [
            (family[0], Sex::Male, Presentation::Man, 52u16),
            (family[1], Sex::Female, Presentation::Woman, 48u16),
            (family[2], Sex::Female, Presentation::Woman, 24u16),
            (family[3], Sex::Male, Presentation::Man, 21u16),
        ];
        for (character_id, sex, presentation, age) in assigned {
            let mut character = ctx
                .db
                .character()
                .id()
                .find(character_id)
                .ok_or("Seeded family member is missing its Character")?;
            character.age_years = age;
            ctx.db.character().id().update(character);
            let mut personality = ctx
                .db
                .character_personality()
                .character_id()
                .find(character_id)
                .ok_or("Seeded family member is missing personality")?;
            personality.sex = sex;
            personality.presentation = presentation;
            ctx.db
                .character_personality()
                .character_id()
                .update(personality);
            let birth = CharacterBirth {
                character_id,
                birth_minute: -(i64::from(age)
                    * i64::try_from(MINUTES_PER_YEAR).unwrap_or(i64::MAX)),
            };
            if ctx
                .db
                .character_birth()
                .character_id()
                .find(character_id)
                .is_some()
            {
                ctx.db.character_birth().character_id().update(birth);
            } else {
                ctx.db.character_birth().insert(birth);
            }
        }
        for child in [family[2], family[3]] {
            for parent in [family[0], family[1]] {
                ensure_kinship(ctx, child, parent, KinshipKind::Parent, 0);
                ensure_kinship(ctx, parent, child, KinshipKind::Child, 0);
            }
        }
        ensure_kinship(ctx, family[2], family[3], KinshipKind::Sibling, 0);
        ensure_kinship(ctx, family[3], family[2], KinshipKind::Sibling, 0);
    }
    Ok(())
}

fn father_of_at(ctx: &ReducerContext, child_id: u64, minute: u64) -> Result<Option<u64>, String> {
    let father = ctx.db.character_kinship().iter().find_map(|edge| {
        (edge.subject_id == child_id
            && edge.kind == KinshipKind::Parent
            && edge.established_minute <= minute)
            .then(|| {
                ctx.db
                    .character_personality()
                    .character_id()
                    .find(edge.related_id)
                    .filter(|personality| personality.sex == Sex::Male)
                    .map(|_| edge.related_id)
            })
            .flatten()
    });
    let Some(father) = father else {
        return Ok(None);
    };
    if canonical_now(ctx, father)? != minute {
        return Err("The prospective bride's father has not reached the relationship date".into());
    }
    Ok(character_alive_at(ctx, father, minute).then_some(father))
}

fn relationship_conflicts_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
    permitted_courtship_id: Option<&str>,
) -> bool {
    let courtship_conflict = ctx.db.courtship().iter().any(|row| {
        (row.first_character_id == character_id || row.second_character_id == character_id)
            && Some(row.id.as_str()) != permitted_courtship_id
            && row.started_minute <= minute
            && row.resolved_minute.is_none_or(|resolved| resolved > minute)
    });
    let commitment_conflict = ctx.db.exclusive_commitment().iter().any(|row| {
        (row.first_character_id == character_id || row.second_character_id == character_id)
            && row.created_minute <= minute
            && row.resolved_minute.is_none_or(|resolved| resolved > minute)
    });
    let marriage_conflict = ctx.db.marriage().iter().any(|row| {
        (row.first_character_id == character_id || row.second_character_id == character_id)
            && row.married_minute <= minute
            && row.resolved_minute.is_none_or(|resolved| resolved > minute)
    });
    courtship_conflict || commitment_conflict || marriage_conflict
}

fn formal_dowry_amount(father_wealth: u64) -> u32 {
    if father_wealth >= 300 {
        100
    } else if father_wealth >= 100 {
        45
    } else if father_wealth >= 30 {
        15
    } else {
        0
    }
}
