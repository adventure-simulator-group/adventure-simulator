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
    settlement_population::settlement_npc,
    strategic::strategic_gateway_authority__view,
    surgery::{limb_injury, retained_projectile},
    time::{advance_character_wait_time, character_time, character_time__view},
};

const EXTERNAL_EXAMINATION_MINUTES: u64 = 20;
const INTERNAL_EXAMINATION_MINUTES: u64 = 45;
const OPEN_BODY_MINUTES: u64 = 60;
const EXHUMATION_MINUTES: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CorpsePermissionKind {
    Family,
    Priest,
    SecularAuthority,
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
    pub exhumed: bool,
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
    pub granted_minute: u64,
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
) -> Option<CorpsePermission> {
    ctx.db
        .corpse_permission()
        .corpse_id()
        .filter(corpse_id)
        .find(|row| row.party_id == party_id)
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
    let location = corpse_location(corpse.discovered_minute, minute, corpse.exhumed);
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
) -> Result<(), String> {
    if permission_for(ctx, &corpse.id, party_id).is_some() {
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

fn action_receipt_id(actor_id: u64, corpse_id: &str, action_id: &str) -> String {
    format!("{actor_id}:{corpse_id}:{action_id}")
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
) -> String {
    let injuries: Vec<_> = ctx
        .db
        .corpse_injury()
        .corpse_id()
        .filter(&corpse.id)
        .collect();
    match discipline {
        "surgery" if check >= 2.0 => format!(
            "External wound examination distinguishes {} bounded wound tracks and their tissue geometry.",
            injuries.len()
        ),
        "surgery" => {
            "External examination confirms trauma, but wound geometry is uncertain.".into()
        }
        "physiology" if check >= 2.0 => {
            let blood = ctx
                .db
                .corpse_body_state()
                .corpse_id()
                .find(&corpse.id)
                .map_or(0.0, |body| body.blood_loss_fraction);
            format!(
                "External signs are consistent with substantial systemic stress; estimated blood loss is roughly {:.0}%.",
                blood * 100.0
            )
        }
        "physiology" => {
            "External signs show bodily collapse without a reliable physiological interpretation."
                .into()
        }
        "bestiary" if check >= 2.0 => format!(
            "Learned lore recognizes physical patterns compatible with {}, without identifying a culprit.",
            corpse.creature_kind
        ),
        "bestiary" => "No learned creature lore securely explains the observed marks.".into(),
        _ => unreachable!(),
    }
}

fn summarize_internal(
    ctx: &ReducerContext,
    corpse: &StrategicCorpse,
    discipline: &str,
    check: f32,
) -> String {
    let obscured = corpse.opening_obscuration_bps;
    match discipline {
        "surgery" if check >= 2.0 => format!(
            "Internal tissue damage can be separated from the incision with about {}% obscuration.",
            obscured / 100
        ),
        "surgery" => "Incision damage makes several internal wound margins ambiguous.".into(),
        "physiology" if check >= 2.0 && obscured < 7_500 => {
            let body = ctx.db.corpse_body_state().corpse_id().find(&corpse.id);
            let worst = body
                .map(|body| body.health.into_iter().fold(1.0_f32, f32::min))
                .unwrap_or(1.0);
            format!("Internal organ condition supports severe systemic failure; the worst region retained about {:.0}% function.", worst * 100.0)
        }
        "physiology" => "Internal systemic effects remain uncertain because decomposition or dissection obscured them.".into(),
        "bestiary" if check >= 2.0 && obscured < 7_500 => format!(
            "Learned lore finds the internal pattern compatible with {}, among other possibilities.",
            corpse.creature_kind
        ),
        "bestiary" => "No creature-specific internal pattern can be defended from the visible evidence.".into(),
        _ => unreachable!(),
    }
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
        {
            let permission = ctx
                .db
                .corpse_permission()
                .corpse_id()
                .filter(&corpse.id)
                .find(|row| row.party_id == party_id)
                .map_or("none", |row| match row.kind {
                    CorpsePermissionKind::Family => "family",
                    CorpsePermissionKind::Priest => "priest",
                    CorpsePermissionKind::SecularAuthority => "authority",
                });
            let findings = ctx
                .db
                .autopsy_action_receipt()
                .observer_id()
                .filter(actor.id)
                .filter(|row| row.corpse_id == corpse.id)
                .map(|row| row.finding)
                .collect();
            rows.push(BackendCorpse {
                owner_character_id: actor.id,
                corpse_id: corpse.id.clone(),
                display_name: corpse.display_name.clone(),
                creature_kind: corpse.creature_kind.clone(),
                source_id: corpse.source_id.clone(),
                location: location_label(corpse_location(
                    corpse.discovered_minute,
                    minute,
                    corpse.exhumed,
                ))
                .into(),
                decomposition: decomposition_label(decomposition_band(
                    corpse.death_minute,
                    minute,
                    corpse.handling_damage_bps,
                ))
                .into(),
                case_site_id: corpse.case_site_id.clone(),
                settlement_id: corpse.settlement_id.clone(),
                opened: corpse.opened,
                permission: permission.into(),
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
                exhumed: false,
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
    ctx.db.strategic_corpse().insert(StrategicCorpse {
        id: corpse_id.clone(),
        source_id: source_id.into(),
        discovering_party_id: subject.party_id.clone().unwrap_or_default(),
        subject_character_id: Some(character_id),
        display_name: subject.name,
        creature_kind: "human".into(),
        settlement_id: subject.current_settlement_id.unwrap_or_default(),
        case_site_id,
        death_minute,
        discovered_minute: death_minute,
        exhumed: false,
        handling_damage_bps: 0,
        opened: false,
        opening_quality_bps: 0,
        opening_obscuration_bps: 0,
        revision: 0,
    });
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
    let receipt_id = action_receipt_id(actor_id, &corpse_id, &action_id);
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
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before examining it".into());
    }
    if stage == "internal" && !corpse.opened {
        return Err("The body has not been opened".into());
    }
    if !matches!(stage.as_str(), "external" | "internal") {
        return Err("Unknown examination stage".into());
    }
    if permission_for(ctx, &corpse_id, &party_id).is_none() && !confirm_unauthorized {
        return Err(
            "Permission is missing; confirm the likely family penalties and settlement infamy"
                .into(),
        );
    }
    apply_unauthorized_consequences(ctx, actor_id, &corpse, &party_id, &action_id)?;
    let duration = if stage == "external" {
        EXTERNAL_EXAMINATION_MINUTES
    } else {
        INTERNAL_EXAMINATION_MINUTES
    };
    if !advance_character_wait_time(ctx, actor_id, duration)? {
        return Ok(());
    }
    let check = skill_check(ctx, actor_id, &discipline)?;
    let finding = if stage == "external" {
        summarize_external(ctx, &corpse, &discipline, check)
    } else {
        summarize_internal(ctx, &corpse, &discipline, check)
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
            performed_minute: minute.saturating_add(duration),
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
    let receipt_id = action_receipt_id(actor_id, &corpse_id, &action_id);
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
    if corpse.opened {
        return Ok(());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before opening it".into());
    }
    if permission_for(ctx, &corpse_id, &party_id).is_none() && !confirm_unauthorized {
        return Err(
            "Permission is missing; confirm the likely family penalties and settlement infamy"
                .into(),
        );
    }
    apply_unauthorized_consequences(ctx, actor_id, &corpse, &party_id, &action_id)?;
    let surgery = skill_check(ctx, actor_id, "surgery")?;
    if !advance_character_wait_time(ctx, actor_id, OPEN_BODY_MINUTES)? {
        return Ok(());
    }
    let entropy = (adventuresim_core::settlement_population::stable_hash(&format!(
        "{}:{action_id}:opening",
        corpse.id
    )) % 10_001) as u16;
    let (quality, obscuration) = opening_quality_bps(surgery, entropy);
    corpse.opened = true;
    corpse.opening_quality_bps = quality;
    corpse.opening_obscuration_bps = obscuration;
    corpse.handling_damage_bps = corpse.handling_damage_bps.saturating_add(obscuration / 8);
    corpse.revision = corpse.revision.saturating_add(1);
    ctx.db.strategic_corpse().id().update(corpse);
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
        performed_minute: minute.saturating_add(OPEN_BODY_MINUTES),
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
    let receipt_id = action_receipt_id(actor_id, &corpse_id, &action_id);
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
    if corpse_location(corpse.discovered_minute, minute, corpse.exhumed) != CorpseLocation::Interred
    {
        return Err("Only an interred corpse can be exhumed".into());
    }
    if corpse.revision != expected_revision {
        return Err("Corpse state changed; refresh before exhuming it".into());
    }
    if permission_for(ctx, &corpse_id, &party_id).is_none() && !confirm_unauthorized {
        return Err(
            "Permission is missing; confirm the severe family penalty and settlement infamy".into(),
        );
    }
    apply_unauthorized_consequences(ctx, actor_id, &corpse, &party_id, &action_id)?;
    if !advance_character_wait_time(ctx, actor_id, EXHUMATION_MINUTES)? {
        return Ok(());
    }
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
            performed_minute: minute.saturating_add(EXHUMATION_MINUTES),
        });
    Ok(())
}

pub(crate) fn grant_permission_from_dialogue(
    ctx: &ReducerContext,
    actor_id: u64,
    corpse_id: &str,
    npc_id: &str,
) -> Result<(), String> {
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
    if npc.home_settlement_id != corpse.settlement_id {
        return Err("NPC has no local authority over this corpse".into());
    }
    let family = ctx
        .db
        .corpse_family_binding()
        .corpse_id()
        .filter(corpse_id)
        .any(|row| row.npc_id == npc_id);
    let priest = npc.profession.contains("priest")
        || npc.local_role.contains("priest")
        || npc.organization_id.contains("church");
    let authority = npc.local_role.contains("lord")
        || npc.local_role.contains("reeve")
        || npc.local_role.contains("authority")
        || npc.service_id == "keep";
    let kind = if family {
        CorpsePermissionKind::Family
    } else if priest {
        CorpsePermissionKind::Priest
    } else if authority {
        CorpsePermissionKind::SecularAuthority
    } else {
        return Err("NPC cannot grant permission for this corpse".into());
    };
    let id = format!("{corpse_id}:{party_id}");
    if ctx.db.corpse_permission().id().find(&id).is_none() {
        ctx.db.corpse_permission().insert(CorpsePermission {
            id,
            corpse_id: corpse_id.into(),
            party_id: party_id.clone(),
            granted_by_npc_id: npc_id.into(),
            kind,
            granted_minute: now(ctx, actor_id)?,
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
    Ok(())
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
    ctx.db
        .strategic_corpse()
        .discovering_party_id()
        .filter(&party_id)
        .filter(|corpse| {
            permission_for(ctx, &corpse.id, &party_id).is_none()
                && (ctx
                    .db
                    .corpse_family_binding()
                    .corpse_id()
                    .filter(&corpse.id)
                    .any(|row| row.npc_id == npc_id)
                    || npc.profession.contains("priest")
                    || npc.local_role.contains("priest")
                    || npc.organization_id.contains("church")
                    || npc.local_role.contains("lord")
                    || npc.local_role.contains("reeve")
                    || npc.local_role.contains("authority")
                    || npc.service_id == "keep")
        })
        .map(|corpse| {
            (
                format!("corpse-permission:{}", corpse.id),
                format!("Ask permission to examine {}", corpse.display_name),
            )
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
        assert!(source.contains("action_receipt_id(actor_id, &corpse_id, &action_id)"));
        assert!(source.contains("confirm_unauthorized"));
        assert!(source.contains("apply_unauthorized_consequences"));
        assert!(source.contains("permission_for(ctx, &corpse_id, &party_id)"));
        assert!(source.contains("actor_site.as_deref() == Some(corpse.case_site_id.as_str())"));
        assert!(source.contains(
            "actor.current_settlement_id.as_deref() == Some(corpse.settlement_id.as_str())"
        ));
        assert!(source.contains("CorpsePermissionKind::Family"));
        assert!(source.contains("CorpsePermissionKind::Priest"));
        assert!(source.contains("CorpsePermissionKind::SecularAuthority"));
    }
}
