use super::*;

#[derive(Serialize)]
pub(super) struct ReviewerCombatant<'a> {
    name: &'a str,
    build: &'a str,
    equipment: &'a str,
    attributes: AttributeContext,
    skills: SkillContext,
}

#[derive(Serialize)]
struct AttributeContext {
    endurance: f32,
    strength: f32,
    agility: f32,
    instinct: f32,
}

#[derive(Serialize)]
struct SkillContext {
    primary_weapon_rank: f32,
    dodge_rank: f32,
    block_rank: f32,
    will_rank: f32,
    balance_rank: f32,
}

#[derive(Serialize)]
pub(super) struct ReviewerPacket<'a> {
    pub(super) balance_concept: &'static str,
    pub(super) scale_context: &'static str,
    pub(super) matchup: String,
    pub(super) combatants: [ReviewerCombatant<'a>; 2],
    pub(super) tactical_trace_file: &'static str,
    pub(super) all_tactical_traces_file: &'static str,
    pub(super) autoresolve_trace_file: &'static str,
    pub(super) all_autoresolve_traces_file: &'static str,
    pub(super) aggregate_summary_file: &'static str,
    pub(super) acceptance_evidence_file: &'static str,
    pub(super) acceptance_audit_file: &'static str,
}

#[derive(Serialize)]
pub(super) struct ReviewerIndex<'a> {
    pub(super) gate: &'static str,
    pub(super) tactical_seeds_per_matchup: u64,
    pub(super) autoresolve_seeds_per_matchup: u64,
    pub(super) summary_file: &'static str,
    pub(super) acceptance_evidence_file: &'static str,
    pub(super) acceptance_audit_file: &'static str,
    pub(super) matchup_directories: Vec<&'a str>,
}

pub(super) fn reviewer_combatant(build: &MeleeIterationBuild) -> ReviewerCombatant<'_> {
    let attributes = &build.combatant.attributes;
    let skills = &build.combatant.skills;
    let primary_hours = build
        .combatant
        .equipment
        .melee_weapon
        .map_or(0.0, |weapon| {
            weapon
                .skills
                .weighted_check(|skill| skills.skill_hours_trained(skill))
        });
    ReviewerCombatant {
        name: build.name,
        build: build.description,
        equipment: build.equipment_description,
        attributes: AttributeContext {
            endurance: attributes.endurance,
            strength: (attributes.left_arm_strength + attributes.right_arm_strength) * 0.5,
            agility: (attributes.left_arm_agility + attributes.right_arm_agility) * 0.5,
            instinct: attributes.instinct,
        },
        skills: SkillContext {
            primary_weapon_rank: Skill::Sword.training_rank(primary_hours),
            dodge_rank: Skill::Dodge.training_rank(skills.dodge_hours),
            block_rank: Skill::Block.training_rank(skills.block_hours),
            will_rank: Skill::Will.training_rank(skills.will_hours),
            balance_rank: Skill::Balance.training_rank(skills.balance_hours),
        },
    }
}
