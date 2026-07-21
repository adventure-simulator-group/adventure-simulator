//! Framework-neutral strategic filth, exposure, and automatic washing rules.

use crate::disease::{DiseaseId, TransmissionVector, definition};

/// The character sheet meter is deliberately bounded and deposits are clipped.
pub const MAX_FILTH: u16 = 100;
/// One whole unit of soft soap removes this much filth. Unused capacity is lost.
pub const SOAP_CLEANSING_CAPACITY: u16 = 25;
/// Blood remains visible, but its disease exposure falls linearly to zero after two days.
pub const BLOOD_INFECTIOUS_MINUTES: u64 = 2 * 1_440;
pub const TRAVEL_DIRT_PER_DAY: f32 = 8.0;
pub const COMBAT_DIRT: u16 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Substance {
    Dirt,
    Blood,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiseaseSnapshot {
    pub disease_id: DiseaseId,
    pub episode_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    pub id: u64,
    pub character_id: u64,
    pub substance: Substance,
    pub source_character_id: Option<u64>,
    pub amount: u16,
    pub deposited_at: u64,
    pub diseases: Vec<DiseaseSnapshot>,
}

impl Deposit {
    pub fn foreign_blood(&self) -> bool {
        self.substance == Substance::Blood && self.source_character_id != Some(self.character_id)
    }

    pub fn compatible_diseased_blood(&self) -> bool {
        self.substance == Substance::Blood
            && self
                .diseases
                .iter()
                .any(|d| definition(d.disease_id).supports(TransmissionVector::Blood))
    }
}

pub fn bounded_deposit_amount(existing: u16, requested: u16) -> u16 {
    requested.min(MAX_FILTH.saturating_sub(existing.min(MAX_FILTH)))
}

pub fn travel_dirt(minutes: u64) -> u16 {
    ((minutes as f32 / 1_440.0) * TRAVEL_DIRT_PER_DAY).round() as u16
}

/// Carries exact travel-dirt progress between movement chunks. The remainder is
/// measured in dirt-minutes over a denominator of 1,440.
pub fn travel_dirt_accrual(remainder_numerator: u16, minutes: u64) -> (u16, u16) {
    let numerator =
        u128::from(remainder_numerator).saturating_add(u128::from(minutes).saturating_mul(8));
    let dirt = u16::try_from(numerator / 1_440).unwrap_or(u16::MAX);
    (dirt, (numerator % 1_440) as u16)
}

pub fn infectious_fraction(deposited_at: u64, now: u64) -> f32 {
    let age = now.saturating_sub(deposited_at);
    if age >= BLOOD_INFECTIOUS_MINUTES {
        0.0
    } else {
        1.0 - age as f32 / BLOOD_INFECTIOUS_MINUTES as f32
    }
}

/// Combines cut routes with diminishing returns. Open cuts are most vulnerable;
/// bandaging helps substantially and stitching helps further.
pub fn cut_exposure(open: u32, bandaged: u32, stitched: u32) -> f32 {
    let raw = open as f32 + bandaged as f32 * 0.40 + stitched as f32 * 0.18;
    1.0 - (-raw).exp()
}

pub fn dirt_wound_multiplier(total_dirt: u16) -> f32 {
    1.0 + 0.35 * (f32::from(total_dirt.min(MAX_FILTH)) / f32::from(MAX_FILTH))
}

pub fn blood_exposure(
    deposits: &[Deposit],
    disease_id: DiseaseId,
    now: u64,
    cut_route: f32,
) -> f32 {
    blood_exposure_for_vector(
        deposits,
        disease_id,
        now,
        cut_route,
        definition(disease_id).supports(TransmissionVector::Blood),
    )
}

pub fn blood_exposure_for_vector(
    deposits: &[Deposit],
    disease_id: DiseaseId,
    now: u64,
    cut_route: f32,
    blood_compatible: bool,
) -> f32 {
    if !blood_compatible {
        return 0.0;
    }
    let dose = deposits
        .iter()
        .filter(|d| d.substance == Substance::Blood && d.foreign_blood())
        .filter(|d| d.diseases.iter().any(|s| s.disease_id == disease_id))
        .map(|d| {
            f32::from(d.amount) / f32::from(MAX_FILTH) * infectious_fraction(d.deposited_at, now)
        })
        .sum::<f32>();
    (dose * (0.04 + 0.96 * cut_route.clamp(0.0, 1.0))).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SoapSource {
    Personal,
    Party,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SoapStackId {
    pub source: SoapSource,
    pub id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WashStack {
    pub key: SoapStackId,
    pub quantity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WashPlan {
    pub soap_stacks: Vec<(SoapStackId, u32)>,
    pub cleaned_deposits: Vec<(u64, u16)>,
}

fn cleaning_rank(d: &Deposit, has_cut: bool) -> u8 {
    if d.compatible_diseased_blood() {
        0
    } else if d.foreign_blood() && has_cut {
        1
    } else if d.foreign_blood() {
        2
    } else if d.substance == Substance::Blood {
        3
    } else {
        4
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WashPriority {
    pub character_id: u64,
    pub best_substance_rank: u8,
    pub exposure: f32,
    pub total_filth: u32,
}

pub fn wash_priority(
    character_id: u64,
    deposits: &[Deposit],
    has_cut: bool,
    exposure: f32,
) -> WashPriority {
    WashPriority {
        character_id,
        best_substance_rank: deposits
            .iter()
            .map(|d| cleaning_rank(d, has_cut))
            .min()
            .unwrap_or(5),
        exposure: exposure.max(0.0),
        total_filth: deposits.iter().map(|d| u32::from(d.amount)).sum(),
    }
}

pub fn sort_wash_priorities(priorities: &mut [WashPriority]) {
    priorities.sort_by(|left, right| {
        left.best_substance_rank
            .cmp(&right.best_substance_rank)
            .then_with(|| right.exposure.total_cmp(&left.exposure))
            .then_with(|| right.total_filth.cmp(&left.total_filth))
            .then_with(|| left.character_id.cmp(&right.character_id))
    });
}

/// Plans a stable, personal-first wash. Every used soap unit is consumed whole.
pub fn plan_wash(deposits: &[Deposit], stacks: &[WashStack], has_cut: bool) -> WashPlan {
    let total: u32 = deposits.iter().map(|d| u32::from(d.amount)).sum();
    if total == 0 {
        return WashPlan {
            soap_stacks: vec![],
            cleaned_deposits: vec![],
        };
    }
    let needed = total.div_ceil(u32::from(SOAP_CLEANSING_CAPACITY));
    let mut ordered_stacks = stacks.to_vec();
    ordered_stacks.sort_by_key(|s| (s.key.source, s.key.id));
    let mut remaining_units = needed;
    let mut soap_stacks = Vec::new();
    for stack in ordered_stacks {
        let used = stack.quantity.min(remaining_units);
        if used > 0 {
            soap_stacks.push((stack.key, used));
            remaining_units -= used;
        }
        if remaining_units == 0 {
            break;
        }
    }
    let units: u32 = soap_stacks.iter().map(|(_, q)| *q).sum();
    let mut capacity = units * u32::from(SOAP_CLEANSING_CAPACITY);
    let mut ordered = deposits.to_vec();
    ordered.sort_by_key(|d| (cleaning_rank(d, has_cut), d.deposited_at, d.id));
    let mut cleaned_deposits = Vec::new();
    for d in ordered {
        if capacity == 0 {
            break;
        }
        let removed = u32::from(d.amount).min(capacity) as u16;
        capacity -= u32::from(removed);
        cleaned_deposits.push((d.id, removed));
    }
    WashPlan {
        soap_stacks,
        cleaned_deposits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(id: u64, kind: Substance, source: Option<u64>, amount: u16) -> Deposit {
        Deposit {
            id,
            character_id: 7,
            substance: kind,
            source_character_id: source,
            amount,
            deposited_at: id,
            diseases: vec![],
        }
    }
    fn stack(source: SoapSource, id: u64, quantity: u32) -> WashStack {
        WashStack {
            key: SoapStackId { source, id },
            quantity,
        }
    }

    #[test]
    fn soap_rounds_up_and_uses_personal_stacks_first() {
        let plan = plan_wash(
            &[d(1, Substance::Dirt, None, 26)],
            &[
                stack(SoapSource::Party, 9, 4),
                stack(SoapSource::Personal, 3, 1),
            ],
            false,
        );
        assert_eq!(
            plan.soap_stacks,
            vec![
                (
                    SoapStackId {
                        source: SoapSource::Personal,
                        id: 3
                    },
                    1
                ),
                (
                    SoapStackId {
                        source: SoapSource::Party,
                        id: 9
                    },
                    1
                )
            ]
        );
        assert_eq!(plan.cleaned_deposits, vec![(1, 26)]);
    }

    #[test]
    fn wash_is_input_order_invariant_and_prioritizes_foreign_blood() {
        let a = d(2, Substance::Dirt, None, 25);
        let b = d(1, Substance::Blood, Some(8), 25);
        let stacks = [stack(SoapSource::Personal, 4, 1)];
        assert_eq!(
            plan_wash(&[a.clone(), b.clone()], &stacks, true),
            plan_wash(&[b, a.clone()], &stacks, true)
        );
        assert_eq!(
            plan_wash(&[a, d(1, Substance::Blood, Some(8), 25)], &stacks, true).cleaned_deposits[0]
                .0,
            1
        );
    }

    #[test]
    fn equal_numeric_ids_from_personal_and_party_tables_remain_distinct() {
        let plan = plan_wash(
            &[d(1, Substance::Dirt, None, 40)],
            &[
                stack(SoapSource::Party, 7, 1),
                stack(SoapSource::Personal, 7, 1),
            ],
            false,
        );
        assert_eq!(
            plan.soap_stacks,
            vec![
                (
                    SoapStackId {
                        source: SoapSource::Personal,
                        id: 7
                    },
                    1
                ),
                (
                    SoapStackId {
                        source: SoapSource::Party,
                        id: 7
                    },
                    1
                ),
            ]
        );
    }

    #[test]
    fn scarce_shared_soap_priority_is_risk_first_and_deterministic() {
        let mut dangerous = d(1, Substance::Blood, Some(99), 10);
        dangerous.character_id = 20;
        dangerous.diseases.push(DiseaseSnapshot {
            disease_id: DiseaseId::Plague,
            episode_id: 5,
        });
        let safe = d(2, Substance::Dirt, None, 100);
        let priorities = [
            wash_priority(7, &[safe], false, 0.0),
            wash_priority(20, &[dangerous], true, 0.4),
        ];
        let mut forward = priorities;
        let mut reverse = [priorities[1], priorities[0]];
        sort_wash_priorities(&mut forward);
        sort_wash_priorities(&mut reverse);
        assert_eq!(forward, reverse);
        assert_eq!(forward[0].character_id, 20);
    }

    #[test]
    fn travel_dirt_is_invariant_to_elapsed_time_chunking() {
        let one_chunk = travel_dirt_accrual(0, 1_440);
        let mut accumulated = 0_u16;
        let mut remainder = 0_u16;
        for _ in 0..24 {
            let (dirt, next) = travel_dirt_accrual(remainder, 60);
            accumulated = accumulated.saturating_add(dirt);
            remainder = next;
        }
        assert_eq!(one_chunk, (8, 0));
        assert_eq!((accumulated, remainder), one_chunk);
    }

    #[test]
    fn infectiousness_decays_but_visible_amount_does_not() {
        assert_eq!(infectious_fraction(100, 100), 1.0);
        assert_eq!(
            infectious_fraction(100, 100 + BLOOD_INFECTIOUS_MINUTES),
            0.0
        );
        let blood = d(1, Substance::Blood, Some(8), 20);
        assert_eq!(blood.amount, 20);
    }

    #[test]
    fn wound_routes_are_ordered_and_capped() {
        assert!(cut_exposure(1, 0, 0) > cut_exposure(0, 1, 0));
        assert!(cut_exposure(0, 1, 0) > cut_exposure(0, 0, 1));
        assert!(cut_exposure(100, 100, 100) <= 1.0);
    }

    #[test]
    fn only_blood_compatible_disease_snapshots_create_exposure() {
        let mut blood = d(1, Substance::Blood, Some(8), 50);
        blood.diseases.push(DiseaseSnapshot {
            disease_id: DiseaseId::Influenza,
            episode_id: 44,
        });
        assert_eq!(
            blood_exposure_for_vector(&[blood.clone()], DiseaseId::Influenza, 1, 1.0, false),
            0.0
        );
        assert!(blood_exposure_for_vector(&[blood], DiseaseId::Influenza, 1, 1.0, true) > 0.4);
        assert!(crate::disease::definition(DiseaseId::Plague).supports(TransmissionVector::Blood));
        assert!(
            !crate::disease::definition(DiseaseId::Influenza).supports(TransmissionVector::Blood)
        );
    }
}
