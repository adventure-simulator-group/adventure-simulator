//! Private relational organization-role authority.
//!
//! A character may belong to many organization instances, but has exactly one
//! current role in each. Social position is never collapsed into an individual
//! estate scalar.

use adventuresim_core::organization::{
    self, OrganizationKind, OrganizationRoleDefinition, initial_social_role,
};
use spacetimedb::{ReducerContext, Table, ViewContext, table};

use crate::character::character;

#[derive(Clone, Debug)]
#[table(accessor = social_organization_instance)]
pub struct SocialOrganizationInstance {
    #[primary_key]
    pub id: String,
    pub definition_id: String,
    pub settlement_id: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = character_organization_role)]
pub struct CharacterOrganizationRole {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub organization_instance_id: String,
    pub role_id: String,
}

fn settlement_organization_instance_id(definition_id: &str, settlement_id: &str) -> Option<String> {
    let prefix = match definition_id {
        "settlement_civic_community" => "civic",
        "local_noble_house" => "noble-house",
        "local_lordship" => "lordship",
        _ => return None,
    };
    Some(format!("{prefix}:{settlement_id}"))
}

pub(crate) fn ensure_instance(
    ctx: &ReducerContext,
    definition_id: &str,
    settlement_id: Option<&str>,
) -> Result<String, String> {
    let definition =
        organization::organization(definition_id).ok_or("Unknown organization definition")?;
    let id = if let Some(settlement_id) = settlement_id {
        if !matches!(
            definition.kind,
            OrganizationKind::CivicCommunity
                | OrganizationKind::NobleHouse
                | OrganizationKind::Lordship
                | OrganizationKind::Family
        ) {
            return Err("This organization kind cannot be settlement-scoped".into());
        }

        settlement_organization_instance_id(definition_id, settlement_id)
            .ok_or("Only local social templates may be settlement-scoped")?
    } else {
        if settlement_organization_instance_id(definition_id, "").is_some() {
            return Err("Local social templates require a settlement".into());
        }
        definition_id.to_owned()
    };
    let expected = SocialOrganizationInstance {
        id: id.clone(),
        definition_id: definition_id.to_owned(),
        settlement_id: settlement_id.map(str::to_owned),
    };
    if let Some(existing) = ctx.db.social_organization_instance().id().find(&id) {
        if existing.definition_id != expected.definition_id
            || existing.settlement_id != expected.settlement_id
        {
            return Err("Organization instance does not match its definition or settlement".into());
        }
    } else {
        ctx.db.social_organization_instance().insert(expected);
    }
    Ok(id)
}

pub fn ensure_settlement_social_organizations(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    for definition_id in [
        "settlement_civic_community",
        "local_noble_house",
        "local_lordship",
    ] {
        ensure_instance(ctx, definition_id, Some(settlement_id))?;
    }
    Ok(())
}

fn assigned_role(
    ctx: &ReducerContext,
    instance_id: &str,
    role_id: &str,
) -> Result<&'static OrganizationRoleDefinition, String> {
    let instance = ctx
        .db
        .social_organization_instance()
        .id()
        .find(instance_id.to_owned())
        .ok_or("Unknown organization instance")?;
    let definition = organization::organization(&instance.definition_id)
        .ok_or("Organization instance references an unknown definition")?;
    definition.role(role_id).ok_or_else(|| {
        format!(
            "Unknown role {role_id:?} for organization {}",
            definition.id
        )
    })
}

pub(crate) fn character_assignment_id(character_id: u64, instance_id: &str) -> String {
    format!("character:{character_id}:{instance_id}")
}

/// Materialize one durable birth-family identity. The family key is stable and
/// deliberately independent of the household in which its members presently
/// live. Noble and common families use different authored role catalogs; no
/// individual noble flag is stored.
pub fn ensure_character_family_role(
    ctx: &ReducerContext,
    character_id: u64,
    family_key: &str,
    noble: bool,
) -> Result<(), String> {
    if family_key.is_empty() || family_key.chars().any(char::is_control) {
        return Err("Family identity key is invalid".into());
    }
    let instance_id = format!("family:{family_key}");
    let provisional = ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .filter(|assignment| {
            assignment
                .organization_instance_id
                .starts_with("family:origin:")
                && assignment.organization_instance_id != instance_id
        })
        .collect::<Vec<_>>();
    for assignment in provisional {
        let old_instance = assignment.organization_instance_id.clone();
        ctx.db
            .character_organization_role()
            .id()
            .delete(&assignment.id);
        delete_organization_instance_if_unreferenced(ctx, &old_instance);
    }
    let definition_id = if noble {
        "local_noble_house"
    } else {
        "common_family"
    };
    let expected = SocialOrganizationInstance {
        id: instance_id.clone(),
        definition_id: definition_id.into(),
        settlement_id: None,
    };
    if let Some(existing) = ctx
        .db
        .social_organization_instance()
        .id()
        .find(&instance_id)
    {
        if existing.definition_id != expected.definition_id {
            return Err("Family organization conflicts with its established kind".into());
        }
    } else {
        ctx.db.social_organization_instance().insert(expected);
    }
    insert_character_role(
        ctx,
        character_id,
        &instance_id,
        if noble {
            "house_member"
        } else {
            "family_member"
        },
    )?;
    Ok(())
}

pub fn copy_birth_family_roles(
    ctx: &ReducerContext,
    parent_id: u64,
    child_id: u64,
) -> Result<(), String> {
    let provisional = ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(child_id)
        .filter(|assignment| {
            assignment
                .organization_instance_id
                .starts_with("family:origin:")
        })
        .collect::<Vec<_>>();
    for assignment in provisional {
        let old_instance = assignment.organization_instance_id.clone();
        ctx.db
            .character_organization_role()
            .id()
            .delete(&assignment.id);
        delete_organization_instance_if_unreferenced(ctx, &old_instance);
    }
    let families = ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(parent_id)
        .filter(|assignment| assignment.organization_instance_id.starts_with("family:"))
        .collect::<Vec<_>>();
    if families.is_empty() {
        return Err("Parent has no durable birth-family organization".into());
    }
    for family in families {
        insert_character_role(
            ctx,
            child_id,
            &family.organization_instance_id,
            &family.role_id,
        )?;
    }
    Ok(())
}

pub fn insert_character_role(
    ctx: &ReducerContext,
    character_id: u64,
    instance_id: &str,
    role_id: &str,
) -> Result<CharacterOrganizationRole, String> {
    if ctx.db.character().id().find(character_id).is_none() {
        return Err("Role assignment references an unknown character".into());
    }
    assigned_role(ctx, instance_id, role_id)?;
    let id = character_assignment_id(character_id, instance_id);
    if let Some(existing) = ctx.db.character_organization_role().id().find(&id) {
        return if existing.role_id == role_id {
            Ok(existing)
        } else {
            Err("Character already has a different role in this organization instance".into())
        };
    }
    if ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .any(|row| row.organization_instance_id == instance_id)
    {
        return Err("Duplicate character organization role".into());
    }
    Ok(ctx
        .db
        .character_organization_role()
        .insert(CharacterOrganizationRole {
            id,
            character_id,
            organization_instance_id: instance_id.to_owned(),
            role_id: role_id.to_owned(),
        }))
}

pub fn update_character_role(
    ctx: &ReducerContext,
    character_id: u64,
    instance_id: &str,
    role_id: &str,
) -> Result<CharacterOrganizationRole, String> {
    assigned_role(ctx, instance_id, role_id)?;
    let id = character_assignment_id(character_id, instance_id);
    let mut assignment = ctx
        .db
        .character_organization_role()
        .id()
        .find(&id)
        .ok_or("Character has no role in this organization instance")?;
    assignment.role_id = role_id.to_owned();
    Ok(ctx.db.character_organization_role().id().update(assignment))
}

pub fn ensure_character_organization_role(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
    role_id: &str,
) -> Result<CharacterOrganizationRole, String> {
    let instance_id = ensure_instance(ctx, organization_id, None)?;
    insert_character_role(ctx, character_id, &instance_id, role_id)
}

pub fn assigned_organization_role(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
) -> Result<CharacterOrganizationRole, String> {
    ctx.db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .find(|assignment| assignment.organization_instance_id == organization_id)
        .ok_or_else(|| "Character has no role in this organization".into())
}

pub fn ensure_character_social_roles(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    urban: bool,
) -> Result<(), String> {
    let selected = initial_social_role(&format!("character:{character_id}"), settlement_id, urban);
    let instance_id = ensure_instance(
        ctx,
        selected.definition_id,
        selected.settlement_scoped.then_some(settlement_id),
    )?;
    insert_character_role(ctx, character_id, &instance_id, selected.role_id)?;
    ensure_character_family_role(
        ctx,
        character_id,
        &format!("origin:{character_id}"),
        selected.role_id == "house_member",
    )?;
    Ok(())
}

pub(crate) fn religious_organization_for(religion_id: &str) -> Option<&'static str> {
    Some(match religion_id {
        "roman_catholic" => "roman_catholic_learned_chapter",
        "lutheran" => "lutheran_learned_visitation",
        "reformed" => "reformed_learned_chapter",
        "anglican" => "anglican_learned_fellowship",
        "eastern_orthodox" => "orthodox_learned_brotherhood",
        "islamic" => "islamic_learned_fellowship",
        "judaism" => "jewish_learned_fellowship",
        _ => return None,
    })
}

pub fn ensure_character_professional_role(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
    role_id: &str,
) -> Result<(), String> {
    let Some(definition) = organization::organization(organization_id) else {
        return Err("Unknown professional organization".into());
    };
    if definition.role(role_id).is_none() {
        return Err("Professional role is not in its organization".into());
    }
    ensure_character_organization_role(ctx, character_id, organization_id, role_id)?;
    Ok(())
}

pub fn character_roles(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Vec<&'static OrganizationRoleDefinition>, String> {
    ctx.db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .map(|assignment| {
            assigned_role(
                ctx,
                &assignment.organization_instance_id,
                &assignment.role_id,
            )
        })
        .collect()
}

/// Precedence of the same publicly recognizable role that wins address/title
/// presentation. A clerical role can therefore overwrite noble or civic
/// standing instead of mixing one role's title with another role's precedence.
pub fn character_social_precedence_view(ctx: &ViewContext, character_id: u64) -> i16 {
    ctx.db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .filter_map(|assignment| {
            let instance = ctx
                .db
                .social_organization_instance()
                .id()
                .find(&assignment.organization_instance_id)?;
            organization::organization(&instance.definition_id)?.role(&assignment.role_id)
        })
        .filter(|role| role.publicly_recognizable && !role.address_title.is_empty())
        .max_by_key(|role| (role.address_priority, role.id.as_str()))
        .map(|role| role.social_precedence)
        .unwrap_or_default()
}

pub fn character_has_profession(
    ctx: &ReducerContext,
    character_id: u64,
    profession: &str,
) -> Result<bool, String> {
    Ok(character_roles(ctx, character_id)?
        .into_iter()
        .any(|role| role.profession == profession))
}

pub fn character_creation_literacy(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Option<adventuresim_world_schema::WrittenLanguage>, String> {
    Ok(character_roles(ctx, character_id)?
        .into_iter()
        .filter(|role| role.creation_literacy.is_some())
        .max_by_key(|role| (role.address_priority, role.id.as_str()))
        .and_then(|role| role.creation_literacy))
}

pub fn delete_character_social_roles(ctx: &ReducerContext, character_id: u64) {
    let roles = ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>();
    let instance_ids = roles
        .iter()
        .map(|role| role.organization_instance_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for role in roles {
        ctx.db.character_organization_role().id().delete(&role.id);
    }
    for instance_id in instance_ids {
        delete_organization_instance_if_unreferenced(ctx, &instance_id);
    }
}

fn delete_organization_instance_if_unreferenced(ctx: &ReducerContext, instance_id: &str) {
    let referenced_by_character = ctx
        .db
        .character_organization_role()
        .iter()
        .any(|role| role.organization_instance_id == instance_id);
    let instance_id = instance_id.to_owned();
    if !referenced_by_character
        && ctx
            .db
            .social_organization_instance()
            .id()
            .find(&instance_id)
            .is_some()
    {
        ctx.db
            .social_organization_instance()
            .id()
            .delete(&instance_id);
    }
}

pub fn delete_unreferenced_settlement_social_organizations(
    ctx: &ReducerContext,
    settlement_id: &str,
) {
    for definition_id in [
        "settlement_civic_community",
        "local_noble_house",
        "local_lordship",
    ] {
        let instance_id = settlement_organization_instance_id(definition_id, settlement_id)
            .expect("listed definitions are settlement-scoped templates");
        delete_organization_instance_if_unreferenced(ctx, &instance_id);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn assignment_key_enforces_one_role_per_organization_instance() {
        let source = crate::production_source(include_str!("social_roles.rs"));
        let insertion = source
            .split("fn insert_character_role")
            .nth(1)
            .unwrap()
            .split("pub fn ensure_character_social_roles")
            .next()
            .unwrap();
        assert!(source.contains("character:{character_id}:{instance_id}"));
        assert!(insertion.contains("existing.role_id == role_id"));
        assert!(insertion.contains("character_organization_role().id().find(&id)"));
        assert!(insertion.contains("Err("));
    }

    #[test]
    fn family_identity_is_stable_and_newborns_copy_birth_family_roles() {
        let source = crate::production_source(include_str!("social_roles.rs"));
        let family = source
            .split("pub fn ensure_character_family_role")
            .nth(1)
            .unwrap()
            .split("pub fn copy_birth_family_roles")
            .next()
            .unwrap();
        assert!(family.contains("family:{family_key}"));
        assert!(family.contains("local_noble_house"));
        assert!(family.contains("common_family"));
        let birth = source
            .split("pub fn copy_birth_family_roles")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn religious_organization_for")
            .next()
            .unwrap();
        assert!(birth.contains("starts_with(\"family:\")"));
        assert!(birth.contains("insert_character_role"));
    }

    #[test]
    fn gateway_precedence_comes_from_the_address_winning_public_role() {
        let source = crate::production_source(include_str!("social_roles.rs"));
        let projection = source
            .split("pub fn character_social_precedence_view")
            .nth(1)
            .unwrap()
            .split("pub fn character_has_profession")
            .next()
            .unwrap();
        assert!(projection.contains("role.publicly_recognizable"));
        assert!(projection.contains("role.social_precedence"));
        assert!(projection.contains("address_priority"));
    }
}
