//! Framework-neutral, deterministic combat simulation for strategic autoresolve.

use crate::prelude::*;
use adventuresim_world_schema::{BestiaryCategory, BestiaryHours};
use fabelgeist_determinism::SplitMix64;
use serde::Serialize;

mod melee_exchange;
mod melee_iteration;
mod melee_movement;
mod melee_round;
mod model;
mod power;

use melee_exchange::*;
pub use melee_iteration::*;
use melee_movement::*;
use melee_round::*;
pub use model::*;
pub use power::autoresolve_combat_power;
#[cfg(test)]
use power::finite_log_component;

const AUTORESOLVE_MAX_COMBAT_ROUNDS: usize = 256;
const AUTORESOLVE_MAX_RANGED_ATTACKS_PER_PHASE: usize = 64;
#[derive(Clone, Debug, Serialize)]
pub struct CombatBody {
    pub health: [f32; 7],
    pub weight_kg: f32,
    pub primary_side: BodySide,
}

impl Default for CombatBody {
    fn default() -> Self {
        Self {
            health: [1.0; 7],
            weight_kg: 70.0,
            primary_side: BodySide::Right,
        }
    }
}

impl CombatBody {
    pub fn health(&self, part: BodyPart) -> f32 {
        self.health[body_part_index(part)]
    }

    pub fn apply_damage(&mut self, part: BodyPart, damage: f32) -> f32 {
        let health = &mut self.health[body_part_index(part)];
        apply_clamped_limb_damage(health, damage)
    }

    pub fn total_damage(&self) -> f32 {
        self.health
            .iter()
            .map(|health| (1.0 - health).max(0.0))
            .sum()
    }
}

impl PlayerBody for CombatBody {
    fn body_part_health(&self, part: BodyPart) -> f32 {
        self.health(part)
    }

    fn body_weight(&self) -> f32 {
        self.weight_kg
    }

    fn primary_side(&self) -> BodySide {
        self.primary_side
    }
}

#[derive(Clone, Debug, Default)]
pub struct CombatEssentials {
    pub calories_used_today: f32,
    pub focus_level: f32,
}

impl PlayerEssentials for CombatEssentials {
    fn calories_used_today(&self) -> f32 {
        self.calories_used_today
    }

    fn focus_level(&self) -> f32 {
        self.focus_level
    }
}

#[derive(Clone, Debug, Default)]
pub struct CombatSkills {
    pub polearm_hours: f32,
    pub axe_hours: f32,
    pub bludgeon_hours: f32,
    pub sword_hours: f32,
    pub knife_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub bow_hours: f32,
    pub crossbow_hours: f32,
    pub firearm_hours: f32,
    pub throw_hours: f32,
    pub will_hours: f32,
    pub insight_hours: f32,
    pub charm_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub physiology_hours: f32,
    pub religion_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub bestiary_hours: BestiaryHours,
    pub surgery_hours: f32,
    pub tailoring_hours: f32,
    pub smithing_hours: f32,
}

impl PlayerSkills for CombatSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Polearm => self.polearm_hours,
            Skill::Axe => self.axe_hours,
            Skill::Bludgeon => self.bludgeon_hours,
            Skill::Sword => self.sword_hours,
            Skill::Knife => self.knife_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Block => self.block_hours,
            Skill::Bow => self.bow_hours,
            Skill::Crossbow => self.crossbow_hours,
            Skill::Firearm => self.firearm_hours,
            Skill::Throw => self.throw_hours,
            Skill::Will => self.will_hours,
            Skill::Insight => self.insight_hours,
            Skill::Charm => self.charm_hours,
            Skill::Command => self.command_hours,
            Skill::Deception => self.deception_hours,
            Skill::Physiology => self.physiology_hours,
            Skill::Cooking => 0.0,
            Skill::Herbalism => 0.0,
            Skill::Religion => self.religion_hours,
            Skill::Bestiary => self.bestiary_hours.aggregate_effective(),
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains
            | Skill::TerrainForest
            | Skill::TerrainHills
            | Skill::TerrainWetlands
            | Skill::TerrainUrban
            | Skill::TerrainSnow => 0.0,
            Skill::Surgery => self.surgery_hours,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }

    fn bestiary_hours_for(&self, category: BestiaryCategory) -> f32 {
        self.bestiary_hours.effective(category)
    }
}

#[derive(Clone, Debug)]
pub struct Combatant {
    pub id: u64,
    pub attributes: PlayerAttributeValues,
    pub body: CombatBody,
    pub essentials: CombatEssentials,
    pub equipment: CombatEquipment,
    pub skills: CombatSkills,
    /// Physical creature facets used to select the attacker's anatomical lore.
    pub bestiary_categories: Vec<BestiaryCategory>,
    /// Strategic incapacitation not recomputed in battle, such as fear or hunger.
    pub starting_incapacitation: f32,
    pub starting_blood_fraction: f32,
    #[doc(hidden)]
    pub imbalance: f32,
    #[doc(hidden)]
    pub blood_loss_fraction: f32,
    #[doc(hidden)]
    pub wounds: Vec<CombatWound>,
    #[doc(hidden)]
    pub oxygen_debt_joules: f32,
    #[doc(hidden)]
    pub local_action_fatigue: f32,
    #[doc(hidden)]
    pub acute_trauma: f32,
    #[doc(hidden)]
    pub active_work_seconds: f32,
    #[doc(hidden)]
    #[doc(hidden)]
    pub melee_attack_power_multiplier: f32,
    #[doc(hidden)]
    pub melee_interval_jitter_seconds: f32,
    #[doc(hidden)]
    melee_consecutive_intercepts: u8,
    #[doc(hidden)]
    melee_phase_adaptation_delay_seconds: f32,
    #[doc(hidden)]
    melee_engagement_target: Option<u64>,
    #[doc(hidden)]
    melee_engagement_distance_metres: f32,
    /// Signed contribution to separation velocity: closing is negative.
    melee_separation_velocity_metres_per_second: f32,
    #[doc(hidden)]
    melee_recovery_until_seconds: f32,
    #[doc(hidden)]
    melee_attack_started_at_seconds: Option<f32>,
    #[doc(hidden)]
    melee_attack_contact_at_seconds: Option<f32>,
    #[doc(hidden)]
    melee_attack_scheduled_measure_metres: Option<f32>,
    #[doc(hidden)]
    #[doc(hidden)]
    pub yielded: bool,
    #[doc(hidden)]
    /// Cut damage committed at the tactical/strategic boundary. This is durable
    /// wound provenance, not tactical tick state.
    pub cut_damage: f32,
    #[doc(hidden)]
    pub initial_ammunition: u32,
    #[doc(hidden)]
    pub ranged_attack_progress: f32,
}

impl Combatant {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            attributes: PlayerAttributeValues::default(),
            body: CombatBody::default(),
            essentials: CombatEssentials {
                focus_level: 1.0,
                ..CombatEssentials::default()
            },
            equipment: CombatEquipment::default(),
            skills: CombatSkills::default(),
            bestiary_categories: vec![BestiaryCategory::Human],
            starting_incapacitation: 0.0,
            starting_blood_fraction: 1.0,
            imbalance: 0.0,
            blood_loss_fraction: 0.0,
            wounds: Vec::new(),
            oxygen_debt_joules: 0.0,
            local_action_fatigue: 0.0,
            acute_trauma: 0.0,
            active_work_seconds: 0.0,
            melee_attack_power_multiplier: 1.0,
            melee_interval_jitter_seconds: 0.0,
            melee_consecutive_intercepts: 0,
            melee_phase_adaptation_delay_seconds: 0.0,
            melee_engagement_target: None,
            melee_engagement_distance_metres: 0.0,
            melee_separation_velocity_metres_per_second: 0.0,
            melee_recovery_until_seconds: 0.0,
            melee_attack_started_at_seconds: None,
            melee_attack_contact_at_seconds: None,
            melee_attack_scheduled_measure_metres: None,
            yielded: false,
            cut_damage: 0.0,
            initial_ammunition: 0,
            ranged_attack_progress: 0.0,
        }
    }

    fn view_with_equipment<'a>(
        &'a self,
        equipment: &'a CombatEquipment,
    ) -> PlayerInfo<
        &'a PlayerAttributeValues,
        &'a CombatBody,
        &'a CombatEssentials,
        &'a CombatEquipment,
        &'a CombatSkills,
    > {
        PlayerInfo::empty()
            .with_attributes(&self.attributes)
            .with_body(&self.body)
            .with_essentials(&self.essentials)
            .with_equipment(equipment)
            .with_skills(&self.skills)
    }

    pub fn incapacitation(&self) -> f32 {
        let will = self.skills.skill_check_by_parts(
            Skill::Will,
            &self.attributes,
            &self.body,
            &self.essentials,
            &self.equipment,
            LimbWeights::all_equal(),
        );
        combat_incapacitation(
            self.starting_incapacitation,
            self.starting_blood_fraction,
            self.blood_loss_fraction,
            self.body.total_damage(),
            will,
            self.imbalance,
        ) + self.acute_trauma
            + oxygen_debt_incapacitation(self.oxygen_debt_joules, self.attributes.endurance)
    }

    pub fn is_incapacitated(&self) -> bool {
        self.incapacitation() >= 1.0
    }

    fn is_defeated(&self) -> bool {
        self.is_incapacitated() || self.yielded
    }

    fn recover_balance(&mut self, elapsed_seconds: f32) {
        self.imbalance = recover_combat_imbalance(self.imbalance, elapsed_seconds);
    }

    fn advance_condition(&mut self, elapsed_seconds: f32) {
        self.recover_balance(elapsed_seconds);
        self.blood_loss_fraction =
            advance_combat_bleeding(self.blood_loss_fraction, &self.wounds, elapsed_seconds);
        let recovery_seconds = (elapsed_seconds - self.active_work_seconds).max(0.0);
        recover_combat_fatigue(
            &mut self.oxygen_debt_joules,
            &mut self.local_action_fatigue,
            recovery_seconds,
            self.attributes.endurance,
        );
        self.active_work_seconds = 0.0;
    }

    fn charge_action_work(&mut self, work: CombatActionWork, duration_seconds: f32) {
        let weapon = self.equipment.melee_weapon.unwrap_or_default();
        let workload = combat_action_workload(
            work,
            duration_seconds,
            weapon.weight,
            weapon.moment_of_inertia_kg_m2,
            self.equipment.inventory_weight,
            self.body.weight_kg,
            self.attributes.endurance,
        );
        apply_combat_workload(
            &mut self.oxygen_debt_joules,
            &mut self.local_action_fatigue,
            workload,
            self.attributes.endurance,
        );
        self.active_work_seconds = self.active_work_seconds.max(duration_seconds);
    }

    fn fatigue_performance(&self) -> f32 {
        combat_fatigue_performance(
            self.oxygen_debt_joules,
            self.local_action_fatigue,
            self.attributes.endurance,
        )
    }

    fn can_attack_ranged(&self) -> bool {
        self.equipment.ranged_weapon.is_some() && self.equipment.ammunition > 0
    }

    fn can_attack_melee(&self) -> bool {
        self.equipment.melee_weapon.is_some()
            && melee_attack_capability(&self.body, &self.equipment.for_melee()).is_available()
    }

    fn movement_speed_meters_per_second(&self, minimum_speed: f32) -> f32 {
        let leg_agility = self.attributes.limb_attr_by_weight_by_parts(
            LimbAttribute::Agility,
            &self.body,
            LimbWeights::both_legs(),
        );
        let armor = self.equipment.armor_penalty(BodyPart::LOWER_BODY);
        let encumbrance = self
            .equipment
            .encumbrance_penalty_by_parts(&self.attributes, &self.body);
        let fatigue = self
            .essentials
            .fatigue_penalty_by_parts(&self.attributes, &self.body);
        ((1.0 + leg_agility) * armor * encumbrance * fatigue).max(minimum_speed)
    }
}

/// The quest policy's conservative 5:4 party-to-opposition margin. Overflow
/// is an invalid assessment, not implicit permission to fight.
pub fn combat_power_meets_safety_margin(party_power: u64, enemy_power: u64) -> Option<bool> {
    Some(party_power.checked_mul(4)? >= enemy_power.checked_mul(5)?)
}

/// Build the same authored threat combatant used by strategic autoresolve and
/// observer-safe contract assessment.
pub fn authored_threat_combatant(
    id: u64,
    enemy_type: &str,
    difficulty: i32,
    combat_scale_bps: u32,
    countermeasure_multiplier_bps: u32,
) -> Result<Combatant, String> {
    use crate::bestiary::{AttackStyle, Protection, RigTopology, ThreatId};
    let threat: ThreatId = enemy_type
        .parse()
        .map_err(|_| format!("Unknown threat ID: {enemy_type}"))?;
    let (physical_scale, training_scale) = crate::threat_escalation::combat_scaling_multipliers(
        combat_scale_bps,
        countermeasure_multiplier_bps,
    );
    let base_rating = 1.2 + difficulty.max(1) as f32 * 0.35;
    let physical_rating = base_rating * physical_scale;
    let threat_profile = threat.profile();
    let profile = threat_profile.combat;
    let mut combatant = Combatant::new(id);
    combatant.bestiary_categories = threat_profile.categories().collect();
    combatant.attributes = PlayerAttributeValues {
        endurance: physical_rating,
        immunity: physical_rating,
        gut: physical_rating,
        intelligence: physical_rating * 0.7,
        instinct: physical_rating,
        eyesight: physical_rating,
        hearing: physical_rating,
        left_arm_strength: physical_rating,
        right_arm_strength: physical_rating,
        left_leg_strength: physical_rating,
        right_leg_strength: physical_rating,
        left_arm_agility: physical_rating,
        right_arm_agility: physical_rating,
        left_leg_agility: physical_rating,
        right_leg_agility: physical_rating,
    };
    let training = base_rating * 1_500.0 * profile.training_multiplier * training_scale;
    combatant.skills = CombatSkills {
        sword_hours: training,
        bow_hours: if profile.ranged { training * 2.0 } else { 0.0 },
        dodge_hours: training,
        block_hours: if matches!(
            profile.protection,
            Protection::Shielded | Protection::Armored
        ) {
            training
        } else {
            training * 0.4
        },
        will_hours: training * (0.5 + f32::from(profile.morale) / 50.0),
        balance_hours: training,
        ..CombatSkills::default()
    };
    combatant.body.weight_kg = profile.weight_kg;
    let (blunt, slash, pierce) = match profile.attack {
        AttackStyle::Blunt => (true, false, false),
        AttackStyle::Blade => (false, true, false),
        AttackStyle::Knife
        | AttackStyle::Spear
        | AttackStyle::Bow
        | AttackStyle::Bite
        | AttackStyle::Claw => (false, false, true),
    };
    let weapon = CombatWeapon {
        skills: if profile.ranged {
            crate::equipment::WeaponSkillDistribution {
                bow: 1.0,
                ..Default::default()
            }
        } else {
            crate::equipment::WeaponSkillDistribution {
                sword: 1.0,
                ..Default::default()
            }
        },
        melee: !profile.ranged,
        ranged: profile.ranged,
        blunt,
        slash,
        pierce,
        accuracy: 0.8 + profile.precision_bonus,
        swing_precision: if profile.ranged {
            0.0
        } else {
            0.8 + profile.precision_bonus
        },
        stab_precision: if profile.ranged {
            0.0
        } else {
            0.8 + profile.precision_bonus
        },
        preferred_melee_style: if pierce && !slash {
            crate::combat_style::MeleeAttackStyle::Stab
        } else {
            crate::combat_style::MeleeAttackStyle::Swing
        },
        weight: if profile.rig == RigTopology::Quadruped {
            1.0
        } else {
            1.5
        },
        moment_of_inertia_kg_m2: 0.0,
        penetration: if matches!(profile.attack, AttackStyle::Spear | AttackStyle::Claw) {
            1.5
        } else {
            0.8
        },
        melee_reach: if profile.ranged { 0.0 } else { 0.8 },
        grip_to_tip_m: if profile.ranged { 0.0 } else { 0.8 },
        total_length_m: if profile.ranged { 0.0 } else { 0.8 },
        striking_head_length_m: 0.0,
        distal_headed: false,
        body_material: None,
        striking_material: None,
        ranged_range: if profile.ranged { 20.0 } else { 0.0 },
        attack_interval_seconds: if profile.ranged { 1.0 } else { 0.75 },
        precise: profile.precision_bonus > 0.0,
        balance: 0.3,
        ranged_force_joules: if profile.ranged { 40.0 } else { 0.0 },
    };
    combatant.equipment.weapon = Some(weapon);
    if profile.ranged {
        combatant.equipment.ranged_weapon = Some(weapon);
        combatant.equipment.ranged_projectile_kind = Some(CombatProjectileKind::Arrowhead);
        combatant.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            slash: true,
            pierce: true,
            accuracy: 1.0,
            weight: 0.5,
            penetration: 0.5,
            melee_reach: 0.5,
            attack_interval_seconds: 0.6,
            balance: 0.5,
            ..CombatWeapon::default()
        });
        combatant.equipment.ammunition = 12;
        combatant.initial_ammunition = 12;
    } else {
        combatant.equipment.melee_weapon = Some(weapon);
    }
    let innate = profile.innate_protection;
    if innate.resistance_joules > 0.0 || innate.padding_joules > 0.0 {
        combatant.equipment.armor.fill(CombatArmor::innate(
            innate.resistance_joules,
            innate.padding_joules,
        ));
    }
    if matches!(profile.protection, Protection::Armored) {
        combatant.equipment.shield_block_bonus = 1.0;
        combatant.equipment.armor.fill(armored_threat_armor());
    }
    Ok(combatant)
}

fn armored_threat_armor() -> CombatArmor {
    CombatArmor {
        inventory_item_id: None,
        material: None,
        resistance: 25.0,
        padding: 15.0,
        flexibility: 0.8,
        range_of_motion: 0.9,
        coverage: 0.5,
        coverage_span: None,
        coverage_geometry: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleResolution {
    AlliesVictory,
    EnemiesVictory,
    MutualIncapacitation,
    Timeout,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatantOutcome {
    pub id: u64,
    pub body: CombatBody,
    pub blood_loss_fraction: f32,
    pub cut_damage: f32,
    pub incapacitated: bool,
    pub yielded: bool,
    pub incapacitation: f32,
    pub imbalance: f32,
    pub acute_trauma: f32,
    pub pain_incapacitation: f32,
    pub oxygen_debt_joules: f32,
    pub local_action_fatigue: f32,
    pub wound_count: usize,
    pub open_wound_count: usize,
    pub internal_wound_count: usize,
    pub wound_flow_fraction_per_second: f32,
    pub ammunition_used: u32,
    pub terminal_cause: Option<CombatTerminalCause>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CombatTerminalCause {
    YieldedUnableToContinue,
    StartingCondition,
    Pain,
    BloodLoss,
    AcuteTrauma,
    OxygenDebt,
    Imbalance,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BattleSummary {
    pub stealth_attempts: u32,
    pub stealth_successes: u32,
    pub opening_ranged_attacks: u32,
    pub ranged_attacks: u32,
    pub melee_attacks: u32,
    pub hits: u32,
    pub total_health_damage: f32,
    pub ammunition_used: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct BattleLogEntry {
    pub sequence: u32,
    pub phase: String,
    pub round: usize,
    pub attacker_id: u64,
    pub defender_id: u64,
    pub attack_kind: String,
    /// Exact strategic inventory instance used, when the combatant came from
    /// persistent equipment rather than an abstract enemy profile.
    pub weapon_inventory_item_id: Option<u64>,
    /// Exact shield or weapon contacted on a successful block or parry.
    pub defender_contact_item_id: Option<u64>,
    pub defender_response: &'static str,
    pub body_part: BodyPart,
    pub outcome: String,
    pub health_damage: f32,
    /// Applied shares from this individual hit. These are durable combat facts
    /// used by strategic bruising/fracture/wound generation.
    pub cut_damage: f32,
    pub blunt_damage: f32,
    pub projectile_kind: Option<CombatProjectileKind>,
    /// Pre-absorption contact force; remains positive when armor absorbs all
    /// health damage.
    pub contact_stress: f32,
    pub armor_impact: Option<ArmorImpact>,
    pub melee_telemetry: Option<MeleeContactTelemetry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ArmorLayerTelemetry {
    pub inventory_item_id: Option<u64>,
    pub material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub geometry: Option<AuthoredArmorCoverage>,
    pub intersected: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeleeContactTelemetry {
    pub anatomical_subregion: AnatomicalSubregion,
    pub surface_coordinate: f32,
    pub armor_layer_chain: Vec<ArmorLayerTelemetry>,
    pub redirected_from: Option<BodyPart>,
    pub dodge_closest_approach_metres: Option<f32>,
    pub dodge_displacement_time_seconds: Option<f32>,
    pub dodge_contacted_body_part: Option<BodyPart>,
    pub scheduled_contact_measure_metres: f32,
    /// Gap from the attack origin to the target body's near surface.
    pub actual_contact_measure_metres: f32,
    /// Physical center separation after adding both 0.4 m humanoid radii.
    pub actual_center_separation_metres: f32,
    pub contact_classification: MeleeContactClassification,
    pub contact_lever_arm_metres: f32,
    pub contact_energy_fraction: f32,
    pub contact_invalidation_cause: Option<MeleeContactInvalidationCause>,
    pub contact_material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub defense_success_probability: Option<f32>,
    pub defense_alignment_sample: Option<f32>,
    pub defense_engagement: Option<f32>,
    pub effective_defender_response: &'static str,
    pub defender_attack_commitment: &'static str,
    pub defender_retained_attack_power: Option<f32>,
    pub attack_power_multiplier: f32,
    pub attacker_fatigue_performance: f32,
    pub attack_interval_seconds: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct BattleOutcome {
    pub seed: u64,
    pub resolution: BattleResolution,
    pub rounds: usize,
    pub allies: Vec<CombatantOutcome>,
    pub enemies: Vec<CombatantOutcome>,
    pub summary: BattleSummary,
    pub log: Vec<BattleLogEntry>,
    pub timeline: Vec<MeleeTimelineEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeTimelineKind {
    Movement,
    AttackStarted,
    Response,
    AttackCanceled,
    AttackTransformed,
    Contact,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeMovementAction {
    Close,
    Retreat,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeTimelinePhase {
    NeutralGuard,
    Windup,
    Recovery,
    UnableToContinue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeResponseAvailability {
    NeutralGuard,
    ReciprocalWindup,
    OccupiedByEarlierAttack,
    OccupiedRecovery,
    NoImplement,
    DodgeChosen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeResponseChoice {
    None,
    Block,
    Parry,
    Dodge,
    FinishTrade,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MeleeTimelineEvent {
    pub sequence: u64,
    pub tick: u64,
    pub time_seconds: f32,
    pub kind: MeleeTimelineKind,
    pub combatant_id: Option<u64>,
    pub target_id: Option<u64>,
    pub engagement_distance_before_metres: Option<f32>,
    pub engagement_distance_after_metres: Option<f32>,
    pub movement_action: Option<MeleeMovementAction>,
    pub movement_elapsed_seconds: Option<f32>,
    pub movement_displacement_metres: Option<f32>,
    pub movement_velocity_before_metres_per_second: Option<f32>,
    pub movement_velocity_after_metres_per_second: Option<f32>,
    pub readiness_before_seconds: Option<f32>,
    pub readiness_after_seconds: Option<f32>,
    pub attack_id: Option<u64>,
    pub attack_started_tick: Option<u64>,
    pub attack_contact_tick: Option<u64>,
    pub attack_recovery_tick: Option<u64>,
    pub phase_before: Option<MeleeTimelinePhase>,
    pub phase_after: Option<MeleeTimelinePhase>,
    pub response_choice: Option<MeleeResponseChoice>,
    pub committed_finish_trade_probability: Option<f32>,
    pub committed_completed_work_fraction: Option<f32>,
    pub committed_expected_intercept_benefit: Option<f32>,
    pub consecutive_intercepts_before: Option<u8>,
    pub consecutive_intercepts_after: Option<u8>,
    /// One-shot guarded hesitation added after a repeated phase interception.
    /// This is elapsed readiness time, not uncharged action work.
    pub phase_adaptation_delay_seconds: Option<f32>,
    pub response_availability: Option<MeleeResponseAvailability>,
    pub affected_attack_id: Option<u64>,
    pub simultaneous_batch_id: Option<u64>,
    pub simultaneous_members: Vec<u64>,
    pub simultaneous_order: Option<u32>,
    pub terminal_resolution: Option<BattleResolution>,
}

impl MeleeTimelineEvent {
    const STEPS_PER_SECOND: f32 = 64.0;

    fn at(kind: MeleeTimelineKind, time_seconds: f32) -> Self {
        Self {
            sequence: 0,
            tick: Self::tick_at(time_seconds),
            time_seconds,
            kind,
            combatant_id: None,
            target_id: None,
            engagement_distance_before_metres: None,
            engagement_distance_after_metres: None,
            movement_action: None,
            movement_elapsed_seconds: None,
            movement_displacement_metres: None,
            movement_velocity_before_metres_per_second: None,
            movement_velocity_after_metres_per_second: None,
            readiness_before_seconds: None,
            readiness_after_seconds: None,
            attack_id: None,
            attack_started_tick: None,
            attack_contact_tick: None,
            attack_recovery_tick: None,
            phase_before: None,
            phase_after: None,
            response_choice: None,
            committed_finish_trade_probability: None,
            committed_completed_work_fraction: None,
            committed_expected_intercept_benefit: None,
            consecutive_intercepts_before: None,
            consecutive_intercepts_after: None,
            phase_adaptation_delay_seconds: None,
            response_availability: None,
            affected_attack_id: None,
            simultaneous_batch_id: None,
            simultaneous_members: Vec::new(),
            simultaneous_order: None,
            terminal_resolution: None,
        }
    }

    fn tick_at(time_seconds: f32) -> u64 {
        (time_seconds.max(0.0) * Self::STEPS_PER_SECOND).round() as u64
    }

    fn with_terminal(mut self, resolution: BattleResolution) -> Self {
        self.terminal_resolution = Some(resolution);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttackMode {
    Melee,
    Ranged,
}

#[derive(Clone, Copy)]
struct PendingAttack {
    attacker_index: usize,
    target_index: usize,
    result: AttackResult,
    part: BodyPart,
    mode: AttackMode,
    phase: &'static str,
    round: usize,
}

#[derive(Clone, Copy, Default)]
struct OpeningVolleyPlan {
    direct_attacks: usize,
    total_attacks: usize,
}

#[derive(Default)]
struct BattleRecorder {
    summary: BattleSummary,
    log: Vec<BattleLogEntry>,
    timeline: Vec<MeleeTimelineEvent>,
    next_timeline_sequence: u64,
}

#[derive(Clone, Copy)]
struct AttackEffect {
    hit: bool,
    health_damage: f32,
}

impl BattleRecorder {
    fn record_timeline(&mut self, mut event: MeleeTimelineEvent) {
        event.sequence = self.next_timeline_sequence;
        self.next_timeline_sequence = self.next_timeline_sequence.saturating_add(1);
        self.timeline.push(event);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "this domain boundary names each independent input explicitly"
    )]
    fn record_attack(
        &mut self,
        phase: &str,
        round: usize,
        attacker_id: u64,
        defender_id: u64,
        mode: AttackMode,
        weapon_inventory_item_id: Option<u64>,
        projectile_kind: Option<CombatProjectileKind>,
        defender_contact_item_id: Option<u64>,
        defender_response: &'static str,
        part: BodyPart,
        result: AttackResult,
        effect: AttackEffect,
        melee_telemetry: Option<MeleeContactTelemetry>,
    ) {
        match mode {
            AttackMode::Melee => self.summary.melee_attacks += 1,
            AttackMode::Ranged => self.summary.ranged_attacks += 1,
        }
        if effect.hit {
            self.summary.hits += 1;
        }
        self.summary.total_health_damage += effect.health_damage;
        let outcome = match result {
            AttackResult::ToAttacker {
                physical_contact: true,
                ..
            } => "blocked".to_string(),
            AttackResult::ToAttacker { .. } => "missed".to_string(),
            AttackResult::ToDefender { .. } if effect.health_damage > 0.0 => {
                format!("hit for {:.3} health", effect.health_damage)
            }
            AttackResult::ToDefender { .. } => "hit armor".to_string(),
        };
        let (raw_cut, raw_blunt) = match result {
            AttackResult::ToDefender {
                cut_damage,
                blunt_damage,
                ..
            } => (cut_damage.max(0.0), blunt_damage.max(0.0)),
            _ => (0.0, 0.0),
        };
        let raw_total = raw_cut + raw_blunt;
        let (cut_damage, blunt_damage) = if raw_total > 0.0 {
            (
                effect.health_damage * raw_cut / raw_total,
                effect.health_damage * raw_blunt / raw_total,
            )
        } else {
            (0.0, 0.0)
        };
        self.log.push(BattleLogEntry {
            sequence: self.log.len() as u32,
            phase: phase.to_string(),
            round,
            attacker_id,
            defender_id,
            attack_kind: match mode {
                AttackMode::Melee => "melee",
                AttackMode::Ranged => "ranged",
            }
            .to_string(),
            weapon_inventory_item_id,
            defender_contact_item_id,
            defender_response,
            body_part: part,
            outcome,
            health_damage: effect.health_damage,
            cut_damage,
            blunt_damage,
            projectile_kind: (mode == AttackMode::Ranged)
                .then_some(projectile_kind.unwrap_or(CombatProjectileKind::Arrowhead)),
            contact_stress: match result {
                AttackResult::ToDefender { contact_force, .. } => contact_force.max(0.0),
                AttackResult::ToAttacker { contact_force, .. } => contact_force.max(0.0),
            },
            armor_impact: match result {
                AttackResult::ToDefender { armor_impact, .. } => armor_impact,
                AttackResult::ToAttacker { .. } => None,
            },
            melee_telemetry,
        });
    }
}

fn defender_contact_item_id(result: AttackResult, equipment: &CombatEquipment) -> Option<u64> {
    match result {
        AttackResult::ToDefender {
            armor_impact: Some(impact),
            ..
        } => impact.surface.inventory_item_id,
        AttackResult::ToAttacker {
            physical_contact: true,
            ..
        } => equipment.defense_item_id,
        _ => None,
    }
}

fn melee_defender_contact_item_id(
    result: AttackResult,
    response: DefenderResponse,
    equipment: &CombatEquipment,
) -> Option<u64> {
    match result {
        AttackResult::ToDefender {
            armor_impact: Some(impact),
            ..
        } => impact.surface.inventory_item_id,
        AttackResult::ToAttacker {
            physical_contact: true,
            ..
        } => match response {
            DefenderResponse::Parry { .. } => equipment.melee_weapon_id,
            DefenderResponse::Block { .. } => {
                equipment.defense_item_id.or(equipment.melee_weapon_id)
            }
            DefenderResponse::None | DefenderResponse::Dodge { .. } => None,
        },
        _ => None,
    }
}

/// Resolve an abstract battle using the same attack calculations as direct
/// combat. The supplied seed makes the result reproducible and the hard round
/// cap keeps reducer execution bounded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BattleOpening {
    #[default]
    Normal,
    AlliesSurprise,
    EnemiesSurprise,
}

pub fn resolve_battle(
    mut allies: Vec<Combatant>,
    mut enemies: Vec<Combatant>,
    seed: u64,
    opening: BattleOpening,
) -> BattleOutcome {
    let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
    let mut random = SplitMix64::new(seed);
    let mut recorder = BattleRecorder::default();
    let mut resolution = None;
    let mut rounds = 0;

    initialize_melee_phases(&mut allies, &mut enemies, &mut random, parameters);

    // Awareness was already resolved by the strategic encounter. Do not roll
    // stealth again here: exactly one authoritative side receives the opener.
    match opening {
        BattleOpening::Normal => {}
        BattleOpening::AlliesSurprise => take_side_turns(
            &mut allies,
            &mut enemies,
            0,
            &mut random,
            &mut recorder,
            parameters,
        ),
        BattleOpening::EnemiesSurprise => take_side_turns(
            &mut enemies,
            &mut allies,
            0,
            &mut random,
            &mut recorder,
            parameters,
        ),
    }
    resolve_opening_volleys(
        &mut allies,
        &mut enemies,
        &mut random,
        &mut recorder,
        parameters,
    );

    for round in 0..AUTORESOLVE_MAX_COMBAT_ROUNDS {
        if let Some(terminal) =
            classify_battle_resolution(side_defeated(&allies), side_defeated(&enemies), false)
        {
            resolution = Some(terminal);
            break;
        }
        rounds = round + 1;

        resolve_battle_round(
            &mut allies,
            &mut enemies,
            round + 1,
            &mut random,
            &mut recorder,
            parameters,
        );
    }

    let resolution = resolution.unwrap_or_else(|| {
        classify_battle_resolution(side_defeated(&allies), side_defeated(&enemies), true)
            .expect("exhausting the bounded round schedule is terminal")
    });

    let terminal_seconds = recorder
        .timeline
        .last()
        .map_or(rounds as f32 * parameters.combat_round_seconds, |event| {
            event.time_seconds
        });
    recorder.record_timeline(
        MeleeTimelineEvent::at(MeleeTimelineKind::Terminal, terminal_seconds)
            .with_terminal(resolution),
    );

    recorder.summary.ammunition_used = allies
        .iter()
        .chain(&enemies)
        .map(|combatant| {
            combatant
                .initial_ammunition
                .saturating_sub(combatant.equipment.ammunition)
        })
        .sum();

    BattleOutcome {
        seed,
        resolution,
        rounds,
        allies: allies.into_iter().map(outcome).collect(),
        enemies: enemies.into_iter().map(outcome).collect(),
        summary: recorder.summary,
        log: recorder.log,
        timeline: recorder.timeline,
    }
}

fn classify_battle_resolution(
    allies_defeated: bool,
    enemies_defeated: bool,
    round_limit_reached: bool,
) -> Option<BattleResolution> {
    match (allies_defeated, enemies_defeated) {
        (true, true) => Some(BattleResolution::MutualIncapacitation),
        (true, false) => Some(BattleResolution::EnemiesVictory),
        (false, true) => Some(BattleResolution::AlliesVictory),
        (false, false) if round_limit_reached => Some(BattleResolution::Timeout),
        (false, false) => None,
    }
}

fn initialize_melee_phases(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    random: &mut SplitMix64,
    parameters: crate::combat::AutoresolveParameters,
) {
    let mut order = allies
        .iter()
        .enumerate()
        .map(|(index, combatant)| (combatant.id, ScheduledBattleSide::Allies, index))
        .chain(
            enemies
                .iter()
                .enumerate()
                .map(|(index, combatant)| (combatant.id, ScheduledBattleSide::Enemies, index)),
        )
        .collect::<Vec<_>>();
    order.sort_by_key(|(id, _, _)| *id);
    for (_, side, index) in order {
        let combatant = match side {
            ScheduledBattleSide::Allies => &mut allies[index],
            ScheduledBattleSide::Enemies => &mut enemies[index],
        };
        let sampled_delay = parameters.melee_initiative_delay_min_seconds
            + random.unit_f32()
                * (parameters.melee_initiative_delay_max_seconds
                    - parameters.melee_initiative_delay_min_seconds);
        let delay = sampled_delay * (3.0 / combatant.attributes.instinct.max(0.5)).clamp(0.6, 2.0);
        combatant.melee_recovery_until_seconds = delay;
        combatant.melee_interval_jitter_seconds =
            random.unit_f32() * parameters.melee_cadence_jitter_seconds;
    }
}

fn resolve_battle_round(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    resolve_ranged_round(allies, enemies, round, random, recorder, parameters);
    resolve_joint_melee_round(allies, enemies, round, random, recorder, parameters);
    allies
        .iter_mut()
        .chain(enemies)
        .for_each(|combatant| combatant.advance_condition(parameters.combat_round_seconds));
}

#[derive(Clone, Copy)]
enum ScheduledBattleSide {
    Allies,
    Enemies,
}

fn resolve_joint_melee_round(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    const TACTICAL_SERVER_STEPS_PER_SECOND: f32 = 64.0;
    let step_count = (parameters.combat_round_seconds * TACTICAL_SERVER_STEPS_PER_SECOND)
        .round()
        .max(1.0) as usize;
    let window_seconds = parameters.combat_round_seconds / step_count as f32;
    let round_start_seconds = round.saturating_sub(1) as f32 * parameters.combat_round_seconds;
    for step in 0..step_count {
        if side_defeated(allies) || side_defeated(enemies) {
            break;
        }
        resolve_joint_melee_window(
            allies,
            enemies,
            round,
            round_start_seconds + step as f32 * window_seconds,
            window_seconds,
            random,
            recorder,
            parameters,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the discrete event window owns both mutable battle sides and its deterministic recorder"
)]
fn resolve_joint_melee_window(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    round: usize,
    window_start_seconds: f32,
    window_seconds: f32,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    let window_end_seconds = window_start_seconds + window_seconds;
    reschedule_joint_swept_contacts(
        allies,
        enemies,
        window_start_seconds,
        window_seconds,
        parameters,
    );
    let allies_first = allies
        .first()
        .zip(enemies.first())
        .is_none_or(|(ally, enemy)| ally.id <= enemy.id);
    let allied_attacks = scheduled_side_contacts_in_window(
        allies,
        enemies,
        window_start_seconds,
        window_end_seconds,
        parameters,
    );
    let enemy_attacks = scheduled_side_contacts_in_window(
        enemies,
        allies,
        window_start_seconds,
        window_end_seconds,
        parameters,
    );
    let mut attacks = allied_attacks
        .into_iter()
        .map(|attack| (ScheduledBattleSide::Allies, attack))
        .chain(
            enemy_attacks
                .into_iter()
                .map(|attack| (ScheduledBattleSide::Enemies, attack)),
        )
        .collect::<Vec<_>>();
    let movement_until_seconds = attacks
        .iter()
        .map(|(_, attack)| attack.attack_timing.contact_at_seconds)
        .min_by(f32::total_cmp)
        .unwrap_or(window_end_seconds);
    attacks.retain(|(_, attack)| {
        (attack.attack_timing.contact_at_seconds - movement_until_seconds).abs() <= 1.0e-6
    });
    advance_joint_melee_movement(
        allies,
        enemies,
        window_start_seconds,
        (movement_until_seconds - window_start_seconds).max(0.0),
        recorder,
        parameters,
    );
    attacks.sort_by(|(left_side, left), (right_side, right)| {
        left.attack_timing
            .contact_at_seconds
            .total_cmp(&right.attack_timing.contact_at_seconds)
            .then_with(|| {
                let left_id = match left_side {
                    ScheduledBattleSide::Allies => allies[left.attacker_index].id,
                    ScheduledBattleSide::Enemies => enemies[left.attacker_index].id,
                };
                let right_id = match right_side {
                    ScheduledBattleSide::Allies => allies[right.attacker_index].id,
                    ScheduledBattleSide::Enemies => enemies[right.attacker_index].id,
                };
                left_id.cmp(&right_id)
            })
    });
    let contact_batches = attacks
        .iter()
        .map(|(side, attack)| {
            let contact_seconds = attack.attack_timing.contact_at_seconds;
            let members = attacks
                .iter()
                .filter(|(_, candidate)| {
                    candidate.attack_timing.contact_at_seconds == contact_seconds
                })
                .map(|(candidate_side, candidate)| {
                    let attacker_id = match candidate_side {
                        ScheduledBattleSide::Allies => allies[candidate.attacker_index].id,
                        ScheduledBattleSide::Enemies => enemies[candidate.attacker_index].id,
                    };
                    candidate.attack_timing.attack_id(attacker_id)
                })
                .collect::<Vec<_>>();
            let attacker_id = match side {
                ScheduledBattleSide::Allies => allies[attack.attacker_index].id,
                ScheduledBattleSide::Enemies => enemies[attack.attacker_index].id,
            };
            let attack_id = attack.attack_timing.attack_id(attacker_id);
            let order = members
                .iter()
                .position(|member| *member == attack_id)
                .unwrap_or_default() as u32;
            MeleeContactBatch {
                id: members.first().copied().unwrap_or(attack_id),
                members,
                order,
            }
        })
        .collect::<Vec<_>>();
    for ((side, attack), contact_batch) in attacks.into_iter().zip(contact_batches) {
        let simultaneous = contact_batch.members.len() > 1;
        let attacker = match side {
            ScheduledBattleSide::Allies => &allies[attack.attacker_index],
            ScheduledBattleSide::Enemies => &enemies[attack.attacker_index],
        };
        if !scheduled_attack_is_current(attacker, attack.attack_timing)
            || (attacker.is_defeated() && !simultaneous)
        {
            continue;
        }
        match side {
            ScheduledBattleSide::Allies => resolve_melee_turn(
                attack.attacker_index,
                attack.target_index,
                attack.flanking,
                allies,
                enemies,
                round,
                random,
                recorder,
                parameters,
                attack.attack_timing,
                contact_batch.clone(),
            ),
            ScheduledBattleSide::Enemies => resolve_melee_turn(
                attack.attacker_index,
                attack.target_index,
                attack.flanking,
                enemies,
                allies,
                round,
                random,
                recorder,
                parameters,
                attack.attack_timing,
                contact_batch,
            ),
        }
        let attacker = match side {
            ScheduledBattleSide::Allies => &mut allies[attack.attacker_index],
            ScheduledBattleSide::Enemies => &mut enemies[attack.attacker_index],
        };
        attacker.melee_attack_started_at_seconds = None;
        attacker.melee_attack_contact_at_seconds = None;
        attacker.melee_attack_scheduled_measure_metres = None;
    }
    if side_defeated(allies) || side_defeated(enemies) {
        return;
    }
    if movement_until_seconds + 1.0e-6 < window_end_seconds {
        // A reciprocal attack can enter its striking band later in this same
        // fixed step.  Resolve that remaining interval as its own event
        // window so movement can never advance past the later contact.
        resolve_joint_melee_window(
            allies,
            enemies,
            round,
            movement_until_seconds,
            window_end_seconds - movement_until_seconds,
            random,
            recorder,
            parameters,
        );
        return;
    }
    advance_joint_melee_movement(
        allies,
        enemies,
        movement_until_seconds,
        (window_end_seconds - movement_until_seconds).max(0.0),
        recorder,
        parameters,
    );
    if allies_first {
        schedule_side_melee_attacks_in_window(
            allies,
            enemies,
            window_end_seconds,
            0.0,
            random,
            recorder,
            parameters,
        );
        schedule_side_melee_attacks_in_window(
            enemies,
            allies,
            window_end_seconds,
            0.0,
            random,
            recorder,
            parameters,
        );
    } else {
        schedule_side_melee_attacks_in_window(
            enemies,
            allies,
            window_end_seconds,
            0.0,
            random,
            recorder,
            parameters,
        );
        schedule_side_melee_attacks_in_window(
            allies,
            enemies,
            window_end_seconds,
            0.0,
            random,
            recorder,
            parameters,
        );
    }
}

fn reschedule_joint_swept_contacts(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    interval_start_seconds: f32,
    elapsed_seconds: f32,
    parameters: crate::combat::AutoresolveParameters,
) {
    let pair_count = allies.len().min(enemies.len());
    for index in 0..pair_count {
        if allies[index].id <= enemies[index].id {
            reschedule_swept_pair_contacts(
                &mut allies[index],
                &mut enemies[index],
                interval_start_seconds,
                elapsed_seconds,
                parameters,
            );
        } else {
            reschedule_swept_pair_contacts(
                &mut enemies[index],
                &mut allies[index],
                interval_start_seconds,
                elapsed_seconds,
                parameters,
            );
        }
    }
}

fn advance_joint_melee_movement(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    interval_start_seconds: f32,
    elapsed_seconds: f32,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    // The current strategic assignment model owns one distance per combatant.
    // Pair each combatant at most once so no actor can consume the interval's
    // movement time twice in an outnumbered engagement.
    let pair_count = allies.len().min(enemies.len());
    for index in 0..pair_count {
        if allies[index].id <= enemies[index].id {
            advance_melee_pair_movement(
                &mut allies[index],
                &mut enemies[index],
                interval_start_seconds,
                elapsed_seconds,
                recorder,
                parameters,
            );
        } else {
            advance_melee_pair_movement(
                &mut enemies[index],
                &mut allies[index],
                interval_start_seconds,
                elapsed_seconds,
                recorder,
                parameters,
            );
        }
    }
}

fn apply_pending_attacks(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    attacks: &[PendingAttack],
    recorder: &mut BattleRecorder,
) {
    for attack in attacks {
        let effect = apply_attack_result(
            &mut attackers[attack.attacker_index],
            &mut defenders[attack.target_index],
            attack.result,
            attack.part,
        );
        recorder.record_attack(
            attack.phase,
            attack.round,
            attackers[attack.attacker_index].id,
            defenders[attack.target_index].id,
            attack.mode,
            match attack.mode {
                AttackMode::Melee => attackers[attack.attacker_index].equipment.melee_weapon_id,
                AttackMode::Ranged => attackers[attack.attacker_index].equipment.ranged_weapon_id,
            },
            (attack.mode == AttackMode::Ranged)
                .then_some(
                    attackers[attack.attacker_index]
                        .equipment
                        .ranged_projectile_kind,
                )
                .flatten(),
            defender_contact_item_id(attack.result, &defenders[attack.target_index].equipment),
            "none",
            attack.part,
            attack.result,
            effect,
            None,
        );
    }
}

fn resolve_opening_volleys(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    let ally_plans = opening_volley_plans(allies, enemies, parameters);
    let enemy_plans = opening_volley_plans(enemies, allies, parameters);
    let ally_detour_targets: Vec<_> = active_melee_indices(enemies)
        .into_iter()
        .skip(active_melee_indices(allies).len())
        .collect();
    let enemy_detour_targets: Vec<_> = active_melee_indices(allies)
        .into_iter()
        .skip(active_melee_indices(enemies).len())
        .collect();
    let steps = ally_plans
        .iter()
        .chain(&enemy_plans)
        .map(|plan| plan.total_attacks)
        .max()
        .unwrap_or(0);

    for step in 0..steps {
        if side_defeated(allies) || side_defeated(enemies) {
            break;
        }
        if random.next_u64().is_multiple_of(2) {
            take_opening_volley_step(
                allies,
                &ally_plans,
                enemies,
                &ally_detour_targets,
                step,
                random,
                recorder,
                parameters,
            );
            take_opening_volley_step(
                enemies,
                &enemy_plans,
                allies,
                &enemy_detour_targets,
                step,
                random,
                recorder,
                parameters,
            );
        } else {
            take_opening_volley_step(
                enemies,
                &enemy_plans,
                allies,
                &enemy_detour_targets,
                step,
                random,
                recorder,
                parameters,
            );
            take_opening_volley_step(
                allies,
                &ally_plans,
                enemies,
                &ally_detour_targets,
                step,
                random,
                recorder,
                parameters,
            );
        }
    }
}

fn opening_volley_plans(
    ranged_side: &[Combatant],
    closing_side: &[Combatant],
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<OpeningVolleyPlan> {
    let screen_count = active_melee_indices(ranged_side).len();
    let closing_melee = active_melee_indices(closing_side);
    let closing_melee_count = closing_melee.len();
    let direct_closing_speed = closing_melee
        .iter()
        .map(|index| {
            closing_side[*index].movement_speed_meters_per_second(
                parameters.minimum_movement_speed_metres_per_second,
            )
        })
        .fold(
            parameters.minimum_movement_speed_metres_per_second,
            f32::max,
        );
    ranged_side
        .iter()
        .map(|attacker| {
            if attacker.is_defeated()
                || preferred_attack_mode(attacker) != AttackMode::Ranged
                || closing_melee_count == 0
            {
                return OpeningVolleyPlan::default();
            }

            let weapon = attacker.equipment.ranged_weapon.unwrap();
            let interval = weapon
                .attack_interval_seconds
                .max(parameters.minimum_attack_interval_seconds);
            let range = weapon.ranged_range.max(0.0);
            let direct_seconds = range / direct_closing_speed;
            let direct_attacks = (direct_seconds / interval)
                .ceil()
                .clamp(0.0, AUTORESOLVE_MAX_RANGED_ATTACKS_PER_PHASE as f32)
                as usize;
            let detour = if closing_melee_count > screen_count && screen_count > 0 {
                let formation_radius =
                    screen_count as f32 * parameters.formation_spacing_metres * 0.5;
                std::f32::consts::PI * formation_radius
            } else {
                0.0
            };
            let surplus_speed = closing_melee
                .iter()
                .skip(screen_count)
                .map(|index| {
                    closing_side[*index].movement_speed_meters_per_second(
                        parameters.minimum_movement_speed_metres_per_second,
                    )
                })
                .fold(direct_closing_speed, f32::max);
            let total_seconds = direct_seconds + detour / surplus_speed;
            OpeningVolleyPlan {
                direct_attacks,
                total_attacks: (total_seconds / interval)
                    .ceil()
                    .clamp(0.0, AUTORESOLVE_MAX_RANGED_ATTACKS_PER_PHASE as f32)
                    as usize,
            }
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "opening volleys carry two sides, their plan, deterministic state, and tuning"
)]
fn take_opening_volley_step(
    attackers: &mut [Combatant],
    plans: &[OpeningVolleyPlan],
    defenders: &mut [Combatant],
    detour_targets: &[usize],
    step: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    for attacker_index in 0..attackers.len() {
        let plan = plans[attacker_index];
        if plan.total_attacks <= step
            || attackers[attacker_index].is_defeated()
            || !attackers[attacker_index].can_attack_ranged()
        {
            continue;
        }
        let targets = if step < plan.direct_attacks {
            prioritized_ranged_targets(defenders)
        } else {
            let ranged = active_ranged_indices(defenders);
            if ranged.is_empty() {
                detour_targets
                    .iter()
                    .copied()
                    .filter(|index| !defenders[*index].is_defeated())
                    .collect()
            } else {
                ranged
            }
        };
        if targets.is_empty() {
            break;
        }
        let target_index = targets[random.index(targets.len())];
        let part = crate::combat::targeting::body_part_from_contact_sample(random.unit_f32());
        let result = autoresolve_optimal_ranged_exchange(
            &attackers[attacker_index],
            &defenders[target_index],
            autoresolve_hit_precision(random, parameters),
            part,
            parameters,
        );
        attackers[attacker_index].equipment.ammunition -= 1;
        let effect = apply_attack_result(
            &mut attackers[attacker_index],
            &mut defenders[target_index],
            result,
            part,
        );
        recorder.summary.opening_ranged_attacks += 1;
        recorder.record_attack(
            "opening",
            0,
            attackers[attacker_index].id,
            defenders[target_index].id,
            AttackMode::Ranged,
            attackers[attacker_index].equipment.ranged_weapon_id,
            attackers[attacker_index].equipment.ranged_projectile_kind,
            defender_contact_item_id(result, &defenders[target_index].equipment),
            "none",
            part,
            result,
            effect,
            None,
        );
    }
}

fn resolve_ranged_round(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    let ally_attacks = plan_ranged_round(allies, enemies, round, random, parameters);
    let enemy_attacks = plan_ranged_round(enemies, allies, round, random, parameters);
    apply_pending_attacks(allies, enemies, &ally_attacks, recorder);
    apply_pending_attacks(enemies, allies, &enemy_attacks, recorder);
}

fn plan_ranged_round(
    attackers: &mut [Combatant],
    defenders: &[Combatant],
    round: usize,
    random: &mut SplitMix64,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<PendingAttack> {
    let mut attacks = Vec::new();
    for (attacker_index, attacker) in attackers.iter_mut().enumerate() {
        if attacker.is_defeated() || !attacker.can_attack_ranged() {
            continue;
        }
        let interval = attacker
            .equipment
            .ranged_weapon
            .unwrap()
            .attack_interval_seconds
            .max(parameters.minimum_attack_interval_seconds);
        attacker.ranged_attack_progress += parameters.combat_round_seconds / interval;
        let attack_count =
            (attacker.ranged_attack_progress.floor() as u32).min(attacker.equipment.ammunition);
        attacker.ranged_attack_progress -= attack_count as f32;

        for _ in 0..attack_count {
            let targets = prioritized_ranged_targets(defenders);
            if targets.is_empty() {
                break;
            }
            let target_index = targets[random.index(targets.len())];
            let part = crate::combat::targeting::body_part_from_contact_sample(random.unit_f32());
            let result = autoresolve_optimal_ranged_exchange(
                attacker,
                &defenders[target_index],
                autoresolve_hit_precision(random, parameters),
                part,
                parameters,
            );
            attacker.equipment.ammunition -= 1;
            attacks.push(PendingAttack {
                attacker_index,
                target_index,
                result,
                part,
                mode: AttackMode::Ranged,
                phase: "main",
                round,
            });
        }
    }
    attacks
}

fn apply_attack_result(
    attacker: &mut Combatant,
    defender: &mut Combatant,
    result: AttackResult,
    part: BodyPart,
) -> AttackEffect {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            attacker.imbalance += balance_damage.max(0.0);
            AttackEffect {
                hit: false,
                health_damage: 0.0,
            }
        }
        AttackResult::ToDefender {
            balance_damage,
            blunt_damage,
            ..
        } => {
            defender.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, part);
            let applied = defender.body.apply_damage(part, damage);
            defender.acute_trauma += acute_trauma_incapacitation(part, applied);
            let (applied_cut, applied_blunt) = apportion_attack_health_damage(result, applied);
            defender.cut_damage += applied_cut;
            defender.wounds.extend(wounds_from_applied_health_damage(
                part,
                applied_cut,
                applied_blunt,
                blunt_damage,
            ));
            AttackEffect {
                hit: true,
                health_damage: applied,
            }
        }
    }
}

fn preferred_attack_mode(attacker: &Combatant) -> AttackMode {
    if attacker.can_attack_ranged() {
        AttackMode::Ranged
    } else {
        AttackMode::Melee
    }
}

fn ranged_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    part: BodyPart,
    response: DefenderResponse,
) -> AttackResult {
    let attacker_equipment = attacker.equipment.for_ranged();
    let attacker_view = attacker.view_with_equipment(&attacker_equipment);
    attacker_view.resolve_ranged_attack(
        crate::combat::EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        &defender.view_with_equipment(&defender.equipment),
        &defender.bestiary_categories,
        response,
        precision,
        flanking,
        part,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AutoresolveMeleeDefenderDecision {
    response: DefenderResponse,
    committed: Option<CommittedThreatDecision>,
}

impl AutoresolveMeleeDefenderDecision {
    const fn response(response: DefenderResponse) -> Self {
        Self {
            response,
            committed: None,
        }
    }
}

fn autoresolve_melee_defender_response(
    defender: &Combatant,
    reaction_sample: f32,
    reaction_timing_sample: f32,
    commitment_sample: f32,
    incoming: MeleeAttackTiming,
    defender_phase: MeleeDefenderPhase,
    parameters: crate::combat::AutoresolveParameters,
) -> AutoresolveMeleeDefenderDecision {
    let can_block =
        defender.equipment.melee_weapon.is_some() || defender.equipment.shield_block_bonus > 0.0;
    let reflex = autoresolve_melee_input_reflex(reaction_timing_sample, parameters);
    // Tactical bots choose a dodge when the incoming attack starts. That
    // decision is not an outcome oracle: the shared physical geometry later
    // applies agility, load, fatigue, elapsed contact time, tracking, reach,
    // and arc to determine a miss, redirection, or unchanged contact.
    if reaction_sample < parameters.melee_dodge_reaction_chance {
        return AutoresolveMeleeDefenderDecision::response(DefenderResponse::Dodge {
            input_reflex: reflex,
        });
    }
    if can_block && let MeleeDefenderPhase::CommittedAttack(defender_attack) = defender_phase {
        let started_after_incoming =
            defender_attack.started_at_seconds > incoming.started_at_seconds;
        let started_before_contact =
            defender_attack.started_at_seconds <= incoming.contact_at_seconds;
        let response_delay = defender_attack.started_at_seconds - incoming.started_at_seconds;
        if started_after_incoming
            && started_before_contact
            && response_delay <= parameters.melee_reflex_window_seconds
        {
            // This is the same temporal contract as tactical authority: only
            // an attack committed after the observed incoming start can bind
            // that incoming implement, and the exact start-time difference
            // determines reflex. No random overlap gate or outcome oracle is
            // involved.
            let weapon = defender.equipment.melee_weapon.unwrap_or_default();
            let melee_equipment = defender.equipment.for_melee();
            let weapon_skill = weapon.skills.weighted_check(|skill| {
                defender.skills.skill_check_by_parts(
                    skill,
                    &defender.attributes,
                    &defender.body,
                    &defender.essentials,
                    &melee_equipment,
                    LimbWeights::all_equal(),
                )
            });
            let intercept_timing =
                (1.0 - response_delay / parameters.melee_reflex_window_seconds).clamp(0.0, 1.0);
            let expected_intercept_engagement = (intercept_timing
                * ((weapon_skill + defender.attributes.instinct) / 10.0).clamp(0.0, 1.0)
                * defender.fatigue_performance())
            .clamp(0.0, 1.0);
            let committed = choose_committed_threat_response(CommittedThreatFacts {
                own_contact_after_incoming_seconds: defender_attack.contact_at_seconds
                    - incoming.contact_at_seconds,
                own_windup_seconds: defender_attack.contact_at_seconds
                    - defender_attack.started_at_seconds,
                expected_intercept_engagement,
                incapacitation: defender.incapacitation(),
                weapon_moment_of_inertia_kg_m2: weapon.moment_of_inertia_kg_m2,
                weapon_recovery_seconds: (weapon.attack_interval_seconds
                    - parameters.melee_windup_seconds)
                    .max(0.0),
                consecutive_intercepts: defender.melee_consecutive_intercepts,
                decision_sample: commitment_sample,
            });
            let response = if committed.choice == CommittedThreatChoice::FinishTrade {
                DefenderResponse::None
            } else {
                reciprocal_intercept_response(
                    1.0 - response_delay / parameters.melee_reflex_window_seconds,
                    parameters.maximum_hit_precision,
                    defender.equipment.shield_block_bonus,
                )
            };
            return AutoresolveMeleeDefenderDecision {
                response,
                committed: Some(committed),
            };
        }
        if defender_attack.started_at_seconds <= incoming.contact_at_seconds
            && defender_attack.recovery_until_seconds > incoming.contact_at_seconds
        {
            return AutoresolveMeleeDefenderDecision::response(DefenderResponse::None);
        }
    }
    if let MeleeDefenderPhase::OccupiedRecovery { until_seconds } = defender_phase
        && until_seconds > incoming.contact_at_seconds
    {
        if can_block {
            // Recovery is a continuous loss of guard mobility, not a binary
            // period of helplessness. The fraction of the incoming windup not
            // consumed by remaining recovery is the implement-alignment time
            // available before contact; the shared alignment resolver applies
            // this reduced effectiveness to the actual defense outcome.
            let effectiveness = recovery_guard_effectiveness(
                until_seconds,
                incoming.contact_at_seconds,
                parameters.melee_windup_seconds,
            );
            if effectiveness > 0.0 {
                return AutoresolveMeleeDefenderDecision::response(DefenderResponse::Block {
                    effectiveness,
                });
            }
        }
        return AutoresolveMeleeDefenderDecision::response(DefenderResponse::None);
    }
    if can_block {
        // A neutral tactical bot holds its implement in a raised guard. The
        // shared continuous alignment resolver still determines whether and
        // how deeply that selected block catches the incoming weapon.
        return AutoresolveMeleeDefenderDecision::response(DefenderResponse::Block {
            effectiveness: 1.0,
        });
    }
    AutoresolveMeleeDefenderDecision::response(DefenderResponse::None)
}

fn recovery_guard_effectiveness(
    recovery_until_seconds: f32,
    contact_at_seconds: f32,
    incoming_windup_seconds: f32,
) -> f32 {
    let remaining_recovery = (recovery_until_seconds - contact_at_seconds).max(0.0);
    (1.0 - remaining_recovery / incoming_windup_seconds.max(f32::EPSILON)).clamp(0.0, 1.0)
}

fn autoresolve_melee_input_reflex(
    timing_sample: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    autoresolve_melee_reaction_timing(timing_sample, parameters).input_reflex
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MeleeAttackTiming {
    pub started_at_seconds: f32,
    pub contact_at_seconds: f32,
    pub recovery_until_seconds: f32,
}

impl MeleeAttackTiming {
    fn attack_id(self, attacker_id: u64) -> u64 {
        attacker_id.wrapping_shl(32) ^ MeleeTimelineEvent::tick_at(self.started_at_seconds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MeleeDefenderPhase {
    NeutralGuard,
    CommittedAttack(MeleeAttackTiming),
    OccupiedRecovery { until_seconds: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AutoresolveMeleeReactionTiming {
    input_reflex: f32,
    displacement_time_seconds: f32,
}

fn autoresolve_melee_reaction_timing(
    timing_sample: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> AutoresolveMeleeReactionTiming {
    let windup = parameters.melee_windup_seconds;
    let reaction_delay = parameters.melee_reaction_delay_min_seconds
        + (parameters.melee_reaction_delay_max_seconds
            - parameters.melee_reaction_delay_min_seconds)
            * timing_sample.clamp(0.0, 1.0);
    let elapsed_after_input = (windup - reaction_delay).max(0.0);
    AutoresolveMeleeReactionTiming {
        input_reflex: (1.0 - elapsed_after_input / parameters.melee_reflex_window_seconds)
            .clamp(parameters.minimum_melee_input_reflex, 1.0),
        // Preserve actual contact time for physical displacement. Clamping the
        // derived reflex is a defense-effectiveness floor, not permission to
        // erase time the body has already spent moving.
        displacement_time_seconds: elapsed_after_input,
    }
}

fn autoresolve_optimal_ranged_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    part: BodyPart,
    parameters: crate::combat::AutoresolveParameters,
) -> AttackResult {
    let block = if defender.equipment.melee_weapon.is_some()
        || defender.equipment.shield_block_bonus > 0.0
    {
        DefenderResponse::Block { effectiveness: 1.0 }
    } else {
        DefenderResponse::None
    };
    [
        DefenderResponse::None,
        DefenderResponse::Dodge {
            input_reflex: parameters.ranged_defense_input_reflex,
        },
        block,
    ]
    .into_iter()
    .map(|response| ranged_exchange(attacker, defender, precision, 0.0, part, response))
    .min_by(|left, right| attack_harm(*left).total_cmp(&attack_harm(*right)))
    .unwrap()
}

fn autoresolve_hit_precision(
    random: &mut SplitMix64,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    parameters.minimum_hit_precision
        + random.unit_f32() * (parameters.maximum_hit_precision - parameters.minimum_hit_precision)
}

fn attack_harm(result: AttackResult) -> f32 {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => -balance_damage,
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => cut_damage + blunt_damage + balance_damage,
    }
}

fn side_defeated(side: &[Combatant]) -> bool {
    side.is_empty() || side.iter().all(Combatant::is_defeated)
}

fn outcome(combatant: Combatant) -> CombatantOutcome {
    let incapacitated = combatant.is_incapacitated();
    let incapacitation = combatant.incapacitation();
    let open_wound_count = combatant
        .wounds
        .iter()
        .filter(|wound| wound.kind == CombatWoundKind::Open)
        .count();
    let internal_wound_count = combatant
        .wounds
        .iter()
        .filter(|wound| wound.kind == CombatWoundKind::Internal)
        .count();
    let wound_flow_fraction_per_second = combatant
        .wounds
        .iter()
        .map(|wound| wound.blood_fraction_per_second)
        .sum();
    let will = combatant.skills.skill_check_by_parts(
        Skill::Will,
        &combatant.attributes,
        &combatant.body,
        &combatant.essentials,
        &combatant.equipment,
        LimbWeights::all_equal(),
    );
    let pain_incapacitation =
        crate::morale::pain_incapacitation(combatant.body.total_damage(), will);
    let blood_loss_incapacitation = crate::morale::blood_loss_incapacitation(
        (combatant.starting_blood_fraction - combatant.blood_loss_fraction).clamp(0.0, 1.0),
        1.0,
    );
    let oxygen_debt_incapacitation =
        oxygen_debt_incapacitation(combatant.oxygen_debt_joules, combatant.attributes.endurance);
    let terminal_cause = if combatant.yielded {
        Some(CombatTerminalCause::YieldedUnableToContinue)
    } else if incapacitated {
        [
            (
                combatant.starting_incapacitation,
                CombatTerminalCause::StartingCondition,
            ),
            (pain_incapacitation, CombatTerminalCause::Pain),
            (blood_loss_incapacitation, CombatTerminalCause::BloodLoss),
            (combatant.acute_trauma, CombatTerminalCause::AcuteTrauma),
            (oxygen_debt_incapacitation, CombatTerminalCause::OxygenDebt),
            (combatant.imbalance, CombatTerminalCause::Imbalance),
        ]
        .into_iter()
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, cause)| cause)
    } else {
        None
    };
    CombatantOutcome {
        id: combatant.id,
        body: combatant.body,
        blood_loss_fraction: combatant.blood_loss_fraction,
        cut_damage: combatant.cut_damage,
        incapacitated,
        yielded: combatant.yielded,
        incapacitation,
        imbalance: combatant.imbalance,
        acute_trauma: combatant.acute_trauma,
        pain_incapacitation,
        oxygen_debt_joules: combatant.oxygen_debt_joules,
        local_action_fatigue: combatant.local_action_fatigue,
        wound_count: combatant.wounds.len(),
        open_wound_count,
        internal_wound_count,
        wound_flow_fraction_per_second,
        ammunition_used: combatant
            .initial_ammunition
            .saturating_sub(combatant.equipment.ammunition),
        terminal_cause,
    }
}

pub const fn body_part_index(part: BodyPart) -> usize {
    match part {
        BodyPart::LeftArm => 0,
        BodyPart::RightArm => 1,
        BodyPart::LeftLeg => 2,
        BodyPart::RightLeg => 3,
        BodyPart::Chest => 4,
        BodyPart::Stomach => 5,
        BodyPart::Head => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bestiary::{ThreatId, profile};
    use crate::item_catalog_schema::EquipmentMaterial;

    fn autoresolve_parameters() -> crate::combat::AutoresolveParameters {
        crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS
    }

    fn fighter(id: u64, skill: f32, ranged: bool) -> Combatant {
        let mut fighter = Combatant::new(id);
        fighter.attributes = PlayerAttributeValues {
            endurance: 3.0,
            intelligence: 2.0,
            instinct: 3.0,
            left_arm_strength: skill,
            right_arm_strength: skill,
            left_leg_strength: 3.0,
            right_leg_strength: 3.0,
            left_arm_agility: skill,
            right_arm_agility: skill,
            left_leg_agility: skill,
            right_leg_agility: skill,
            ..PlayerAttributeValues::default()
        };
        fighter.skills = CombatSkills {
            sword_hours: skill * 2_000.0,
            bow_hours: skill * 3_000.0,
            dodge_hours: skill * 2_000.0,
            block_hours: skill * 2_000.0,
            will_hours: skill * 1_000.0,
            balance_hours: skill * 2_000.0,
            ..CombatSkills::default()
        };
        let weapon = CombatWeapon {
            skills: if ranged {
                crate::equipment::WeaponSkillDistribution {
                    bow: 1.0,
                    ..Default::default()
                }
            } else {
                crate::equipment::WeaponSkillDistribution {
                    sword: 1.0,
                    ..Default::default()
                }
            },
            melee: !ranged,
            ranged,
            slash: !ranged,
            pierce: ranged,
            accuracy: 1.5,
            swing_precision: if ranged { 0.0 } else { 1.5 },
            stab_precision: if ranged { 0.0 } else { 1.5 },
            preferred_melee_style: crate::combat_style::MeleeAttackStyle::Swing,
            weight: 1.5,
            penetration: 1.0,
            melee_reach: if ranged { 0.0 } else { 1.0 },
            grip_to_tip_m: if ranged { 0.0 } else { 0.8 },
            total_length_m: if ranged { 0.0 } else { 1.0 },
            striking_head_length_m: if ranged { 0.0 } else { 0.8 },
            body_material: (!ranged).then_some(EquipmentMaterial::RoughSteel),
            striking_material: (!ranged).then_some(EquipmentMaterial::RoughSteel),
            ranged_range: if ranged { 20.0 } else { 0.0 },
            attack_interval_seconds: 1.0,
            ranged_force_joules: 50.0,
            ..CombatWeapon::default()
        };
        fighter.equipment.weapon = Some(weapon);
        if ranged {
            fighter.equipment.ranged_weapon = Some(weapon);
            fighter.equipment.ammunition = 32;
            fighter.initial_ammunition = 32;
        } else {
            fighter.equipment.melee_weapon = Some(weapon);
        }
        fighter
    }

    #[test]
    fn combat_power_tracks_training_equipment_and_condition() {
        let novice = fighter(1, 1.0, false);
        let mut trained = novice.clone();
        trained.id = 2;
        trained.skills = fighter(2, 4.0, false).skills;
        assert!(autoresolve_combat_power(&trained) > autoresolve_combat_power(&novice));

        let mut armored = novice.clone();
        armored.equipment.armor.fill(CombatArmor {
            resistance: 25.0,
            padding: 15.0,
            coverage: 0.8,
            ..CombatArmor::default()
        });
        assert!(autoresolve_combat_power(&armored) > autoresolve_combat_power(&novice));

        let mut impaired = trained.clone();
        impaired.starting_incapacitation = 0.75;
        assert!(autoresolve_combat_power(&impaired) < autoresolve_combat_power(&trained));

        let mut heavier = novice.clone();
        let weapon = heavier.equipment.melee_weapon.as_mut().unwrap();
        weapon.weight *= 2.0;
        heavier.equipment.weapon = heavier.equipment.melee_weapon;
        assert!(autoresolve_combat_power(&heavier) > autoresolve_combat_power(&novice));

        let mut penetrating = novice.clone();
        let weapon = penetrating.equipment.melee_weapon.as_mut().unwrap();
        weapon.penetration *= 2.0;
        penetrating.equipment.weapon = penetrating.equipment.melee_weapon;
        assert!(autoresolve_combat_power(&penetrating) > autoresolve_combat_power(&novice));

        let mut longer = novice.clone();
        let weapon = longer.equipment.melee_weapon.as_mut().unwrap();
        weapon.melee_reach *= 2.0;
        longer.equipment.weapon = longer.equipment.melee_weapon;
        assert!(autoresolve_combat_power(&longer) > autoresolve_combat_power(&novice));

        let mut slower = novice.clone();
        let weapon = slower.equipment.melee_weapon.as_mut().unwrap();
        weapon.attack_interval_seconds *= 2.0;
        slower.equipment.weapon = slower.equipment.melee_weapon;
        assert!(autoresolve_combat_power(&slower) < autoresolve_combat_power(&novice));

        let mut chest_only = novice.clone();
        chest_only.equipment.armor[body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 25.0,
            padding: 15.0,
            coverage: 0.8,
            flexibility: 0.8,
            ..CombatArmor::default()
        };
        assert!(autoresolve_combat_power(&chest_only) > autoresolve_combat_power(&novice));
    }

    #[test]
    fn combat_power_bounds_non_finite_components_without_saturating() {
        assert_eq!(finite_log_component(f32::NAN, 1.0), 0.0);
        assert_eq!(finite_log_component(f32::NEG_INFINITY, 1.0), 0.0);
        let infinite = finite_log_component(f32::INFINITY, 4_000_000.0);
        assert!(infinite.is_finite());

        let mut extreme = fighter(1, 1.0, false);
        extreme.equipment.armor.fill(CombatArmor {
            resistance: f32::INFINITY,
            padding: f32::MAX,
            coverage: 1.0,
            range_of_motion: 0.5,
            ..CombatArmor::default()
        });
        let power = autoresolve_combat_power(&extreme);
        assert!(power > 0);
        assert!(power < u64::MAX);
    }

    #[test]
    fn combat_power_orders_authored_cross_profiles_and_margin_boundaries() {
        let cultist = authored_threat_combatant(1, "cultist", 1, 10_000, 10_000).unwrap();
        let veteran = authored_threat_combatant(2, "cultist", 6, 10_000, 10_000).unwrap();
        assert!(autoresolve_combat_power(&veteran) > autoresolve_combat_power(&cultist));
        assert_eq!(combat_power_meets_safety_margin(125, 100), Some(true));
        assert_eq!(combat_power_meets_safety_margin(124, 100), Some(false));
        assert_eq!(combat_power_meets_safety_margin(u64::MAX, 1), None);
    }

    #[test]
    fn precision_cap_averages_every_target_bestiary_category() {
        let mut attacker = fighter(1, 4.0, false);
        attacker.skills.bestiary_hours.human = adventuresim_world_schema::BESTIARY_MASTERY_HOURS;
        let mut human = fighter(2, 1.0, false);
        human.bestiary_categories = vec![BestiaryCategory::Human];
        let human_cap = attacker
            .view_with_equipment(&attacker.equipment)
            .precision_damage_multiplier_cap(&human.bestiary_categories);

        human.bestiary_categories = vec![BestiaryCategory::Human, BestiaryCategory::Draconid];
        let combined_cap = attacker
            .view_with_equipment(&attacker.equipment)
            .precision_damage_multiplier_cap(&human.bestiary_categories);

        assert!(human_cap > 2.0);
        assert!(combined_cap > 2.0);
        assert!(combined_cap < human_cap);
    }

    fn resolved_melee_health_damage(mut weapon: CombatWeapon, protection: CombatArmor) -> f32 {
        let mut attacker = fighter(1, 3.0, false);
        weapon.skills = crate::equipment::WeaponSkillDistribution {
            sword: 1.0,
            ..Default::default()
        };
        weapon.melee = true;
        weapon.accuracy = 1.5;
        weapon.swing_precision = 1.5;
        weapon.stab_precision = 1.5;
        weapon.preferred_melee_style = crate::combat_style::MeleeAttackStyle::Swing;
        weapon.weight = 1.5;
        weapon.melee_reach = 1.0;
        weapon.attack_interval_seconds = 1.0;
        attacker.equipment.weapon = Some(weapon);
        attacker.equipment.melee_weapon = Some(weapon);

        let mut defender = fighter(2, 1.0, false);
        defender.equipment.armor.fill(protection);
        let exchange = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            0.5,
            DefenderResponse::None,
            0.5,
            0.5,
        );
        health_damage_from_attack(exchange.result, exchange.contact.body_part)
    }

    #[test]
    fn fixed_seed_is_reproducible() {
        let first = resolve_battle(
            vec![fighter(1, 3.0, false)],
            vec![fighter(2, 2.0, false)],
            42,
            BattleOpening::Normal,
        );
        let second = resolve_battle(
            vec![fighter(1, 3.0, false)],
            vec![fighter(2, 2.0, false)],
            42,
            BattleOpening::Normal,
        );
        assert_eq!(first.resolution, second.resolution);
        assert_eq!(first.rounds, second.rounds);
        assert_eq!(first.allies[0].body.health, second.allies[0].body.health);
        assert_eq!(first.enemies[0].body.health, second.enemies[0].body.health);
    }

    #[test]
    fn melee_event_order_is_independent_of_ally_enemy_array_side() {
        let first = fighter(11, 3.0, false);
        let second = fighter(22, 3.0, false);
        let forward = resolve_battle(
            vec![first.clone()],
            vec![second.clone()],
            91,
            BattleOpening::Normal,
        );
        let reversed = resolve_battle(vec![second], vec![first], 91, BattleOpening::Normal);
        let causal_sequence = |outcome: &BattleOutcome| {
            outcome
                .log
                .iter()
                .map(|entry| {
                    (
                        entry.attacker_id,
                        entry.defender_id,
                        entry.round,
                        entry.outcome.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(causal_sequence(&forward), causal_sequence(&reversed));
        let nonterminal_timeline = |outcome: &BattleOutcome| {
            outcome
                .timeline
                .iter()
                .filter(|event| event.kind != MeleeTimelineKind::Terminal)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            nonterminal_timeline(&forward),
            nonterminal_timeline(&reversed)
        );
        assert_eq!(forward.rounds, reversed.rounds);
    }

    #[test]
    fn simultaneous_scheduled_contacts_both_resolve() {
        let mut ally = fighter(11, 3.0, false);
        let mut enemy = fighter(22, 3.0, false);
        ally.melee_engagement_target = Some(enemy.id);
        enemy.melee_engagement_target = Some(ally.id);
        ally.melee_engagement_distance_metres = 0.4;
        enemy.melee_engagement_distance_metres = 0.4;
        let mut allies = vec![ally];
        let mut enemies = vec![enemy];
        let mut recorder = BattleRecorder::default();
        resolve_joint_melee_round(
            &mut allies,
            &mut enemies,
            1,
            &mut SplitMix64::new(7),
            &mut recorder,
            autoresolve_parameters(),
        );
        assert_eq!(recorder.log.len(), 2);
        assert_eq!(recorder.log[0].round, 1);
        assert_eq!(recorder.log[1].round, 1);
        let contacts = recorder
            .timeline
            .iter()
            .filter(|event| event.kind == MeleeTimelineKind::Contact)
            .collect::<Vec<_>>();
        assert_eq!(contacts.len(), 2);
        assert_eq!(
            contacts[0].simultaneous_batch_id,
            contacts[1].simultaneous_batch_id
        );
        assert_eq!(contacts[0].simultaneous_members.len(), 2);
        assert_eq!(
            contacts[0].simultaneous_members,
            contacts[1].simultaneous_members
        );
        assert_eq!(contacts[0].simultaneous_order, Some(0));
        assert_eq!(contacts[1].simultaneous_order, Some(1));
    }

    #[test]
    fn timeline_canceled_attack_ids_never_emit_contacts() {
        let outcome = (0..64)
            .map(|seed| {
                resolve_battle(
                    vec![fighter(11, 3.0, false)],
                    vec![fighter(22, 3.0, false)],
                    seed,
                    BattleOpening::Normal,
                )
            })
            .find(|outcome| {
                outcome
                    .timeline
                    .iter()
                    .any(|event| event.kind == MeleeTimelineKind::AttackCanceled)
            })
            .expect("the deterministic seed range includes a committed cancellation");
        let canceled = outcome
            .timeline
            .iter()
            .filter(|event| event.kind == MeleeTimelineKind::AttackCanceled)
            .filter_map(|event| event.affected_attack_id)
            .collect::<Vec<_>>();
        let contacts = outcome
            .timeline
            .iter()
            .filter(|event| event.kind == MeleeTimelineKind::Contact)
            .filter_map(|event| event.attack_id)
            .collect::<Vec<_>>();

        assert!(!canceled.is_empty());
        assert!(
            canceled
                .iter()
                .all(|attack_id| !contacts.contains(attack_id))
        );
    }

    #[test]
    fn explicit_surprise_grants_exactly_one_authoritative_side_the_opener() {
        let allies_first = resolve_battle(
            vec![fighter(1, 5.0, false)],
            vec![fighter(2, 5.0, false)],
            77,
            BattleOpening::AlliesSurprise,
        );
        let enemies_first = resolve_battle(
            vec![fighter(1, 5.0, false)],
            vec![fighter(2, 5.0, false)],
            77,
            BattleOpening::EnemiesSurprise,
        );
        assert_eq!(allies_first.log.first().map(|hit| hit.attacker_id), Some(1));
        assert_eq!(
            enemies_first.log.first().map(|hit| hit.attacker_id),
            Some(2)
        );
        assert_eq!(allies_first.summary.stealth_attempts, 0);
        assert_eq!(enemies_first.summary.stealth_attempts, 0);
    }

    #[test]
    fn ranged_combat_resolves_without_melee_force() {
        let attacker = fighter(1, 4.0, true);
        let defender = fighter(2, 1.0, false);
        let result = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            1.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );
        assert!(health_damage_from_attack(result, BodyPart::Chest) > 0.0);
    }

    #[test]
    fn ranged_blocking_works_with_a_weapon_and_no_shield() {
        let attacker = fighter(1, 4.0, true);
        let defender = fighter(2, 3.0, false);
        let undefended = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );
        let weapon_block = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::Block { effectiveness: 1.0 },
        );
        assert!(
            health_damage_from_attack(weapon_block, BodyPart::Chest)
                < health_damage_from_attack(undefended, BodyPart::Chest)
        );
    }

    #[test]
    fn melee_block_carries_contact_force_while_dodge_does_not() {
        let attacker = fighter(1, 0.1, false);
        let mut defender = fighter(2, 5.0, false);
        defender.equipment.shield_block_bonus = 5.0;

        let block = melee_exchange(
            &attacker,
            &defender,
            0.65,
            0.0,
            0.5,
            DefenderResponse::Block { effectiveness: 1.0 },
            0.5,
            0.5,
        )
        .result;
        assert!(matches!(
            block,
            AttackResult::ToAttacker {
                physical_contact: true,
                contact_force,
                ..
            } if contact_force > 0.0
        ));

        let dodge = melee_exchange(
            &attacker,
            &defender,
            0.65,
            0.0,
            0.5,
            DefenderResponse::Dodge { input_reflex: 1.0 },
            0.5,
            0.5,
        )
        .result;
        if let AttackResult::ToAttacker {
            physical_contact,
            contact_force,
            ..
        } = dodge
        {
            assert!(!physical_contact);
            assert_eq!(contact_force, 0.0);
        }
    }

    #[test]
    fn melee_exchange_uses_global_armor_coverage() {
        let attacker = fighter(1, 3.0, false);
        let mut defender = fighter(2, 1.0, false);
        defender.equipment.armor.fill(CombatArmor::default());
        defender.equipment.armor[body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 10_000.0,
            padding: 10_000.0,
            range_of_motion: 1.0,
            coverage: 1.0,
            ..CombatArmor::default()
        };

        let exchange = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            0.5,
            DefenderResponse::None,
            0.5,
            0.5,
        );
        let result = exchange.result;
        let part = exchange.contact.body_part;

        assert_eq!(part, BodyPart::Chest);
        assert!(matches!(
            result,
            AttackResult::ToDefender {
                armor_impact: Some(_),
                ..
            }
        ));
        assert_eq!(health_damage_from_attack(result, part), 0.0);
    }

    #[test]
    fn agility_does_not_add_to_block_defense_below_the_mastery_cap() {
        let low_agility = fighter(1, 3.0, false);
        let mut high_agility = low_agility.clone();
        high_agility.attributes.left_arm_agility = 5.0;
        high_agility.attributes.right_arm_agility = 5.0;

        let block = |combatant: &Combatant| {
            combatant.skills.skill_check_by_parts(
                Skill::Block,
                &combatant.attributes,
                &combatant.body,
                &combatant.essentials,
                &combatant.equipment,
                LimbWeights::all_equal(),
            )
        };
        assert_eq!(block(&low_agility), block(&high_agility));
    }

    #[test]
    fn defense_matrix_exercises_skill_fatigue_implement_timing_and_leverage() {
        let attacker = fighter(1, 3.0, false);
        let novice = fighter(2, 1.0, false);
        let expert = fighter(3, 4.0, false);
        let mut buckler_expert = expert.clone();
        buckler_expert.equipment.shield_block_bonus = 1.5;
        let attack_equipment = attacker.equipment.for_melee();
        let attack_value = |defender: &Combatant, response| {
            melee_attack_value_by_parts(
                &attacker.skills,
                &attacker.attributes,
                &attacker.body,
                &attacker.essentials,
                &attack_equipment,
                attacker.equipment.melee_holding_side,
                attack_equipment.weapon_preferred_melee_style(),
                1.0,
                0.0,
                response,
                &defender.skills,
                &defender.attributes,
                &defender.body,
                &defender.essentials,
                &defender.equipment,
            )
        };

        let fresh_weapon = attack_value(&expert, DefenderResponse::Block { effectiveness: 1.0 });
        let fatigued_weapon = attack_value(&expert, DefenderResponse::Block { effectiveness: 0.5 });
        let fresh_buckler = attack_value(
            &buckler_expert,
            DefenderResponse::Block { effectiveness: 1.0 },
        );
        let novice_weapon = attack_value(&novice, DefenderResponse::Block { effectiveness: 1.0 });
        let favorable_parry = attack_value(
            &expert,
            DefenderResponse::Parry {
                input_reflex: 1.0,
                precision: 1.0,
            },
        );
        let unfavorable_parry = attack_value(
            &expert,
            DefenderResponse::Parry {
                input_reflex: 0.35,
                precision: 0.35,
            },
        );

        assert!(fresh_buckler < fresh_weapon);
        assert!(fresh_weapon < fatigued_weapon);
        assert!(fresh_weapon < novice_weapon);
        assert!(favorable_parry < unfavorable_parry);
        assert!(fresh_buckler < 0.0 && novice_weapon > 0.0);
    }

    #[test]
    fn precise_ranged_criticals_bypass_armor() {
        let mut attacker = fighter(1, 5.0, true);
        attacker.skills.bow_hours = 100_000.0;
        let weapon = attacker.equipment.ranged_weapon.as_mut().unwrap();
        weapon.accuracy = 2.0;
        weapon.precise = true;

        let mut defender = fighter(2, 1.0, false);
        defender.equipment.armor.fill(CombatArmor {
            resistance: 10_000.0,
            padding: 10_000.0,
            flexibility: 0.0,
            range_of_motion: 1.0,
            coverage: 1.0,
            ..CombatArmor::default()
        });

        let critical = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );
        attacker.equipment.ranged_weapon.as_mut().unwrap().precise = false;
        let armored = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );

        assert!(health_damage_from_attack(critical, BodyPart::Chest) > 0.0);
        assert_eq!(health_damage_from_attack(armored, BodyPart::Chest), 0.0);
        assert!(
            matches!(armored, AttackResult::ToDefender { contact_force, armor_impact: Some(_), .. } if contact_force > 0.0)
        );
    }

    #[test]
    fn precise_melee_attacks_do_not_reclassify_an_intact_surface_as_a_gap() {
        let mut attacker = fighter(1, 5.0, false);
        attacker.skills.sword_hours = 100_000.0;
        let weapon = attacker.equipment.melee_weapon.as_mut().unwrap();
        weapon.accuracy = 2.0;
        weapon.precise = true;

        let mut defender = fighter(2, 1.0, false);
        defender.equipment.armor.fill(CombatArmor {
            resistance: 10_000.0,
            padding: 10_000.0,
            flexibility: 0.0,
            range_of_motion: 1.0,
            coverage: 1.0,
            ..CombatArmor::default()
        });

        let critical = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            0.5,
            DefenderResponse::None,
            0.5,
            0.5,
        );
        attacker.equipment.melee_weapon.as_mut().unwrap().precise = false;
        let armored = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            0.5,
            DefenderResponse::None,
            0.5,
            0.5,
        );

        assert_eq!(
            health_damage_from_attack(critical.result, critical.contact.body_part),
            0.0
        );
        assert_eq!(
            health_damage_from_attack(armored.result, armored.contact.body_part),
            0.0
        );
    }

    #[test]
    fn ranged_combatants_switch_to_their_melee_weapon_when_ammunition_runs_out() {
        let mut hybrid = fighter(1, 3.0, true);
        hybrid.equipment.melee_weapon = fighter(9, 3.0, false).equipment.melee_weapon;
        assert_eq!(preferred_attack_mode(&hybrid), AttackMode::Ranged);

        hybrid.equipment.ammunition = 0;
        assert_eq!(preferred_attack_mode(&hybrid), AttackMode::Melee);
    }

    #[test]
    fn battle_log_attributes_bow_and_sidearm_to_distinct_instances() {
        let result = AttackResult::ToDefender {
            cut_damage: 0.0,
            blunt_damage: 0.0,
            balance_damage: 0.1,
            contact_force: 40.0,
            armor_impact: None,
        };
        let effect = AttackEffect {
            hit: true,
            health_damage: 0.0,
        };
        let mut recorder = BattleRecorder::default();
        recorder.record_attack(
            "opening",
            0,
            1,
            2,
            AttackMode::Ranged,
            Some(101),
            Some(CombatProjectileKind::Arrowhead),
            None,
            "none",
            BodyPart::Chest,
            result,
            effect,
            None,
        );
        recorder.record_attack(
            "main",
            1,
            1,
            2,
            AttackMode::Melee,
            Some(202),
            None,
            None,
            "none",
            BodyPart::Chest,
            result,
            effect,
            None,
        );
        assert_eq!(recorder.log[0].weapon_inventory_item_id, Some(101));
        assert_eq!(recorder.log[1].weapon_inventory_item_id, Some(202));
        assert!(
            recorder
                .log
                .iter()
                .all(|entry| entry.contact_stress == 40.0)
        );
    }

    #[test]
    fn successful_block_records_wear_for_both_contacting_instances() {
        let result = AttackResult::ToAttacker {
            balance_damage: 0.2,
            contact_force: 55.0,
            physical_contact: true,
        };
        let defender = CombatEquipment {
            defense_item_id: Some(303),
            ..CombatEquipment::default()
        };
        let mut recorder = BattleRecorder::default();
        recorder.record_attack(
            "main",
            1,
            1,
            2,
            AttackMode::Melee,
            Some(202),
            None,
            defender_contact_item_id(result, &defender),
            "block",
            BodyPart::Chest,
            result,
            AttackEffect {
                hit: false,
                health_damage: 0.0,
            },
            None,
        );

        assert_eq!(recorder.log[0].weapon_inventory_item_id, Some(202));
        assert_eq!(recorder.log[0].defender_contact_item_id, Some(303));
        assert_eq!(recorder.log[0].contact_stress, 55.0);
        assert_eq!(recorder.log[0].outcome, "blocked");
        assert!(recorder.log[0].armor_impact.is_none());
    }

    #[test]
    fn melee_screen_forces_engagement_before_backline_access() {
        let attackers = vec![fighter(1, 3.0, false), fighter(2, 3.0, false)];
        let defenders = vec![fighter(3, 3.0, false), fighter(4, 3.0, true)];

        assert_eq!(
            melee_assignment(0, &attackers, &defenders, autoresolve_parameters()),
            (0, 0.0)
        );
        assert_eq!(
            melee_assignment(1, &attackers, &defenders, autoresolve_parameters()),
            (1, 0.0)
        );

        let mut surplus = attackers;
        surplus.push(fighter(5, 3.0, false));
        assert_eq!(
            melee_assignment(2, &surplus, &defenders, autoresolve_parameters()),
            (0, 0.5)
        );
    }

    #[test]
    fn formation_detour_grants_additional_opening_volleys() {
        let ranged_side = vec![
            fighter(1, 3.0, true),
            fighter(2, 3.0, false),
            fighter(3, 3.0, false),
            fighter(4, 3.0, false),
        ];
        let matched_closers = vec![
            fighter(5, 3.0, false),
            fighter(6, 3.0, false),
            fighter(7, 3.0, false),
        ];
        let mut surplus_closers = matched_closers.clone();
        surplus_closers.push(fighter(8, 3.0, false));

        let direct_plan =
            opening_volley_plans(&ranged_side, &matched_closers, autoresolve_parameters())[0];
        let detour_plan =
            opening_volley_plans(&ranged_side, &surplus_closers, autoresolve_parameters())[0];
        assert!(direct_plan.direct_attacks > 0);
        assert_eq!(direct_plan.direct_attacks, direct_plan.total_attacks);
        assert_eq!(detour_plan.direct_attacks, direct_plan.direct_attacks);
        assert!(detour_plan.total_attacks > detour_plan.direct_attacks);
    }

    #[test]
    fn ranged_characters_fire_during_the_enemy_approach() {
        let mut archer = fighter(1, 5.0, true);
        archer.skills.bow_hours = 100_000.0;
        archer.equipment.ranged_weapon.as_mut().unwrap().accuracy = 2.0;
        let mut allies = vec![archer];
        let mut enemies = vec![fighter(2, 1.0, false)];

        resolve_opening_volleys(
            &mut allies,
            &mut enemies,
            &mut SplitMix64::new(7),
            &mut BattleRecorder::default(),
            autoresolve_parameters(),
        );

        assert!(enemies[0].body.total_damage() > 0.0);
    }

    #[test]
    fn detour_volleys_only_target_surplus_melee() {
        let mut archer = fighter(1, 5.0, true);
        archer.skills.bow_hours = 100_000.0;
        archer.equipment.ranged_weapon.as_mut().unwrap().accuracy = 2.0;
        let screen = fighter(2, 3.0, false);
        let mut attackers = vec![archer, screen];
        let mut defenders = vec![fighter(3, 1.0, false), fighter(4, 1.0, false)];
        let plans = opening_volley_plans(&attackers, &defenders, autoresolve_parameters());
        let direct_attacks = plans[0].direct_attacks;

        take_opening_volley_step(
            &mut attackers,
            &plans,
            &mut defenders,
            &[1],
            direct_attacks,
            &mut SplitMix64::new(17),
            &mut BattleRecorder::default(),
            autoresolve_parameters(),
        );

        assert_eq!(defenders[0].body.total_damage(), 0.0);
        assert!(defenders[1].body.total_damage() > 0.0);
    }

    #[test]
    fn empty_opposition_is_an_immediate_victory() {
        let outcome = resolve_battle(
            vec![fighter(1, 3.0, false)],
            Vec::new(),
            1,
            BattleOpening::Normal,
        );
        assert_eq!(outcome.resolution, BattleResolution::AlliesVictory);
        assert_eq!(outcome.rounds, 0);
        assert_eq!(outcome.seed, 1);
    }

    #[test]
    fn ranged_attack_interval_controls_attack_count() {
        let mut fast = fighter(1, 3.0, true);
        fast.equipment
            .ranged_weapon
            .as_mut()
            .unwrap()
            .attack_interval_seconds = 0.5;
        let defenders = vec![fighter(2, 3.0, false)];
        let attacks = plan_ranged_round(
            &mut [fast],
            &defenders,
            1,
            &mut SplitMix64::new(3),
            autoresolve_parameters(),
        );
        assert_eq!(attacks.len(), 2);
    }

    #[test]
    fn movement_speed_changes_approach_fire_window() {
        let ranged = vec![fighter(1, 3.0, true)];
        let mut slow = fighter(2, 3.0, false);
        slow.attributes.left_leg_agility = 0.0;
        slow.attributes.right_leg_agility = 0.0;
        let fast = fighter(3, 5.0, false);

        let slow_attacks =
            opening_volley_plans(&ranged, &[slow], autoresolve_parameters())[0].total_attacks;
        let fast_attacks =
            opening_volley_plans(&ranged, &[fast], autoresolve_parameters())[0].total_attacks;
        assert!(slow_attacks > fast_attacks);
    }

    #[test]
    fn autoresolve_reaction_timing_matches_fixed_server_windup() {
        let reflex = autoresolve_melee_input_reflex(0.5, autoresolve_parameters());
        assert!((0.1..0.25).contains(&reflex));
    }

    #[test]
    fn autoresolve_dodge_choice_uses_authored_bot_chance_and_preserves_elapsed_time() {
        let defender = fighter(2, 3.0, false);
        let parameters = autoresolve_parameters();
        let incoming = MeleeAttackTiming {
            started_at_seconds: 0.0,
            contact_at_seconds: 0.65,
            recovery_until_seconds: 0.9,
        };
        let below = parameters.melee_dodge_reaction_chance - f32::EPSILON;
        let above = parameters.melee_dodge_reaction_chance + f32::EPSILON;
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                below,
                0.0,
                0.5,
                incoming,
                MeleeDefenderPhase::NeutralGuard,
                parameters
            )
            .response,
            DefenderResponse::Dodge { .. }
        ));
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                above,
                0.0,
                0.5,
                incoming,
                MeleeDefenderPhase::NeutralGuard,
                parameters
            )
            .response,
            DefenderResponse::Block { .. }
        ));
        let early = autoresolve_melee_reaction_timing(0.0, parameters);
        let late = autoresolve_melee_reaction_timing(1.0, parameters);
        assert!(early.displacement_time_seconds > late.displacement_time_seconds);
        assert!((early.displacement_time_seconds - 0.45).abs() < 1.0e-6);
        assert!((late.displacement_time_seconds - 0.38).abs() < 1.0e-6);
    }

    #[test]
    fn melee_defense_is_precommitted_and_not_unconditional() {
        let mut defender = fighter(2, 3.0, false);
        defender.equipment.shield_block_bonus = 2.0;
        let parameters = autoresolve_parameters();
        let incoming = MeleeAttackTiming {
            started_at_seconds: 0.0,
            contact_at_seconds: 0.65,
            recovery_until_seconds: 0.9,
        };

        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                0.5,
                0.5,
                incoming,
                MeleeDefenderPhase::NeutralGuard,
                parameters
            )
            .response,
            DefenderResponse::Block { effectiveness: 1.0 }
        ));
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                0.0,
                0.5,
                0.5,
                incoming,
                MeleeDefenderPhase::NeutralGuard,
                parameters
            )
            .response,
            DefenderResponse::Dodge { .. }
        ));
        let occupied = MeleeAttackTiming {
            started_at_seconds: -0.1,
            contact_at_seconds: 0.5,
            recovery_until_seconds: 0.8,
        };
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                0.5,
                0.5,
                incoming,
                MeleeDefenderPhase::CommittedAttack(occupied),
                parameters,
            )
            .response,
            DefenderResponse::None
        ));
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                0.5,
                0.5,
                incoming,
                MeleeDefenderPhase::OccupiedRecovery { until_seconds: 0.8 },
                parameters,
            ).response,
            DefenderResponse::Block { effectiveness }
                if effectiveness > 0.0 && effectiveness < 1.0
        ));
    }

    #[test]
    fn guard_access_recovers_continuously_before_incoming_contact() {
        let contact = 1.0;
        let windup = 0.65;
        let almost_recovered = recovery_guard_effectiveness(1.1, contact, windup);
        let deeply_committed = recovery_guard_effectiveness(1.6, contact, windup);
        let recovered = recovery_guard_effectiveness(contact, contact, windup);

        assert!(deeply_committed < almost_recovered);
        assert!(almost_recovered < recovered);
        assert_eq!(recovered, 1.0);
    }

    #[test]
    fn ranged_attackers_prioritize_ranged_targets() {
        let targets = vec![fighter(1, 3.0, false), fighter(2, 3.0, true)];
        assert_eq!(prioritized_ranged_targets(&targets), vec![1]);
    }

    #[test]
    fn battle_summary_and_log_are_reproducible() {
        let outcome = resolve_battle(
            vec![fighter(1, 4.0, false)],
            vec![fighter(2, 1.0, false)],
            27,
            BattleOpening::Normal,
        );
        assert_eq!(outcome.seed, 27);
        assert_eq!(outcome.summary.melee_attacks as usize, outcome.log.len());
        assert_eq!(outcome.log.first().map(|entry| entry.sequence), Some(0));
    }

    #[test]
    fn skill_and_numbers_change_battle_odds() {
        let strong_wins = (0..64)
            .filter(|seed| {
                resolve_battle(
                    vec![fighter(1, 4.0, false), fighter(2, 4.0, false)],
                    vec![fighter(3, 1.5, false)],
                    *seed,
                    BattleOpening::Normal,
                )
                .resolution
                    == BattleResolution::AlliesVictory
            })
            .count();
        let weak_wins = (0..64)
            .filter(|seed| {
                resolve_battle(
                    vec![fighter(1, 1.5, false)],
                    vec![fighter(2, 4.0, false), fighter(3, 4.0, false)],
                    *seed,
                    BattleOpening::Normal,
                )
                .resolution
                    == BattleResolution::AlliesVictory
            })
            .count();
        assert!(strong_wins > weak_wins, "{strong_wins} versus {weak_wins}");
    }

    #[test]
    fn skeleton_matchup_emerges_from_resistance_and_padding_resolution() {
        let innate = profile(ThreatId::Skeleton).combat.innate_protection;
        let protection = CombatArmor::innate(innate.resistance_joules, innate.padding_joules);
        let cutting = resolved_melee_health_damage(
            CombatWeapon {
                slash: true,
                // Hand axe and sword catalog coefficient.
                penetration: 1.0,
                ..Default::default()
            },
            protection,
        );
        let blunt = resolved_melee_health_damage(
            CombatWeapon {
                blunt: true,
                // Flanged mace and war hammer catalog coefficient.
                penetration: 0.5,
                ..Default::default()
            },
            protection,
        );

        assert!(blunt > cutting, "blunt {blunt} versus cutting {cutting}");
    }

    #[test]
    fn ordinary_unprotected_damage_remains_coherent() {
        let unprotected_cut = resolved_melee_health_damage(
            CombatWeapon {
                slash: true,
                penetration: 1.0,
                ..Default::default()
            },
            CombatArmor::default(),
        );
        let unprotected_blunt = resolved_melee_health_damage(
            CombatWeapon {
                blunt: true,
                penetration: 0.5,
                ..Default::default()
            },
            CombatArmor::default(),
        );
        assert!(unprotected_cut > unprotected_blunt);
        assert!(unprotected_blunt > 0.0);
    }

    #[test]
    fn committed_parry_discards_prepared_strike_and_costs_work() {
        let mut defender = fighter(2, 3.0, false);
        let fatigue_before = defender.local_action_fatigue;
        let commitment = commit_defensive_action(
            &mut defender,
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 0.8,
            },
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 0.8,
            },
            MeleeDefenderPhase::CommittedAttack(MeleeAttackTiming {
                started_at_seconds: 0.0,
                contact_at_seconds: 0.65,
                recovery_until_seconds: 0.9,
            }),
        );
        defender.charge_action_work(CombatActionWork::WeaponDefense, 0.5);
        assert_eq!(
            commitment.kind,
            MeleeDefenseCommitmentKind::CanceledSameWeapon
        );
        assert!(defender.local_action_fatigue > fatigue_before);
    }

    #[test]
    fn scheduled_committed_attack_cancels_even_after_readiness_was_reserved() {
        let mut defender = fighter(2, 3.0, false);
        let commitment = commit_defensive_action(
            &mut defender,
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 0.8,
            },
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 0.8,
            },
            MeleeDefenderPhase::CommittedAttack(MeleeAttackTiming {
                started_at_seconds: 0.0,
                contact_at_seconds: 0.65,
                recovery_until_seconds: 0.9,
            }),
        );
        assert_eq!(
            commitment.kind,
            MeleeDefenseCommitmentKind::CanceledSameWeapon
        );
    }

    #[test]
    fn canceled_scheduled_attack_cannot_emit_a_ghost_contact() {
        let mut attacker = fighter(2, 3.0, false);
        let timing = MeleeAttackTiming {
            started_at_seconds: 1.0,
            contact_at_seconds: 1.65,
            recovery_until_seconds: 1.9,
        };
        attacker.melee_attack_started_at_seconds = Some(timing.started_at_seconds);
        attacker.melee_attack_contact_at_seconds = Some(timing.contact_at_seconds);
        assert!(scheduled_attack_is_current(&attacker, timing));
        attacker.melee_attack_started_at_seconds = None;
        attacker.melee_attack_contact_at_seconds = None;
        assert!(!scheduled_attack_is_current(&attacker, timing));
    }

    #[test]
    fn reciprocal_intercept_requires_an_overlapping_committed_attack_phase() {
        let defender = fighter(2, 3.0, false);
        let parameters = autoresolve_parameters();
        let incoming = MeleeAttackTiming {
            started_at_seconds: 1.0,
            contact_at_seconds: 1.65,
            recovery_until_seconds: 1.9,
        };
        let later = MeleeAttackTiming {
            started_at_seconds: 1.2,
            contact_at_seconds: 1.85,
            recovery_until_seconds: 2.1,
        };
        assert!(matches!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                0.5,
                1.0,
                incoming,
                MeleeDefenderPhase::CommittedAttack(later),
                parameters,
            ).response,
            DefenderResponse::Parry { precision, .. }
                if precision == parameters.maximum_hit_precision
        ));
        let earlier = MeleeAttackTiming {
            started_at_seconds: 0.9,
            contact_at_seconds: 1.55,
            recovery_until_seconds: 1.8,
        };
        assert_eq!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                0.5,
                0.5,
                incoming,
                MeleeDefenderPhase::CommittedAttack(earlier),
                parameters,
            )
            .response,
            DefenderResponse::None
        );
        let too_late = MeleeAttackTiming {
            started_at_seconds: 1.7,
            contact_at_seconds: 2.35,
            recovery_until_seconds: 2.6,
        };
        assert_eq!(
            autoresolve_melee_defender_response(
                &defender,
                1.0,
                1.0,
                0.5,
                incoming,
                MeleeDefenderPhase::CommittedAttack(too_late),
                parameters,
            )
            .response,
            DefenderResponse::Block { effectiveness: 1.0 }
        );
    }

    #[test]
    fn offhand_shield_preserves_prepared_strike_but_reduces_its_power() {
        let mut defender = fighter(2, 3.0, false);
        defender.equipment.shield_block_bonus = 1.5;
        commit_defensive_action(
            &mut defender,
            DefenderResponse::Block { effectiveness: 0.8 },
            DefenderResponse::Block { effectiveness: 0.8 },
            MeleeDefenderPhase::CommittedAttack(MeleeAttackTiming {
                started_at_seconds: 0.0,
                contact_at_seconds: 0.65,
                recovery_until_seconds: 0.9,
            }),
        );
        assert!((defender.melee_attack_power_multiplier - 0.68).abs() < 1.0e-6);
    }

    #[test]
    fn neutral_guard_does_not_cancel_an_attack_that_has_not_started() {
        let mut defender = fighter(2, 3.0, false);
        let commitment = commit_defensive_action(
            &mut defender,
            DefenderResponse::Block { effectiveness: 0.8 },
            DefenderResponse::Block { effectiveness: 0.8 },
            MeleeDefenderPhase::NeutralGuard,
        );
        assert_eq!(
            commitment.kind,
            MeleeDefenseCommitmentKind::NeutralGuardRecovery
        );
    }

    #[test]
    fn reading_defender_phase_never_mutates_the_scheduled_attack() {
        let mut defender = fighter(2, 3.0, false);
        defender.melee_engagement_target = Some(1);
        defender.melee_attack_started_at_seconds = Some(1.3);
        defender.melee_attack_contact_at_seconds = Some(1.95);
        defender.melee_recovery_until_seconds = 2.2;
        let phase = defender_phase_at_contact(
            &defender,
            1,
            MeleeAttackTiming {
                started_at_seconds: 1.0,
                contact_at_seconds: 1.65,
                recovery_until_seconds: 1.9,
            },
        );
        assert!(matches!(phase, MeleeDefenderPhase::CommittedAttack(_)));
        assert_eq!(defender.melee_attack_started_at_seconds, Some(1.3));
        assert_eq!(defender.melee_attack_contact_at_seconds, Some(1.95));
        assert_eq!(defender.melee_recovery_until_seconds, 2.2);
    }

    #[test]
    fn phase_adaptation_is_one_shot_elapsed_readiness_not_duplicate_attack_work() {
        let mut attacker = fighter(1, 3.0, false);
        let mut defender = fighter(2, 3.0, false);
        attacker.melee_engagement_target = Some(defender.id);
        defender.melee_engagement_target = Some(attacker.id);
        attacker.melee_engagement_distance_metres = 0.5;
        defender.melee_engagement_distance_metres = 0.5;
        attacker.melee_recovery_until_seconds = 1.0;
        attacker.melee_phase_adaptation_delay_seconds = 0.2;
        let mut attackers = vec![attacker];
        let mut defenders = vec![defender];
        let mut recorder = BattleRecorder::default();
        let scheduled = schedule_side_melee_attacks_in_window(
            &mut attackers,
            &mut defenders,
            0.0,
            2.0,
            &mut SplitMix64::new(11),
            &mut recorder,
            autoresolve_parameters(),
        );

        assert_eq!(scheduled.len(), 1);
        assert!((scheduled[0].attack_timing.started_at_seconds - 1.2).abs() < 1.0e-6);
        assert_eq!(attackers[0].melee_phase_adaptation_delay_seconds, 0.0);
        assert_eq!(attackers[0].active_work_seconds, 0.0);
        let start = recorder
            .timeline
            .iter()
            .find(|event| event.kind == MeleeTimelineKind::AttackStarted)
            .expect("attack start is recorded");
        assert_eq!(start.phase_adaptation_delay_seconds, Some(0.2));
    }

    #[test]
    fn melee_only_fighter_with_disabled_weapon_arm_yields() {
        let mut disabled = fighter(2, 3.0, false);
        disabled.body.health[body_part_index(BodyPart::RightArm)] = 0.0;
        let mut attackers = vec![disabled];
        let mut defenders = vec![fighter(1, 3.0, false)];
        take_side_turns(
            &mut attackers,
            &mut defenders,
            1,
            &mut SplitMix64::new(7),
            &mut BattleRecorder::default(),
            autoresolve_parameters(),
        );
        assert!(attackers[0].yielded);
        assert!(side_defeated(&attackers));
    }

    #[test]
    fn battle_resolution_distinguishes_mutual_incapacitation_from_timeout() {
        assert_eq!(
            classify_battle_resolution(true, true, false),
            Some(BattleResolution::MutualIncapacitation)
        );
        assert_eq!(classify_battle_resolution(false, false, false), None);
        assert_eq!(
            classify_battle_resolution(false, false, true),
            Some(BattleResolution::Timeout)
        );
    }
}
