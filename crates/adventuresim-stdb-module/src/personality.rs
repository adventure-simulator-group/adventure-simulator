use crate::strategic::strategic_gateway_authority__view;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Nerve {
    Neutral,
    Brave,
    Fearful,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Drive {
    Neutral,
    Ambitious,
    Content,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Outlook {
    Neutral,
    Sanguine,
    Brooding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Sociability {
    Neutral,
    Gregarious,
    Solitary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Conscience {
    Neutral,
    Compassionate,
    Callous,
    Cruel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum SelfRegard {
    Neutral,
    Proud,
    Humble,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Conviction {
    Neutral,
    Zealous,
    Irreverent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Hygiene {
    Neutral,
    Slovenly,
    Cleanly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Temperance {
    Neutral,
    Temperate,
    Drunkard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Mirth {
    Neutral,
    Merry,
    Grave,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Courtship {
    Neutral,
    Amorous,
    Proper,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Transparency {
    Neutral,
    Open,
    Guarded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum SelfKnowledge {
    Neutral,
    Introspective,
    SelfDeceiving,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Inclination {
    Men,
    Either,
    Women,
    Neither,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Presentation {
    Man,
    Ambiguous,
    Woman,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum Sex {
    Female,
    Male,
}

/// Gateway-safe, derived visibility of strategic temperament. Behavioral
/// fields are a cache projected from the private continuous score row; raw
/// scores never cross the gateway boundary.
#[derive(Clone, Debug)]
#[table(accessor = character_personality)]
pub struct CharacterPersonality {
    #[primary_key]
    pub character_id: u64,
    /// Duplicate bounded traversal key for the fail-closed gateway view.
    #[index(btree)]
    pub projection_character_id: u64,
    pub nerve: Nerve,
    pub drive: Drive,
    pub outlook: Outlook,
    pub sociability: Sociability,
    pub conscience: Conscience,
    pub self_regard: SelfRegard,
    pub conviction: Conviction,
    pub hygiene: Hygiene,
    pub temperance: Temperance,
    pub mirth: Mirth,
    pub courtship: Courtship,
    pub transparency: Transparency,
    pub self_knowledge: SelfKnowledge,
    /// Private demographic truth. Attraction never reads this field.
    pub sex: Sex,
    /// Always assigned, normally observable social signal.
    pub presentation: Presentation,
    /// Always assigned private preference.
    pub inclination: Inclination,
}

pub const PERSONALITY_SCORE_LIMIT: i16 = 10_000;
pub const PERSONALITY_VISIBLE_THRESHOLD: i16 = 5_000;
pub const CONSCIENCE_CRUEL_THRESHOLD: i16 = -8_000;
pub const CHIVALRIC_DEED_DELTA: i16 = 6_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum PersonalityAxis {
    Nerve,
    Drive,
    Outlook,
    Sociability,
    Conscience,
    SelfRegard,
    Conviction,
    Hygiene,
    Temperance,
    Mirth,
    Courtship,
    Transparency,
    SelfKnowledge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ChivalricVirtue {
    Courage,
    Mercy,
    Faith,
}

/// Sole durable authority for the thirteen mutable behavioral axes. Values
/// are signed fixed point in `-10_000..=10_000`; zero is dispositionally
/// neutral and the endpoints preserve the legacy trait potency.
#[derive(Clone, Debug)]
#[table(accessor = character_personality_scores)]
pub struct CharacterPersonalityScores {
    #[primary_key]
    pub character_id: u64,
    pub nerve: i16,
    pub drive: i16,
    pub outlook: i16,
    pub sociability: i16,
    pub conscience: i16,
    pub self_regard: i16,
    pub conviction: i16,
    pub hygiene: i16,
    pub temperance: i16,
    pub mirth: i16,
    pub courtship: i16,
    pub transparency: i16,
    pub self_knowledge: i16,
}

/// Immutable audit entry for a personality-changing deed. `source_id` is
/// derived from authoritative gameplay state, making exact reducer retries a
/// no-op and conflicting reuse fail closed.
#[derive(Clone, Debug)]
#[table(accessor = personality_development_event)]
pub struct PersonalityDevelopmentEvent {
    #[primary_key]
    pub source_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub axis: PersonalityAxis,
    pub delta: i16,
    pub resulting_score: i16,
    pub deed: String,
    pub virtue: ChivalricVirtue,
    pub occurred_at_minute: u64,
}

impl CharacterPersonalityScores {
    pub fn neutral(character_id: u64) -> Self {
        Self {
            character_id,
            nerve: 0,
            drive: 0,
            outlook: 0,
            sociability: 0,
            conscience: 0,
            self_regard: 0,
            conviction: 0,
            hygiene: 0,
            temperance: 0,
            mirth: 0,
            courtship: 0,
            transparency: 0,
            self_knowledge: 0,
        }
    }

    pub fn score(&self, axis: PersonalityAxis) -> i16 {
        match axis {
            PersonalityAxis::Nerve => self.nerve,
            PersonalityAxis::Drive => self.drive,
            PersonalityAxis::Outlook => self.outlook,
            PersonalityAxis::Sociability => self.sociability,
            PersonalityAxis::Conscience => self.conscience,
            PersonalityAxis::SelfRegard => self.self_regard,
            PersonalityAxis::Conviction => self.conviction,
            PersonalityAxis::Hygiene => self.hygiene,
            PersonalityAxis::Temperance => self.temperance,
            PersonalityAxis::Mirth => self.mirth,
            PersonalityAxis::Courtship => self.courtship,
            PersonalityAxis::Transparency => self.transparency,
            PersonalityAxis::SelfKnowledge => self.self_knowledge,
        }
    }

    pub fn set_score(&mut self, axis: PersonalityAxis, value: i16) {
        let target = match axis {
            PersonalityAxis::Nerve => &mut self.nerve,
            PersonalityAxis::Drive => &mut self.drive,
            PersonalityAxis::Outlook => &mut self.outlook,
            PersonalityAxis::Sociability => &mut self.sociability,
            PersonalityAxis::Conscience => &mut self.conscience,
            PersonalityAxis::SelfRegard => &mut self.self_regard,
            PersonalityAxis::Conviction => &mut self.conviction,
            PersonalityAxis::Hygiene => &mut self.hygiene,
            PersonalityAxis::Temperance => &mut self.temperance,
            PersonalityAxis::Mirth => &mut self.mirth,
            PersonalityAxis::Courtship => &mut self.courtship,
            PersonalityAxis::Transparency => &mut self.transparency,
            PersonalityAxis::SelfKnowledge => &mut self.self_knowledge,
        };
        *target = value.clamp(-PERSONALITY_SCORE_LIMIT, PERSONALITY_SCORE_LIMIT);
    }

    pub fn from_visible(value: &CharacterPersonality) -> Self {
        let mut scores = Self::neutral(value.character_id);
        scores.nerve = match value.nerve {
            Nerve::Brave => PERSONALITY_SCORE_LIMIT,
            Nerve::Fearful => -PERSONALITY_SCORE_LIMIT,
            Nerve::Neutral => 0,
        };
        scores.drive = match value.drive {
            Drive::Ambitious => PERSONALITY_SCORE_LIMIT,
            Drive::Content => -PERSONALITY_SCORE_LIMIT,
            Drive::Neutral => 0,
        };
        scores.outlook = match value.outlook {
            Outlook::Sanguine => PERSONALITY_SCORE_LIMIT,
            Outlook::Brooding => -PERSONALITY_SCORE_LIMIT,
            Outlook::Neutral => 0,
        };
        scores.sociability = match value.sociability {
            Sociability::Gregarious => PERSONALITY_SCORE_LIMIT,
            Sociability::Solitary => -PERSONALITY_SCORE_LIMIT,
            Sociability::Neutral => 0,
        };
        scores.conscience = match value.conscience {
            Conscience::Compassionate => PERSONALITY_SCORE_LIMIT,
            Conscience::Callous => -PERSONALITY_VISIBLE_THRESHOLD,
            Conscience::Cruel => -PERSONALITY_SCORE_LIMIT,
            Conscience::Neutral => 0,
        };
        scores.self_regard = match value.self_regard {
            SelfRegard::Proud => PERSONALITY_SCORE_LIMIT,
            SelfRegard::Humble => -PERSONALITY_SCORE_LIMIT,
            SelfRegard::Neutral => 0,
        };
        scores.conviction = match value.conviction {
            Conviction::Zealous => PERSONALITY_SCORE_LIMIT,
            Conviction::Irreverent => -PERSONALITY_SCORE_LIMIT,
            Conviction::Neutral => 0,
        };
        scores.hygiene = match value.hygiene {
            Hygiene::Cleanly => PERSONALITY_SCORE_LIMIT,
            Hygiene::Slovenly => -PERSONALITY_SCORE_LIMIT,
            Hygiene::Neutral => 0,
        };
        scores.temperance = match value.temperance {
            Temperance::Temperate => PERSONALITY_SCORE_LIMIT,
            Temperance::Drunkard => -PERSONALITY_SCORE_LIMIT,
            Temperance::Neutral => 0,
        };
        scores.mirth = match value.mirth {
            Mirth::Merry => PERSONALITY_SCORE_LIMIT,
            Mirth::Grave => -PERSONALITY_SCORE_LIMIT,
            Mirth::Neutral => 0,
        };
        scores.courtship = match value.courtship {
            Courtship::Amorous => PERSONALITY_SCORE_LIMIT,
            Courtship::Proper => -PERSONALITY_SCORE_LIMIT,
            Courtship::Neutral => 0,
        };
        scores.transparency = match value.transparency {
            Transparency::Open => PERSONALITY_SCORE_LIMIT,
            Transparency::Guarded => -PERSONALITY_SCORE_LIMIT,
            Transparency::Neutral => 0,
        };
        scores.self_knowledge = match value.self_knowledge {
            SelfKnowledge::Introspective => PERSONALITY_SCORE_LIMIT,
            SelfKnowledge::SelfDeceiving => -PERSONALITY_SCORE_LIMIT,
            SelfKnowledge::Neutral => 0,
        };
        scores
    }
}

/// Fail-closed truth projection for trusted SSR. Browser subscriptions never
/// receive authoritative personality or demographic rows.
#[view(accessor = backend_character_personalities, public)]
pub fn backend_character_personalities(ctx: &ViewContext) -> Vec<CharacterPersonality> {
    let trusted = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    if !trusted {
        return Vec::new();
    }
    ctx.db
        .character_personality()
        .projection_character_id()
        .filter(0u64..)
        .collect()
}

impl CharacterPersonality {
    pub fn neutral(character_id: u64) -> Self {
        Self {
            character_id,
            projection_character_id: character_id,
            nerve: Nerve::Neutral,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Neutral,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance: Temperance::Neutral,
            mirth: Mirth::Neutral,
            courtship: Courtship::Neutral,
            transparency: Transparency::Neutral,
            self_knowledge: SelfKnowledge::Neutral,
            sex: Sex::Male,
            presentation: Presentation::Man,
            inclination: Inclination::Women,
        }
    }

    pub fn non_neutral_count(&self) -> usize {
        usize::from(self.nerve != Nerve::Neutral)
            + usize::from(self.drive != Drive::Neutral)
            + usize::from(self.outlook != Outlook::Neutral)
            + usize::from(self.sociability != Sociability::Neutral)
            + usize::from(self.conscience != Conscience::Neutral)
            + usize::from(self.self_regard != SelfRegard::Neutral)
            + usize::from(self.conviction != Conviction::Neutral)
            + usize::from(self.hygiene != Hygiene::Neutral)
            + usize::from(self.temperance != Temperance::Neutral)
            + usize::from(self.mirth != Mirth::Neutral)
            + usize::from(self.courtship != Courtship::Neutral)
            + usize::from(self.transparency != Transparency::Neutral)
            + usize::from(self.self_knowledge != SelfKnowledge::Neutral)
    }
}

fn bipolar<T: Copy>(score: i16, positive: T, neutral: T, negative: T) -> T {
    if score >= PERSONALITY_VISIBLE_THRESHOLD {
        positive
    } else if score <= -PERSONALITY_VISIBLE_THRESHOLD {
        negative
    } else {
        neutral
    }
}

pub fn project_scores(
    scores: &CharacterPersonalityScores,
    demographics: &CharacterPersonality,
) -> CharacterPersonality {
    CharacterPersonality {
        character_id: scores.character_id,
        projection_character_id: scores.character_id,
        nerve: bipolar(scores.nerve, Nerve::Brave, Nerve::Neutral, Nerve::Fearful),
        drive: bipolar(
            scores.drive,
            Drive::Ambitious,
            Drive::Neutral,
            Drive::Content,
        ),
        outlook: bipolar(
            scores.outlook,
            Outlook::Sanguine,
            Outlook::Neutral,
            Outlook::Brooding,
        ),
        sociability: bipolar(
            scores.sociability,
            Sociability::Gregarious,
            Sociability::Neutral,
            Sociability::Solitary,
        ),
        conscience: if scores.conscience >= PERSONALITY_VISIBLE_THRESHOLD {
            Conscience::Compassionate
        } else if scores.conscience <= CONSCIENCE_CRUEL_THRESHOLD {
            Conscience::Cruel
        } else if scores.conscience <= -PERSONALITY_VISIBLE_THRESHOLD {
            Conscience::Callous
        } else {
            Conscience::Neutral
        },
        self_regard: bipolar(
            scores.self_regard,
            SelfRegard::Proud,
            SelfRegard::Neutral,
            SelfRegard::Humble,
        ),
        conviction: bipolar(
            scores.conviction,
            Conviction::Zealous,
            Conviction::Neutral,
            Conviction::Irreverent,
        ),
        hygiene: bipolar(
            scores.hygiene,
            Hygiene::Cleanly,
            Hygiene::Neutral,
            Hygiene::Slovenly,
        ),
        temperance: bipolar(
            scores.temperance,
            Temperance::Temperate,
            Temperance::Neutral,
            Temperance::Drunkard,
        ),
        mirth: bipolar(scores.mirth, Mirth::Merry, Mirth::Neutral, Mirth::Grave),
        courtship: bipolar(
            scores.courtship,
            Courtship::Amorous,
            Courtship::Neutral,
            Courtship::Proper,
        ),
        transparency: bipolar(
            scores.transparency,
            Transparency::Open,
            Transparency::Neutral,
            Transparency::Guarded,
        ),
        self_knowledge: bipolar(
            scores.self_knowledge,
            SelfKnowledge::Introspective,
            SelfKnowledge::Neutral,
            SelfKnowledge::SelfDeceiving,
        ),
        sex: demographics.sex,
        presentation: demographics.presentation,
        inclination: demographics.inclination,
    }
}

fn write_projection(ctx: &ReducerContext, visible: CharacterPersonality) {
    if ctx
        .db
        .character_personality()
        .character_id()
        .find(visible.character_id)
        .is_some()
    {
        ctx.db
            .character_personality()
            .character_id()
            .update(visible);
    } else {
        ctx.db.character_personality().insert(visible);
    }
}

/// Initialize authority from an external discrete profile. If authority is
/// already present, its continuous scores win and only demographics are
/// refreshed. This prevents a lossy projection from erasing hidden progress.
pub fn initialize_personality_from_visible(ctx: &ReducerContext, visible: CharacterPersonality) {
    let scores = if let Some(existing) = ctx
        .db
        .character_personality_scores()
        .character_id()
        .find(visible.character_id)
    {
        existing
    } else {
        ctx.db
            .character_personality_scores()
            .insert(CharacterPersonalityScores::from_visible(&visible))
    };
    write_projection(ctx, project_scores(&scores, &visible));
}

/// Deliberately replace all behavioral scores from a discrete fixture/import.
/// Ordinary personality updates must use a narrow score or demographic helper.
pub fn reset_personality_from_visible(ctx: &ReducerContext, visible: CharacterPersonality) {
    let scores = CharacterPersonalityScores::from_visible(&visible);
    if ctx
        .db
        .character_personality_scores()
        .character_id()
        .find(visible.character_id)
        .is_some()
    {
        ctx.db
            .character_personality_scores()
            .character_id()
            .update(scores.clone());
    } else {
        ctx.db.character_personality_scores().insert(scores.clone());
    }
    write_projection(ctx, project_scores(&scores, &visible));
}

pub fn set_personality_axis_score(
    ctx: &ReducerContext,
    character_id: u64,
    axis: PersonalityAxis,
    score: i16,
) -> Result<(), String> {
    let mut scores = ctx
        .db
        .character_personality_scores()
        .character_id()
        .find(character_id)
        .ok_or("Character personality scores not found")?;
    scores.set_score(axis, score);
    ctx.db
        .character_personality_scores()
        .character_id()
        .update(scores.clone());
    let demographics = personality_or_neutral(ctx, character_id);
    write_projection(ctx, project_scores(&scores, &demographics));
    Ok(())
}

pub fn update_personality_demographics(
    ctx: &ReducerContext,
    character_id: u64,
    sex: Sex,
    presentation: Presentation,
    inclination: Inclination,
) -> Result<(), String> {
    let scores = ctx
        .db
        .character_personality_scores()
        .character_id()
        .find(character_id)
        .ok_or("Character personality scores not found")?;
    let mut demographics = personality_or_neutral(ctx, character_id);
    demographics.sex = sex;
    demographics.presentation = presentation;
    demographics.inclination = inclination;
    write_projection(ctx, project_scores(&scores, &demographics));
    Ok(())
}

pub fn personality_scores_or_neutral(
    ctx: &ReducerContext,
    character_id: u64,
) -> CharacterPersonalityScores {
    ctx.db
        .character_personality_scores()
        .character_id()
        .find(character_id)
        .unwrap_or_else(|| CharacterPersonalityScores::neutral(character_id))
}

/// Apply one authoritative deed. The caller supplies a source identity bound
/// to durable gameplay truth, never a free-standing client mutation token.
pub fn apply_personality_development(
    ctx: &ReducerContext,
    source_id: &str,
    character_id: u64,
    axis: PersonalityAxis,
    delta: i16,
    deed: &str,
    virtue: ChivalricVirtue,
    occurred_at_minute: u64,
) -> Result<(), String> {
    if let Some(existing) = ctx
        .db
        .personality_development_event()
        .source_id()
        .find(source_id.to_string())
    {
        return if development_replay_matches(&existing, character_id, axis, delta, deed, virtue) {
            Ok(())
        } else {
            Err("Conflicting personality development source".into())
        };
    }
    let mut scores = ctx
        .db
        .character_personality_scores()
        .character_id()
        .find(character_id)
        .ok_or("Character personality scores not found")?;
    let resulting = scores
        .score(axis)
        .saturating_add(delta)
        .clamp(-PERSONALITY_SCORE_LIMIT, PERSONALITY_SCORE_LIMIT);
    scores.set_score(axis, resulting);
    ctx.db
        .character_personality_scores()
        .character_id()
        .update(scores.clone());
    let demographics = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
        .ok_or("Character personality projection not found")?;
    ctx.db
        .character_personality()
        .character_id()
        .update(project_scores(&scores, &demographics));
    ctx.db
        .personality_development_event()
        .insert(PersonalityDevelopmentEvent {
            source_id: source_id.into(),
            character_id,
            axis,
            delta,
            resulting_score: resulting,
            deed: deed.into(),
            virtue,
            occurred_at_minute,
        });
    Ok(())
}

fn development_replay_matches(
    existing: &PersonalityDevelopmentEvent,
    character_id: u64,
    axis: PersonalityAxis,
    delta: i16,
    deed: &str,
    virtue: ChivalricVirtue,
) -> bool {
    existing.character_id == character_id
        && existing.axis == axis
        && existing.delta == delta
        && existing.deed == deed
        && existing.virtue == virtue
}

impl Conviction {
    /// Ardor is a personality property, independent of religious knowledge.
    pub const fn strength(self) -> f32 {
        match self {
            Self::Zealous => 5.0,
            Self::Neutral => 2.5,
            Self::Irreverent => 0.0,
        }
    }
}

pub fn conviction_strength_for_character(ctx: &ReducerContext, character_id: u64) -> f32 {
    2.5 + 2.5 * f32::from(personality_scores_or_neutral(ctx, character_id).conviction)
        / f32::from(PERSONALITY_SCORE_LIMIT)
}

/// Generate a sparse profile with exactly two through four distinct axes.
pub fn random_personality(
    character_id: u64,
    mut random: impl FnMut() -> u64,
) -> CharacterPersonality {
    let mut result = CharacterPersonality::neutral(character_id);
    let mut axes = [0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    for index in (1..axes.len()).rev() {
        axes.swap(index, random() as usize % (index + 1));
    }
    let count = 2 + random() as usize % 3;
    for axis in axes.into_iter().take(count) {
        match axis {
            0 => {
                result.nerve = if random() % 2 == 0 {
                    Nerve::Brave
                } else {
                    Nerve::Fearful
                }
            }
            1 => {
                result.drive = if random() % 2 == 0 {
                    Drive::Ambitious
                } else {
                    Drive::Content
                }
            }
            2 => {
                result.outlook = if random() % 2 == 0 {
                    Outlook::Sanguine
                } else {
                    Outlook::Brooding
                }
            }
            3 => {
                result.sociability = if random() % 2 == 0 {
                    Sociability::Gregarious
                } else {
                    Sociability::Solitary
                }
            }
            4 => {
                result.conscience = match random() % 3 {
                    0 => Conscience::Compassionate,
                    1 => Conscience::Callous,
                    _ => Conscience::Cruel,
                }
            }
            5 => {
                result.self_regard = if random() % 2 == 0 {
                    SelfRegard::Proud
                } else {
                    SelfRegard::Humble
                }
            }
            6 => {
                result.conviction = if random() % 2 == 0 {
                    Conviction::Zealous
                } else {
                    Conviction::Irreverent
                }
            }
            7 => {
                result.hygiene = if random() % 2 == 0 {
                    Hygiene::Slovenly
                } else {
                    Hygiene::Cleanly
                }
            }
            8 => {
                result.temperance = if random() % 2 == 0 {
                    Temperance::Temperate
                } else {
                    Temperance::Drunkard
                }
            }
            9 => {
                result.mirth = if random() % 2 == 0 {
                    Mirth::Merry
                } else {
                    Mirth::Grave
                }
            }
            10 => {
                result.courtship = if random() % 2 == 0 {
                    Courtship::Amorous
                } else {
                    Courtship::Proper
                }
            }
            11 => {
                result.transparency = if random() % 2 == 0 {
                    Transparency::Open
                } else {
                    Transparency::Guarded
                }
            }
            _ => {
                result.self_knowledge = if random() % 2 == 0 {
                    SelfKnowledge::Introspective
                } else {
                    SelfKnowledge::SelfDeceiving
                }
            }
        }
    }
    result.sex = if random() % 2 == 0 {
        Sex::Female
    } else {
        Sex::Male
    };
    let presentation_roll = random() % 100;
    result.presentation = match (result.sex, presentation_roll) {
        (_, 0..=3) => Presentation::Ambiguous,
        (Sex::Female, 4) => Presentation::Man,
        (Sex::Male, 4) => Presentation::Woman,
        (Sex::Female, _) => Presentation::Woman,
        (Sex::Male, _) => Presentation::Man,
    };
    // Direction is generated from demographic sex rather than the public
    // presentation signal. The signal is the only field attraction consumes.
    result.inclination = match random() % 100 {
        0 => Inclination::Neither,
        1..=4 => Inclination::Either,
        5..=9 => match result.sex {
            Sex::Female => Inclination::Women,
            Sex::Male => Inclination::Men,
        },
        _ => match result.sex {
            Sex::Female => Inclination::Men,
            Sex::Male => Inclination::Women,
        },
    };
    result
}

/// Generate an NPC personality from an identity-stable seed.
///
/// Reducer RNG is intentionally not involved. The same NPC therefore receives
/// the same demographic axes and sparse traits regardless of bootstrap order,
/// retries, or unrelated random draws in the surrounding transaction.
pub fn personality_from_stable_seed(character_id: u64, stable_seed: u64) -> CharacterPersonality {
    let mut state = stable_seed ^ character_id.rotate_left(29) ^ 0x7065_7273_6f6e_616c;
    random_personality(character_id, || {
        // SplitMix64 is small, deterministic, and has no ambient/global state.
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    })
}

pub fn personality_or_neutral(ctx: &ReducerContext, character_id: u64) -> CharacterPersonality {
    ctx.db
        .character_personality()
        .character_id()
        .find(character_id)
        .unwrap_or_else(|| CharacterPersonality::neutral(character_id))
}

pub fn initialize_npc_personality(ctx: &ReducerContext, character_id: u64, stable_seed: u64) {
    if ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
        .is_none()
    {
        initialize_personality_from_visible(
            ctx,
            personality_from_stable_seed(character_id, stable_seed),
        );
    }
}

pub fn initialize_personality(ctx: &ReducerContext, character_id: u64, npc: bool) {
    if ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
        .is_none()
    {
        let row = if npc {
            personality_from_stable_seed(character_id, character_id)
        } else {
            // Non-candidate characters remain behaviorally neutral, but the
            // always-assigned demographic axes must still have real values.
            let generated = random_personality(character_id, || ctx.random());
            let mut neutral = CharacterPersonality::neutral(character_id);
            neutral.sex = generated.sex;
            neutral.presentation = generated.presentation;
            neutral.inclination = generated.inclination;
            neutral
        };
        initialize_personality_from_visible(ctx, row);
    }
}

pub fn assign_random_personality(ctx: &ReducerContext, character_id: u64) {
    let row = random_personality(character_id, || ctx.random());
    if ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
        .is_some()
    {
        reset_personality_from_visible(ctx, row);
    } else {
        reset_personality_from_visible(ctx, row);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoraleStimulus {
    Threat,
    Victory,
    Defeat,
    Religious,
    Other,
}

/// Classify durable event kinds once so religious events cannot silently miss
/// Conviction merely because their label does not contain "religious".
pub fn morale_event_stimulus(kind: &str) -> MoraleStimulus {
    match kind {
        "victory" => MoraleStimulus::Victory,
        "defeat" => MoraleStimulus::Defeat,
        "holy_day_observed" | "religious_observance_neglected" | "travel_prayer_neglected" => {
            MoraleStimulus::Religious
        }
        kind if kind.contains("religious") || kind.contains("prayer") => MoraleStimulus::Religious,
        _ => MoraleStimulus::Other,
    }
}

/// Apply a character's semantic reaction to one raw stimulus. Returned names
/// are suitable for appending to the existing source label.
pub fn react_raw(
    personality: &CharacterPersonality,
    stimulus: MoraleStimulus,
    magnitude: f32,
) -> (f32, Vec<&'static str>) {
    react_raw_with_scores(
        personality,
        &CharacterPersonalityScores::from_visible(personality),
        stimulus,
        magnitude,
    )
}

pub fn continuous_axis_multiplier(
    score: i16,
    positive_endpoint: f32,
    negative_endpoint: f32,
) -> f32 {
    let bounded = score.clamp(-PERSONALITY_SCORE_LIMIT, PERSONALITY_SCORE_LIMIT);
    let ratio = f32::from(bounded.unsigned_abs()) / f32::from(PERSONALITY_SCORE_LIMIT as u16);
    let endpoint = if bounded >= 0 {
        positive_endpoint
    } else {
        negative_endpoint
    };
    1.0 + (endpoint - 1.0) * ratio
}

/// Scale one nightly alcohol reaction from neutral preference at score zero
/// to the legacy Temperate (zero) or Drunkard (+/-5) endpoint. Seeking and
/// consumption remain governed by the derived discrete preference.
pub fn temperance_morale_magnitude(score: i16, neutral_magnitude: f32, satisfied: bool) -> f32 {
    let bounded = score.clamp(-PERSONALITY_SCORE_LIMIT, PERSONALITY_SCORE_LIMIT);
    let ratio = f32::from(bounded.unsigned_abs()) / f32::from(PERSONALITY_SCORE_LIMIT as u16);
    let endpoint = if bounded >= 0 {
        0.0
    } else if satisfied {
        5.0
    } else {
        -5.0
    };
    neutral_magnitude + (endpoint - neutral_magnitude) * ratio
}

pub fn react_raw_with_scores(
    personality: &CharacterPersonality,
    scores: &CharacterPersonalityScores,
    stimulus: MoraleStimulus,
    mut magnitude: f32,
) -> (f32, Vec<&'static str>) {
    let mut names = Vec::new();
    if stimulus == MoraleStimulus::Threat && magnitude < 0.0 {
        magnitude *= continuous_axis_multiplier(scores.nerve, 0.5, 2.0);
        match personality.nerve {
            Nerve::Brave => names.push("Brave"),
            Nerve::Fearful => names.push("Fearful"),
            Nerve::Neutral => {}
        }
    }
    if matches!(stimulus, MoraleStimulus::Victory | MoraleStimulus::Defeat) {
        magnitude *= continuous_axis_multiplier(scores.drive, 1.5, 0.5);
        match personality.drive {
            Drive::Ambitious => names.push("Ambitious"),
            Drive::Content => names.push("Content"),
            Drive::Neutral => {}
        }
        let proud_endpoint = if stimulus == MoraleStimulus::Victory {
            1.5
        } else {
            3.0
        };
        magnitude *= continuous_axis_multiplier(scores.self_regard, proud_endpoint, 0.75);
        match personality.self_regard {
            SelfRegard::Proud => names.push("Proud"),
            SelfRegard::Humble => names.push("Humble"),
            SelfRegard::Neutral => {}
        }
    }
    if stimulus == MoraleStimulus::Religious {
        magnitude *= continuous_axis_multiplier(scores.conviction, 1.5, 0.5);
        match personality.conviction {
            Conviction::Zealous => names.push("Zealous"),
            Conviction::Irreverent => names.push("Irreverent"),
            Conviction::Neutral => {}
        }
    }
    let outlook_multiplier = if magnitude > 0.0 {
        continuous_axis_multiplier(scores.outlook, 1.25, 0.75)
    } else {
        continuous_axis_multiplier(scores.outlook, 0.75, 1.25)
    };
    magnitude *= outlook_multiplier;
    match personality.outlook {
        Outlook::Sanguine => names.push("Sanguine"),
        Outlook::Brooding => names.push("Brooding"),
        Outlook::Neutral => {}
    }
    (magnitude, names)
}

pub fn react_raw_for_character(
    ctx: &ReducerContext,
    character_id: u64,
    stimulus: MoraleStimulus,
    magnitude: f32,
) -> (f32, Vec<&'static str>) {
    react_raw_with_scores(
        &personality_or_neutral(ctx, character_id),
        &personality_scores_or_neutral(ctx, character_id),
        stimulus,
        magnitude,
    )
}

pub fn negative_event_duration(personality: &CharacterPersonality, duration: u64) -> u64 {
    match personality.outlook {
        Outlook::Sanguine => duration / 2,
        Outlook::Brooding => duration.saturating_mul(2),
        Outlook::Neutral => duration,
    }
}

pub fn negative_event_duration_for_character(
    ctx: &ReducerContext,
    character_id: u64,
    duration: u64,
) -> u64 {
    let score = personality_scores_or_neutral(ctx, character_id).outlook;
    negative_event_duration_with_score(score, duration)
}

pub fn negative_event_duration_with_score(score: i16, duration: u64) -> u64 {
    let bounded = score.clamp(-PERSONALITY_SCORE_LIMIT, PERSONALITY_SCORE_LIMIT);
    if bounded >= 0 {
        let endpoint = duration / 2;
        let total_reduction = duration - endpoint;
        let reduction = (u128::from(total_reduction) * u128::from(bounded as u16)
            / u128::from(PERSONALITY_SCORE_LIMIT as u16)) as u64;
        duration - reduction
    } else {
        let endpoint = duration.saturating_mul(2);
        let total_increase = endpoint - duration;
        let increase = (u128::from(total_increase) * u128::from(bounded.unsigned_abs())
            / u128::from(PERSONALITY_SCORE_LIMIT as u16)) as u64;
        duration.saturating_add(increase)
    }
}

pub fn ally_restoration_multiplier(
    personality: &CharacterPersonality,
) -> (f32, Option<&'static str>) {
    match personality.sociability {
        Sociability::Gregarious => (1.5, Some("Gregarious")),
        Sociability::Solitary => (0.5, Some("Solitary")),
        Sociability::Neutral => (1.0, None),
    }
}

pub fn ally_restoration_multiplier_for_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> (f32, Option<&'static str>) {
    let visible = personality_or_neutral(ctx, character_id);
    let multiplier = continuous_axis_multiplier(
        personality_scores_or_neutral(ctx, character_id).sociability,
        1.5,
        0.5,
    );
    let name = match visible.sociability {
        Sociability::Gregarious => Some("Gregarious"),
        Sociability::Solitary => Some("Solitary"),
        Sociability::Neutral => None,
    };
    (multiplier, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_profiles_are_sparse_and_axes_are_mutually_exclusive() {
        for seed in 0..100_u64 {
            let mut value = seed;
            let personality = random_personality(seed, || {
                value = value.wrapping_mul(6364136223846793005).wrapping_add(1);
                value
            });
            assert!((2..=4).contains(&personality.non_neutral_count()));
            let scores = CharacterPersonalityScores::from_visible(&personality);
            let projected = project_scores(&scores, &personality);
            assert_eq!(
                projected.non_neutral_count(),
                personality.non_neutral_count()
            );
        }
    }

    #[test]
    fn stable_npc_profiles_ignore_generation_order_and_ambient_randomness() {
        let first = personality_from_stable_seed(77, 0xabc);
        let _unrelated = personality_from_stable_seed(91, 0xdef);
        let repeated = personality_from_stable_seed(77, 0xabc);
        assert_eq!(first.character_id, repeated.character_id);
        assert_eq!(first.nerve, repeated.nerve);
        assert_eq!(first.drive, repeated.drive);
        assert_eq!(first.outlook, repeated.outlook);
        assert_eq!(first.sociability, repeated.sociability);
        assert_eq!(first.conscience, repeated.conscience);
        assert_eq!(first.self_regard, repeated.self_regard);
        assert_eq!(first.conviction, repeated.conviction);
        assert_eq!(first.hygiene, repeated.hygiene);
        assert_eq!(first.temperance, repeated.temperance);
        assert_eq!(first.mirth, repeated.mirth);
        assert_eq!(first.courtship, repeated.courtship);
        assert_eq!(first.transparency, repeated.transparency);
        assert_eq!(first.self_knowledge, repeated.self_knowledge);
        assert_eq!(first.sex, repeated.sex);
        assert_eq!(first.presentation, repeated.presentation);
        assert_eq!(first.inclination, repeated.inclination);
        assert_eq!(first.non_neutral_count(), repeated.non_neutral_count());
    }

    #[test]
    fn neutral_profile_has_no_visible_axes() {
        assert_eq!(CharacterPersonality::neutral(1).non_neutral_count(), 0);
    }

    #[test]
    fn conviction_strength_is_independent_from_religious_knowledge() {
        assert_eq!(Conviction::Zealous.strength(), 5.0);
        assert_eq!(Conviction::Neutral.strength(), 2.5);
        assert_eq!(Conviction::Irreverent.strength(), 0.0);
    }

    #[test]
    fn every_active_axis_modifies_only_its_semantic_hook() {
        let mut p = CharacterPersonality::neutral(1);
        p.nerve = Nerve::Brave;
        assert_eq!(react_raw(&p, MoraleStimulus::Threat, -10.0).0, -5.0);
        p.nerve = Nerve::Fearful;
        assert_eq!(react_raw(&p, MoraleStimulus::Threat, -10.0).0, -20.0);
        p.nerve = Nerve::Neutral;
        p.drive = Drive::Ambitious;
        assert_eq!(react_raw(&p, MoraleStimulus::Victory, 8.0).0, 12.0);
        p.drive = Drive::Content;
        assert_eq!(react_raw(&p, MoraleStimulus::Defeat, -8.0).0, -4.0);
        p.drive = Drive::Neutral;
        p.self_regard = SelfRegard::Proud;
        assert_eq!(react_raw(&p, MoraleStimulus::Victory, 8.0).0, 12.0);
        assert_eq!(react_raw(&p, MoraleStimulus::Defeat, -8.0).0, -24.0);
        p.self_regard = SelfRegard::Humble;
        assert_eq!(react_raw(&p, MoraleStimulus::Victory, 8.0).0, 6.0);
        p.self_regard = SelfRegard::Neutral;
        p.conviction = Conviction::Zealous;
        assert_eq!(react_raw(&p, MoraleStimulus::Religious, 8.0).0, 12.0);
        p.conviction = Conviction::Irreverent;
        assert_eq!(react_raw(&p, MoraleStimulus::Religious, -8.0).0, -4.0);
        p.conviction = Conviction::Neutral;
        p.outlook = Outlook::Sanguine;
        assert_eq!(react_raw(&p, MoraleStimulus::Other, 8.0).0, 10.0);
        assert_eq!(react_raw(&p, MoraleStimulus::Other, -8.0).0, -6.0);
        p.outlook = Outlook::Brooding;
        assert_eq!(react_raw(&p, MoraleStimulus::Other, 8.0).0, 6.0);
        assert_eq!(react_raw(&p, MoraleStimulus::Other, -8.0).0, -10.0);
    }

    #[test]
    fn multipliers_compose_and_event_memory_changes() {
        let mut p = CharacterPersonality::neutral(1);
        p.drive = Drive::Ambitious;
        p.self_regard = SelfRegard::Proud;
        p.outlook = Outlook::Brooding;
        assert_eq!(react_raw(&p, MoraleStimulus::Defeat, -8.0).0, -45.0);
        assert_eq!(negative_event_duration(&p, 7), 14);
        p.outlook = Outlook::Sanguine;
        assert_eq!(negative_event_duration(&p, 8), 4);
    }

    #[test]
    fn sociability_is_separate_from_command_and_caps_can_apply_after_it() {
        let mut p = CharacterPersonality::neutral(1);
        p.sociability = Sociability::Gregarious;
        let (multiplier, _) = ally_restoration_multiplier(&p);
        assert_eq!((8.0_f32 * multiplier).min(10.0), 10.0);
        p.sociability = Sociability::Solitary;
        assert_eq!(ally_restoration_multiplier(&p).0, 0.5);
    }

    #[test]
    fn observed_holy_days_receive_conviction_reactions_and_annotations() {
        let mut p = CharacterPersonality::neutral(1);
        p.conviction = Conviction::Zealous;
        let stimulus = morale_event_stimulus("holy_day_observed");
        assert_eq!(stimulus, MoraleStimulus::Religious);
        let (magnitude, annotations) = react_raw(&p, stimulus, 2.0);
        assert_eq!(magnitude, 3.0);
        assert_eq!(annotations, ["Zealous"]);
    }

    #[test]
    fn scores_clamp_and_project_only_after_visibility_thresholds() {
        let demographics = CharacterPersonality::neutral(7);
        let mut scores = CharacterPersonalityScores::neutral(7);
        scores.set_score(PersonalityAxis::Nerve, 25_000);
        assert_eq!(scores.nerve, PERSONALITY_SCORE_LIMIT);
        assert_eq!(project_scores(&scores, &demographics).nerve, Nerve::Brave);
        scores.set_score(PersonalityAxis::Nerve, -4_999);
        assert_eq!(project_scores(&scores, &demographics).nerve, Nerve::Neutral);
        scores.set_score(PersonalityAxis::Nerve, -5_000);
        assert_eq!(project_scores(&scores, &demographics).nerve, Nerve::Fearful);
    }

    #[test]
    fn conscience_has_compassionate_callous_and_cruel_bands() {
        let demographics = CharacterPersonality::neutral(8);
        let mut scores = CharacterPersonalityScores::neutral(8);
        for (score, expected) in [
            (4_999, Conscience::Neutral),
            (5_000, Conscience::Compassionate),
            (-4_999, Conscience::Neutral),
            (-5_000, Conscience::Callous),
            (-7_999, Conscience::Callous),
            (-8_000, Conscience::Cruel),
        ] {
            scores.set_score(PersonalityAxis::Conscience, score);
            assert_eq!(project_scores(&scores, &demographics).conscience, expected);
        }
    }

    #[test]
    fn continuous_multipliers_are_monotonic_bounded_and_preserve_endpoints() {
        assert_eq!(continuous_axis_multiplier(0, 0.5, 2.0), 1.0);
        assert_eq!(
            continuous_axis_multiplier(PERSONALITY_SCORE_LIMIT, 0.5, 2.0),
            0.5
        );
        assert_eq!(
            continuous_axis_multiplier(-PERSONALITY_SCORE_LIMIT, 0.5, 2.0),
            2.0
        );
        let quarter = continuous_axis_multiplier(2_500, 0.5, 2.0);
        let half = continuous_axis_multiplier(5_000, 0.5, 2.0);
        assert!(quarter > half && half > 0.5);
        assert_eq!(continuous_axis_multiplier(30_000, 0.5, 2.0), 0.5);
    }

    #[test]
    fn demographic_projection_preserves_hidden_unrelated_scores_and_forces_key() {
        let mut scores = CharacterPersonalityScores::neutral(42);
        scores.nerve = 3_750;
        scores.conviction = -2_250;
        let mut demographics = CharacterPersonality::neutral(999);
        demographics.sex = Sex::Female;
        demographics.presentation = Presentation::Woman;
        demographics.inclination = Inclination::Either;
        let projected = project_scores(&scores, &demographics);
        assert_eq!((scores.nerve, scores.conviction), (3_750, -2_250));
        assert_eq!(
            (projected.character_id, projected.projection_character_id),
            (42, 42)
        );
        assert_eq!(
            (projected.nerve, projected.conviction),
            (Nerve::Neutral, Conviction::Neutral)
        );
        assert_eq!(
            (projected.sex, projected.presentation, projected.inclination),
            (Sex::Female, Presentation::Woman, Inclination::Either)
        );
    }

    #[test]
    fn temperance_morale_moves_continuously_and_preserves_legacy_endpoints() {
        assert_eq!(temperance_morale_magnitude(0, 1.0, true), 1.0);
        assert_eq!(temperance_morale_magnitude(2_500, 1.0, true), 0.75);
        assert_eq!(temperance_morale_magnitude(-2_500, 1.0, true), 2.0);
        assert_eq!(temperance_morale_magnitude(2_500, -1.0, false), -0.75);
        assert_eq!(temperance_morale_magnitude(-2_500, -1.0, false), -2.0);
        assert_eq!(
            temperance_morale_magnitude(PERSONALITY_SCORE_LIMIT, 3.0, true),
            0.0
        );
        assert_eq!(
            temperance_morale_magnitude(-PERSONALITY_SCORE_LIMIT, 1.0, true),
            5.0
        );
        assert_eq!(
            temperance_morale_magnitude(-PERSONALITY_SCORE_LIMIT, -1.0, false),
            -5.0
        );
    }

    #[test]
    fn continuous_event_duration_preserves_integer_endpoints_and_saturates() {
        assert_eq!(
            negative_event_duration_with_score(PERSONALITY_SCORE_LIMIT, 7),
            3
        );
        assert_eq!(
            negative_event_duration_with_score(-PERSONALITY_SCORE_LIMIT, 7),
            14
        );
        assert_eq!(negative_event_duration_with_score(0, 7), 7);
        assert_eq!(negative_event_duration_with_score(2_500, 7), 6);
        assert_eq!(negative_event_duration_with_score(5_000, 7), 5);
        assert_eq!(negative_event_duration_with_score(-2_500, 7), 8);
        assert_eq!(negative_event_duration_with_score(-5_000, 7), 10);
        assert_eq!(
            negative_event_duration_with_score(-PERSONALITY_SCORE_LIMIT, u64::MAX),
            u64::MAX
        );
    }

    #[test]
    fn score_and_development_tables_have_no_public_views_and_delete_with_character() {
        let personality_source = include_str!("personality.rs");
        assert!(!personality_source.contains("#[view(accessor = character_personality_scores"));
        assert!(!personality_source.contains("#[view(accessor = personality_development_event"));
        let deletion = include_str!("character.rs");
        assert!(deletion.contains("character_personality_scores()"));
        assert!(deletion.contains("personality_development_event()"));
        assert!(deletion.contains(".character_id()\n        .filter(character.id)"));
    }

    #[test]
    fn hidden_subthreshold_scores_already_change_morale_without_leaking_annotation() {
        let visible = CharacterPersonality::neutral(9);
        let mut scores = CharacterPersonalityScores::neutral(9);
        scores.nerve = 2_500;
        let (magnitude, annotations) =
            react_raw_with_scores(&visible, &scores, MoraleStimulus::Threat, -8.0);
        assert_eq!(magnitude, -7.0);
        assert!(annotations.is_empty());
    }

    #[test]
    fn development_source_accepts_exact_replay_and_rejects_conflicts() {
        let event = PersonalityDevelopmentEvent {
            source_id: "road-challenge:one".into(),
            character_id: 9,
            axis: PersonalityAxis::Nerve,
            delta: CHIVALRIC_DEED_DELTA,
            resulting_score: CHIVALRIC_DEED_DELTA,
            deed: "RallyAndEscortCourierThroughFord".into(),
            virtue: ChivalricVirtue::Courage,
            occurred_at_minute: 60,
        };
        assert!(development_replay_matches(
            &event,
            9,
            PersonalityAxis::Nerve,
            CHIVALRIC_DEED_DELTA,
            "RallyAndEscortCourierThroughFord",
            ChivalricVirtue::Courage,
        ));
        assert!(!development_replay_matches(
            &event,
            10,
            PersonalityAxis::Nerve,
            CHIVALRIC_DEED_DELTA,
            "RallyAndEscortCourierThroughFord",
            ChivalricVirtue::Courage,
        ));
        assert!(!development_replay_matches(
            &event,
            9,
            PersonalityAxis::Conscience,
            CHIVALRIC_DEED_DELTA,
            "TendCourierAndCarryWarning",
            ChivalricVirtue::Mercy,
        ));
    }
}
