//! Private relational social-estate authority.
//!
//! Estate is never stored on an actor. It is derived from exactly one
//! estate-bearing organization-role assignment selected as that actor's basis.

use adventuresim_core::organization::{
    self, Estate, OrganizationKind, OrganizationRoleDefinition, OrganizationRolePurpose,
    initial_social_role,
};
use spacetimedb::{ReducerContext, Table, table};

use crate::{character::character, settlement_population::settlement_npc};

pub const CHURCH_DEFINITION_ID: &str = "roman_catholic_church";

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

#[derive(Clone, Debug)]
#[table(accessor = character_estate_basis)]
pub struct CharacterEstateBasis {
    #[primary_key]
    pub character_id: u64,
    #[unique]
    pub assignment_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_npc_organization_role)]
pub struct SettlementNpcOrganizationRole {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub npc_id: String,
    pub organization_instance_id: String,
    pub role_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_npc_estate_basis)]
pub struct SettlementNpcEstateBasis {
    #[primary_key]
    pub npc_id: String,
    #[unique]
    pub assignment_id: String,
}

pub fn civic_organization_instance_id(settlement_id: &str) -> String {
    format!("civic:{settlement_id}")
}

fn ensure_instance(
    ctx: &ReducerContext,
    definition_id: &str,
    settlement_id: Option<&str>,
) -> Result<String, String> {
    let definition =
        organization::organization(definition_id).ok_or("Unknown organization definition")?;
    let id = if definition.kind == OrganizationKind::CivicCommunity {
        let settlement_id = settlement_id.ok_or("Civic organizations require a settlement")?;
        civic_organization_instance_id(settlement_id)
    } else {
        if settlement_id.is_some() {
            return Err("Only civic organizations may be settlement-scoped".into());
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

fn assigned_role(
    ctx: &ReducerContext,
    instance_id: &str,
    role_id: &str,
) -> Result<&'static OrganizationRoleDefinition, String> {
    let instance = ctx
        .db
        .social_organization_instance()
        .id()
        .find(&instance_id.to_owned())
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

fn role_estate(role: &OrganizationRoleDefinition) -> Result<Estate, String> {
    match role.purpose {
        OrganizationRolePurpose::Estate { estate } => Ok(estate),
        _ => Err("An estate basis must reference an estate-bearing role".into()),
    }
}

fn character_assignment_id(character_id: u64, instance_id: &str, role_id: &str) -> String {
    format!("character:{character_id}:{instance_id}:{role_id}")
}

fn npc_assignment_id(npc_id: &str, instance_id: &str, role_id: &str) -> String {
    format!("npc:{npc_id}:{instance_id}:{role_id}")
}

fn insert_character_role(
    ctx: &ReducerContext,
    character_id: u64,
    instance_id: &str,
    role_id: &str,
) -> Result<CharacterOrganizationRole, String> {
    if ctx.db.character().id().find(character_id).is_none() {
        return Err("Role assignment references an unknown character".into());
    }
    assigned_role(ctx, instance_id, role_id)?;
    let id = character_assignment_id(character_id, instance_id, role_id);
    if let Some(existing) = ctx.db.character_organization_role().id().find(&id) {
        return Ok(existing);
    }
    if ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .any(|row| row.organization_instance_id == instance_id && row.role_id == role_id)
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

fn insert_npc_role(
    ctx: &ReducerContext,
    npc_id: &str,
    instance_id: &str,
    role_id: &str,
) -> Result<SettlementNpcOrganizationRole, String> {
    if ctx
        .db
        .settlement_npc()
        .id()
        .find(&npc_id.to_owned())
        .is_none()
    {
        return Err("Role assignment references an unknown settlement NPC".into());
    }
    assigned_role(ctx, instance_id, role_id)?;
    let id = npc_assignment_id(npc_id, instance_id, role_id);
    if let Some(existing) = ctx.db.settlement_npc_organization_role().id().find(&id) {
        return Ok(existing);
    }
    if ctx
        .db
        .settlement_npc_organization_role()
        .npc_id()
        .filter(&npc_id.to_owned())
        .any(|row| row.organization_instance_id == instance_id && row.role_id == role_id)
    {
        return Err("Duplicate settlement NPC organization role".into());
    }
    Ok(ctx
        .db
        .settlement_npc_organization_role()
        .insert(SettlementNpcOrganizationRole {
            id,
            npc_id: npc_id.to_owned(),
            organization_instance_id: instance_id.to_owned(),
            role_id: role_id.to_owned(),
        }))
}

pub fn ensure_character_social_roles(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    urban: bool,
) -> Result<(), String> {
    if let Some(existing) = ctx
        .db
        .character_estate_basis()
        .character_id()
        .find(character_id)
    {
        character_estate_from_basis(ctx, &existing)?;
        return Ok(());
    }
    let selected = initial_social_role(&format!("character:{character_id}"), urban);
    let instance_id = ensure_instance(
        ctx,
        selected.definition_id,
        selected.settlement_scoped.then_some(settlement_id),
    )?;
    let assignment = insert_character_role(ctx, character_id, &instance_id, selected.role_id)?;
    if role_estate(assigned_role(ctx, &instance_id, selected.role_id)?)? != selected.estate {
        return Err("Selected role does not derive its expected estate".into());
    }
    ctx.db
        .character_estate_basis()
        .insert(CharacterEstateBasis {
            character_id,
            assignment_id: assignment.id,
        });
    Ok(())
}

pub fn ensure_settlement_npc_social_roles(
    ctx: &ReducerContext,
    npc_id: &str,
    settlement_id: &str,
    urban: bool,
    profession: &str,
) -> Result<(), String> {
    if ctx
        .db
        .settlement_npc_estate_basis()
        .npc_id()
        .find(&npc_id.to_owned())
        .is_none()
    {
        let selected = initial_social_role(&format!("settlement-npc:{npc_id}"), urban);
        let instance_id = ensure_instance(
            ctx,
            selected.definition_id,
            selected.settlement_scoped.then_some(settlement_id),
        )?;
        let assignment = insert_npc_role(ctx, npc_id, &instance_id, selected.role_id)?;
        if role_estate(assigned_role(ctx, &instance_id, selected.role_id)?)? != selected.estate {
            return Err("Selected role does not derive its expected estate".into());
        }
        ctx.db
            .settlement_npc_estate_basis()
            .insert(SettlementNpcEstateBasis {
                npc_id: npc_id.to_owned(),
                assignment_id: assignment.id,
            });
    } else {
        settlement_npc_estate(ctx, npc_id)?;
    }

    if profession == "cleric" {
        let church = ensure_instance(ctx, CHURCH_DEFINITION_ID, None)?;
        insert_npc_role(ctx, npc_id, &church, "priest")?;
    }
    Ok(())
}

fn character_estate_from_basis(
    ctx: &ReducerContext,
    basis: &CharacterEstateBasis,
) -> Result<Estate, String> {
    let assignment = ctx
        .db
        .character_organization_role()
        .id()
        .find(&basis.assignment_id)
        .ok_or("Character estate basis references a missing assignment")?;
    if assignment.character_id != basis.character_id {
        return Err("Character estate basis references another actor's assignment".into());
    }
    role_estate(assigned_role(
        ctx,
        &assignment.organization_instance_id,
        &assignment.role_id,
    )?)
}

pub fn character_estate(ctx: &ReducerContext, character_id: u64) -> Result<Estate, String> {
    let basis = ctx
        .db
        .character_estate_basis()
        .character_id()
        .find(character_id)
        .ok_or("Character has no estate basis")?;
    character_estate_from_basis(ctx, &basis)
}

pub fn settlement_npc_estate(ctx: &ReducerContext, npc_id: &str) -> Result<Estate, String> {
    let basis = ctx
        .db
        .settlement_npc_estate_basis()
        .npc_id()
        .find(&npc_id.to_owned())
        .ok_or("Settlement NPC has no estate basis")?;
    let assignment = ctx
        .db
        .settlement_npc_organization_role()
        .id()
        .find(&basis.assignment_id)
        .ok_or("Settlement NPC estate basis references a missing assignment")?;
    if assignment.npc_id != basis.npc_id {
        return Err("Settlement NPC estate basis references another actor's assignment".into());
    }
    role_estate(assigned_role(
        ctx,
        &assignment.organization_instance_id,
        &assignment.role_id,
    )?)
}

pub fn delete_character_social_roles(ctx: &ReducerContext, character_id: u64) {
    if ctx
        .db
        .character_estate_basis()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .character_estate_basis()
            .character_id()
            .delete(character_id);
    }
    for role in ctx
        .db
        .character_organization_role()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.character_organization_role().id().delete(&role.id);
    }
}

pub fn delete_settlement_npc_social_roles(ctx: &ReducerContext, npc_id: &str) {
    let npc_id = npc_id.to_owned();
    if ctx
        .db
        .settlement_npc_estate_basis()
        .npc_id()
        .find(&npc_id)
        .is_some()
    {
        ctx.db
            .settlement_npc_estate_basis()
            .npc_id()
            .delete(&npc_id);
    }
    for role in ctx
        .db
        .settlement_npc_organization_role()
        .npc_id()
        .filter(&npc_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .settlement_npc_organization_role()
            .id()
            .delete(&role.id);
    }
}
