use super::*;

pub(super) fn canonical_impact_surface(
    attacker_position: Vec3,
    target_transform: &Transform,
    body_part: BodyPart,
    config: &TacticalCombatConfig,
) -> (Vec3, Vec3) {
    let Some(hitbox) = config
        .targeting
        .body_part_hitboxes
        .iter()
        .find(|hitbox| hitbox.body_part == body_part)
    else {
        return (Vec3::ZERO, Vec3::Z);
    };
    let center = Vec3::from_array(hitbox.center_metres);
    let half = Vec3::from_array(hitbox.half_extents_metres);
    let attacker_local = target_transform
        .compute_affine()
        .inverse()
        .transform_point3(attacker_position);
    let direction = (attacker_local - center).normalize_or(Vec3::Z);
    let scale = [
        (half.x / direction.x.abs())
            .is_finite()
            .then_some(half.x / direction.x.abs()),
        (half.y / direction.y.abs())
            .is_finite()
            .then_some(half.y / direction.y.abs()),
        (half.z / direction.z.abs())
            .is_finite()
            .then_some(half.z / direction.z.abs()),
    ]
    .into_iter()
    .flatten()
    .fold(f32::INFINITY, f32::min);
    let point = center + direction * scale;
    let normalized = (point - center) / half;
    let normal =
        if normalized.x.abs() >= normalized.y.abs() && normalized.x.abs() >= normalized.z.abs() {
            Vec3::X * normalized.x.signum()
        } else if normalized.y.abs() >= normalized.z.abs() {
            Vec3::Y * normalized.y.signum()
        } else {
            Vec3::Z * normalized.z.signum()
        };
    (point, normal)
}

pub(super) struct InitialMeleeContact {
    pub(super) sample: f32,
    pub(super) defense_alignment_sample: f32,
    pub(super) body_part: Option<BodyPart>,
    pub(super) weapon_reach: f32,
}

pub(super) fn initial_melee_contact(
    viewer: &TacticalPlayerViewer<'_, '_>,
    event: &MeleeAttackStartedIntent,
    strike_family: StrikeFamily,
    random: &mut crate::bot::CombatRandom,
) -> InitialMeleeContact {
    let sample = random.unit_f32();
    let attacker = viewer.get_for_attack(event.attacker, event.hand).ok();
    let weapon_reach = attacker.as_ref().map_or(0.0, |view| view.weapon_reach());
    let body_part = event.target.and_then(|target| {
        let attacker = attacker.as_ref()?;
        let defender = viewer.get(target).ok()?;
        let side = attacker.weapon_holding_side()?;
        Some(
            attacker
                .melee_contact_location(
                    side,
                    strike_family.melee_style(),
                    &defender,
                    event.reported_precision.get(),
                    sample,
                )
                .body_part,
        )
    });
    InitialMeleeContact {
        sample,
        defense_alignment_sample: random.unit_f32(),
        body_part,
        weapon_reach,
    }
}

pub(super) fn attacker_has_weapon(
    viewer: &TacticalPlayerViewer<'_, '_>,
    entity: Entity,
    hand: AttackHand,
) -> bool {
    viewer
        .inventory
        .get_for_attack(entity, hand)
        .has_striking_item()
}

pub(super) fn windup_duration(contact_tick: u64, start_tick: u64) -> CombatDuration {
    CombatDuration::from_secs_f32(
        contact_tick.saturating_sub(start_tick) as f32 / locomotion_sample_hz(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "resolution consumes the authorized attack and live defense facets"
)]
pub(super) fn resolve_melee_contact(
    attacker: &TacticalPlayerView<'_, '_, '_>,
    defender: &TacticalPlayerView<'_, '_, '_>,
    defender_categories: &[BestiaryCategory],
    parameters: adventuresim_core::combat::CombatResolutionParameters,
    attacker_side: BodySide,
    attack_style: MeleeAttackStyle,
    defender_response: DefenderResponse,
    reported_precision: ReportedPrecision,
    flanking: f32,
    sample: f32,
    forced_body_part: Option<BodyPart>,
    contact_at_time: MeleeContactAtTime,
) -> (MeleeContactLocation, AttackResult) {
    let mut contact = attacker.melee_contact_location(
        attacker_side,
        attack_style,
        defender,
        reported_precision.get(),
        sample,
    );
    if let Some(body_part) = forced_body_part {
        let surface_coordinate = sample.clamp(0.0, 1.0 - f32::EPSILON);
        let armor_surface = defender.armor_surface(body_part, surface_coordinate);
        contact = MeleeContactLocation::new(
            body_part,
            anatomical_subregion(body_part, surface_coordinate),
            surface_coordinate,
            armor_surface,
        );
    }
    let result = attacker.resolve_melee_attack(
        parameters,
        attacker_side,
        attack_style,
        defender,
        defender_categories,
        defender_response,
        reported_precision.get(),
        flanking,
        contact,
        contact_at_time,
    );
    (contact, result)
}
