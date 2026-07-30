//! Durable strategic filth and automatic washing.

use adventuresim_core::filth::{
    self, Deposit, DiseaseSnapshot, SoapSource, SoapStackId, Substance, WashStack,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, table};

use crate::character::character;
use crate::personality::character_personality;
use crate::{
    character_attributes, character_time, infection_episode, inventory_item, limb_injury,
    party_inventory_item, retained_projectile,
};

pub use adventuresim_core::item_references::SOFT_SOAP_ID as SOAP_ITEM_ID;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum FilthSubstance {
    Dirt,
    Blood,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum FilthOrigin {
    Own,
    Foreign,
    Unknown,
}

/// Sanitized visible deposit. Disease identity is never copied into this table.
#[derive(Clone, Debug)]
#[table(accessor = character_filth, public)]
pub struct CharacterFilth {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub substance: FilthSubstance,
    pub origin: FilthOrigin,
    pub amount: u16,
    pub deposited_at: u64,
}

/// Exact source identity is private; public subscribers receive only origin.
#[derive(Clone, Debug)]
#[table(accessor = filth_provenance)]
pub struct FilthProvenance {
    #[primary_key]
    pub filth_id: u64,
    pub source_character_id: Option<u64>,
}

/// Private snapshot of source infection provenance at the instant of deposition.
#[derive(Clone, Debug)]
#[table(accessor = filth_disease_snapshot)]
pub struct FilthDiseaseSnapshot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub filth_id: u64,
    pub disease_id: String,
    pub episode_id: u64,
}

/// Exact fractional travel-dirt accumulator. Eight dirt per 1,440 movement
/// minutes is represented as integer dirt-minute numerator for chunk invariance.
#[derive(Clone, Debug)]
#[table(accessor = travel_filth_progress)]
pub struct TravelFilthProgress {
    #[primary_key]
    pub character_id: u64,
    pub remainder_numerator: u16,
}

/// Private deterministic exposure cursor. Absolute-minute seeds plus this
/// cursor prevent split intervals from rerolling already evaluated blood.
#[derive(Clone, Debug)]
#[table(accessor = blood_exposure_checkpoint)]
pub struct BloodExposureCheckpoint {
    #[primary_key]
    pub id: String,
    pub character_id: u64,
    pub disease_id: String,
    pub evaluated_through: u64,
}

fn predicted_wound_routes(
    ctx: &ReducerContext,
    character_id: u64,
    allow_healing: bool,
) -> Result<Vec<filth::TimedCutRoute>, String> {
    let natural = if allow_healing {
        crate::time::health_recovered_per_day(crate::time::party_physiology_check(
            ctx,
            character_id,
        )?)
    } else {
        0.0
    };
    Ok(ctx
        .db
        .limb_injury()
        .character_id()
        .filter(character_id)
        .filter(|injury| injury.cut_damage > 0.0)
        .map(|injury| {
            let state = if injury.stitched {
                filth::CutRouteState::Stitched
            } else if injury.bandaged {
                filth::CutRouteState::Bandaged
            } else {
                filth::CutRouteState::Open
            };
            let active_minutes = if allow_healing && injury.bandaged {
                let stitch_bonus = if injury.stitched {
                    injury.stitch_quality.max(0.0) * crate::surgery::STITCH_HEALING_BONUS_PER_LEVEL
                } else {
                    0.0
                };
                let projectile_term = if ctx
                    .db
                    .retained_projectile()
                    .character_id()
                    .filter(character_id)
                    .any(|projectile| projectile.limb == injury.limb)
                {
                    crate::surgery::RETAINED_PROJECTILE_HEALING_MULTIPLIER
                } else {
                    1.0
                };
                let per_day = (natural + 0.01 + stitch_bonus) * projectile_term;
                Some(((injury.cut_damage / per_day) * 1_440.0).ceil().max(1.0) as u64)
            } else {
                None
            };
            filth::TimedCutRoute {
                state,
                active_minutes,
            }
        })
        .collect())
}

pub fn blood_episodes_through(
    ctx: &ReducerContext,
    character_id: u64,
    from: u64,
    to: u64,
    persist_checkpoint: bool,
    allow_healing: bool,
) -> Result<Vec<adventuresim_core::disease::InfectionEpisode>, String> {
    if to <= from {
        return Ok(Vec::new());
    }
    let all_deposits = deposits(ctx, character_id)?;
    let has_active_compatible_foreign_blood = adventuresim_core::disease::STARTER_DISEASES
        .iter()
        .filter(|definition| {
            definition.supports(adventuresim_core::disease::TransmissionVector::Blood)
        })
        .any(|definition| {
            !filth::blood_infectious_windows(&all_deposits, definition.id, from, to).is_empty()
        });
    if !has_active_compatible_foreign_blood {
        return Ok(Vec::new());
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let routes = predicted_wound_routes(ctx, character_id, allow_healing)?;
    let mut episodes = crate::disease::character_episodes(ctx, character_id)?;
    let original_len = episodes.len();
    for disease_id in adventuresim_core::disease::STARTER_DISEASES
        .iter()
        .filter(|definition| {
            definition.supports(adventuresim_core::disease::TransmissionVector::Blood)
        })
        .map(|definition| definition.id)
    {
        let key = format!("{character_id}:{}", crate::disease::disease_key(disease_id));
        let checkpoint = ctx.db.blood_exposure_checkpoint().id().find(&key);
        let start = from.saturating_add(1).max(
            checkpoint
                .as_ref()
                .map_or(0, |row| row.evaluated_through.saturating_add(1)),
        );
        let relevant = all_deposits
            .iter()
            .filter(|deposit| {
                deposit.foreign_blood()
                    && deposit
                        .diseases
                        .iter()
                        .any(|snapshot| snapshot.disease_id == disease_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let windows =
            filth::blood_infectious_windows(&relevant, disease_id, start.saturating_sub(1), to);
        for minute in windows
            .into_iter()
            .flat_map(|(window_start, window_end)| window_start..=window_end)
        {
            if adventuresim_core::disease::has_unresolved_disease(
                &episodes, disease_id, minute, immunity,
            ) {
                continue;
            }
            let route = filth::timed_cut_exposure(&routes, minute.saturating_sub(from));
            let exposure = crate::disease::protected_exposure_at(
                ctx,
                character_id,
                minute,
                adventuresim_core::disease::TransmissionVector::Blood,
                filth::blood_exposure(&relevant, disease_id, minute, route) / 1_440.0,
            );
            if exposure <= 0.0 {
                continue;
            }
            let prior = adventuresim_core::disease::acquired_immunity(
                &episodes, disease_id, minute, immunity,
            );
            let seed = adventuresim_core::disease::outbreak_exposure_seed(
                character_id,
                &format!("blood:{}:{minute}", crate::disease::disease_key(disease_id)),
            );
            if adventuresim_core::disease::acquisition_succeeds(
                seed,
                adventuresim_core::disease::definition(disease_id),
                immunity,
                prior,
                exposure,
            ) {
                episodes.push(adventuresim_core::disease::InfectionEpisode {
                    id: seed,
                    character_id,
                    disease_id,
                    contracted_at: minute,
                    ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
                    phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
                });
                break;
            }
        }
        if persist_checkpoint {
            let row = BloodExposureCheckpoint {
                id: key,
                character_id,
                disease_id: crate::disease::disease_key(disease_id).into(),
                evaluated_through: to,
            };
            if checkpoint.is_some() {
                ctx.db.blood_exposure_checkpoint().id().update(row);
            } else {
                ctx.db.blood_exposure_checkpoint().insert(row);
            }
        }
    }
    Ok(episodes.split_off(original_len))
}

pub fn next_travel_dirt_boundary(ctx: &ReducerContext, character_id: u64) -> u64 {
    let remainder = ctx
        .db
        .travel_filth_progress()
        .character_id()
        .find(character_id)
        .map_or(0, |row| u64::from(row.remainder_numerator));
    (1_440 - remainder).div_ceil(8).max(1)
}

pub fn record_travel_elapsed(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u64,
    at: u64,
) -> Result<u16, String> {
    let mut row = ctx
        .db
        .travel_filth_progress()
        .character_id()
        .find(character_id)
        .unwrap_or(TravelFilthProgress {
            character_id,
            remainder_numerator: 0,
        });
    let (dirt, remainder) = filth::travel_dirt_accrual(row.remainder_numerator, minutes);
    row.remainder_numerator = remainder;
    if ctx
        .db
        .travel_filth_progress()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db.travel_filth_progress().character_id().update(row);
    } else {
        ctx.db.travel_filth_progress().insert(row);
    }
    if dirt > 0 {
        deposit(ctx, character_id, FilthSubstance::Dirt, None, dirt, at)?;
    }
    Ok(dirt)
}

fn total(ctx: &ReducerContext, character_id: u64) -> u16 {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|d| d.amount)
        .fold(0u16, u16::saturating_add)
        .min(filth::MAX_FILTH)
}

pub fn dirt_total(ctx: &ReducerContext, character_id: u64) -> u16 {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0u16, u16::saturating_add)
        .min(filth::MAX_FILTH)
}

/// Reusable strategic boundary: callers persist only final dirt/blood outcomes.
pub fn deposit(
    ctx: &ReducerContext,
    character_id: u64,
    substance: FilthSubstance,
    source_character_id: Option<u64>,
    amount: u16,
    at: u64,
) -> Result<Option<u64>, String> {
    let amount = filth::bounded_deposit_amount(total(ctx, character_id), amount);
    if amount == 0 {
        return Ok(None);
    }
    let row = ctx.db.character_filth().insert(CharacterFilth {
        id: 0,
        character_id,
        substance,
        origin: match source_character_id {
            Some(source) if source == character_id => FilthOrigin::Own,
            Some(_) => FilthOrigin::Foreign,
            None => FilthOrigin::Unknown,
        },
        amount,
        deposited_at: at,
    });
    ctx.db.filth_provenance().insert(FilthProvenance {
        filth_id: row.id,
        source_character_id,
    });
    if substance == FilthSubstance::Blood {
        if let Some(source) = source_character_id {
            let immunity = ctx
                .db
                .character_attributes()
                .character_id()
                .find(source)
                .map_or(3.0, |attributes| attributes.immunity);
            let mut seen = std::collections::BTreeSet::new();
            for episode in ctx.db.infection_episode().character_id().filter(source) {
                let disease_id = crate::disease::parse_id(&episode.disease_id)?;
                if !adventuresim_core::disease::definition(disease_id)
                    .supports(adventuresim_core::disease::TransmissionVector::Blood)
                    || episode.contracted_at > at
                    || matches!(
                        adventuresim_core::disease::evaluate(
                            adventuresim_core::disease::InfectionEpisode {
                                id: episode.id,
                                character_id: source,
                                disease_id,
                                contracted_at: episode.contracted_at,
                                ruleset_version: episode.ruleset_version,
                                phenotype_key_version: episode.phenotype_key_version,
                            },
                            at,
                            immunity,
                        )
                        .stage,
                        adventuresim_core::disease::DiseaseStage::Resolved
                    )
                    || !seen.insert((disease_id as u8, episode.id))
                {
                    continue;
                }
                ctx.db
                    .filth_disease_snapshot()
                    .insert(FilthDiseaseSnapshot {
                        id: 0,
                        filth_id: row.id,
                        disease_id: crate::disease::disease_key(disease_id).into(),
                        episode_id: episode.id,
                    });
            }
        }
    }
    Ok(Some(row.id))
}

pub fn deposit_now(
    ctx: &ReducerContext,
    character_id: u64,
    substance: FilthSubstance,
    source_character_id: Option<u64>,
    amount: u16,
) -> Result<Option<u64>, String> {
    let at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    deposit(
        ctx,
        character_id,
        substance,
        source_character_id,
        amount,
        at,
    )
}

pub(crate) fn deposits(ctx: &ReducerContext, character_id: u64) -> Result<Vec<Deposit>, String> {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|row| -> Result<Deposit, String> {
            let diseases = ctx
                .db
                .filth_disease_snapshot()
                .filth_id()
                .filter(row.id)
                .map(|s| {
                    crate::disease::parse_id(&s.disease_id).map(|disease_id| DiseaseSnapshot {
                        disease_id,
                        episode_id: s.episode_id,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let source_character_id = ctx
                .db
                .filth_provenance()
                .filth_id()
                .find(row.id)
                .ok_or("Filth provenance is missing")?
                .source_character_id;
            Ok(Deposit {
                id: row.id,
                character_id,
                substance: match row.substance {
                    FilthSubstance::Dirt => Substance::Dirt,
                    FilthSubstance::Blood => Substance::Blood,
                },
                source_character_id,
                amount: row.amount,
                deposited_at: row.deposited_at,
                diseases,
            })
        })
        .collect()
}

fn has_cut(ctx: &ReducerContext, character_id: u64) -> bool {
    ctx.db
        .limb_injury()
        .character_id()
        .filter(character_id)
        .any(|i| i.cut_damage > 0.0)
}

pub const SOAP_MILLIUNITS_PER_CLEANSING_POINT: u32 =
    crate::inventory_amount::FULL_AMOUNT_MILLIUNITS / filth::SOAP_CLEANSING_CAPACITY as u32;

fn consume_personal(ctx: &ReducerContext, stack_id: u64, points: u32) -> Result<(), String> {
    let stack = ctx
        .db
        .inventory_item()
        .id()
        .find(stack_id)
        .ok_or("Planned personal soap stack is missing")?;
    if stack.item_id != SOAP_ITEM_ID {
        return Err("Planned personal stack is not soap".into());
    }
    let amount = points
        .checked_mul(SOAP_MILLIUNITS_PER_CLEANSING_POINT)
        .ok_or("Planned personal soap amount overflow")?;
    crate::inventory_amount::consume_personal(ctx, stack.id, amount)?;
    Ok(())
}
fn consume_party(ctx: &ReducerContext, stack_id: u64, points: u32) -> Result<(), String> {
    let stack = ctx
        .db
        .party_inventory_item()
        .id()
        .find(stack_id)
        .ok_or("Planned shared soap stack is missing")?;
    if stack.item_id != SOAP_ITEM_ID {
        return Err("Planned shared stack is not soap".into());
    }
    let amount = points
        .checked_mul(SOAP_MILLIUNITS_PER_CLEANSING_POINT)
        .ok_or("Planned shared soap amount overflow")?;
    crate::inventory_amount::consume_party(ctx, stack.id, amount)?;
    Ok(())
}

pub fn consume_personal_soap_points(
    ctx: &ReducerContext,
    stack_id: u64,
    points: u32,
) -> Result<(), String> {
    let required = points
        .checked_mul(SOAP_MILLIUNITS_PER_CLEANSING_POINT)
        .ok_or("Requested soap amount overflow")?;
    if crate::inventory_amount::personal_amount(ctx, stack_id).unwrap_or(0) < required {
        return Err("Not enough soap remains for the requested use".into());
    }
    consume_personal(ctx, stack_id, points)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WashSummary {
    pub total_units: u32,
    pub personal_units: u32,
    pub shared_units: u32,
    pub stacks: Vec<(SoapStackId, u32)>,
}

struct PlannedCharacterWash {
    character_id: u64,
    plan: filth::WashPlan,
}

fn take_units(pool: &mut [WashStack], mut wanted: u32) -> Vec<WashStack> {
    let mut assigned = Vec::new();
    pool.sort_by_key(|stack| stack.key);
    for stack in pool {
        if wanted == 0 {
            break;
        }
        let quantity = stack.quantity.min(wanted);
        stack.quantity -= quantity;
        wanted -= quantity;
        if quantity > 0 {
            assigned.push(WashStack {
                key: stack.key,
                quantity,
            });
        }
    }
    assigned
}

fn plan_party_wash(
    ctx: &ReducerContext,
    character_ids: &[u64],
) -> Result<(Vec<PlannedCharacterWash>, WashSummary), String> {
    struct Subject {
        id: u64,
        dirty: Vec<Deposit>,
        cut: bool,
        assigned: Vec<WashStack>,
        remaining: u32,
    }
    let mut subjects = Vec::new();
    let mut party_id = None;
    for id in character_ids.iter().copied() {
        let character = ctx
            .db
            .character()
            .id()
            .find(id)
            .ok_or("Character not found")?;
        if !character.alive {
            continue;
        }
        party_id = party_id.or(character.party_id.clone());
        let dirty = deposits(ctx, id)?;
        if dirty.is_empty() {
            continue;
        }
        let needed = dirty.iter().map(|d| u32::from(d.amount)).sum::<u32>();
        let mut personal = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((id, SOAP_ITEM_ID))
            .map(|stack| WashStack {
                key: SoapStackId {
                    source: SoapSource::Personal,
                    id: stack.id,
                },
                quantity: crate::inventory_amount::personal_amount(ctx, stack.id).unwrap_or(0)
                    / SOAP_MILLIUNITS_PER_CLEANSING_POINT,
            })
            .collect::<Vec<_>>();
        let assigned = take_units(&mut personal, needed);
        let personal_used = assigned.iter().map(|stack| stack.quantity).sum::<u32>();
        subjects.push(Subject {
            id,
            cut: has_cut(ctx, id),
            dirty,
            assigned,
            remaining: needed.saturating_sub(personal_used),
        });
    }
    let mut shared = party_id.as_ref().map_or_else(Vec::new, |party_id| {
        ctx.db
            .party_inventory_item()
            .party_id()
            .filter(party_id)
            .filter(|stack| stack.item_id == SOAP_ITEM_ID)
            .map(|stack| WashStack {
                key: SoapStackId {
                    source: SoapSource::Party,
                    id: stack.id,
                },
                quantity: crate::inventory_amount::party_amount(ctx, stack.id).unwrap_or(0)
                    / SOAP_MILLIUNITS_PER_CLEANSING_POINT,
            })
            .collect::<Vec<_>>()
    });
    let mut priorities = subjects
        .iter()
        .map(|subject| {
            let now = ctx
                .db
                .character_time()
                .character_id()
                .find(subject.id)
                .map_or(0, |row| row.minutes);
            let exposure = filth::blood_exposure(
                &subject.dirty,
                adventuresim_core::disease::DiseaseId::Plague,
                now,
                filth::timed_cut_exposure(&predicted_wound_routes(ctx, subject.id, false)?, 0),
            );
            Ok::<_, String>(filth::wash_priority(
                subject.id,
                &subject.dirty,
                subject.cut,
                exposure,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    filth::sort_wash_priorities(&mut priorities);
    for priority in priorities {
        let subject = subjects
            .iter_mut()
            .find(|subject| subject.id == priority.character_id)
            .unwrap();
        let assigned = take_units(&mut shared, subject.remaining);
        let used = assigned.iter().map(|stack| stack.quantity).sum::<u32>();
        subject.remaining -= used;
        subject.assigned.extend(assigned);
    }
    let mut summary = WashSummary::default();
    let planned = subjects
        .into_iter()
        .map(|subject| {
            let plan = filth::plan_wash(&subject.dirty, &subject.assigned, subject.cut);
            for (key, quantity) in &plan.soap_stacks {
                summary.total_units += quantity;
                match key.source {
                    SoapSource::Personal => summary.personal_units += quantity,
                    SoapSource::Party => summary.shared_units += quantity,
                }
                summary.stacks.push((*key, *quantity));
            }
            PlannedCharacterWash {
                character_id: subject.id,
                plan,
            }
        })
        .collect();
    summary.stacks.sort_by_key(|(key, _)| *key);
    Ok((planned, summary))
}

pub fn preview_party_wash(
    ctx: &ReducerContext,
    character_ids: &[u64],
) -> Result<WashSummary, String> {
    plan_party_wash(ctx, character_ids).map(|(_, summary)| summary)
}

pub fn wash_party_before_explicit_rest(
    ctx: &ReducerContext,
    character_ids: &[u64],
) -> Result<WashSummary, String> {
    let (planned, summary) = plan_party_wash(ctx, character_ids)?;
    // Preflight exact tagged identities before the first mutation.
    for (key, quantity) in &summary.stacks {
        match key.source {
            SoapSource::Personal => {
                let stack = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(key.id)
                    .ok_or("Planned personal soap stack is missing")?;
                if stack.item_id != SOAP_ITEM_ID
                    || crate::inventory_amount::personal_amount(ctx, stack.id).unwrap_or(0)
                        < quantity.saturating_mul(SOAP_MILLIUNITS_PER_CLEANSING_POINT)
                {
                    return Err("Planned personal soap is no longer available".into());
                }
            }
            SoapSource::Party => {
                let stack = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(key.id)
                    .ok_or("Planned shared soap stack is missing")?;
                if stack.item_id != SOAP_ITEM_ID
                    || crate::inventory_amount::party_amount(ctx, stack.id).unwrap_or(0)
                        < quantity.saturating_mul(SOAP_MILLIUNITS_PER_CLEANSING_POINT)
                {
                    return Err("Planned shared soap is no longer available".into());
                }
            }
        }
    }
    for (key, quantity) in &summary.stacks {
        match key.source {
            SoapSource::Personal => consume_personal(ctx, key.id, *quantity)?,
            SoapSource::Party => consume_party(ctx, key.id, *quantity)?,
        }
    }
    for character in planned {
        for (id, removed) in character.plan.cleaned_deposits {
            if let Some(mut row) = ctx.db.character_filth().id().find(id) {
                if row.character_id != character.character_id || row.amount < removed {
                    return Err("Planned filth deposit changed before washing".into());
                }
                row.amount = row
                    .amount
                    .checked_sub(removed)
                    .ok_or("Filth removal underflow")?;
                if row.amount == 0 {
                    for snapshot in ctx
                        .db
                        .filth_disease_snapshot()
                        .filth_id()
                        .filter(id)
                        .collect::<Vec<_>>()
                    {
                        ctx.db.filth_disease_snapshot().id().delete(snapshot.id);
                    }
                    ctx.db.filth_provenance().filth_id().delete(id);
                    ctx.db.character_filth().id().delete(id);
                } else {
                    ctx.db.character_filth().id().update(row);
                }
            } else {
                return Err("Planned filth deposit is missing".into());
            }
        }
    }
    Ok(summary)
}

/// Single-character settlement rest wrapper.
pub fn wash_before_explicit_rest(ctx: &ReducerContext, character_id: u64) -> Result<u32, String> {
    Ok(wash_party_before_explicit_rest(ctx, &[character_id])?.total_units)
}

pub fn preview_soap_units(ctx: &ReducerContext, character_id: u64) -> u32 {
    let amount: u32 = ctx
        .db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|d| u32::from(d.amount))
        .sum();
    if amount == 0 {
        return 0;
    }
    let available: u32 = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, SOAP_ITEM_ID))
        .map(|s| {
            crate::inventory_amount::personal_amount(ctx, s.id).unwrap_or(0)
                / SOAP_MILLIUNITS_PER_CLEANSING_POINT
        })
        .sum::<u32>()
        + ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|c| c.party_id)
            .map_or(0, |p| {
                ctx.db
                    .party_inventory_item()
                    .iter()
                    .filter(|s| s.party_id == p && s.item_id == SOAP_ITEM_ID)
                    .map(|s| {
                        crate::inventory_amount::party_amount(ctx, s.id).unwrap_or(0)
                            / SOAP_MILLIUNITS_PER_CLEANSING_POINT
                    })
                    .sum()
            });
    amount.min(available)
}

pub(crate) fn seed_demo(
    ctx: &ReducerContext,
    character_id: u64,
    foreign_source_id: u64,
) -> Result<(), String> {
    for row in ctx
        .db
        .character_filth()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        for snapshot in ctx
            .db
            .filth_disease_snapshot()
            .filth_id()
            .filter(row.id)
            .collect::<Vec<_>>()
        {
            ctx.db.filth_disease_snapshot().id().delete(snapshot.id);
        }
        ctx.db.character_filth().id().delete(row.id);
    }
    deposit(ctx, character_id, FilthSubstance::Dirt, None, 38, 86_000)?;
    deposit(
        ctx,
        character_id,
        FilthSubstance::Blood,
        Some(foreign_source_id),
        27,
        86_200,
    )?;
    if let Some(mut personality) = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
    {
        personality.hygiene = crate::personality::Hygiene::Cleanly;
        ctx.db
            .character_personality()
            .character_id()
            .update(personality);
    }
    let existing = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, SOAP_ITEM_ID))
        .map(|row| row.quantity)
        .sum::<u32>();
    if existing < 3 {
        crate::add_inventory_item(ctx, character_id, SOAP_ITEM_ID, 3 - existing);
    }
    Ok(())
}

#[cfg(test)]
mod source_tests {
    #[test]
    fn blood_route_receives_partial_physician_protection_after_physical_controls() {
        let source = include_str!("filth.rs");
        let exposure = source
            .split("pub fn blood_episodes_through")
            .nth(1)
            .and_then(|tail| tail.split("/// Reusable strategic boundary").next())
            .expect("blood exposure source");
        let prevention = exposure.find("protected_exposure_at").unwrap();
        let physical = exposure.find("filth::blood_exposure").unwrap();
        assert!(prevention < physical);
        assert!(exposure.contains("TransmissionVector::Blood"));
        assert!(exposure.contains("timed_cut_exposure"));
    }
}
