use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct CombatArmor {
    pub inventory_item_id: Option<u64>,
    pub material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
    pub coverage: f32,
    pub coverage_span: Option<ArmorCoverageSpan>,
    pub coverage_geometry: Option<AuthoredArmorCoverage>,
}

impl CombatArmor {
    /// Full-body anatomical protection using the ordinary armor calculation.
    pub fn innate(resistance: f32, padding: f32) -> Self {
        Self {
            inventory_item_id: None,
            material: None,
            resistance,
            padding,
            flexibility: 0.5,
            range_of_motion: 1.0,
            coverage: 1.0,
            coverage_span: Some(ArmorCoverageSpan::centered(1.0)),
            coverage_geometry: None,
        }
    }
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
    pub swing_precision: f32,
    pub stab_precision: f32,
    pub preferred_melee_style: crate::combat_style::MeleeAttackStyle,
    pub weight: f32,
    pub moment_of_inertia_kg_m2: f32,
    pub penetration: f32,
    pub melee_reach: f32,
    pub grip_to_tip_m: f32,
    pub total_length_m: f32,
    pub striking_head_length_m: f32,
    pub distal_headed: bool,
    pub body_material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub striking_material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub ranged_range: f32,
    pub attack_interval_seconds: f32,
    pub precise: bool,
    pub balance: f32,
    pub ranged_force_joules: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CombatProjectileKind {
    Arrowhead,
    Ball,
}

#[derive(Clone, Debug)]
pub struct CombatEquipment {
    pub weapon: Option<CombatWeapon>,
    pub melee_weapon: Option<CombatWeapon>,
    pub ranged_weapon: Option<CombatWeapon>,
    pub melee_weapon_id: Option<u64>,
    pub ranged_weapon_id: Option<u64>,
    pub ranged_projectile_kind: Option<CombatProjectileKind>,
    pub defense_item_id: Option<u64>,
    pub ammunition: u32,
    pub holding_side: BodySide,
    pub melee_holding_side: BodySide,
    pub ranged_holding_side: BodySide,
    pub shield_block_bonus: f32,
    pub shield_side: Option<BodySide>,
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
            shield_side: None,
            armor: [CombatArmor {
                inventory_item_id: None,
                material: None,
                range_of_motion: 1.0,
                flexibility: 1.0,
                ..CombatArmor::default()
            }; 7],
            inventory_weight: 0.0,
        }
    }
}

impl CombatEquipment {
    pub(super) fn for_melee(&self) -> Self {
        let mut equipment = self.clone();
        equipment.weapon = self.melee_weapon;
        equipment.holding_side = self.melee_holding_side;
        equipment
    }

    pub(super) fn for_ranged(&self) -> Self {
        let mut equipment = self.clone();
        equipment.weapon = self.ranged_weapon;
        equipment.holding_side = self.ranged_holding_side;
        equipment
    }
}

impl PlayerEquipment for CombatEquipment {
    fn weapon_skill_distribution(&self) -> crate::equipment::WeaponSkillDistribution {
        self.weapon.map_or(
            crate::equipment::WeaponSkillDistribution::UNARMED,
            |weapon| weapon.skills,
        )
    }
    fn weapon_is_melee(&self) -> bool {
        self.weapon.is_none_or(|weapon| weapon.melee)
    }
    fn weapon_is_ranged(&self) -> bool {
        self.weapon.is_some_and(|weapon| weapon.ranged)
    }
    fn weapon_is_unarmed(&self) -> bool {
        self.weapon.is_none()
    }
    fn weapon_does_blunt(&self) -> bool {
        self.weapon.is_none_or(|weapon| weapon.blunt)
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
    fn weapon_swing_precision(&self) -> f32 {
        self.weapon
            .map_or(UNARMED_SWING_PRECISION, |weapon| weapon.swing_precision)
    }
    fn weapon_stab_precision(&self) -> f32 {
        self.weapon
            .map_or(UNARMED_STAB_PRECISION, |weapon| weapon.stab_precision)
    }
    fn weapon_preferred_melee_style(&self) -> crate::combat_style::MeleeAttackStyle {
        self.weapon
            .map_or(crate::combat_style::MeleeAttackStyle::Swing, |weapon| {
                weapon.preferred_melee_style
            })
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
    fn weapon_grip_to_tip(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.grip_to_tip_m)
    }
    fn weapon_total_length(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.total_length_m)
    }
    fn weapon_striking_head_length(&self) -> f32 {
        self.weapon
            .map_or(0.0, |weapon| weapon.striking_head_length_m)
    }
    fn weapon_body_material(&self) -> Option<crate::item_catalog_schema::EquipmentMaterial> {
        self.weapon.and_then(|weapon| weapon.body_material)
    }
    fn weapon_striking_material(&self) -> Option<crate::item_catalog_schema::EquipmentMaterial> {
        self.weapon.and_then(|weapon| weapon.striking_material)
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
    fn weapon_moment_of_inertia(&self) -> f32 {
        self.weapon
            .map_or(0.0, |weapon| weapon.moment_of_inertia_kg_m2)
    }
    fn weapon_ranged_force_joules(&self) -> f32 {
        self.weapon.map_or(0.0, |weapon| weapon.ranged_force_joules)
    }
    fn shield_block_bonus(&self) -> f32 {
        self.shield_block_bonus
    }
    fn shield_holding_side(&self) -> Option<BodySide> {
        self.shield_side
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
    fn armor_surface(&self, part: BodyPart, sample: f32) -> Option<crate::equipment::ArmorSurface> {
        let armor = self.armor[body_part_index(part)];
        armor
            .coverage_geometry
            .map(|geometry| geometry.span)
            .or(armor.coverage_span)
            .unwrap_or_else(|| ArmorCoverageSpan::centered(armor.coverage))
            .contains(sample)
            .then_some(crate::equipment::ArmorSurface {
                inventory_item_id: armor.inventory_item_id,
                material: armor.material,
                resistance: armor.resistance,
                padding: armor.padding,
                flexibility: armor.flexibility,
            })
    }
    fn inventory_weight(&self) -> f32 {
        self.inventory_weight
    }
}
