//! Framework-neutral, deterministic combat simulation for strategic autoresolve.

use crate::prelude::*;

const MAX_COMBAT_ROUNDS: usize = 256;
const MAX_RANGED_ATTACKS_PER_PHASE: usize = 64;
const BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;
const FORMATION_SPACING_METERS: f32 = 2.0;
const CONTESTED_CHECK_RANDOM_RANGE: f32 = 5.0;
const COMBAT_ROUND_SECONDS: f32 = 1.0;
const REFERENCE_MELEE_ATTACK_SECONDS: f32 = 1.0;
const MIN_MOVEMENT_SPEED_METERS_PER_SECOND: f32 = 0.25;

#[derive(Clone, Debug, Default)]
pub struct CombatAttributes {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

impl PlayerAttributes for CombatAttributes {
    fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32 {
        match (attr, limb) {
            (LimbAttribute::Strength, BodyPart::LeftArm) => self.left_arm_strength,
            (LimbAttribute::Strength, BodyPart::RightArm) => self.right_arm_strength,
            (LimbAttribute::Strength, BodyPart::LeftLeg) => self.left_leg_strength,
            (LimbAttribute::Strength, BodyPart::RightLeg) => self.right_leg_strength,
            (LimbAttribute::Agility, BodyPart::LeftArm) => self.left_arm_agility,
            (LimbAttribute::Agility, BodyPart::RightArm) => self.right_arm_agility,
            (LimbAttribute::Agility, BodyPart::LeftLeg) => self.left_leg_agility,
            (LimbAttribute::Agility, BodyPart::RightLeg) => self.right_leg_agility,
            _ => 0.0,
        }
    }

    fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32 {
        match attr {
            SimpleAttribute::Endurance => self.endurance,
            SimpleAttribute::Immunity => self.immunity,
            SimpleAttribute::Gut => self.gut,
            SimpleAttribute::Intelligence => self.intelligence,
            SimpleAttribute::Instinct => self.instinct,
            SimpleAttribute::Eyesight => self.eyesight,
            SimpleAttribute::Hearing => self.hearing,
        }
    }

    fn raw_precision(&self) -> f32 {
        self.precision
    }

    fn has_dedicated_precision(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
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
        let applied = damage.max(0.0).min(health.max(0.0));
        *health = (*health - applied).max(0.0);
        applied
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
    pub self_awareness_hours: f32,
    pub humor_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub seduction_hours: f32,
    pub medicine_hours: f32,
    pub religion_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub anatomy_hours: f32,
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
            Skill::SelfAwareness => self.self_awareness_hours,
            Skill::Humor => self.humor_hours,
            Skill::Command => self.command_hours,
            Skill::Deception => self.deception_hours,
            Skill::Seduction => self.seduction_hours,
            Skill::Medicine => self.medicine_hours,
            Skill::Cooking => 0.0,
            Skill::Religion => self.religion_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains
            | Skill::TerrainForest
            | Skill::TerrainHills
            | Skill::TerrainUrban => 0.0,
            Skill::Anatomy => self.anatomy_hours,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CombatArmor {
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
    pub coverage: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CombatWeapon {
    pub skills: crate::equipment::WeaponSkillDistribution,
    pub melee: bool,
    pub ranged: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub accuracy: f32,
    pub weight: f32,
    pub penetration: f32,
    pub melee_reach: f32,
    pub ranged_range: f32,
    pub attack_interval_seconds: f32,
    pub precise: bool,
    pub balance: f32,
    pub ranged_force_joules: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatProjectileKind {
    Arrowhead,
    Ball,
}

#[derive(Clone, Debug)]
pub struct CombatEquipment {
    /// Weapon selected for a particular pure attack calculation.
    pub weapon: Option<CombatWeapon>,
    pub melee_weapon: Option<CombatWeapon>,
    pub ranged_weapon: Option<CombatWeapon>,
    pub melee_weapon_id: Option<u64>,
    pub ranged_weapon_id: Option<u64>,
    pub ranged_projectile_kind: Option<CombatProjectileKind>,
    /// Shield instance used for blocks, falling back to the melee weapon used to parry.
    pub defense_item_id: Option<u64>,
    pub ammunition: u32,
    pub holding_side: BodySide,
    pub melee_holding_side: BodySide,
    pub ranged_holding_side: BodySide,
    pub shield_block_bonus: f32,
    pub armor: [CombatArmor; 7],
    pub inventory_weight: f32,
}

impl Default for CombatEquipment {
    fn default() -> Self {
        Self {
            weapon: None,
            melee_weapon: None,
            ranged_weapon: None,
            melee_weapon_id: None,
            ranged_weapon_id: None,
            ranged_projectile_kind: None,
            defense_item_id: None,
            ammunition: 0,
            holding_side: BodySide::Right,
            melee_holding_side: BodySide::Right,
            ranged_holding_side: BodySide::Right,
            shield_block_bonus: 0.0,
            armor: [CombatArmor {
                range_of_motion: 1.0,
                flexibility: 1.0,
                ..CombatArmor::default()
            }; 7],
            inventory_weight: 0.0,
        }
    }
}

impl CombatEquipment {
    fn for_melee(&self) -> Self {
        let mut equipment = self.clone();
        equipment.weapon = self.melee_weapon;
        equipment.holding_side = self.melee_holding_side;
        equipment
    }

    fn for_ranged(&self) -> Self {
        let mut equipment = self.clone();
        equipment.weapon = self.ranged_weapon;
        equipment.holding_side = self.ranged_holding_side;
        equipment
    }
}

impl PlayerEquipment for CombatEquipment {
    fn weapon_skill_distribution(&self) -> crate::equipment::WeaponSkillDistribution {
        self.weapon
            .map_or_else(Default::default, |weapon| weapon.skills)
    }
    fn weapon_is_melee(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.melee)
    }
    fn weapon_is_ranged(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.ranged)
    }
    fn weapon_does_blunt(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.blunt)
    }
    fn weapon_does_slash(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.slash)
    }
    fn weapon_does_pierce(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.pierce)
    }
    fn weapon_accuracy(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.accuracy)
    }
    fn weapon_weight(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.weight)
    }
    fn weapon_penetration(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.penetration)
    }
    fn weapon_reach(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.melee_reach)
    }
    fn weapon_holding_side(&self) -> Option<BodySide> {
        self.weapon.map(|_| self.holding_side)
    }
    fn weapon_is_precise(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.precise)
    }
    fn weapon_balance(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.balance)
    }
    fn weapon_ranged_force_joules(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.ranged_force_joules)
    }
    fn shield_block_bonus(&self) -> f32 {
        self.shield_block_bonus
    }
    fn armor_resistance(&self, part: BodyPart) -> f32 {
        self.armor[body_part_index(part)].resistance
    }
    fn armor_padding(&self, part: BodyPart) -> f32 {
        self.armor[body_part_index(part)].padding
    }
    fn armor_flexibility(&self, part: BodyPart) -> f32 {
        self.armor[body_part_index(part)].flexibility
    }
    fn armor_range_of_motion(&self, part: BodyPart) -> f32 {
        self.armor[body_part_index(part)].range_of_motion
    }
    fn armor_coverage(&self, part: BodyPart) -> f32 {
        self.armor[body_part_index(part)].coverage
    }
    fn inventory_weight(&self) -> f32 {
        self.inventory_weight
    }
}

#[derive(Clone, Debug)]
pub struct Combatant {
    pub id: u64,
    pub attributes: CombatAttributes,
    pub body: CombatBody,
    pub essentials: CombatEssentials,
    pub equipment: CombatEquipment,
    pub skills: CombatSkills,
    /// Incapacitation from strategic factors not recomputed inside the battle,
    /// such as fear, hunger, and thirst.
    pub starting_incapacitation: f32,
    pub starting_blood_fraction: f32,
    #[doc(hidden)]
    pub imbalance: f32,
    #[doc(hidden)]
    pub blood_loss_fraction: f32,
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
            attributes: CombatAttributes::default(),
            body: CombatBody::default(),
            essentials: CombatEssentials {
                focus_level: 1.0,
                ..CombatEssentials::default()
            },
            equipment: CombatEquipment::default(),
            skills: CombatSkills::default(),
            starting_incapacitation: 0.0,
            starting_blood_fraction: 1.0,
            imbalance: 0.0,
            blood_loss_fraction: 0.0,
            cut_damage: 0.0,
            initial_ammunition: 0,
            ranged_attack_progress: 0.0,
        }
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
        let pain = pain_incapacitation(self.body.total_damage(), will);
        let remaining_blood =
            (self.starting_blood_fraction - self.blood_loss_fraction).clamp(0.0, 1.0);
        let blood_loss = blood_loss_incapacitation(remaining_blood, 1.0);
        self.starting_incapacitation + pain + blood_loss + self.imbalance
    }

    pub fn is_incapacitated(&self) -> bool {
        self.incapacitation() >= 1.0
    }

    fn recover_balance(&mut self) {
        let balance = self.skills.skill_check_by_parts(
            Skill::Balance,
            &self.attributes,
            &self.body,
            &self.essentials,
            &self.equipment,
            LimbWeights::both_legs(),
        );
        self.imbalance = (self.imbalance - 0.03 * balance.max(0.25)).max(0.0);
    }

    fn can_attack_ranged(&self) -> bool {
        self.equipment.ranged_weapon.is_some() && self.equipment.ammunition > 0
    }

    fn can_attack_melee(&self) -> bool {
        self.equipment.melee_weapon.is_some()
    }

    fn movement_speed_meters_per_second(&self) -> f32 {
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
        ((1.0 + leg_agility) * armor * encumbrance * fatigue)
            .max(MIN_MOVEMENT_SPEED_METERS_PER_SECOND)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleVictor {
    Allies,
    Enemies,
    Stalemate,
}

#[derive(Clone, Debug)]
pub struct CombatantOutcome {
    pub id: u64,
    pub body: CombatBody,
    pub blood_loss_fraction: f32,
    pub cut_damage: f32,
    pub incapacitated: bool,
    pub ammunition_used: u32,
}

#[derive(Clone, Debug, Default)]
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

#[derive(Clone, Debug)]
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
    /// Exact shield or parrying weapon contacted on a successful defense.
    pub defender_contact_item_id: Option<u64>,
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
    pub armor_contact: bool,
}

#[derive(Clone, Debug)]
pub struct BattleOutcome {
    pub seed: u64,
    pub victor: BattleVictor,
    pub rounds: usize,
    pub allies: Vec<CombatantOutcome>,
    pub enemies: Vec<CombatantOutcome>,
    pub summary: BattleSummary,
    pub log: Vec<BattleLogEntry>,
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
}

#[derive(Clone, Copy)]
struct AttackEffect {
    hit: bool,
    health_damage: f32,
}

impl BattleRecorder {
    #[allow(clippy::too_many_arguments)]
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
        part: BodyPart,
        result: AttackResult,
        effect: AttackEffect,
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
            armor_contact: matches!(
                result,
                AttackResult::ToDefender {
                    armor_contact: true,
                    ..
                }
            ),
        });
    }
}

fn defender_contact_item_id(result: AttackResult, equipment: &CombatEquipment) -> Option<u64> {
    matches!(
        result,
        AttackResult::ToAttacker {
            physical_contact: true,
            ..
        }
    )
    .then_some(equipment.defense_item_id)
    .flatten()
}

/// Resolve an abstract battle using the same attack calculations as direct
/// combat. The supplied seed makes the result reproducible and the hard round
/// cap keeps reducer execution bounded.
pub fn resolve_battle(
    mut allies: Vec<Combatant>,
    mut enemies: Vec<Combatant>,
    seed: u64,
) -> BattleOutcome {
    let mut random = SplitMix64::new(seed);
    let mut recorder = BattleRecorder::default();
    let mut victor = BattleVictor::Stalemate;
    let mut rounds = 0;

    resolve_stealth_openers(&mut allies, &mut enemies, &mut random, &mut recorder);
    resolve_opening_volleys(&mut allies, &mut enemies, &mut random, &mut recorder);

    for round in 0..MAX_COMBAT_ROUNDS {
        match (side_defeated(&allies), side_defeated(&enemies)) {
            (true, true) => break,
            (true, false) => {
                victor = BattleVictor::Enemies;
                break;
            }
            (false, true) => {
                victor = BattleVictor::Allies;
                break;
            }
            (false, false) => {}
        }
        rounds = round + 1;

        resolve_ranged_round(
            &mut allies,
            &mut enemies,
            round + 1,
            &mut random,
            &mut recorder,
        );

        if random.next_u64().is_multiple_of(2) {
            take_side_turns(
                &mut allies,
                &mut enemies,
                round + 1,
                &mut random,
                &mut recorder,
            );
            take_side_turns(
                &mut enemies,
                &mut allies,
                round + 1,
                &mut random,
                &mut recorder,
            );
        } else {
            take_side_turns(
                &mut enemies,
                &mut allies,
                round + 1,
                &mut random,
                &mut recorder,
            );
            take_side_turns(
                &mut allies,
                &mut enemies,
                round + 1,
                &mut random,
                &mut recorder,
            );
        }
        allies
            .iter_mut()
            .chain(&mut enemies)
            .for_each(Combatant::recover_balance);
    }

    if victor == BattleVictor::Stalemate {
        victor = match (side_defeated(&allies), side_defeated(&enemies)) {
            (true, false) => BattleVictor::Enemies,
            (false, true) => BattleVictor::Allies,
            _ => BattleVictor::Stalemate,
        };
    }

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
        victor,
        rounds,
        allies: allies.into_iter().map(outcome).collect(),
        enemies: enemies.into_iter().map(outcome).collect(),
        summary: recorder.summary,
        log: recorder.log,
    }
}

fn resolve_stealth_openers(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) {
    let allies_first = random.next_u64().is_multiple_of(2);
    let (ally_attacks, enemy_attacks) = if allies_first {
        (
            plan_stealth_openers(allies, enemies, random, recorder),
            plan_stealth_openers(enemies, allies, random, recorder),
        )
    } else {
        let enemy_attacks = plan_stealth_openers(enemies, allies, random, recorder);
        let ally_attacks = plan_stealth_openers(allies, enemies, random, recorder);
        (ally_attacks, enemy_attacks)
    };
    if allies_first {
        apply_pending_attacks(allies, enemies, &ally_attacks, recorder);
        apply_pending_attacks(enemies, allies, &enemy_attacks, recorder);
    } else {
        apply_pending_attacks(enemies, allies, &enemy_attacks, recorder);
        apply_pending_attacks(allies, enemies, &ally_attacks, recorder);
    }
}

fn plan_stealth_openers(
    attackers: &[Combatant],
    defenders: &[Combatant],
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) -> Vec<PendingAttack> {
    let mut attacks = Vec::new();
    for (attacker_index, attacker) in attackers.iter().enumerate() {
        if attacker.is_incapacitated() || preferred_attack_mode(attacker) != AttackMode::Melee {
            continue;
        }
        recorder.summary.stealth_attempts += 1;
        let targets = active_indices(defenders);
        if targets.is_empty() {
            break;
        }
        let target_index = targets[random.index(targets.len())];
        let stealth = attacker.skills.skill_check_by_parts(
            Skill::Stealth,
            &attacker.attributes,
            &attacker.body,
            &attacker.essentials,
            &attacker.equipment,
            LimbWeights::all_equal(),
        );
        let awareness = defender_awareness(&defenders[target_index]);
        let stealth_roll = stealth + random.unit_f32() * CONTESTED_CHECK_RANDOM_RANGE;
        let awareness_roll = awareness + random.unit_f32() * CONTESTED_CHECK_RANDOM_RANGE;
        if stealth_roll <= awareness_roll {
            continue;
        }
        recorder.summary.stealth_successes += 1;

        let part = random_body_part(random);
        let result = melee_exchange(
            attacker,
            &defenders[target_index],
            1.0,
            1.0,
            part,
            DefenderResponse::None,
        );
        attacks.push(PendingAttack {
            attacker_index,
            target_index,
            result,
            part,
            mode: AttackMode::Melee,
            phase: "stealth",
            round: 0,
        });
    }
    attacks
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
            attack.part,
            attack.result,
            effect,
        );
    }
}

fn defender_awareness(defender: &Combatant) -> f32 {
    let eyesight = defender
        .attributes
        .attr_by_parts(SimpleAttribute::Eyesight, &defender.body);
    let hearing = defender
        .attributes
        .attr_by_parts(SimpleAttribute::Hearing, &defender.body);
    (eyesight + hearing) * 0.5
}

fn resolve_opening_volleys(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) {
    let ally_plans = opening_volley_plans(allies, enemies);
    let enemy_plans = opening_volley_plans(enemies, allies);
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
            );
            take_opening_volley_step(
                enemies,
                &enemy_plans,
                allies,
                &enemy_detour_targets,
                step,
                random,
                recorder,
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
            );
            take_opening_volley_step(
                allies,
                &ally_plans,
                enemies,
                &ally_detour_targets,
                step,
                random,
                recorder,
            );
        }
    }
}

fn opening_volley_plans(
    ranged_side: &[Combatant],
    closing_side: &[Combatant],
) -> Vec<OpeningVolleyPlan> {
    let screen_count = active_melee_indices(ranged_side).len();
    let closing_melee = active_melee_indices(closing_side);
    let closing_melee_count = closing_melee.len();
    let direct_closing_speed = closing_melee
        .iter()
        .map(|index| closing_side[*index].movement_speed_meters_per_second())
        .fold(MIN_MOVEMENT_SPEED_METERS_PER_SECOND, f32::max);
    ranged_side
        .iter()
        .map(|attacker| {
            if attacker.is_incapacitated()
                || preferred_attack_mode(attacker) != AttackMode::Ranged
                || closing_melee_count == 0
            {
                return OpeningVolleyPlan::default();
            }

            let weapon = attacker.equipment.ranged_weapon.unwrap();
            let interval = weapon.attack_interval_seconds.max(0.1);
            let range = weapon.ranged_range.max(0.0);
            let direct_seconds = range / direct_closing_speed;
            let direct_attacks = (direct_seconds / interval)
                .ceil()
                .clamp(0.0, MAX_RANGED_ATTACKS_PER_PHASE as f32)
                as usize;
            let detour = if closing_melee_count > screen_count && screen_count > 0 {
                let formation_radius = screen_count as f32 * FORMATION_SPACING_METERS * 0.5;
                std::f32::consts::PI * formation_radius
            } else {
                0.0
            };
            let surplus_speed = closing_melee
                .iter()
                .skip(screen_count)
                .map(|index| closing_side[*index].movement_speed_meters_per_second())
                .fold(direct_closing_speed, f32::max);
            let total_seconds = direct_seconds + detour / surplus_speed;
            OpeningVolleyPlan {
                direct_attacks,
                total_attacks: (total_seconds / interval)
                    .ceil()
                    .clamp(0.0, MAX_RANGED_ATTACKS_PER_PHASE as f32)
                    as usize,
            }
        })
        .collect()
}

fn take_opening_volley_step(
    attackers: &mut [Combatant],
    plans: &[OpeningVolleyPlan],
    defenders: &mut [Combatant],
    detour_targets: &[usize],
    step: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) {
    for attacker_index in 0..attackers.len() {
        let plan = plans[attacker_index];
        if plan.total_attacks <= step
            || attackers[attacker_index].is_incapacitated()
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
                    .filter(|index| !defenders[*index].is_incapacitated())
                    .collect()
            } else {
                ranged
            }
        };
        if targets.is_empty() {
            break;
        }
        let target_index = targets[random.index(targets.len())];
        let part = random_body_part(random);
        let result = optimal_ranged_exchange(
            &attackers[attacker_index],
            &defenders[target_index],
            0.65 + random.unit_f32() * 0.35,
            part,
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
            part,
            result,
            effect,
        );
    }
}

fn resolve_ranged_round(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) {
    let ally_attacks = plan_ranged_round(allies, enemies, round, random);
    let enemy_attacks = plan_ranged_round(enemies, allies, round, random);
    apply_pending_attacks(allies, enemies, &ally_attacks, recorder);
    apply_pending_attacks(enemies, allies, &enemy_attacks, recorder);
}

fn plan_ranged_round(
    attackers: &mut [Combatant],
    defenders: &[Combatant],
    round: usize,
    random: &mut SplitMix64,
) -> Vec<PendingAttack> {
    let mut attacks = Vec::new();
    for (attacker_index, attacker) in attackers.iter_mut().enumerate() {
        if attacker.is_incapacitated() || !attacker.can_attack_ranged() {
            continue;
        }
        let interval = attacker
            .equipment
            .ranged_weapon
            .unwrap()
            .attack_interval_seconds
            .max(0.1);
        attacker.ranged_attack_progress += COMBAT_ROUND_SECONDS / interval;
        let attack_count =
            (attacker.ranged_attack_progress.floor() as u32).min(attacker.equipment.ammunition);
        attacker.ranged_attack_progress -= attack_count as f32;

        for _ in 0..attack_count {
            let targets = prioritized_ranged_targets(defenders);
            if targets.is_empty() {
                break;
            }
            let target_index = targets[random.index(targets.len())];
            let part = random_body_part(random);
            let result = optimal_ranged_exchange(
                attacker,
                &defenders[target_index],
                0.65 + random.unit_f32() * 0.35,
                part,
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

fn take_side_turns(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
) {
    for attacker_index in 0..attackers.len() {
        if attackers[attacker_index].is_incapacitated() || side_defeated(defenders) {
            continue;
        }
        let mode = preferred_attack_mode(&attackers[attacker_index]);
        if mode != AttackMode::Melee || !attackers[attacker_index].can_attack_melee() {
            continue;
        }
        let (target_index, flanking) = melee_assignment(attacker_index, attackers, defenders);
        let part = random_body_part(random);
        let hit_precision = 0.65 + random.unit_f32() * 0.35;
        let result = optimal_melee_exchange(
            &attackers[attacker_index],
            &defenders[target_index],
            hit_precision,
            flanking,
            part,
        );
        let effect = apply_attack_result(
            &mut attackers[attacker_index],
            &mut defenders[target_index],
            result,
            part,
        );
        recorder.record_attack(
            "main",
            round,
            attackers[attacker_index].id,
            defenders[target_index].id,
            AttackMode::Melee,
            attackers[attacker_index].equipment.melee_weapon_id,
            None,
            defender_contact_item_id(result, &defenders[target_index].equipment),
            part,
            result,
            effect,
        );
    }
}

fn melee_assignment(
    attacker_index: usize,
    attackers: &[Combatant],
    defenders: &[Combatant],
) -> (usize, f32) {
    let mut ordered_defenders = active_melee_indices(defenders);
    ordered_defenders.extend(active_ranged_indices(defenders));
    for index in active_indices(defenders) {
        if !ordered_defenders.contains(&index) {
            ordered_defenders.push(index);
        }
    }
    debug_assert!(!ordered_defenders.is_empty());
    let melee_rank = attackers[..=attacker_index]
        .iter()
        .filter(|combatant| {
            !combatant.is_incapacitated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee
        })
        .count()
        .saturating_sub(1);
    let target = ordered_defenders[melee_rank % ordered_defenders.len()];
    let flanking = if melee_rank >= ordered_defenders.len() {
        0.5
    } else {
        0.0
    };
    (target, flanking)
}

fn active_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| (!combatant.is_incapacitated()).then_some(index))
        .collect()
}

fn active_melee_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_incapacitated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee)
                .then_some(index)
        })
        .collect()
}

fn active_ranged_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_incapacitated() && combatant.can_attack_ranged()).then_some(index)
        })
        .collect()
}

fn prioritized_ranged_targets(side: &[Combatant]) -> Vec<usize> {
    let ranged = active_ranged_indices(side);
    if ranged.is_empty() {
        active_indices(side)
    } else {
        ranged
    }
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
        AttackResult::ToDefender { balance_damage, .. } => {
            defender.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, part);
            let applied = defender.body.apply_damage(part, damage);
            let raw_cut = match result {
                AttackResult::ToDefender { cut_damage, .. } => cut_damage.max(0.0),
                _ => 0.0,
            };
            let raw_total = match result {
                AttackResult::ToDefender {
                    cut_damage,
                    blunt_damage,
                    ..
                } => (cut_damage + blunt_damage).max(0.0),
                _ => 0.0,
            };
            defender.cut_damage += if raw_total > 0.0 {
                applied * raw_cut / raw_total
            } else {
                0.0
            };
            defender.blood_loss_fraction += applied * BLOOD_LOSS_PER_HEALTH_DAMAGE;
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

fn melee_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    part: BodyPart,
    response: DefenderResponse,
) -> AttackResult {
    let attacker_equipment = attacker.equipment.for_melee();
    resolve_melee_attack_by_parts(
        &attacker.skills,
        &attacker.attributes,
        &attacker.body,
        &attacker.essentials,
        &attacker_equipment,
        attacker.equipment.holding_side,
        precision,
        flanking,
        part,
        response,
        &defender.skills,
        &defender.attributes,
        &defender.body,
        &defender.essentials,
        &defender.equipment,
    )
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
    resolve_ranged_attack_by_parts(
        &attacker.skills,
        &attacker.attributes,
        &attacker.body,
        &attacker.essentials,
        &attacker_equipment,
        precision,
        flanking,
        part,
        response,
        &defender.skills,
        &defender.attributes,
        &defender.body,
        &defender.essentials,
        &defender.equipment,
    )
}

fn optimal_melee_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    part: BodyPart,
) -> AttackResult {
    let reflex = melee_input_reflex(attacker);
    [
        DefenderResponse::None,
        DefenderResponse::Dodge {
            input_reflex: reflex,
        },
        DefenderResponse::Parry {
            input_reflex: reflex,
        },
    ]
    .into_iter()
    .map(|response| melee_exchange(attacker, defender, precision, flanking, part, response))
    .min_by(|left, right| attack_harm(*left).total_cmp(&attack_harm(*right)))
    .unwrap()
}

fn melee_input_reflex(attacker: &Combatant) -> f32 {
    let interval = attacker
        .equipment
        .melee_weapon
        .map_or(REFERENCE_MELEE_ATTACK_SECONDS, |weapon| {
            weapon.attack_interval_seconds.max(0.1)
        });
    (interval / REFERENCE_MELEE_ATTACK_SECONDS).clamp(0.1, 1.0)
}

fn optimal_ranged_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    part: BodyPart,
) -> AttackResult {
    [
        DefenderResponse::None,
        DefenderResponse::Dodge { input_reflex: 1.0 },
        DefenderResponse::Parry { input_reflex: 1.0 },
    ]
    .into_iter()
    .map(|response| ranged_exchange(attacker, defender, precision, 0.0, part, response))
    .min_by(|left, right| attack_harm(*left).total_cmp(&attack_harm(*right)))
    .unwrap()
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
    side.is_empty() || side.iter().all(Combatant::is_incapacitated)
}

fn outcome(combatant: Combatant) -> CombatantOutcome {
    let incapacitated = combatant.is_incapacitated();
    CombatantOutcome {
        id: combatant.id,
        body: combatant.body,
        blood_loss_fraction: combatant.blood_loss_fraction,
        cut_damage: combatant.cut_damage,
        incapacitated,
        ammunition_used: combatant
            .initial_ammunition
            .saturating_sub(combatant.equipment.ammunition),
    }
}

fn random_body_part(random: &mut SplitMix64) -> BodyPart {
    match random.next_u64() % 100 {
        0..=11 => BodyPart::LeftArm,
        12..=23 => BodyPart::RightArm,
        24..=35 => BodyPart::LeftLeg,
        36..=47 => BodyPart::RightLeg,
        48..=69 => BodyPart::Chest,
        70..=89 => BodyPart::Stomach,
        _ => BodyPart::Head,
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

struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn unit_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1_u32 << 24) as f32
    }

    fn index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        self.next_u64() as usize % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fighter(id: u64, skill: f32, ranged: bool) -> Combatant {
        let mut fighter = Combatant::new(id);
        fighter.attributes = CombatAttributes {
            endurance: 3.0,
            precision: skill,
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
            ..CombatAttributes::default()
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
            weight: 1.5,
            penetration: 1.0,
            melee_reach: if ranged { 0.0 } else { 1.0 },
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
    fn fixed_seed_is_reproducible() {
        let first = resolve_battle(
            vec![fighter(1, 3.0, false)],
            vec![fighter(2, 2.0, false)],
            42,
        );
        let second = resolve_battle(
            vec![fighter(1, 3.0, false)],
            vec![fighter(2, 2.0, false)],
            42,
        );
        assert_eq!(first.victor, second.victor);
        assert_eq!(first.rounds, second.rounds);
        assert_eq!(first.allies[0].body.health, second.allies[0].body.health);
        assert_eq!(first.enemies[0].body.health, second.enemies[0].body.health);
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
    fn ranged_blocking_requires_a_shield() {
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
        let weapon_parry = ranged_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::Parry { input_reflex: 1.0 },
        );
        assert_eq!(
            health_damage_from_attack(undefended, BodyPart::Chest),
            health_damage_from_attack(weapon_parry, BodyPart::Chest)
        );
    }

    #[test]
    fn melee_parry_carries_contact_force_while_dodge_does_not() {
        let attacker = fighter(1, 0.1, false);
        let mut defender = fighter(2, 5.0, false);
        defender.equipment.shield_block_bonus = 5.0;

        let parry = melee_exchange(
            &attacker,
            &defender,
            0.65,
            0.0,
            BodyPart::Chest,
            DefenderResponse::Parry { input_reflex: 1.0 },
        );
        assert!(matches!(
            parry,
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
            BodyPart::Chest,
            DefenderResponse::Dodge { input_reflex: 1.0 },
        );
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
    fn precision_does_not_increase_block_defense() {
        let mut low_precision = fighter(1, 3.0, false);
        let mut high_precision = low_precision.clone();
        low_precision.attributes.precision = 0.0;
        high_precision.attributes.precision = 5.0;

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
        assert_eq!(block(&low_precision), block(&high_precision));
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
            coverage: 0.0,
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
            matches!(armored, AttackResult::ToDefender { contact_force, armor_contact: true, .. } if contact_force > 0.0)
        );
    }

    #[test]
    fn precise_melee_criticals_bypass_armor() {
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
            coverage: 0.0,
        });

        let critical = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );
        attacker.equipment.melee_weapon.as_mut().unwrap().precise = false;
        let armored = melee_exchange(
            &attacker,
            &defender,
            1.0,
            0.0,
            BodyPart::Chest,
            DefenderResponse::None,
        );

        assert!(health_damage_from_attack(critical, BodyPart::Chest) > 0.0);
        assert_eq!(health_damage_from_attack(armored, BodyPart::Chest), 0.0);
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
            armor_contact: true,
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
            BodyPart::Chest,
            result,
            effect,
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
            BodyPart::Chest,
            result,
            effect,
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
    fn successful_parry_records_wear_for_both_contacting_instances() {
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
            BodyPart::Chest,
            result,
            AttackEffect {
                hit: false,
                health_damage: 0.0,
            },
        );

        assert_eq!(recorder.log[0].weapon_inventory_item_id, Some(202));
        assert_eq!(recorder.log[0].defender_contact_item_id, Some(303));
        assert_eq!(recorder.log[0].contact_stress, 55.0);
        assert_eq!(recorder.log[0].outcome, "blocked");
        assert!(!recorder.log[0].armor_contact);
    }

    #[test]
    fn melee_screen_forces_engagement_before_backline_access() {
        let attackers = vec![fighter(1, 3.0, false), fighter(2, 3.0, false)];
        let defenders = vec![fighter(3, 3.0, false), fighter(4, 3.0, true)];

        assert_eq!(melee_assignment(0, &attackers, &defenders), (0, 0.0));
        assert_eq!(melee_assignment(1, &attackers, &defenders), (1, 0.0));

        let mut surplus = attackers;
        surplus.push(fighter(5, 3.0, false));
        assert_eq!(melee_assignment(2, &surplus, &defenders), (0, 0.5));
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

        let direct_plan = opening_volley_plans(&ranged_side, &matched_closers)[0];
        let detour_plan = opening_volley_plans(&ranged_side, &surplus_closers)[0];
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
        let plans = opening_volley_plans(&attackers, &defenders);
        let direct_attacks = plans[0].direct_attacks;

        take_opening_volley_step(
            &mut attackers,
            &plans,
            &mut defenders,
            &[1],
            direct_attacks,
            &mut SplitMix64::new(17),
            &mut BattleRecorder::default(),
        );

        assert_eq!(defenders[0].body.total_damage(), 0.0);
        assert!(defenders[1].body.total_damage() > 0.0);
    }

    #[test]
    fn successful_stealth_grants_a_flat_footed_melee_attack() {
        let mut ambusher = fighter(1, 5.0, false);
        ambusher.skills.stealth_hours = 100_000.0;
        ambusher.equipment.melee_weapon.as_mut().unwrap().accuracy = 2.0;
        let mut attackers = vec![ambusher];
        let mut defenders = vec![fighter(2, 1.0, false)];

        let mut recorder = BattleRecorder::default();
        let attacks = plan_stealth_openers(
            &attackers,
            &defenders,
            &mut SplitMix64::new(11),
            &mut recorder,
        );
        apply_pending_attacks(&mut attackers, &mut defenders, &attacks, &mut recorder);

        assert!(defenders[0].body.total_damage() > 0.0);
    }

    #[test]
    fn failed_stealth_does_not_grant_an_attack() {
        let mut ambusher = fighter(1, 0.0, false);
        ambusher.skills.stealth_hours = 0.0;
        let mut observer = fighter(2, 1.0, false);
        observer.attributes.eyesight = 100.0;
        observer.attributes.hearing = 100.0;
        let mut attackers = vec![ambusher];
        let mut defenders = vec![observer];

        let mut recorder = BattleRecorder::default();
        let attacks = plan_stealth_openers(
            &attackers,
            &defenders,
            &mut SplitMix64::new(11),
            &mut recorder,
        );
        apply_pending_attacks(&mut attackers, &mut defenders, &attacks, &mut recorder);

        assert_eq!(defenders[0].body.total_damage(), 0.0);
    }

    #[test]
    fn opposing_stealth_openers_are_simultaneous() {
        let mut ally = fighter(1, 5.0, false);
        ally.skills.stealth_hours = 100_000.0;
        ally.equipment.melee_weapon.as_mut().unwrap().accuracy = 2.0;
        let mut enemy = fighter(2, 5.0, false);
        enemy.skills.stealth_hours = 100_000.0;
        enemy.equipment.melee_weapon.as_mut().unwrap().accuracy = 2.0;
        let mut allies = vec![ally];
        let mut enemies = vec![enemy];

        resolve_stealth_openers(
            &mut allies,
            &mut enemies,
            &mut SplitMix64::new(23),
            &mut BattleRecorder::default(),
        );

        assert!(allies[0].body.total_damage() > 0.0);
        assert!(enemies[0].body.total_damage() > 0.0);
    }

    #[test]
    fn empty_opposition_is_an_immediate_victory() {
        let outcome = resolve_battle(vec![fighter(1, 3.0, false)], Vec::new(), 1);
        assert_eq!(outcome.victor, BattleVictor::Allies);
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
        let attacks = plan_ranged_round(&mut [fast], &defenders, 1, &mut SplitMix64::new(3));
        assert_eq!(attacks.len(), 2);
    }

    #[test]
    fn movement_speed_changes_approach_fire_window() {
        let ranged = vec![fighter(1, 3.0, true)];
        let mut slow = fighter(2, 3.0, false);
        slow.attributes.left_leg_agility = 0.0;
        slow.attributes.right_leg_agility = 0.0;
        let fast = fighter(3, 5.0, false);

        let slow_attacks = opening_volley_plans(&ranged, &[slow])[0].total_attacks;
        let fast_attacks = opening_volley_plans(&ranged, &[fast])[0].total_attacks;
        assert!(slow_attacks > fast_attacks);
    }

    #[test]
    fn faster_melee_weapons_reduce_defender_reflex() {
        let mut fast = fighter(1, 3.0, false);
        fast.equipment
            .melee_weapon
            .as_mut()
            .unwrap()
            .attack_interval_seconds = 0.4;
        let mut slow = fighter(2, 3.0, false);
        slow.equipment
            .melee_weapon
            .as_mut()
            .unwrap()
            .attack_interval_seconds = 1.5;
        assert!(melee_input_reflex(&fast) < melee_input_reflex(&slow));
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
                )
                .victor
                    == BattleVictor::Allies
            })
            .count();
        let weak_wins = (0..64)
            .filter(|seed| {
                resolve_battle(
                    vec![fighter(1, 1.5, false)],
                    vec![fighter(2, 4.0, false), fighter(3, 4.0, false)],
                    *seed,
                )
                .victor
                    == BattleVictor::Allies
            })
            .count();
        assert!(strong_wins > weak_wins, "{strong_wins} versus {weak_wins}");
    }
}
