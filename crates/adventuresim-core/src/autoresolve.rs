//! Framework-neutral, deterministic combat simulation for strategic autoresolve.

use crate::prelude::*;

const MAX_COMBAT_ROUNDS: usize = 256;
const BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;

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

fn take_side_turns(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    random: &mut SplitMix64,
) {
    for attacker in attackers.iter_mut() {
        if attacker.is_incapacitated() || side_defeated(defenders) {
            continue;
        }
        let active_targets: Vec<_> = defenders
            .iter()
            .enumerate()
            .filter_map(|(index, target)| (!target.is_incapacitated()).then_some(index))
            .collect();
        let target_index = active_targets[random.index(active_targets.len())];
        let part = random_body_part(random);
        let response = random_response(random);
        let hit_precision = 0.65 + random.unit_f32() * 0.35;
        let flanking = if random.next_u64().is_multiple_of(5) {
            0.5 + random.unit_f32() * 0.5
        } else {
            0.0
        };

        let result = if attacker.equipment.weapon_is_ranged() {
            ranged_exchange(
                attacker,
                &defenders[target_index],
                hit_precision,
                flanking,
                part,
                response,
            )
        } else {
            melee_exchange(
                attacker,
                &defenders[target_index],
                hit_precision,
                flanking,
                part,
                response,
            )
        };

        match result {
            AttackResult::ToAttacker { balance_damage } => {
                attacker.imbalance += balance_damage.max(0.0);
            }
            AttackResult::ToDefender { balance_damage, .. } => {
                defenders[target_index].imbalance += balance_damage.max(0.0);
                let damage = health_damage_from_attack(result, part);
                let applied = defenders[target_index].body.apply_damage(part, damage);
                defenders[target_index].blood_loss_fraction +=
                    applied * BLOOD_LOSS_PER_HEALTH_DAMAGE;
            }
        }
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
