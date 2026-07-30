//! Durable strategic corpses and observer-scoped autopsy authority.
//!
//! Bodies store bounded physical outcomes, never tactical replay, attacker
//! identity, or a canonical cause-of-death answer.

use adventuresim_core::{
    autopsy::{
        CorpseLocation, DecompositionBand, PostCombatBody, corpse_location, decomposition_band,
        opening_quality_bps, post_combat_body,
    },
    autoresolve::BattleOutcome,
    prelude::{BodyPart, Skill},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    capability::character_capability,
    character::{character, character__view, character_limbs, character_skills},
    condition::character_condition,
    investigation::character_case_site_occupancy,
    settlement_population::{SettlementNpc, settlement_npc},
    social::settlement_npc_relationship,
    strategic::{strategic_encounter, strategic_gateway_authority__view},
    surgery::{limb_injury, retained_projectile},
    time::{advance_character_wait_time, character_time, character_time__view},
};

const EXTERNAL_EXAMINATION_MINUTES: u64 = 20;
const INTERNAL_EXAMINATION_MINUTES: u64 = 45;
const OPEN_BODY_MINUTES: u64 = 60;
const EXHUMATION_MINUTES: u64 = 120;
const BURIAL_MINUTES: u64 = 120;
const CREMATION_MINUTES: u64 = 240;
const CREMATION_INFAMY: i32 = 5_000;
const CREMATION_FAMILY_AFFINITY_DELTA: f32 = -40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CorpsePermissionKind {
    Family,
    Priest,
    SecularAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CorpsePermissionScope {
    Examination,
    Exhumation,
}

#[derive(Clone, Debug)]
#[table(accessor = strategic_corpse)]
pub struct StrategicCorpse {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub source_id: String,
    #[index(btree)]
    pub discovering_party_id: String,
    pub subject_character_id: Option<u64>,
    pub display_name: String,
    pub creature_kind: String,
    pub settlement_id: String,
    pub case_site_id: String,
    pub death_minute: u64,
    pub discovered_minute: u64,
    pub buried: bool,
    pub exhumed: bool,
    pub burned: bool,
    pub party_killed_enemy: bool,
    pub handling_damage_bps: u16,
    pub opened: bool,
    pub opening_quality_bps: u16,
    pub opening_obscuration_bps: u16,
    pub revision: u32,
}

#[derive(Clone, Debug)]
#[table(accessor = corpse_body_state)]
pub struct CorpseBodyState {
    #[primary_key]
    pub corpse_id: String,
    pub health: Vec<f32>,
    pub blood_loss_fraction: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = corpse_injury)]
pub struct CorpseInjury {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub corpse_id: String,
    pub sequence: u32,
    pub region: String,
    pub cut_damage: f32,
    pub blunt_damage: f32,
    pub projectile: bool,
    pub contact_stress: f32,
}

/// Explicit kinship authority; generated household labels are not kinship.
#[derive(Clone, Debug)]
#[table(accessor = corpse_family_binding)]
pub struct CorpseFamilyBinding {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub corpse_id: String,
    #[index(btree)]
    pub npc_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = corpse_permission)]
pub struct CorpsePermission {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub corpse_id: String,
    #[index(btree)]
    pub party_id: String,
    pub granted_by_npc_id: String,
    pub kind: CorpsePermissionKind,
    pub scope: CorpsePermissionScope,
    pub granted_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = corpse_permission_attempt)]
pub struct CorpsePermissionAttempt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub corpse_id: String,
    #[index(btree)]
    pub party_id: String,
    pub npc_id: String,
    pub scope: CorpsePermissionScope,
    pub granted: bool,
    pub attempted_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = autopsy_action_receipt)]
pub struct AutopsyActionReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub corpse_id: String,
    #[index(btree)]
    pub observer_id: u64,
    pub action_kind: String,
    pub stage: String,
    pub finding: String,
    pub performed_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCorpse {
    pub owner_character_id: u64,
    pub corpse_id: String,
    pub display_name: String,
    pub creature_kind: String,
    pub source_id: String,
    pub location: String,
    pub decomposition: String,
    pub case_site_id: String,
    pub settlement_id: String,
    pub opened: bool,
    pub permission: String,
    pub exhumation_permission: bool,
    pub penalty_free_burning: bool,
    pub revision: u32,
    pub findings: Vec<String>,
}

fn location_label(value: CorpseLocation) -> &'static str {
    match value {
        CorpseLocation::Scene => "scene",
        CorpseLocation::LocalCustody => "local_custody",
        CorpseLocation::Interred => "interred",
        CorpseLocation::Exhumed => "exhumed",
    }
}

fn decomposition_label(value: DecompositionBand) -> &'static str {
    match value {
        DecompositionBand::Fresh => "fresh",
        DecompositionBand::Early => "early",
        DecompositionBand::Advanced => "advanced",
        DecompositionBand::Skeletal => "skeletal",
    }
}

fn observer_party(ctx: &ReducerContext, actor_id: u64) -> Result<String, String> {
    let actor = crate::character::require_living_character(ctx, actor_id)?;
    actor.party_id.ok_or("Character has no party".into())
}

fn now(ctx: &ReducerContext, actor_id: u64) -> Result<u64, String> {
    ctx.db
        .character_time()
        .character_id()
        .find(actor_id)
        .map(|row| row.minutes)
        .ok_or("Character time not found".into())
}

fn permission_for(
    ctx: &ReducerContext,
    corpse_id: &str,
    party_id: &str,
    required_scope: CorpsePermissionScope,
) -> Option<CorpsePermission> {
    ctx.db
        .corpse_permission()
        .corpse_id()
        .filter(corpse_id)
        .find(|row| {
            row.party_id == party_id
                && (row.scope == required_scope || row.scope == CorpsePermissionScope::Exhumation)
        })
}

fn require_corpse_access(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: &str,
) -> Result<(StrategicCorpse, String, u64), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    let party_id = observer_party(ctx, actor_id)?;
    let corpse = ctx
        .db
        .strategic_corpse()
        .id()
        .find(&corpse_id.to_owned())
        .ok_or("Corpse not found")?;
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_id)
        .is_some_and(|encounter| encounter.status != "resolved")
    {
        return Err("Resolve the active encounter before handling a corpse".into());
    }
    if corpse.discovering_party_id != party_id {
        return Err("This party has not discovered the body".into());
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Character not found")?;
    let minute = now(ctx, actor_id)?;
    if corpse.burned {
        return Err("The corpse has been destroyed by fire".into());
    }
    let location = corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    );
    let actor_site = ctx
        .db
        .character_case_site_occupancy()
        .character_id()
        .find(actor_id)
        .map(|row| row.case_site_id.value);
    let together = match location {
        CorpseLocation::Scene => actor_site.as_deref() == Some(corpse.case_site_id.as_str()),
        CorpseLocation::LocalCustody | CorpseLocation::Interred | CorpseLocation::Exhumed => {
            actor.current_settlement_id.as_deref() == Some(corpse.settlement_id.as_str())
        }
    };
    if !together {
        return Err("Examiner and corpse must be together".into());
    }
    Ok((corpse, party_id, minute))
}

fn apply_unauthorized_consequences(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse: &StrategicCorpse,
    party_id: &str,
    action_id: &str,
    required_scope: CorpsePermissionScope,
) -> Result<(), String> {
    if permission_for(ctx, &corpse.id, party_id, required_scope).is_some() {
        return Ok(());
    }
    if !corpse.settlement_id.is_empty() {
        crate::reputation::record_event(
            ctx,
            format!("unauthorized-autopsy:{action_id}"),
            actor_id,
            &corpse.settlement_id,
            "unauthorized_autopsy",
            &corpse.id,
            0,
            1_500,
            now(ctx, actor_id)?,
        )?;
    }
    for family in ctx
        .db
        .corpse_family_binding()
        .corpse_id()
        .filter(&corpse.id)
    {
        crate::social::apply_corpse_family_offense(
            ctx,
            actor_id,
            &family.npc_id,
            &format!("unauthorized-autopsy:{action_id}:{}", family.npc_id),
            -18.0,
            -12.0,
        )?;
    }
    Ok(())
}

fn validate_client_action_id(action_id: &str) -> Result<(), String> {
    if action_id.is_empty()
        || action_id.len() > 96
        || !action_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
    {
        return Err("Invalid autopsy action ID".into());
    }
    Ok(())
}

fn action_receipt_id(
    actor_id: u64,
    corpse_id: &str,
    action_kind: &str,
    discipline: &str,
    stage: &str,
    revision: u32,
) -> String {
    format!("autopsy:{actor_id}:{corpse_id}:{action_kind}:{discipline}:{stage}:revision:{revision}")
}

fn burning_social_penalty(party_killed_enemy: bool) -> Option<(i32, f32)> {
    (!party_killed_enemy).then_some((CREMATION_INFAMY, CREMATION_FAMILY_AFFINITY_DELTA))
}

fn skill_check(ctx: &ReducerContext, actor_id: u64, discipline: &str) -> Result<f32, String> {
    let capability = ctx
        .db
        .character_capability()
        .character_id()
        .find(actor_id)
        .ok_or("Character capability not found")?;
    Ok(match discipline {
        "surgery" => capability.surgery,
        "physiology" => capability.physiology,
        "bestiary" => ctx
            .db
            .character_skills()
            .character_id()
            .find(actor_id)
            .map_or(0.0, |skills| {
                Skill::Bestiary.training_rank(skills.bestiary_hours.aggregate_effective())
            }),
        _ => return Err("Unknown autopsy discipline".into()),
    })
}

fn summarize_external(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    discipline: &str,
    check: f32,
    minute: u64,
) -> String {
    realized_finding(ctx, corpse, discipline, check, minute, false)
}

fn summarize_internal(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    discipline: &str,
    check: f32,
    minute: u64,
) -> String {
    realized_finding(ctx, corpse, discipline, check, minute, true)
}

fn realized_finding(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    discipline: &str,
    check: f32,
    minute: u64,
    internal: bool,
) -> String {
    use adventuresim_core::autopsy::{
        AutopsyEvidenceContext, BodyInjury, PostCombatBody, bestiary_finding, physiology_finding,
        surgery_finding,
    };
    let injuries = ctx
        .db
        .corpse_injury()
        .corpse_id()
        .filter(&corpse.id)
        .filter_map(|injury| {
            let region = match injury.region.as_str() {
                "left arm" => BodyPart::LeftArm,
                "right arm" => BodyPart::RightArm,
                "left leg" => BodyPart::LeftLeg,
                "right leg" => BodyPart::RightLeg,
                "chest" => BodyPart::Chest,
                "stomach" | "abdomen" => BodyPart::Stomach,
                "head" => BodyPart::Head,
                _ => return None,
            };
            Some(BodyInjury {
                sequence: injury.sequence,
                region,
                cut_damage: injury.cut_damage,
                blunt_damage: injury.blunt_damage,
                projectile: injury.projectile,
                contact_stress: injury.contact_stress,
            })
        })
        .collect::<Vec<_>>();
    let body = ctx
        .db
        .corpse_body_state()
        .corpse_id()
        .find(&corpse.id)
        .and_then(|body| {
            (body.health.len() == 7).then(|| {
                let mut health = [1.0; 7];
                health.copy_from_slice(&body.health);
                PostCombatBody {
                    combatant_id: corpse.subject_character_id.unwrap_or(0),
                    health,
                    blood_loss_fraction: body.blood_loss_fraction,
                    injuries: injuries.clone(),
                }
            })
        });
    let location = corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    );
    let context = AutopsyEvidenceContext {
        decomposition: decomposition_band(corpse.death_minute, minute, corpse.handling_damage_bps),
        at_scene: location == CorpseLocation::Scene,
        opening_obscuration_bps: corpse.opening_obscuration_bps,
    };
    let finding = match discipline {
        "surgery" => surgery_finding(&injuries, check, context, internal),
        "physiology" => body
            .as_ref()
            .and_then(|body| physiology_finding(body, check, context, internal)),
        "bestiary" => bestiary_finding(&injuries, check, context, internal),
        _ => None,
    };
    finding.unwrap_or_else(|| match discipline {
        "surgery" => {
            "Decomposition, handling, or limited precision prevents a defensible wound description."
                .into()
        }
        "physiology" => {
            "The remaining signs do not support a defensible physiological interpretation.".into()
        }
        "bestiary" => {
            "Learned lore cannot narrow the observed physical signs to a useful threat candidate."
                .into()
        }
        _ => "No defensible finding was recorded.".into(),
    })
}

#[view(accessor = backend_corpses, public)]
pub fn backend_corpses(ctx: &ViewContext) -> Vec<BackendCorpse> {
    let trusted = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    if !trusted {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for actor_time in ctx.db.character_time().minutes().filter(0u64..) {
        let Some(actor) = ctx
            .db
            .character()
            .id()
            .find(actor_time.character_id)
            .filter(|row| row.alive)
        else {
            continue;
        };
        let Some(party_id) = actor.party_id.as_deref() else {
            continue;
        };
        let minute = actor_time.minutes;
        for corpse in ctx
            .db
            .strategic_corpse()
            .discovering_party_id()
            .filter(party_id)
            .filter(|corpse| !corpse.burned)
        {
            let location = corpse_location(
                corpse.discovered_minute,
                minute,
                corpse.buried,
                corpse.exhumed,
            );
            let buried = location == CorpseLocation::Interred;
            let permissions = ctx
                .db
                .corpse_permission()
                .corpse_id()
                .filter(&corpse.id)
                .filter(|row| row.party_id == party_id)
                .collect::<Vec<_>>();
            let permission = permissions
                .iter()
                .find(|row| {
                    matches!(
                        row.scope,
                        CorpsePermissionScope::Examination | CorpsePermissionScope::Exhumation
                    )
                })
                .map_or("none", |row| match row.kind {
                    CorpsePermissionKind::Family => "family",
                    CorpsePermissionKind::Priest => "priest",
                    CorpsePermissionKind::SecularAuthority => "authority",
                });
            let findings = if buried {
                Vec::new()
            } else {
                ctx.db
                    .autopsy_action_receipt()
                    .observer_id()
                    .filter(actor.id)
                    .filter(|row| row.corpse_id == corpse.id)
                    .map(|row| row.finding)
                    .collect()
            };
            rows.push(BackendCorpse {
                owner_character_id: actor.id,
                corpse_id: corpse.id.clone(),
                display_name: if buried {
                    "Buried body".into()
                } else {
                    corpse.display_name.clone()
                },
                creature_kind: if buried {
                    String::new()
                } else {
                    corpse.creature_kind.clone()
                },
                source_id: if buried {
                    String::new()
                } else {
                    corpse.source_id.clone()
                },
                location: location_label(location).into(),
                decomposition: if buried {
                    String::new()
                } else {
                    decomposition_label(decomposition_band(
                        corpse.death_minute,
                        minute,
                        corpse.handling_damage_bps,
                    ))
                    .into()
                },
                case_site_id: if buried {
                    String::new()
                } else {
                    corpse.case_site_id.clone()
                },
                settlement_id: corpse.settlement_id.clone(),
                opened: !buried && corpse.opened,
                permission: if buried {
                    "none".into()
                } else {
                    permission.into()
                },
                exhumation_permission: permissions
                    .iter()
                    .any(|row| row.scope == CorpsePermissionScope::Exhumation),
                penalty_free_burning: !buried && corpse.party_killed_enemy,
                revision: corpse.revision,
                findings,
            });
        }
    }
    rows
}

pub(crate) fn persist_autoresolve_enemy_corpses(
    ctx: &ReducerContext,
    source_id: &str,
    party_id: &str,
    settlement_id: &str,
    case_site_id: &str,
    creature_kind: &str,
    outcome: &BattleOutcome,
) -> Result<usize, String> {
    let minute = crate::strategic::living_party_member_ids(ctx, party_id)
        .into_iter()
        .filter_map(|id| ctx.db.character_time().character_id().find(id))
        .map(|row| row.minutes)
        .min()
        .unwrap_or(0);
    let mut inserted = 0;
    for enemy in outcome
        .enemies
        .iter()
        .filter(|enemy| adventuresim_core::autopsy::is_lethal_body(enemy))
    {
        let corpse_id = format!("corpse:{source_id}:{}", enemy.id);
        if ctx.db.strategic_corpse().id().find(&corpse_id).is_some() {
            continue;
        }
        persist_body(
            ctx,
            StrategicCorpse {
                id: corpse_id,
                source_id: source_id.into(),
                discovering_party_id: party_id.into(),
                subject_character_id: None,
                display_name: format!("Fallen {creature_kind}"),
                creature_kind: creature_kind.into(),
                settlement_id: settlement_id.into(),
                case_site_id: case_site_id.into(),
                death_minute: minute,
                discovered_minute: minute,
                buried: false,
                exhumed: false,
                burned: false,
                party_killed_enemy: true,
                handling_damage_bps: 0,
                opened: false,
                opening_quality_bps: 0,
                opening_obscuration_bps: 0,
                revision: 0,
            },
            post_combat_body(enemy, &outcome.log),
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

fn persist_body(
    ctx: &ReducerContext,
    corpse: StrategicCorpse,
    body: PostCombatBody,
) -> Result<(), String> {
    let corpse_id = corpse.id.clone();
    ctx.db.strategic_corpse().insert(corpse);
    ctx.db.corpse_body_state().insert(CorpseBodyState {
        corpse_id: corpse_id.clone(),
        health: body.health.to_vec(),
        blood_loss_fraction: body.blood_loss_fraction,
    });
    for injury in body.injuries {
        ctx.db.corpse_injury().insert(CorpseInjury {
            id: format!("{corpse_id}:injury:{}", injury.sequence),
            corpse_id: corpse_id.clone(),
            sequence: injury.sequence,
            region: body_part_label(injury.region).into(),
            cut_damage: injury.cut_damage,
            blunt_damage: injury.blunt_damage,
            projectile: injury.projectile,
            contact_stress: injury.contact_stress,
        });
    }
    Ok(())
}

const MAX_BOUND_FAMILY_MEMBERS: usize = 8;

/// Materialize only explicit kin supplied by an authoritative producer. Empty
/// input is valid when a death source has no kinship authority; household
/// display text is deliberately never consulted.
pub(crate) fn materialize_corpse_family_bindings(
    ctx: &ReducerContext,
    corpse_id: &str,
    settlement_id: &str,
    family_npc_ids: &[String],
) -> Result<(), String> {
    if family_npc_ids.len() > MAX_BOUND_FAMILY_MEMBERS {
        return Err("Corpse family binding exceeds its bounded limit".into());
    }
    let mut unique = family_npc_ids.to_vec();
    unique.sort();
    unique.dedup();
    for npc_id in unique {
        let npc = ctx
            .db
            .settlement_npc()
            .id()
            .find(&npc_id)
            .ok_or("Corpse family binding references an unknown NPC")?;
        if npc.home_settlement_id != settlement_id {
            return Err("Corpse family member belongs to another settlement".into());
        }
        let id = format!("{corpse_id}:{npc_id}");
        if ctx.db.corpse_family_binding().id().find(&id).is_none() {
            ctx.db.corpse_family_binding().insert(CorpseFamilyBinding {
                id,
                corpse_id: corpse_id.into(),
                npc_id,
            });
        }
    }
    Ok(())
}

/// Materialize any ordinary strategic character death through the same corpse
/// authority. Existing durable injuries are copied as physical findings; no
/// separate cause or culprit clue is invented.
pub(crate) fn persist_character_death_corpse(
    ctx: &ReducerContext,
    character_id: u64,
    source_id: &str,
    death_minute: u64,
) -> Result<(), String> {
    let Some(subject) = ctx.db.character().id().find(character_id) else {
        return Err("Dead character not found".into());
    };
    let corpse_id = format!("corpse:character:{character_id}");
    if ctx.db.strategic_corpse().id().find(&corpse_id).is_some() {
        return Ok(());
    }
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Dead character anatomy not found")?;
    let case_site_id =
        crate::investigation::character_case_site_id(ctx, character_id).unwrap_or_default();
    let settlement_id = subject.current_settlement_id.clone().unwrap_or_default();
    ctx.db.strategic_corpse().insert(StrategicCorpse {
        id: corpse_id.clone(),
        source_id: source_id.into(),
        discovering_party_id: subject.party_id.clone().unwrap_or_default(),
        subject_character_id: Some(character_id),
        display_name: subject.name,
        creature_kind: "human".into(),
        settlement_id: settlement_id.clone(),
        case_site_id,
        death_minute,
        discovered_minute: death_minute,
        buried: false,
        exhumed: false,
        burned: false,
        party_killed_enemy: false,
        handling_damage_bps: 0,
        opened: false,
        opening_quality_bps: 0,
        opening_obscuration_bps: 0,
        revision: 0,
    });
    // Character currently has no authoritative kinship relation. Calling the
    // seam with an explicit empty set prevents household text from becoming kin.
    materialize_corpse_family_bindings(ctx, &corpse_id, &settlement_id, &[])?;
    ctx.db.corpse_body_state().insert(CorpseBodyState {
        corpse_id: corpse_id.clone(),
        health: vec![
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
            limbs.chest_health,
            limbs.stomach_health,
            limbs.head_health,
        ],
        blood_loss_fraction: ctx
            .db
            .character_condition()
            .character_id()
            .find(character_id)
            .map_or(0.0, |row| {
                if row.maximum_blood_ml > 0.0 {
                    (1.0 - row.current_blood_ml / row.maximum_blood_ml).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }),
    });
    for (sequence, injury) in ctx
        .db
        .limb_injury()
        .character_id()
        .filter(character_id)
        .enumerate()
    {
        ctx.db.corpse_injury().insert(CorpseInjury {
            id: format!("{corpse_id}:injury:{sequence}"),
            corpse_id: corpse_id.clone(),
            sequence: sequence as u32,
            region: injury.limb.slug().replace('-', " "),
            cut_damage: injury.cut_damage,
            blunt_damage: injury.bruise_damage,
            projectile: ctx
                .db
                .retained_projectile()
                .character_id()
                .filter(character_id)
                .any(|projectile| projectile.limb == injury.limb),
            contact_stress: injury.fracture_damage,
        });
    }
    Ok(())
}

fn body_part_label(part: BodyPart) -> &'static str {
    match part {
        BodyPart::LeftArm => "left arm",
        BodyPart::RightArm => "right arm",
        BodyPart::LeftLeg => "left leg",
        BodyPart::RightLeg => "right leg",
        BodyPart::Chest => "chest",
        BodyPart::Stomach => "stomach",
        BodyPart::Head => "head",
    }
}

#[reducer]
pub fn examine_corpse(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: String,
    discipline: String,
    stage: String,
    action_id: String,
    expected_revision: u32,
    confirm_unauthorized: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    validate_client_action_id(&action_id)?;
    let receipt_id = action_receipt_id(
        actor_id,
        &corpse_id,
        "examine",
        &discipline,
        &stage,
        expected_revision,
    );
    if ctx
        .db
        .autopsy_action_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let (corpse, party_id, minute) = require_corpse_access(ctx, actor_id, &corpse_id)?;
    if corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    ) == CorpseLocation::Interred
    {
        return Err("The body must be exhumed before it can be examined".into());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before examining it".into());
    }
    if stage == "internal" && !corpse.opened {
        return Err("The body has not been opened".into());
    }
    if !matches!(stage.as_str(), "external" | "internal") {
        return Err("Unknown examination stage".into());
    }
    if permission_for(
        ctx,
        &corpse_id,
        &party_id,
        CorpsePermissionScope::Examination,
    )
    .is_none()
        && !confirm_unauthorized
    {
        return Err(
            "Permission is missing; confirm the likely family penalties and settlement infamy"
                .into(),
        );
    }
    let duration = if stage == "external" {
        EXTERNAL_EXAMINATION_MINUTES
    } else {
        INTERNAL_EXAMINATION_MINUTES
    };
    if !advance_character_wait_time(ctx, actor_id, duration)? {
        return Ok(());
    }
    apply_unauthorized_consequences(
        ctx,
        actor_id,
        &corpse,
        &party_id,
        &receipt_id,
        CorpsePermissionScope::Examination,
    )?;
    let completed_minute = now(ctx, actor_id)?;
    let check = skill_check(ctx, actor_id, &discipline)?;
    let finding = if stage == "external" {
        summarize_external(ctx, &corpse, &discipline, check, completed_minute)
    } else {
        summarize_internal(ctx, &corpse, &discipline, check, completed_minute)
    };
    ctx.db
        .autopsy_action_receipt()
        .insert(AutopsyActionReceipt {
            id: receipt_id,
            corpse_id,
            observer_id: actor_id,
            action_kind: discipline,
            stage,
            finding,
            performed_minute: completed_minute,
        });
    Ok(())
}

#[reducer]
pub fn open_corpse(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: String,
    action_id: String,
    expected_revision: u32,
    confirm_unauthorized: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    validate_client_action_id(&action_id)?;
    let receipt_id = action_receipt_id(
        actor_id,
        &corpse_id,
        "open",
        "surgery",
        "opening",
        expected_revision,
    );
    if ctx
        .db
        .autopsy_action_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let (mut corpse, party_id, minute) = require_corpse_access(ctx, actor_id, &corpse_id)?;
    if corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    ) == CorpseLocation::Interred
    {
        return Err("The body must be exhumed before it can be opened".into());
    }
    if corpse.opened {
        return Ok(());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before opening it".into());
    }
    if permission_for(
        ctx,
        &corpse_id,
        &party_id,
        CorpsePermissionScope::Examination,
    )
    .is_none()
        && !confirm_unauthorized
    {
        return Err(
            "Permission is missing; confirm the likely family penalties and settlement infamy"
                .into(),
        );
    }
    let surgery = skill_check(ctx, actor_id, "surgery")?;
    if !advance_character_wait_time(ctx, actor_id, OPEN_BODY_MINUTES)? {
        return Ok(());
    }
    apply_unauthorized_consequences(
        ctx,
        actor_id,
        &corpse,
        &party_id,
        &receipt_id,
        CorpsePermissionScope::Examination,
    )?;
    let completed_minute = now(ctx, actor_id)?;
    let entropy = (adventuresim_core::settlement_population::stable_hash(&format!(
        "{}:{actor_id}:{}:opening:{}",
        corpse.id, corpse.revision, receipt_id
    )) % 10_001) as u16;
    let (quality, obscuration) = opening_quality_bps(surgery, entropy);
    corpse.opened = true;
    corpse.opening_quality_bps = quality;
    corpse.opening_obscuration_bps = obscuration;
    corpse.handling_damage_bps = corpse.handling_damage_bps.saturating_add(obscuration / 8);
    corpse.revision = corpse.revision.saturating_add(1);
    ctx.db.strategic_corpse().id().update(corpse);
    let exposure = adventuresim_core::surgery::procedure_blood_exposure("open-body", true);
    if exposure > 0 {
        crate::filth::deposit_now(
            ctx,
            actor_id,
            crate::filth::FilthSubstance::Blood,
            None,
            exposure,
        )?;
    }
    ctx.db.autopsy_action_receipt().insert(AutopsyActionReceipt {
        id: receipt_id,
        corpse_id,
        observer_id: actor_id,
        action_kind: "surgery".into(),
        stage: "opening".into(),
        finding: format!(
            "The body was opened with {}% dissection precision; incision damage may obscure internal evidence.",
            quality / 100
        ),
        performed_minute: completed_minute,
    });
    Ok(())
}

#[reducer]
pub fn exhume_corpse(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: String,
    action_id: String,
    expected_revision: u32,
    confirm_unauthorized: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    validate_client_action_id(&action_id)?;
    let receipt_id = action_receipt_id(
        actor_id,
        &corpse_id,
        "exhume",
        "surgery",
        "handling",
        expected_revision,
    );
    if ctx
        .db
        .autopsy_action_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let (mut corpse, party_id, minute) = require_corpse_access(ctx, actor_id, &corpse_id)?;
    if corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    ) != CorpseLocation::Interred
    {
        return Err("Only an interred corpse can be exhumed".into());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before exhuming it".into());
    }
    if permission_for(
        ctx,
        &corpse_id,
        &party_id,
        CorpsePermissionScope::Exhumation,
    )
    .is_none()
        && !confirm_unauthorized
    {
        return Err(
            "Permission is missing; confirm the severe family penalty and settlement infamy".into(),
        );
    }
    if !advance_character_wait_time(ctx, actor_id, EXHUMATION_MINUTES)? {
        return Ok(());
    }
    apply_unauthorized_consequences(
        ctx,
        actor_id,
        &corpse,
        &party_id,
        &receipt_id,
        CorpsePermissionScope::Exhumation,
    )?;
    let completed_minute = now(ctx, actor_id)?;
    corpse.exhumed = true;
    corpse.handling_damage_bps = corpse.handling_damage_bps.saturating_add(800);
    corpse.revision = corpse.revision.saturating_add(1);
    ctx.db.strategic_corpse().id().update(corpse);
    ctx.db
        .autopsy_action_receipt()
        .insert(AutopsyActionReceipt {
            id: receipt_id,
            corpse_id,
            observer_id: actor_id,
            action_kind: "exhume".into(),
            stage: "handling".into(),
            finding: "The body has been exhumed; handling and elapsed time reduced the evidence."
                .into(),
            performed_minute: completed_minute,
        });
    Ok(())
}

#[reducer]
pub fn bury_corpse(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: String,
    action_id: String,
    expected_revision: u32,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    validate_client_action_id(&action_id)?;
    let receipt_id = action_receipt_id(
        actor_id,
        &corpse_id,
        "bury",
        "surgery",
        "handling",
        expected_revision,
    );
    if ctx
        .db
        .autopsy_action_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let (mut corpse, _party_id, minute) = require_corpse_access(ctx, actor_id, &corpse_id)?;
    if corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    ) == CorpseLocation::Interred
    {
        return Err("The corpse is already buried".into());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before burying it".into());
    }
    if !advance_character_wait_time(ctx, actor_id, BURIAL_MINUTES)? {
        return Ok(());
    }
    let completed_minute = now(ctx, actor_id)?;
    corpse.buried = true;
    corpse.exhumed = false;
    corpse.handling_damage_bps = corpse.handling_damage_bps.saturating_add(200);
    corpse.revision = corpse.revision.saturating_add(1);
    ctx.db.strategic_corpse().id().update(corpse);
    ctx.db
        .autopsy_action_receipt()
        .insert(AutopsyActionReceipt {
            id: receipt_id,
            corpse_id,
            observer_id: actor_id,
            action_kind: "bury".into(),
            stage: "handling".into(),
            finding: "The body has been buried. Its identity and physical evidence are concealed until it is exhumed.".into(),
            performed_minute: completed_minute,
        });
    Ok(())
}

#[reducer]
pub fn burn_corpse(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: String,
    action_id: String,
    expected_revision: u32,
    confirm_destruction: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    validate_client_action_id(&action_id)?;
    let receipt_id = action_receipt_id(
        actor_id,
        &corpse_id,
        "burn",
        "surgery",
        "handling",
        expected_revision,
    );
    if ctx
        .db
        .autopsy_action_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let (mut corpse, _party_id, minute) = require_corpse_access(ctx, actor_id, &corpse_id)?;
    if corpse_location(
        corpse.discovered_minute,
        minute,
        corpse.buried,
        corpse.exhumed,
    ) == CorpseLocation::Interred
    {
        return Err("The body must be exhumed before it can be burned".into());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before burning it".into());
    }
    if !corpse.party_killed_enemy && !confirm_destruction {
        return Err(
            "Burning a victim cannot be authorized and will cause severe family affinity loss and settlement infamy; confirm the irreversible destruction".into(),
        );
    }
    if !advance_character_wait_time(ctx, actor_id, CREMATION_MINUTES)? {
        return Ok(());
    }
    let completed_minute = now(ctx, actor_id)?;
    if let Some((infamy, family_affinity_delta)) = burning_social_penalty(corpse.party_killed_enemy)
    {
        if !corpse.settlement_id.is_empty() {
            crate::reputation::record_event(
                ctx,
                format!("corpse-burning:{receipt_id}"),
                actor_id,
                &corpse.settlement_id,
                "corpse_burning",
                &corpse.id,
                0,
                infamy,
                completed_minute,
            )?;
        }
        for family in ctx
            .db
            .corpse_family_binding()
            .corpse_id()
            .filter(&corpse.id)
        {
            crate::social::apply_corpse_family_offense(
                ctx,
                actor_id,
                &family.npc_id,
                &format!("corpse-burning:{receipt_id}:{}", family.npc_id),
                0.0,
                family_affinity_delta,
            )?;
        }
    }
    corpse.burned = true;
    corpse.revision = corpse.revision.saturating_add(1);
    ctx.db.strategic_corpse().id().update(corpse);
    ctx.db
        .autopsy_action_receipt()
        .insert(AutopsyActionReceipt {
            id: receipt_id,
            corpse_id,
            observer_id: actor_id,
            action_kind: "burn".into(),
            stage: "handling".into(),
            finding:
                "The body and all remaining physical evidence were irreversibly destroyed by fire."
                    .into(),
            performed_minute: completed_minute,
        });
    Ok(())
}

pub(crate) fn grant_permission_from_dialogue(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: &str,
    npc_id: &str,
    scope: CorpsePermissionScope,
) -> Result<bool, String> {
    let party_id = observer_party(ctx, actor_id)?;
    let corpse = ctx
        .db
        .strategic_corpse()
        .id()
        .find(&corpse_id.to_owned())
        .ok_or("Corpse not found")?;
    if corpse.discovering_party_id != party_id {
        return Err("Party has not discovered this corpse".into());
    }
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&npc_id.to_owned())
        .ok_or("NPC not found")?;
    let kind = permission_kind_for_npc(ctx, &corpse, &npc)
        .ok_or("NPC cannot grant permission for this corpse")?;
    let scope_label = permission_scope_label(scope);
    let attempt_id = format!("{corpse_id}:{party_id}:{npc_id}:{scope_label}");
    if let Some(attempt) = ctx.db.corpse_permission_attempt().id().find(&attempt_id) {
        return Ok(attempt.granted);
    }
    let affinity = ctx
        .db
        .settlement_npc_relationship()
        .id()
        .find(&format!("{actor_id}:{npc_id}"))
        .map_or(0.0, |row| row.affinity_anchor);
    let charm = crate::condition::mental_check(ctx, actor_id, Skill::Charm)?;
    let difficulty = permission_difficulty(kind, scope);
    let entropy = adventuresim_core::settlement_population::stable_hash(&format!(
        "corpse-permission:{attempt_id}"
    ));
    let circumstance = (entropy % 101) as f32 / 100.0 - 0.5;
    let granted = charm + affinity.clamp(-25.0, 25.0) / 25.0 + circumstance >= difficulty;
    let attempted_minute = now(ctx, actor_id)?;
    ctx.db
        .corpse_permission_attempt()
        .insert(CorpsePermissionAttempt {
            id: attempt_id,
            corpse_id: corpse_id.into(),
            party_id: party_id.clone(),
            npc_id: npc_id.into(),
            scope,
            granted,
            attempted_minute,
        });
    if granted {
        let id = format!("{corpse_id}:{party_id}:{scope_label}");
        ctx.db.corpse_permission().insert(CorpsePermission {
            id,
            corpse_id: corpse_id.into(),
            party_id: party_id.clone(),
            granted_by_npc_id: npc_id.into(),
            kind,
            scope,
            granted_minute: attempted_minute,
        });
        if kind != CorpsePermissionKind::Family {
            for relative in ctx.db.corpse_family_binding().corpse_id().filter(corpse_id) {
                crate::social::apply_corpse_family_offense(
                    ctx,
                    actor_id,
                    &relative.npc_id,
                    &format!("family-bypassed:{corpse_id}:{}", relative.npc_id),
                    -5.0,
                    -3.0,
                )?;
            }
        }
    }
    Ok(granted)
}

fn permission_scope_label(scope: CorpsePermissionScope) -> &'static str {
    match scope {
        CorpsePermissionScope::Examination => "examination",
        CorpsePermissionScope::Exhumation => "exhumation",
    }
}

fn permission_difficulty(kind: CorpsePermissionKind, scope: CorpsePermissionScope) -> f32 {
    match (kind, scope) {
        (CorpsePermissionKind::Family, CorpsePermissionScope::Examination) => 1.0,
        (CorpsePermissionKind::Priest, CorpsePermissionScope::Examination) => 2.0,
        (CorpsePermissionKind::SecularAuthority, CorpsePermissionScope::Examination) => 2.5,
        (CorpsePermissionKind::Family, CorpsePermissionScope::Exhumation) => 3.5,
        (CorpsePermissionKind::Priest, CorpsePermissionScope::Exhumation) => 4.0,
        (CorpsePermissionKind::SecularAuthority, CorpsePermissionScope::Exhumation) => 4.5,
    }
}

fn titled_permission_kind(
    same_settlement: bool,
    family: bool,
    profession: &str,
    local_role: &str,
    service_id: &str,
) -> Option<CorpsePermissionKind> {
    if !same_settlement {
        None
    } else if family {
        Some(CorpsePermissionKind::Family)
    } else if profession == "cleric" && local_role == "parish priest" && service_id == "religion" {
        Some(CorpsePermissionKind::Priest)
    } else if matches!(local_role, "reeve" | "local lord" | "magistrate") {
        Some(CorpsePermissionKind::SecularAuthority)
    } else {
        None
    }
}

fn permission_kind_for_npc(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    npc: &SettlementNpc,
) -> Option<CorpsePermissionKind> {
    let family = ctx
        .db
        .corpse_family_binding()
        .corpse_id()
        .filter(&corpse.id)
        .any(|row| row.npc_id == npc.id);
    titled_permission_kind(
        npc.home_settlement_id == corpse.settlement_id,
        family,
        &npc.profession,
        &npc.local_role,
        &npc.service_id,
    )
}

pub(crate) fn permission_topics_for_npc(
    ctx: &ReducerContext,
    actor_id: u64,
    npc_id: &str,
) -> Vec<(String, String)> {
    let Ok(party_id) = observer_party(ctx, actor_id) else {
        return Vec::new();
    };
    let Some(npc) = ctx.db.settlement_npc().id().find(&npc_id.to_owned()) else {
        return Vec::new();
    };
    let minute = now(ctx, actor_id).unwrap_or(0);
    ctx.db
        .strategic_corpse()
        .discovering_party_id()
        .filter(&party_id)
        .filter(|corpse| !corpse.burned)
        .flat_map(|corpse| {
            if permission_kind_for_npc(ctx, &corpse, &npc).is_none() {
                return Vec::new();
            }
            let mut topics = Vec::new();
            for scope in [
                CorpsePermissionScope::Examination,
                CorpsePermissionScope::Exhumation,
            ] {
                let is_exhumation = scope == CorpsePermissionScope::Exhumation;
                let location = corpse_location(
                    corpse.discovered_minute,
                    minute,
                    corpse.buried,
                    corpse.exhumed,
                );
                let eligible_location = if is_exhumation {
                    location == CorpseLocation::Interred
                } else {
                    location != CorpseLocation::Interred
                };
                let attempted = ctx
                    .db
                    .corpse_permission_attempt()
                    .id()
                    .find(&format!(
                        "{}:{}:{}:{}",
                        corpse.id,
                        party_id,
                        npc_id,
                        permission_scope_label(scope)
                    ))
                    .is_some();
                if eligible_location
                    && permission_for(ctx, &corpse.id, &party_id, scope).is_none()
                    && !attempted
                {
                    topics.push((
                        format!(
                            "corpse-permission:{}:{}",
                            permission_scope_label(scope),
                            corpse.id
                        ),
                        if is_exhumation {
                            "Ask permission to exhume a buried body".into()
                        } else {
                            format!("Ask permission to examine {}", corpse.display_name)
                        },
                    ));
                }
            }
            topics
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_tables_are_private_and_gateway_projection_is_observer_scoped() {
        let source = include_str!("corpse.rs");
        for table in [
            "strategic_corpse",
            "corpse_body_state",
            "corpse_injury",
            "corpse_family_binding",
            "corpse_permission",
            "corpse_permission_attempt",
            "autopsy_action_receipt",
        ] {
            assert!(source.contains(&format!("#[table(accessor = {table})]")));
            assert!(!source.contains(&format!("#[table(accessor = {table}, public)]")));
        }
        assert!(source.contains("owner_character_id: actor.id"));
        assert!(!source.contains("attacker_id"));
        assert!(!source.contains("weapon_inventory_item_id"));
    }

    #[test]
    fn reducers_encode_retry_permission_and_location_authority() {
        let source = include_str!("corpse.rs");
        assert!(source.contains("validate_client_action_id(&action_id)"));
        assert!(source.contains(
            "action_receipt_id(\n        actor_id,\n        &corpse_id,\n        \"examine\""
        ));
        assert!(source.contains("confirm_unauthorized"));
        assert!(source.contains("apply_unauthorized_consequences"));
        assert!(source.contains("CorpsePermissionScope::Examination"));
        assert!(source.contains("CorpsePermissionScope::Exhumation"));
        assert!(source.contains("actor_site.as_deref() == Some(corpse.case_site_id.as_str())"));
        assert!(source.contains(
            "actor.current_settlement_id.as_deref() == Some(corpse.settlement_id.as_str())"
        ));
        assert!(source.contains("Resolve the active encounter before handling a corpse"));
        assert!(source.contains("procedure_blood_exposure(\"open-body\", true)"));
        assert!(source.contains("CorpsePermissionKind::Family"));
        assert!(source.contains("CorpsePermissionKind::Priest"));
        assert!(source.contains("CorpsePermissionKind::SecularAuthority"));
    }

    #[test]
    fn permission_authority_is_exact_and_shared() {
        assert_eq!(
            titled_permission_kind(true, true, "laborer", "neighbor", ""),
            Some(CorpsePermissionKind::Family)
        );
        assert_eq!(
            titled_permission_kind(true, false, "cleric", "parish priest", "religion"),
            Some(CorpsePermissionKind::Priest)
        );
        assert_eq!(
            titled_permission_kind(true, false, "retainer", "reeve", "keep"),
            Some(CorpsePermissionKind::SecularAuthority)
        );
        assert_eq!(
            titled_permission_kind(true, false, "servant", "keep servant", "keep"),
            None
        );
        assert_eq!(
            titled_permission_kind(false, true, "laborer", "neighbor", ""),
            None
        );

        let source = include_str!("corpse.rs");
        assert!(source.matches("permission_kind_for_npc(ctx,").count() >= 2);
        assert!(!source.contains("service_id == \"keep\""));
        assert!(!source.contains("local_role.contains"));
    }

    #[test]
    fn exhumation_permission_is_harder_for_every_authority_kind() {
        for kind in [
            CorpsePermissionKind::Family,
            CorpsePermissionKind::Priest,
            CorpsePermissionKind::SecularAuthority,
        ] {
            assert!(
                permission_difficulty(kind, CorpsePermissionScope::Exhumation)
                    > permission_difficulty(kind, CorpsePermissionScope::Examination)
            );
        }
    }

    #[test]
    fn burning_penalties_apply_to_victims_but_not_party_slain_enemies() {
        assert_eq!(
            burning_social_penalty(false),
            Some((CREMATION_INFAMY, CREMATION_FAMILY_AFFINITY_DELTA))
        );
        assert_eq!(burning_social_penalty(true), None);
        let source = include_str!("corpse.rs");
        assert!(source.contains("Ask permission to exhume a buried body"));
        assert!(!source.contains("Ask permission to exhume {}"));
    }

    #[test]
    fn corpse_family_bindings_have_an_explicit_materialization_seam() {
        let source = include_str!("corpse.rs");
        assert!(source.contains("materialize_corpse_family_bindings"));
        assert!(source.contains("family_npc_ids: &[String]"));
        assert!(!source.contains(".household"));
    }
}
