//! Framework-independent equipment condition, wear, and repair calculations.
//!
//! Damage is continuous, but is recorded in five repair-skill bins.  Bins one
//! and two are field-maintainable; the remaining bins require a settlement.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamageBins(pub [f32; 5]);

pub const MAX_DURABLE_QUANTITY_CHANGE: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairService {
    Weapons,
    Armor,
    Clothing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairItemKind {
    Weapon,
    Shield,
    Armor,
    Clothing,
}

impl RepairService {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "weapons" => Some(Self::Weapons),
            "armor" => Some(Self::Armor),
            "clothing" => Some(Self::Clothing),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
        }
    }

    pub const fn matches(self, kind: RepairItemKind) -> bool {
        match self {
            Self::Weapons => matches!(kind, RepairItemKind::Weapon | RepairItemKind::Shield),
            Self::Armor => matches!(kind, RepairItemKind::Armor),
            Self::Clothing => matches!(kind, RepairItemKind::Clothing),
        }
    }
}

/// Choose durable rows for deletion, preferring spare instances and using IDs
/// only as a stable tie-breaker.
pub fn durable_removal_ids(mut instances: Vec<(u64, bool)>, remove: u32) -> Vec<u64> {
    instances.sort_by_key(|(id, equipped)| (*equipped, *id));
    instances
        .into_iter()
        .take(remove as usize)
        .map(|(id, _)| id)
        .collect()
}

/// Stable workshop quote for the complete portion of a job this smith can do.
pub fn repair_quote(base_value: u32, repairable_damage: f32) -> u32 {
    if !repairable_damage.is_finite() || repairable_damage <= f32::EPSILON {
        return 0;
    }
    ((base_value.max(1) as f32 * repairable_damage.clamp(0.0, 1.0)).ceil() as u32).max(1)
}

/// Number of deterministically ordered completed jobs affordable from the
/// current purse without skipping an unaffordable earlier job.
pub fn affordable_repair_prefix(available_gold: u64, ordered_costs: &[u32]) -> usize {
    let mut remaining = available_gold;
    ordered_costs
        .iter()
        .take_while(|cost| {
            let cost = u64::from(**cost);
            if cost > remaining {
                false
            } else {
                remaining -= cost;
                true
            }
        })
        .count()
}

pub fn repair_budget_after_reservations(available_gold: u64, outstanding_costs: &[u32]) -> u64 {
    available_gold.saturating_sub(outstanding_costs.iter().map(|cost| u64::from(*cost)).sum())
}

pub fn bounded_durable_change(by_quantity: i32) -> Result<(u32, u32), &'static str> {
    let amount = by_quantity.unsigned_abs();
    if amount > MAX_DURABLE_QUANTITY_CHANGE {
        return Err("durable quantity change exceeds the per-call limit");
    }
    Ok(if by_quantity >= 0 {
        (amount, 0)
    } else {
        (0, amount)
    })
}

pub fn valid_repair_escrow_row(
    character_id: u64,
    quantity: u32,
    expected_item_id: &str,
    actual_item_id: &str,
) -> bool {
    character_id == 0 && quantity == 1 && expected_item_id == actual_item_id
}

pub fn remaining_after_priority(available_minutes: u64, required_minutes: u64) -> u64 {
    available_minutes.saturating_sub(required_minutes.min(available_minutes))
}

impl DamageBins {
    pub fn normalized(mut self) -> Self {
        for value in &mut self.0 {
            *value = if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            };
        }
        // Preserve deeper structural damage first if malformed input
        // exceeds the bar's capacity. Never proportionally shrink every bin.
        let mut remaining = 1.0;
        for value in self.0.iter_mut().rev() {
            *value = value.min(remaining);
            remaining -= *value;
        }
        self
    }
    pub fn total(self) -> f32 {
        self.0
            .iter()
            .filter(|value| value.is_finite())
            .map(|value| value.max(0.0))
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }
    pub fn condition(self) -> f32 {
        1.0 - self.total()
    }
    pub fn yellow(self) -> f32 {
        self.0[0] + self.0[1]
    }
    pub fn red(self) -> f32 {
        self.0[2..].iter().sum()
    }
    pub fn repairable(self, skill: u8) -> f32 {
        self.0.iter().take(skill.min(5) as usize).sum()
    }
    pub fn repair_through(&mut self, skill: u8) -> f32 {
        let mut repaired = 0.0;
        for value in self.0.iter_mut().take(skill.min(5) as usize) {
            repaired += *value;
            *value = 0.0;
        }
        repaired
    }
    pub fn add_to_tier(&mut self, tier: u8, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        for value in &mut self.0 {
            *value = if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            };
        }
        let used = self.0.iter().sum::<f32>().clamp(0.0, 1.0);
        let added = amount.min(1.0 - used);
        self.0[tier.clamp(1, 5) as usize - 1] += added;
    }

    /// Collapse damage that would require craftsmanship beyond the item's
    /// quality into the deepest tier that item can possess.
    pub fn capped_to_quality(mut self, quality: u8) -> Self {
        let quality_index = quality.clamp(1, 5) as usize - 1;
        let overflow = self.0[(quality_index + 1)..]
            .iter()
            .filter(|value| value.is_finite())
            .map(|value| value.max(0.0))
            .sum::<f32>();
        self.0[(quality_index + 1)..].fill(0.0);
        self.0[quality_index] += overflow;
        self.normalized()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DurabilityProfile {
    /// Stress below this only produces negligible cosmetic wear.
    pub yield_threshold: f32,
    /// A single-hit stress at which cracking, tearing, or gross deformation occurs.
    pub catastrophic_threshold: f32,
    /// Condition lost per unit of ordinary over-stress.
    pub wear_rate: f32,
    /// Fraction of the whole item's function represented by the struck component.
    pub failure_share: f32,
    /// Quality 1..5. Better work retains performance but its deepest damage is
    /// correspondingly harder to restore completely.
    pub quality: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageEvent {
    pub amount: f32,
    pub tier: u8,
    pub catastrophic: bool,
}

/// Physical endurance supplied by craftsmanship, with munition-grade work as
/// the neutral baseline. Better tempering and fit raise the stress needed to
/// yield or fracture the item and reduce ordinary wear at the same stress.
pub fn quality_durability_multiplier(quality: u8) -> f32 {
    match quality.clamp(1, 5) {
        1 => 0.65,
        2 => 0.80,
        3 => 1.00,
        4 => 1.25,
        5 => 1.60,
        _ => unreachable!(),
    }
}

pub fn damage_from_impact(profile: DurabilityProfile, stress: f32) -> DamageEvent {
    let stress = stress.max(0.0);
    let durability = quality_durability_multiplier(profile.quality);
    let yield_threshold = profile.yield_threshold * durability;
    let catastrophic_threshold = profile.catastrophic_threshold * durability;
    let wear_rate = profile.wear_rate / durability;
    if stress < yield_threshold {
        return DamageEvent {
            amount: 0.0,
            tier: 1,
            catastrophic: false,
        };
    }
    if stress >= catastrophic_threshold {
        let overload = stress / catastrophic_threshold.max(f32::EPSILON) - 1.0;
        return DamageEvent {
            amount: (profile.failure_share * (1.0 + overload * 0.35)).clamp(0.0, 1.0),
            tier: (3 + (overload * 2.0).floor() as u8 + profile.quality.saturating_sub(3) / 2)
                .min(5),
            catastrophic: true,
        };
    }
    let span = (catastrophic_threshold - yield_threshold).max(f32::EPSILON);
    let severity = (stress - yield_threshold) / span;
    DamageEvent {
        amount: (severity * wear_rate).clamp(0.0, profile.failure_share.max(0.01)),
        tier: if severity < 0.55 { 1 } else { 2 },
        catastrophic: false,
    }
}

/// Effective working-surface stats. There is deliberately no protected band:
/// any measurable condition loss has a measurable performance consequence.
pub fn effective_weapon_stat(base: f32, damage: DamageBins, sensitivity: f32) -> f32 {
    base.max(0.0) * (1.0 - damage.total() * sensitivity.clamp(0.0, 1.0)).max(0.0)
}

/// Damage adds handling/mobility/sensory obstruction without inventing a hole
/// in otherwise-present armor coverage.
pub fn effective_handling(base_range_of_motion: f32, damage: DamageBins, sensitivity: f32) -> f32 {
    (base_range_of_motion - damage.total() * sensitivity.max(0.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_service_filter_is_strict_and_three_way() {
        let weapons = RepairService::parse("weapons").unwrap();
        let armor = RepairService::parse("armor").unwrap();
        let clothing = RepairService::parse("clothing").unwrap();
        assert!(weapons.matches(RepairItemKind::Weapon));
        assert!(weapons.matches(RepairItemKind::Shield));
        assert!(!weapons.matches(RepairItemKind::Armor));
        assert!(!weapons.matches(RepairItemKind::Clothing));
        assert!(armor.matches(RepairItemKind::Armor));
        assert!(!armor.matches(RepairItemKind::Clothing));
        assert!(clothing.matches(RepairItemKind::Clothing));
        assert!(!clothing.matches(RepairItemKind::Weapon));
        assert_eq!(RepairService::parse("smith"), None);
        assert_eq!(weapons.as_str(), "weapons");
        assert_eq!(armor.as_str(), "armor");
        assert_eq!(clothing.as_str(), "clothing");
    }
    #[test]
    fn ductile_yields_sooner_but_brittle_fails_sooner() {
        let ductile = DurabilityProfile {
            yield_threshold: 20.0,
            catastrophic_threshold: 120.0,
            wear_rate: 0.2,
            failure_share: 0.8,
            quality: 3,
        };
        let brittle = DurabilityProfile {
            yield_threshold: 70.0,
            catastrophic_threshold: 90.0,
            wear_rate: 0.08,
            failure_share: 0.8,
            quality: 3,
        };
        assert!(
            damage_from_impact(ductile, 50.0).amount > damage_from_impact(brittle, 50.0).amount
        );
        assert!(!damage_from_impact(ductile, 100.0).catastrophic);
        assert!(damage_from_impact(brittle, 100.0).catastrophic);
    }
    #[test]
    fn segmented_construction_localizes_catastrophe() {
        let base = DurabilityProfile {
            yield_threshold: 40.0,
            catastrophic_threshold: 80.0,
            wear_rate: 0.1,
            failure_share: 0.75,
            quality: 3,
        };
        let segmented = DurabilityProfile {
            failure_share: 0.08,
            ..base
        };
        assert!(
            damage_from_impact(base, 100.0).amount
                > damage_from_impact(segmented, 100.0).amount * 5.0
        );
    }

    #[test]
    fn quality_monotonically_increases_physical_durability() {
        let profile = DurabilityProfile {
            yield_threshold: 40.0,
            catastrophic_threshold: 100.0,
            wear_rate: 0.2,
            failure_share: 0.7,
            quality: 1,
        };
        let damage: Vec<_> = (1..=5)
            .map(|quality| damage_from_impact(DurabilityProfile { quality, ..profile }, 75.0))
            .collect();
        assert!(
            damage
                .windows(2)
                .all(|pair| pair[0].amount >= pair[1].amount)
        );
        assert!(damage[0].catastrophic);
        assert!(!damage[4].catastrophic);
    }

    #[test]
    fn damage_never_requires_skill_above_item_quality() {
        let damage = DamageBins([0.1, 0.1, 0.1, 0.1, 0.1]).capped_to_quality(3);
        assert_eq!(damage, DamageBins([0.1, 0.1, 0.3, 0.0, 0.0]));
    }
    #[test]
    fn every_damage_amount_reduces_performance_and_repairs_are_tiered() {
        let mut damage = DamageBins([0.01, 0.1, 0.2, 0.1, 0.0]);
        assert!(effective_weapon_stat(2.0, damage, 1.0) < 2.0);
        assert!((damage.repairable(2) - 0.11).abs() < 0.001);
        damage.repair_through(2);
        assert!((damage.red() - 0.3).abs() < 0.001);
    }

    #[test]
    fn adding_and_repairing_surface_wear_never_erases_structural_damage() {
        let mut damage = DamageBins([0.0, 0.0, 0.7, 0.0, 0.0]);
        for _ in 0..20 {
            damage.add_to_tier(1, 0.1);
        }
        assert!((damage.0[2] - 0.7).abs() < 0.001);
        damage.repair_through(2);
        assert!((damage.0[2] - 0.7).abs() < 0.001);
        assert!((damage.red() - 0.7).abs() < 0.001);
    }

    #[test]
    fn nonfinite_damage_inputs_are_ignored_or_sanitized() {
        let mut damage = DamageBins([f32::NAN, f32::INFINITY, -1.0, 0.25, 0.0]);
        damage.add_to_tier(1, f32::NAN);
        let damage = damage.normalized();
        assert_eq!(damage.0, [0.0, 0.0, 0.0, 0.25, 0.0]);
    }

    #[test]
    fn durable_removal_prefers_spares_before_equipped_instances() {
        assert_eq!(
            durable_removal_ids(vec![(10, true), (12, false), (11, false)], 2),
            vec![11, 12]
        );
        assert_eq!(
            durable_removal_ids(vec![(10, true), (11, false)], 2),
            vec![11, 10]
        );
    }

    #[test]
    fn repair_quote_scales_with_value_and_repairable_job_share() {
        assert_eq!(repair_quote(100, 0.25), 25);
        assert_eq!(repair_quote(3, 0.01), 1);
        assert_eq!(repair_quote(100, 0.0), 0);
        assert_eq!(repair_quote(100, f32::NAN), 0);
    }

    #[test]
    fn bulk_retrieval_uses_an_affordable_ordered_prefix() {
        assert_eq!(affordable_repair_prefix(10, &[4, 6, 1]), 2);
        assert_eq!(affordable_repair_prefix(5, &[6, 1]), 0);
        assert_eq!(affordable_repair_prefix(7, &[3, 5, 1]), 1);
    }

    #[test]
    fn npc_budget_reserves_every_outstanding_quote() {
        assert_eq!(repair_budget_after_reservations(30, &[5, 7, 3]), 15);
        assert_eq!(repair_budget_after_reservations(10, &[8, 9]), 0);
    }

    #[test]
    fn durable_quantity_changes_are_bounded_and_exact() {
        assert_eq!(bounded_durable_change(3), Ok((3, 0)));
        assert_eq!(bounded_durable_change(-2), Ok((0, 2)));
        assert!(bounded_durable_change(65).is_err());
        assert!(bounded_durable_change(i32::MIN).is_err());
    }

    #[test]
    fn repair_escrow_requires_exact_custody_identity() {
        assert!(valid_repair_escrow_row(0, 1, "sword", "sword"));
        assert!(!valid_repair_escrow_row(7, 1, "sword", "sword"));
        assert!(!valid_repair_escrow_row(0, 2, "sword", "sword"));
        assert!(!valid_repair_escrow_row(0, 1, "sword", "axe"));
    }

    #[test]
    fn convalescence_consumes_camp_time_before_maintenance() {
        assert_eq!(remaining_after_priority(1_440, 2_000), 0);
        assert_eq!(remaining_after_priority(1_440, 600), 840);
    }
}
