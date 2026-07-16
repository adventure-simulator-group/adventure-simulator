//! Framework-neutral, deterministic combat simulation for strategic autoresolve.

use crate::prelude::*;

const MAX_COMBAT_ROUNDS: usize = 256;
const MAX_OPENING_VOLLEYS: usize = 8;
const BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;
const FORMATION_SPACING_METERS: f32 = 2.0;
const METERS_CLOSED_PER_VOLLEY: f32 = 5.0;
const CONTESTED_CHECK_RANDOM_RANGE: f32 = 5.0;

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
    pub melee_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub ranged_hours: f32,
    pub will_hours: f32,
    pub charisma_hours: f32,
    pub medicine_hours: f32,
    pub faith_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub surgeon_hours: f32,
}

impl PlayerSkills for CombatSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Melee => self.melee_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Block => self.block_hours,
            Skill::Ranged => self.ranged_hours,
            Skill::Will => self.will_hours,
            Skill::Charisma => self.charisma_hours,
            Skill::Medicine => self.medicine_hours,
            Skill::Faith => self.faith_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::Surgeon => self.surgeon_hours,
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
    pub melee: bool,
    pub ranged: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub accuracy: f32,
    pub weight: f32,
    pub penetration: f32,
    pub reach: f32,
    pub precise: bool,
    pub balance: f32,
    pub ranged_force_joules: f32,
}

#[derive(Clone, Debug)]
pub struct CombatEquipment {
    pub weapon: Option<CombatWeapon>,
    pub holding_side: BodySide,
    pub shield_block_bonus: f32,
    pub armor: [CombatArmor; 7],
    pub inventory_weight: f32,
}

impl Default for CombatEquipment {
    fn default() -> Self {
        Self {
            weapon: None,
            holding_side: BodySide::Right,
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

impl PlayerEquipment for CombatEquipment {
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
        self.weapon.map_or(0.0, |weapon| weapon.reach)
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
    pub incapacitated: bool,
}

#[derive(Clone, Debug)]
pub struct BattleOutcome {
    pub victor: BattleVictor,
    pub rounds: usize,
    pub allies: Vec<CombatantOutcome>,
    pub enemies: Vec<CombatantOutcome>,
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
}

#[derive(Clone, Copy, Default)]
struct OpeningVolleyPlan {
    direct_volleys: usize,
    total_volleys: usize,
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
    let mut victor = BattleVictor::Stalemate;
    let mut rounds = 0;

    resolve_stealth_openers(&mut allies, &mut enemies, &mut random);
    resolve_opening_volleys(&mut allies, &mut enemies, &mut random);

    for round in 0..MAX_COMBAT_ROUNDS {
        if side_defeated(&allies) {
            victor = BattleVictor::Enemies;
            break;
        }
        if side_defeated(&enemies) {
            victor = BattleVictor::Allies;
            break;
        }
        rounds = round + 1;

        if random.next_u64().is_multiple_of(2) {
            take_side_turns(&mut allies, &mut enemies, &mut random);
            take_side_turns(&mut enemies, &mut allies, &mut random);
        } else {
            take_side_turns(&mut enemies, &mut allies, &mut random);
            take_side_turns(&mut allies, &mut enemies, &mut random);
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

    BattleOutcome {
        victor,
        rounds,
        allies: allies.into_iter().map(outcome).collect(),
        enemies: enemies.into_iter().map(outcome).collect(),
    }
}

fn resolve_stealth_openers(
    allies: &mut [Combatant],
    enemies: &mut [Combatant],
    random: &mut SplitMix64,
) {
    let allies_first = random.next_u64().is_multiple_of(2);
    let (ally_attacks, enemy_attacks) = if allies_first {
        (
            plan_stealth_openers(allies, enemies, random),
            plan_stealth_openers(enemies, allies, random),
        )
    } else {
        let enemy_attacks = plan_stealth_openers(enemies, allies, random);
        let ally_attacks = plan_stealth_openers(allies, enemies, random);
        (ally_attacks, enemy_attacks)
    };
    if allies_first {
        apply_pending_attacks(allies, enemies, &ally_attacks);
        apply_pending_attacks(enemies, allies, &enemy_attacks);
    } else {
        apply_pending_attacks(enemies, allies, &enemy_attacks);
        apply_pending_attacks(allies, enemies, &ally_attacks);
    }
}

fn plan_stealth_openers(
    attackers: &[Combatant],
    defenders: &[Combatant],
    random: &mut SplitMix64,
) -> Vec<PendingAttack> {
    let mut attacks = Vec::new();
    for (attacker_index, attacker) in attackers.iter().enumerate() {
        if attacker.is_incapacitated() || preferred_attack_mode(attacker) != AttackMode::Melee {
            continue;
        }
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
        });
    }
    attacks
}

fn apply_pending_attacks(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    attacks: &[PendingAttack],
) {
    for attack in attacks {
        apply_attack_result(
            &mut attackers[attack.attacker_index],
            &mut defenders[attack.target_index],
            attack.result,
            attack.part,
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
        .map(|plan| plan.total_volleys)
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
            );
            take_opening_volley_step(
                enemies,
                &enemy_plans,
                allies,
                &enemy_detour_targets,
                step,
                random,
            );
        } else {
            take_opening_volley_step(
                enemies,
                &enemy_plans,
                allies,
                &enemy_detour_targets,
                step,
                random,
            );
            take_opening_volley_step(
                allies,
                &ally_plans,
                enemies,
                &ally_detour_targets,
                step,
                random,
            );
        }
    }
}

fn opening_volley_plans(
    ranged_side: &[Combatant],
    closing_side: &[Combatant],
) -> Vec<OpeningVolleyPlan> {
    let screen_count = active_melee_indices(ranged_side).len();
    let closing_melee_count = active_melee_indices(closing_side).len();
    ranged_side
        .iter()
        .map(|attacker| {
            if attacker.is_incapacitated()
                || preferred_attack_mode(attacker) != AttackMode::Ranged
                || closing_melee_count == 0
            {
                return OpeningVolleyPlan::default();
            }

            let range = attacker.equipment.weapon_reach().max(0.0);
            let direct_volleys = (range / METERS_CLOSED_PER_VOLLEY)
                .ceil()
                .clamp(0.0, MAX_OPENING_VOLLEYS as f32) as usize;
            let detour = if closing_melee_count > screen_count && screen_count > 0 {
                let formation_radius = screen_count as f32 * FORMATION_SPACING_METERS * 0.5;
                std::f32::consts::PI * formation_radius
            } else {
                0.0
            };
            OpeningVolleyPlan {
                direct_volleys,
                total_volleys: ((range + detour) / METERS_CLOSED_PER_VOLLEY)
                    .ceil()
                    .clamp(0.0, MAX_OPENING_VOLLEYS as f32) as usize,
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
) {
    for attacker_index in 0..attackers.len() {
        let plan = plans[attacker_index];
        if plan.total_volleys <= step || attackers[attacker_index].is_incapacitated() {
            continue;
        }
        let targets = if step < plan.direct_volleys {
            active_melee_indices(defenders)
        } else {
            detour_targets
                .iter()
                .copied()
                .filter(|index| !defenders[*index].is_incapacitated())
                .collect()
        };
        if targets.is_empty() {
            break;
        }
        let target_index = targets[random.index(targets.len())];
        let part = random_body_part(random);
        let result = ranged_exchange(
            &attackers[attacker_index],
            &defenders[target_index],
            0.65 + random.unit_f32() * 0.35,
            0.0,
            part,
            random_response(random),
        );
        apply_attack_result(
            &mut attackers[attacker_index],
            &mut defenders[target_index],
            result,
            part,
        );
    }
}

fn take_side_turns(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    random: &mut SplitMix64,
) {
    for attacker_index in 0..attackers.len() {
        if attackers[attacker_index].is_incapacitated() || side_defeated(defenders) {
            continue;
        }
        let mode = preferred_attack_mode(&attackers[attacker_index]);
        let active_targets = engaged_target_indices(attacker_index, attackers, defenders, mode);
        let target_index = active_targets[random.index(active_targets.len())];
        let part = random_body_part(random);
        let response = random_response(random);
        let hit_precision = 0.65 + random.unit_f32() * 0.35;
        let flanking = if random.next_u64().is_multiple_of(5) {
            0.5 + random.unit_f32() * 0.5
        } else {
            0.0
        };

        let result = if mode == AttackMode::Ranged {
            ranged_exchange(
                &attackers[attacker_index],
                &defenders[target_index],
                hit_precision,
                flanking,
                part,
                response,
            )
        } else {
            melee_exchange(
                &attackers[attacker_index],
                &defenders[target_index],
                hit_precision,
                flanking,
                part,
                response,
            )
        };
        apply_attack_result(
            &mut attackers[attacker_index],
            &mut defenders[target_index],
            result,
            part,
        );
    }
}

fn engaged_target_indices(
    attacker_index: usize,
    attackers: &[Combatant],
    defenders: &[Combatant],
    mode: AttackMode,
) -> Vec<usize> {
    let active = active_indices(defenders);
    if mode == AttackMode::Ranged {
        return active;
    }

    let defending_melee = active_melee_indices(defenders);
    let melee_rank = attackers[..=attacker_index]
        .iter()
        .filter(|combatant| {
            !combatant.is_incapacitated() && preferred_attack_mode(combatant) == AttackMode::Melee
        })
        .count()
        .saturating_sub(1);
    if melee_rank < defending_melee.len() {
        return defending_melee;
    }

    let exposed_backline: Vec<_> = active
        .iter()
        .copied()
        .filter(|index| preferred_attack_mode(&defenders[*index]) == AttackMode::Ranged)
        .collect();
    if exposed_backline.is_empty() {
        active
    } else {
        exposed_backline
    }
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
            (!combatant.is_incapacitated() && preferred_attack_mode(combatant) == AttackMode::Melee)
                .then_some(index)
        })
        .collect()
}

fn apply_attack_result(
    attacker: &mut Combatant,
    defender: &mut Combatant,
    result: AttackResult,
    part: BodyPart,
) {
    match result {
        AttackResult::ToAttacker { balance_damage } => {
            attacker.imbalance += balance_damage.max(0.0);
        }
        AttackResult::ToDefender { balance_damage, .. } => {
            defender.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, part);
            let applied = defender.body.apply_damage(part, damage);
            defender.blood_loss_fraction += applied * BLOOD_LOSS_PER_HEALTH_DAMAGE;
        }
    }
}

fn preferred_attack_mode(attacker: &Combatant) -> AttackMode {
    match (
        attacker.equipment.weapon_is_melee(),
        attacker.equipment.weapon_is_ranged(),
    ) {
        (false, true) => AttackMode::Ranged,
        (true, true) => {
            let melee = attacker.skills.skill_check_by_parts(
                Skill::Melee,
                &attacker.attributes,
                &attacker.body,
                &attacker.essentials,
                &attacker.equipment,
                LimbWeights::arm(
                    attacker.equipment.holding_side,
                    attacker.body.primary_side(),
                ),
            );
            let ranged = attacker.skills.skill_check_by_parts(
                Skill::Ranged,
                &attacker.attributes,
                &attacker.body,
                &attacker.essentials,
                &attacker.equipment,
                LimbWeights::both_arms(),
            );
            if ranged > melee {
                AttackMode::Ranged
            } else {
                AttackMode::Melee
            }
        }
        _ => AttackMode::Melee,
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
    resolve_melee_attack_by_parts(
        &attacker.skills,
        &attacker.attributes,
        &attacker.body,
        &attacker.essentials,
        &attacker.equipment,
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
    resolve_ranged_attack_by_parts(
        &attacker.skills,
        &attacker.attributes,
        &attacker.body,
        &attacker.essentials,
        &attacker.equipment,
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

fn side_defeated(side: &[Combatant]) -> bool {
    side.is_empty() || side.iter().all(Combatant::is_incapacitated)
}

fn outcome(combatant: Combatant) -> CombatantOutcome {
    let incapacitated = combatant.is_incapacitated();
    CombatantOutcome {
        id: combatant.id,
        body: combatant.body,
        blood_loss_fraction: combatant.blood_loss_fraction,
        incapacitated,
    }
}

fn random_response(random: &mut SplitMix64) -> DefenderResponse {
    let reflex = 0.6 + random.unit_f32() * 0.4;
    match random.next_u64() % 100 {
        0..=19 => DefenderResponse::None,
        20..=64 => DefenderResponse::Dodge {
            input_reflex: reflex,
        },
        _ => DefenderResponse::Parry {
            input_reflex: reflex,
        },
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
            melee_hours: skill * 2_000.0,
            ranged_hours: skill * 3_000.0,
            dodge_hours: skill * 2_000.0,
            block_hours: skill * 2_000.0,
            will_hours: skill * 1_000.0,
            balance_hours: skill * 2_000.0,
            ..CombatSkills::default()
        };
        fighter.equipment.weapon = Some(CombatWeapon {
            melee: !ranged,
            ranged,
            slash: !ranged,
            pierce: ranged,
            accuracy: 1.5,
            weight: 1.5,
            penetration: 1.0,
            reach: if ranged { 20.0 } else { 1.0 },
            ranged_force_joules: 50.0,
            ..CombatWeapon::default()
        });
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
        attacker.skills.ranged_hours = 100_000.0;
        let weapon = attacker.equipment.weapon.as_mut().unwrap();
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
        attacker.equipment.weapon.as_mut().unwrap().precise = false;
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
    }

    #[test]
    fn precise_melee_criticals_bypass_armor() {
        let mut attacker = fighter(1, 5.0, false);
        attacker.skills.melee_hours = 100_000.0;
        let weapon = attacker.equipment.weapon.as_mut().unwrap();
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
        attacker.equipment.weapon.as_mut().unwrap().precise = false;
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
    fn hybrid_weapons_use_the_stronger_combat_skill() {
        let mut hybrid = fighter(1, 3.0, false);
        let weapon = hybrid.equipment.weapon.as_mut().unwrap();
        weapon.melee = true;
        weapon.ranged = true;

        hybrid.skills.melee_hours = 20_000.0;
        hybrid.skills.ranged_hours = 0.0;
        assert_eq!(preferred_attack_mode(&hybrid), AttackMode::Melee);

        hybrid.skills.melee_hours = 0.0;
        hybrid.skills.ranged_hours = 30_000.0;
        assert_eq!(preferred_attack_mode(&hybrid), AttackMode::Ranged);
    }

    #[test]
    fn melee_screen_forces_engagement_before_backline_access() {
        let attackers = vec![fighter(1, 3.0, false), fighter(2, 3.0, false)];
        let defenders = vec![fighter(3, 3.0, false), fighter(4, 3.0, true)];

        assert_eq!(
            engaged_target_indices(0, &attackers, &defenders, AttackMode::Melee),
            vec![0]
        );
        assert_eq!(
            engaged_target_indices(1, &attackers, &defenders, AttackMode::Melee),
            vec![1]
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

        let direct_plan = opening_volley_plans(&ranged_side, &matched_closers)[0];
        let detour_plan = opening_volley_plans(&ranged_side, &surplus_closers)[0];
        assert_eq!(direct_plan.total_volleys, 4);
        assert_eq!(detour_plan.direct_volleys, 4);
        assert_eq!(detour_plan.total_volleys, 6);
    }

    #[test]
    fn ranged_characters_fire_during_the_enemy_approach() {
        let mut archer = fighter(1, 5.0, true);
        archer.skills.ranged_hours = 100_000.0;
        archer.equipment.weapon.as_mut().unwrap().accuracy = 2.0;
        let mut allies = vec![archer];
        let mut enemies = vec![fighter(2, 1.0, false)];

        resolve_opening_volleys(&mut allies, &mut enemies, &mut SplitMix64::new(7));

        assert!(enemies[0].body.total_damage() > 0.0);
    }

    #[test]
    fn detour_volleys_only_target_surplus_melee() {
        let mut archer = fighter(1, 5.0, true);
        archer.skills.ranged_hours = 100_000.0;
        archer.equipment.weapon.as_mut().unwrap().accuracy = 2.0;
        let screen = fighter(2, 3.0, false);
        let mut attackers = vec![archer, screen];
        let mut defenders = vec![fighter(3, 1.0, false), fighter(4, 1.0, false)];
        let plans = opening_volley_plans(&attackers, &defenders);
        let direct_volleys = plans[0].direct_volleys;

        take_opening_volley_step(
            &mut attackers,
            &plans,
            &mut defenders,
            &[1],
            direct_volleys,
            &mut SplitMix64::new(17),
        );

        assert_eq!(defenders[0].body.total_damage(), 0.0);
        assert!(defenders[1].body.total_damage() > 0.0);
    }

    #[test]
    fn successful_stealth_grants_a_flat_footed_melee_attack() {
        let mut ambusher = fighter(1, 5.0, false);
        ambusher.skills.stealth_hours = 100_000.0;
        ambusher.equipment.weapon.as_mut().unwrap().accuracy = 2.0;
        let mut attackers = vec![ambusher];
        let mut defenders = vec![fighter(2, 1.0, false)];

        let attacks = plan_stealth_openers(&attackers, &defenders, &mut SplitMix64::new(11));
        apply_pending_attacks(&mut attackers, &mut defenders, &attacks);

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

        let attacks = plan_stealth_openers(&attackers, &defenders, &mut SplitMix64::new(11));
        apply_pending_attacks(&mut attackers, &mut defenders, &attacks);

        assert_eq!(defenders[0].body.total_damage(), 0.0);
    }

    #[test]
    fn opposing_stealth_openers_are_simultaneous() {
        let mut ally = fighter(1, 5.0, false);
        ally.skills.stealth_hours = 100_000.0;
        ally.equipment.weapon.as_mut().unwrap().accuracy = 2.0;
        let mut enemy = fighter(2, 5.0, false);
        enemy.skills.stealth_hours = 100_000.0;
        enemy.equipment.weapon.as_mut().unwrap().accuracy = 2.0;
        let mut allies = vec![ally];
        let mut enemies = vec![enemy];

        resolve_stealth_openers(&mut allies, &mut enemies, &mut SplitMix64::new(23));

        assert!(allies[0].body.total_damage() > 0.0);
        assert!(enemies[0].body.total_damage() > 0.0);
    }

    #[test]
    fn empty_opposition_is_an_immediate_victory() {
        let outcome = resolve_battle(vec![fighter(1, 3.0, false)], Vec::new(), 1);
        assert_eq!(outcome.victor, BattleVictor::Allies);
        assert_eq!(outcome.rounds, 0);
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
