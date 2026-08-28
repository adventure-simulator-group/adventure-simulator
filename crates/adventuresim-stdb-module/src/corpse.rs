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
    character::{
        character, character__view, character_attributes, character_limbs, character_skills,
    },
    condition::character_condition,
    item::inventory_item,
    outbreak::outbreak_patient_authority,
    settlement_population::{SettlementResidentProfile, settlement_resident_profile},
    social::{character_familiarity, current_affinity},
    strategic::{
        StrategicEncounterStatus, settlement, strategic_encounter,
        strategic_gateway_authority__view,
    },
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
    GuildAuthority,
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

/// Generic bounded systemic observables fixed at death. No disease or
/// generated-case truth is stored here.
#[derive(Clone, Debug)]
#[table(accessor = corpse_pathology)]
pub struct CorpsePathology {
    #[primary_key]
    pub corpse_id: String,
    pub snapshot_json: String,
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
    pub resident_character_id: u64,
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
    pub granted_by_resident_character_id: u64,
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
    pub resident_character_id: u64,
    pub scope: CorpsePermissionScope,
    pub approach: String,
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
        .find(corpse_id.to_owned())
        .ok_or("Corpse not found")?;
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_id)
        .is_some_and(|encounter| encounter.status != StrategicEncounterStatus::Resolved)
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
    let actor_site = crate::investigation::current_character_case_site_occupancy(ctx, actor_id)
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
            family.resident_character_id,
            &format!(
                "unauthorized-autopsy:{action_id}:{}",
                family.resident_character_id
            ),
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
        AutopsyEvidenceContext, BodyInjury, PostCombatBody, SystemicPathologySnapshot,
        bestiary_finding, physiology_finding, physiology_pathology_finding, surgery_finding,
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
        "physiology" => ctx
            .db
            .corpse_pathology()
            .corpse_id()
            .find(&corpse.id)
            .and_then(|row| {
                serde_json::from_str::<SystemicPathologySnapshot>(&row.snapshot_json).ok()
            })
            .and_then(|snapshot| physiology_pathology_finding(&snapshot, check, context, internal))
            .or_else(|| {
                body.as_ref()
                    .and_then(|body| physiology_finding(body, check, context, internal))
            }),
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
                    CorpsePermissionKind::GuildAuthority => "guild",
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
    let mut inserted = 0;
    for enemy in outcome
        .enemies
        .iter()
        .filter(|enemy| adventuresim_core::autopsy::is_lethal_body(enemy))
    {
        let was_alive = ctx
            .db
            .character()
            .id()
            .find(enemy.id)
            .is_some_and(|row| row.alive);
        crate::character::transition_character_to_dead(
            ctx,
            enemy.id,
            crate::character::DeathCause::Combat,
            crate::character::DeathSource::Autoresolve,
            Some(source_id.into()),
        )?;
        let corpse_id = format!("corpse:character:{}", enemy.id);
        let mut corpse = ctx
            .db
            .strategic_corpse()
            .id()
            .find(&corpse_id)
            .ok_or("Enemy death did not create its canonical corpse")?;
        corpse.source_id = source_id.into();
        corpse.discovering_party_id = party_id.into();
        corpse.subject_character_id = Some(enemy.id);
        corpse.display_name = format!("Fallen {creature_kind}");
        corpse.creature_kind = creature_kind.into();
        corpse.settlement_id = settlement_id.into();
        corpse.case_site_id = case_site_id.into();
        corpse.party_killed_enemy = true;
        ctx.db.strategic_corpse().id().update(corpse);
        let body = post_combat_body(enemy, &outcome.log);
        if let Some(mut state) = ctx.db.corpse_body_state().corpse_id().find(&corpse_id) {
            state.health = body.health.to_vec();
            state.blood_loss_fraction = body.blood_loss_fraction;
            ctx.db.corpse_body_state().corpse_id().update(state);
        }
        if !was_alive {
            continue;
        }
        inserted += 1;
    }
    Ok(inserted)
}

pub(crate) fn persist_body(
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

pub(crate) fn persist_pathology_snapshot(
    ctx: &ReducerContext,
    corpse_id: &str,
    snapshot: &adventuresim_core::autopsy::SystemicPathologySnapshot,
) -> Result<(), String> {
    if ctx
        .db
        .corpse_pathology()
        .corpse_id()
        .find(corpse_id.to_owned())
        .is_some()
    {
        return Ok(());
    }
    ctx.db.corpse_pathology().insert(CorpsePathology {
        corpse_id: corpse_id.into(),
        snapshot_json: serde_json::to_string(snapshot)
            .map_err(|_| "Could not encode corpse pathology snapshot")?,
    });
    Ok(())
}

const AUTOPSY_DEMO_RECENT_VICTIM_ID: u64 = u64::MAX - 10_001;
const AUTOPSY_DEMO_BURIED_VICTIM_ID: u64 = u64::MAX - 10_002;
const AUTOPSY_DEMO_ENEMY_ID: u64 = u64::MAX - 10_003;

#[expect(
    clippy::too_many_arguments,
    reason = "the autopsy fixture records each authoritative battle fact explicitly"
)]
fn persist_autopsy_demo_body(
    ctx: &ReducerContext,
    actor_id: u64,
    settlement_id: &str,
    source_suffix: &str,
    display_name: &str,
    creature_kind: &str,
    victim_id: u64,
    death_minute: u64,
    discovered_minute: u64,
    buried: bool,
    party_killed_enemy: bool,
    outcome: &BattleOutcome,
) -> Result<String, String> {
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Autopsy demo character not found")?;
    let party_id = actor
        .party_id
        .ok_or("Autopsy demo character has no party")?;
    let source_id = format!("autopsy-demo:{actor_id}:{source_suffix}");
    let corpse_id = format!("corpse:{source_id}");
    if ctx.db.strategic_corpse().id().find(&corpse_id).is_none() {
        let victim = outcome
            .enemies
            .iter()
            .find(|enemy| enemy.id == victim_id)
            .ok_or("Autopsy demo outcome omitted its designated body")?;
        persist_body(
            ctx,
            StrategicCorpse {
                id: corpse_id.clone(),
                source_id,
                discovering_party_id: party_id,
                subject_character_id: None,
                display_name: display_name.into(),
                creature_kind: creature_kind.into(),
                settlement_id: settlement_id.into(),
                case_site_id: String::new(),
                death_minute,
                discovered_minute,
                buried,
                exhumed: false,
                burned: false,
                party_killed_enemy,
                handling_damage_bps: 0,
                opened: false,
                opening_quality_bps: 0,
                opening_obscuration_bps: 0,
                revision: 0,
            },
            post_combat_body(victim, &outcome.log),
        )?;
    }
    Ok(corpse_id)
}

/// Prepare the selected character and three deterministic bodies for a local
/// visual demo. Every wound is produced by ordinary strategic autoresolve;
/// only custody time and identity are staged by the fixture.
pub(crate) fn seed_autopsy_demo(ctx: &ReducerContext, actor_id: u64) -> Result<(), String> {
    let actor = crate::character::require_living_character(ctx, actor_id)?;
    let settlement_id = actor
        .current_settlement_id
        .clone()
        .ok_or("Load the autopsy demo while in a settlement")?;
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(actor_id)
        .ok_or("Autopsy demo character has no skills")?;
    skills.surgery_hours = skills.surgery_hours.max(20_000.0);
    skills.knife_hours = skills.knife_hours.max(20_000.0);
    skills.physiology_hours = skills.physiology_hours.max(20_000.0);
    skills.bestiary_hours = adventuresim_world_schema::BestiaryHours {
        beast: 8_000.0,
        undead: 8_000.0,
        human: 8_000.0,
        werekin: 8_000.0,
        elf: 8_000.0,
        dwarf: 8_000.0,
        fey: 8_000.0,
        spirit: 8_000.0,
        greenskin: 8_000.0,
        insectoid: 8_000.0,
        draconid: 8_000.0,
        construct: 8_000.0,
        wildmen: 8_000.0,
    };
    ctx.db.character_skills().character_id().update(skills);
    crate::capability::refresh_character_capability(ctx, actor_id)?;
    if !ctx
        .db
        .inventory_item()
        .character_id()
        .filter(actor_id)
        .any(|row| row.item_id == "surgery_kit")
    {
        crate::add_inventory_item(ctx, actor_id, "surgery_kit", 1);
    }
    let mut actor_time = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .ok_or("Autopsy demo character has no clock")?;
    actor_time.minutes = actor_time.minutes.max(4_000);
    let minute = actor_time.minutes;
    ctx.db.character_time().character_id().update(actor_time);

    let build_outcome = |victim_id: u64, victim_kind: &str, attacker_kind: &str, seed: u64| {
        let attacker = crate::strategic::autoresolve_enemy(
            victim_id.saturating_sub(100),
            attacker_kind,
            12,
            u32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE),
        )?;
        let victim = crate::strategic::autoresolve_enemy(
            victim_id,
            victim_kind,
            1,
            u32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE),
        )?;
        adventuresim_core::autopsy::resolve_death_required_incident(
            &[attacker],
            &[victim],
            victim_id,
            seed,
            128,
        )
        .ok_or_else(|| {
            String::from("Autopsy demo autoresolve could not produce its required death")
        })
    };
    let recent = build_outcome(
        AUTOPSY_DEMO_RECENT_VICTIM_ID,
        "poacher",
        "bear",
        0x4155_544f_5053_5901,
    )?;
    let buried = build_outcome(
        AUTOPSY_DEMO_BURIED_VICTIM_ID,
        "smuggler",
        "bear",
        0x4155_544f_5053_5902,
    )?;
    let enemy = build_outcome(
        AUTOPSY_DEMO_ENEMY_ID,
        "kobold",
        "armed_retainer",
        0x4155_544f_5053_5903,
    )?;
    let recent_id = persist_autopsy_demo_body(
        ctx,
        actor_id,
        &settlement_id,
        "recent-victim",
        "Elsbeth Bauer",
        "human",
        AUTOPSY_DEMO_RECENT_VICTIM_ID,
        minute.saturating_sub(150),
        minute.saturating_sub(120),
        false,
        false,
        &recent,
    )?;
    let buried_id = persist_autopsy_demo_body(
        ctx,
        actor_id,
        &settlement_id,
        "buried-victim",
        "Konrad Weiss",
        "human",
        AUTOPSY_DEMO_BURIED_VICTIM_ID,
        minute.saturating_sub(3_000),
        minute.saturating_sub(1_500),
        true,
        false,
        &buried,
    )?;
    persist_autopsy_demo_body(
        ctx,
        actor_id,
        &settlement_id,
        "slain-enemy",
        "Fallen kobold",
        "kobold",
        AUTOPSY_DEMO_ENEMY_ID,
        minute.saturating_sub(150),
        minute.saturating_sub(120),
        false,
        true,
        &enemy,
    )?;

    let family_npc = ctx
        .db
        .settlement_resident_profile()
        .home_settlement_id()
        .filter(&settlement_id)
        .filter(|npc| {
            npc.profession != "cleric"
                && !matches!(
                    npc.local_role.as_str(),
                    "reeve" | "local lord" | "magistrate"
                )
        })
        .min_by_key(|npc| npc.character_id)
        .ok_or("Autopsy demo settlement has no NPC available as explicit family")?;
    for corpse_id in [recent_id, buried_id] {
        materialize_corpse_family_bindings(
            ctx,
            &corpse_id,
            &settlement_id,
            std::slice::from_ref(&family_npc.character_id),
        )?;
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
    family_resident_character_ids: &[u64],
) -> Result<(), String> {
    if family_resident_character_ids.len() > MAX_BOUND_FAMILY_MEMBERS {
        return Err("Corpse family binding exceeds its bounded limit".into());
    }
    let mut unique = family_resident_character_ids.to_vec();
    unique.sort();
    unique.dedup();
    for resident_character_id in unique {
        let npc = ctx
            .db
            .settlement_resident_profile()
            .character_id()
            .find(resident_character_id)
            .ok_or("Corpse family binding references an unknown NPC")?;
        if npc.home_settlement_id != settlement_id {
            return Err("Corpse family member belongs to another settlement".into());
        }
        let id = format!("{corpse_id}:{resident_character_id}");
        if ctx.db.corpse_family_binding().id().find(&id).is_none() {
            ctx.db.corpse_family_binding().insert(CorpseFamilyBinding {
                id,
                corpse_id: corpse_id.into(),
                resident_character_id,
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
            region: strategic_body_region_label(injury.limb).into(),
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

fn strategic_body_region_label(region: adventuresim_core::physiology::BodyRegion) -> &'static str {
    use adventuresim_core::physiology::BodyRegion;
    match region {
        BodyRegion::LeftArm => "left arm",
        BodyRegion::RightArm => "right arm",
        BodyRegion::LeftLeg => "left leg",
        BodyRegion::RightLeg => "right leg",
        BodyRegion::Chest => "chest",
        BodyRegion::Abdomen => "stomach",
        BodyRegion::Head => "head",
    }
}

#[reducer]
#[expect(
    clippy::too_many_arguments,
    reason = "the reducer ABI exposes each independently validated examination input"
)]
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
    if discipline == "physiology"
        && finding.starts_with("Systemic examination finds")
        && let Some(patient) = ctx
            .db
            .outbreak_patient_authority()
            .iter()
            .find(|patient| patient.corpse_id.as_deref() == Some(corpse.id.as_str()))
        && let Some(evidence_id) = patient.autopsy_evidence_id.as_deref()
    {
        crate::investigation::record_evidence_knowledge(
            ctx,
            actor_id,
            &patient.case_id,
            evidence_id,
            &receipt_id,
        )?;
    }
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
    let exposure = adventuresim_core::surgery::procedure_blood_exposure(
        adventuresim_core::surgery::SurgeryProcedure::OpenBody,
        true,
    );
    if exposure > 0 {
        crate::filth::deposit_now(
            ctx,
            actor_id,
            adventuresim_core::filth::FilthSubstance::Blood,
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
                family.resident_character_id,
                &format!(
                    "corpse-burning:{receipt_id}:{}",
                    family.resident_character_id
                ),
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
    resident_character_id: u64,
    scope: CorpsePermissionScope,
    approach: &str,
) -> Result<bool, String> {
    use adventuresim_core::social::{
        PermissionPetitionApproach as Approach, PermissionPetitionInput,
        resolve_permission_petition,
    };
    let party_id = observer_party(ctx, actor_id)?;
    let corpse = ctx
        .db
        .strategic_corpse()
        .id()
        .find(corpse_id.to_owned())
        .ok_or("Corpse not found")?;
    if corpse.discovering_party_id != party_id {
        return Err("Party has not discovered this corpse".into());
    }
    let npc = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(resident_character_id)
        .ok_or("NPC not found")?;
    let kind = permission_kind_for_npc(ctx, &corpse, &npc)
        .ok_or("NPC cannot grant permission for this corpse")?;
    let approach = match approach {
        "personal" => Approach::PersonalAppeal,
        "command" => Approach::Command,
        "professional" => Approach::ProfessionalOpinion,
        "religious" => Approach::ReligiousPetition,
        "guild" => Approach::GuildPetition,
        _ => return Err("Unknown corpse permission approach".into()),
    };
    let approach_label = match approach {
        Approach::PersonalAppeal => "personal",
        Approach::Command => "command",
        Approach::ProfessionalOpinion => "professional",
        Approach::ReligiousPetition => "religious",
        Approach::GuildPetition => "guild",
    };
    let scope_label = permission_scope_label(scope);
    let attempt_id =
        format!("{corpse_id}:{party_id}:{resident_character_id}:{scope_label}:{approach_label}");
    if let Some(attempt) = ctx.db.corpse_permission_attempt().id().find(&attempt_id) {
        return Ok(attempt.granted);
    }
    let affinity = current_affinity(ctx, resident_character_id, actor_id);
    let (low_id, high_id) =
        adventuresim_core::social::canonical_pair(actor_id, resident_character_id)
            .ok_or("Permission petitioner and resident must differ")?;
    let familiarity_minutes = ctx
        .db
        .character_familiarity()
        .id()
        .find(format!("{low_id}:{high_id}"))
        .map_or(0, |row| row.shared_minutes);
    let familiarity_bps = ((familiarity_minutes
        .saturating_mul(u64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE))
        / (100 * 60))
        .min(u64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)))
        as u16;
    let (fame, infamy) = crate::reputation::local_reputation(ctx, actor_id, &corpse.settlement_id);
    let reputation_modifier =
        adventuresim_core::reputation::npc_reaction_modifier(fame, infamy, familiarity_bps);
    let skill_check = match approach {
        Approach::PersonalAppeal | Approach::GuildPetition => {
            crate::condition::mental_check(ctx, actor_id, Skill::Charm)?
        }
        Approach::Command => crate::condition::mental_check(ctx, actor_id, Skill::Command)?,
        Approach::ProfessionalOpinion => {
            crate::condition::mental_check(ctx, actor_id, Skill::Physiology)?
        }
        Approach::ReligiousPetition => {
            let religion_id = ctx
                .db
                .settlement()
                .id()
                .find(&corpse.settlement_id)
                .ok_or("Corpse settlement not found")?
                .religion_id;
            let religion = adventuresim_world_schema::OfficialReligion::from_id(&religion_id)
                .ok_or("Settlement religion is unknown")?;
            crate::social::target_religion_check(ctx, actor_id, religion)?
        }
    };
    let language_coefficient = ctx
        .db
        .settlement()
        .id()
        .find(&corpse.settlement_id)
        .and_then(|settlement| {
            let skills = ctx.db.character_skills().character_id().find(actor_id)?;
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(actor_id)?;
            let hours = skills
                .oral_languages
                .effective(settlement.languages.dominant_german());
            Some(
                hours.min(attributes.instinct.max(0.0) * 1_000.0)
                    / adventuresim_world_schema::ORAL_FLUENCY_HOURS,
            )
        })
        .unwrap_or(0.0);
    let difficulty = permission_difficulty(kind, scope);
    let entropy = adventuresim_core::settlement_population::stable_hash(&format!(
        "corpse-permission:{attempt_id}"
    ));
    let roll = (entropy % (u64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE) + 1))
        as f32
        / f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE);
    let professional_fit = matches!(
        npc.profession.as_str(),
        "cleric" | "physician" | "surgeon" | "local healer"
    );
    let authority_fit = match approach {
        Approach::ReligiousPetition => kind == CorpsePermissionKind::Priest,
        Approach::Command => kind == CorpsePermissionKind::SecularAuthority,
        Approach::GuildPetition => kind == CorpsePermissionKind::GuildAuthority,
        _ => true,
    };
    let granted = resolve_permission_petition(PermissionPetitionInput {
        approach,
        skill_check,
        language_coefficient,
        affinity,
        familiarity_hours: familiarity_minutes as f32 / 60.0,
        reputation_modifier,
        professional_fit,
        authority_fit,
        difficulty,
        roll,
    });
    let attempted_minute = now(ctx, actor_id)?;
    ctx.db
        .corpse_permission_attempt()
        .insert(CorpsePermissionAttempt {
            id: attempt_id,
            corpse_id: corpse_id.into(),
            party_id: party_id.clone(),
            resident_character_id,
            scope,
            approach: approach_label.into(),
            granted,
            attempted_minute,
        });
    if granted {
        let id = format!("{corpse_id}:{party_id}:{scope_label}");
        ctx.db.corpse_permission().insert(CorpsePermission {
            id,
            corpse_id: corpse_id.into(),
            party_id: party_id.clone(),
            granted_by_resident_character_id: resident_character_id,
            kind,
            scope,
            granted_minute: attempted_minute,
        });
        if kind != CorpsePermissionKind::Family {
            for relative in ctx.db.corpse_family_binding().corpse_id().filter(corpse_id) {
                crate::social::apply_corpse_family_offense(
                    ctx,
                    actor_id,
                    relative.resident_character_id,
                    &format!(
                        "family-bypassed:{corpse_id}:{}",
                        relative.resident_character_id
                    ),
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
        (CorpsePermissionKind::GuildAuthority, CorpsePermissionScope::Examination) => 3.0,
        (CorpsePermissionKind::Family, CorpsePermissionScope::Exhumation) => 3.5,
        (CorpsePermissionKind::Priest, CorpsePermissionScope::Exhumation) => 4.0,
        (CorpsePermissionKind::SecularAuthority, CorpsePermissionScope::Exhumation) => 4.5,
        (CorpsePermissionKind::GuildAuthority, CorpsePermissionScope::Exhumation) => 5.0,
    }
}

fn titled_permission_kind(
    same_settlement: bool,
    family: bool,
    profession: &str,
    local_role: &str,
    service_id: &str,
    organization_id: &str,
) -> Option<CorpsePermissionKind> {
    if !same_settlement {
        None
    } else if family {
        Some(CorpsePermissionKind::Family)
    } else if profession == "cleric" && local_role == "parish priest" && service_id == "religion" {
        Some(CorpsePermissionKind::Priest)
    } else if matches!(local_role, "reeve" | "local lord" | "magistrate") {
        Some(CorpsePermissionKind::SecularAuthority)
    } else if !organization_id.is_empty() && local_role.starts_with("master ") {
        Some(CorpsePermissionKind::GuildAuthority)
    } else {
        None
    }
}

fn permission_kind_for_npc(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    npc: &SettlementResidentProfile,
) -> Option<CorpsePermissionKind> {
    let family = ctx
        .db
        .corpse_family_binding()
        .corpse_id()
        .filter(&corpse.id)
        .any(|row| row.resident_character_id == npc.character_id);
    titled_permission_kind(
        npc.home_settlement_id == corpse.settlement_id,
        family,
        &npc.profession,
        &npc.local_role,
        &npc.service_id,
        &npc.organization_id,
    )
}

pub(crate) fn permission_topics_for_npc(
    ctx: &ReducerContext,
    actor_id: u64,
    resident_character_id: u64,
) -> Vec<(String, String)> {
    let Ok(party_id) = observer_party(ctx, actor_id) else {
        return Vec::new();
    };
    let Some(npc) = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(resident_character_id)
    else {
        return Vec::new();
    };
    let minute = now(ctx, actor_id).unwrap_or(0);
    ctx.db
        .strategic_corpse()
        .discovering_party_id()
        .filter(&party_id)
        .filter(|corpse| !corpse.burned)
        .flat_map(|corpse| {
            let Some(kind) = permission_kind_for_npc(ctx, &corpse, &npc) else {
                return Vec::new();
            };
            let approaches: &[(&str, &str)] = match kind {
                CorpsePermissionKind::Family => &[
                    ("personal", "Make a personal appeal"),
                    ("professional", "Explain the medical necessity"),
                ],
                CorpsePermissionKind::Priest => &[
                    ("religious", "Petition on religious grounds"),
                    ("professional", "Explain the medical necessity"),
                    ("personal", "Make a personal appeal"),
                ],
                CorpsePermissionKind::SecularAuthority => &[
                    ("command", "Invoke civic necessity"),
                    ("professional", "Give a professional opinion"),
                    ("personal", "Make a personal appeal"),
                ],
                CorpsePermissionKind::GuildAuthority => &[
                    ("guild", "Petition through guild responsibility"),
                    ("professional", "Give a professional opinion"),
                    ("personal", "Make a personal appeal"),
                ],
            };
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
                if eligible_location && permission_for(ctx, &corpse.id, &party_id, scope).is_none()
                {
                    for (approach, approach_label) in approaches {
                        let attempted = ctx
                            .db
                            .corpse_permission_attempt()
                            .id()
                            .find(format!(
                                "{}:{}:{}:{}:{}",
                                corpse.id,
                                party_id,
                                resident_character_id,
                                permission_scope_label(scope),
                                approach
                            ))
                            .is_some();
                        if !attempted {
                            topics.push((
                                format!(
                                    "corpse-permission:{}:{}:{}",
                                    permission_scope_label(scope),
                                    approach,
                                    corpse.id
                                ),
                                if is_exhumation {
                                    format!("{approach_label} to exhume the buried body")
                                } else {
                                    format!("{approach_label} to examine {}", corpse.display_name)
                                },
                            ));
                        }
                    }
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
        let source = crate::production_source(include_str!("corpse.rs"));
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
        let source = crate::production_source(include_str!("corpse.rs"));
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
        assert!(source.contains("SurgeryProcedure::OpenBody"));
        assert!(source.contains("CorpsePermissionKind::Family"));
        assert!(source.contains("CorpsePermissionKind::Priest"));
        assert!(source.contains("CorpsePermissionKind::SecularAuthority"));
    }

    #[test]
    fn permission_authority_is_exact_and_shared() {
        assert_eq!(
            titled_permission_kind(true, true, "laborer", "neighbor", "", ""),
            Some(CorpsePermissionKind::Family)
        );
        assert_eq!(
            titled_permission_kind(true, false, "cleric", "parish priest", "religion", "church"),
            Some(CorpsePermissionKind::Priest)
        );
        assert_eq!(
            titled_permission_kind(true, false, "retainer", "reeve", "keep", ""),
            Some(CorpsePermissionKind::SecularAuthority)
        );
        assert_eq!(
            titled_permission_kind(true, false, "servant", "keep servant", "keep", ""),
            None
        );
        assert_eq!(
            titled_permission_kind(false, true, "laborer", "neighbor", "", ""),
            None
        );

        let source = crate::production_source(include_str!("corpse.rs"));
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
            CorpsePermissionKind::GuildAuthority,
        ] {
            assert!(
                permission_difficulty(kind, CorpsePermissionScope::Exhumation)
                    > permission_difficulty(kind, CorpsePermissionScope::Examination)
            );
        }
    }

    #[test]
    fn permission_uses_relationship_and_local_reputation_inputs() {
        let source = crate::production_source(include_str!("corpse.rs"));
        assert!(source.contains("local_reputation(ctx, actor_id, &corpse.settlement_id)"));
        assert!(source.contains("npc_reaction_modifier(fame, infamy, familiarity_bps)"));
        assert!(source.contains("resolve_permission_petition"));
        assert!(source.contains("language_coefficient"));
        assert!(source.contains("target_religion_check"));
    }

    #[test]
    fn burning_penalties_apply_to_victims_but_not_party_slain_enemies() {
        assert_eq!(
            burning_social_penalty(false),
            Some((CREMATION_INFAMY, CREMATION_FAMILY_AFFINITY_DELTA))
        );
        assert_eq!(burning_social_penalty(true), None);
        let source = crate::production_source(include_str!("corpse.rs"));
        assert!(source.contains("format!(\"{approach_label} to exhume the buried body\")"));
        assert!(!source.contains("Ask permission to exhume {}"));
    }

    #[test]
    fn corpse_family_bindings_have_an_explicit_materialization_seam() {
        let source = crate::production_source(include_str!("corpse.rs"));
        assert!(source.contains("materialize_corpse_family_bindings"));
        assert!(source.contains("family_resident_character_ids: &[u64]"));
        assert!(!source.contains(".household"));
    }
}
