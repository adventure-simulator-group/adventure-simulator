use spacetimedb::{ReducerContext, SpacetimeType, Table, table};

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

/// Immutable strategic temperament. Each field is one mutually-exclusive axis.
#[derive(Clone, Debug)]
#[table(accessor = character_personality, public)]
pub struct CharacterPersonality {
    #[primary_key]
    pub character_id: u64,
    pub nerve: Nerve,
    pub drive: Drive,
    pub outlook: Outlook,
    pub sociability: Sociability,
    pub conscience: Conscience,
    pub self_regard: SelfRegard,
    pub conviction: Conviction,
    pub hygiene: Hygiene,
}

impl CharacterPersonality {
    pub fn neutral(character_id: u64) -> Self {
        Self {
            character_id,
            nerve: Nerve::Neutral,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Neutral,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
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
    }
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

/// Generate a sparse profile with exactly two through four distinct axes.
pub fn random_personality(
    character_id: u64,
    mut random: impl FnMut() -> u64,
) -> CharacterPersonality {
    let mut result = CharacterPersonality::neutral(character_id);
    let mut axes = [0_u8, 1, 2, 3, 4, 5, 6, 7];
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
            _ => {
                result.hygiene = if random() % 2 == 0 {
                    Hygiene::Slovenly
                } else {
                    Hygiene::Cleanly
                }
            }
        }
    }
    result
}

pub fn personality_or_neutral(ctx: &ReducerContext, character_id: u64) -> CharacterPersonality {
    ctx.db
        .character_personality()
        .character_id()
        .find(character_id)
        .unwrap_or_else(|| CharacterPersonality::neutral(character_id))
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
            random_personality(character_id, || ctx.random())
        } else {
            CharacterPersonality::neutral(character_id)
        };
        ctx.db.character_personality().insert(row);
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
        ctx.db.character_personality().character_id().update(row);
    } else {
        ctx.db.character_personality().insert(row);
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
    mut magnitude: f32,
) -> (f32, Vec<&'static str>) {
    let mut names = Vec::new();
    if stimulus == MoraleStimulus::Threat && magnitude < 0.0 {
        match personality.nerve {
            Nerve::Brave => {
                magnitude *= 0.5;
                names.push("Brave");
            }
            Nerve::Fearful => {
                magnitude *= 2.0;
                names.push("Fearful");
            }
            Nerve::Neutral => {}
        }
    }
    if matches!(stimulus, MoraleStimulus::Victory | MoraleStimulus::Defeat) {
        match personality.drive {
            Drive::Ambitious => {
                magnitude *= 1.5;
                names.push("Ambitious");
            }
            Drive::Content => {
                magnitude *= 0.5;
                names.push("Content");
            }
            Drive::Neutral => {}
        }
        match personality.self_regard {
            SelfRegard::Proud => {
                magnitude *= if stimulus == MoraleStimulus::Victory {
                    1.5
                } else {
                    3.0
                };
                names.push("Proud");
            }
            SelfRegard::Humble => {
                magnitude *= 0.75;
                names.push("Humble");
            }
            SelfRegard::Neutral => {}
        }
    }
    if stimulus == MoraleStimulus::Religious {
        match personality.conviction {
            Conviction::Zealous => {
                magnitude *= 1.5;
                names.push("Zealous");
            }
            Conviction::Irreverent => {
                magnitude *= 0.5;
                names.push("Irreverent");
            }
            Conviction::Neutral => {}
        }
    }
    match personality.outlook {
        Outlook::Sanguine => {
            magnitude *= if magnitude > 0.0 { 1.25 } else { 0.75 };
            names.push("Sanguine");
        }
        Outlook::Brooding => {
            magnitude *= if magnitude > 0.0 { 0.75 } else { 1.25 };
            names.push("Brooding");
        }
        Outlook::Neutral => {}
    }
    (magnitude, names)
}

pub fn negative_event_duration(personality: &CharacterPersonality, duration: u64) -> u64 {
    match personality.outlook {
        Outlook::Sanguine => duration / 2,
        Outlook::Brooding => duration.saturating_mul(2),
        Outlook::Neutral => duration,
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
        }
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
    fn sociability_is_separate_from_charisma_and_caps_can_apply_after_it() {
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
}
