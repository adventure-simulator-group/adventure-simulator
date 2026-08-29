//! Canonical personality vocabulary shared by persistence and simulation.

use serde::{Deserialize, Serialize};

macro_rules! personality_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }
    };
}

personality_enum!(Nerve {
    Neutral,
    Brave,
    Fearful,
});
personality_enum!(Drive {
    Neutral,
    Ambitious,
    Content,
});
personality_enum!(Outlook {
    Neutral,
    Sanguine,
    Brooding,
});
personality_enum!(Sociability {
    Neutral,
    Gregarious,
    Solitary,
});
personality_enum!(Conscience {
    Neutral,
    Compassionate,
    Callous,
    Cruel,
});
personality_enum!(SelfRegard {
    Neutral,
    Proud,
    Humble,
});
personality_enum!(Conviction {
    Neutral,
    Zealous,
    Irreverent,
});

impl Conviction {
    /// Ardor expressed on the shared personality strength scale.
    pub const fn strength(self) -> f32 {
        match self {
            Self::Zealous => 5.0,
            Self::Neutral => 2.5,
            Self::Irreverent => 0.0,
        }
    }
}
personality_enum!(Hygiene {
    Neutral,
    Slovenly,
    Cleanly,
});
personality_enum!(Temperance {
    Neutral,
    Temperate,
    Drunkard,
});
personality_enum!(Mirth {
    Neutral,
    Merry,
    Grave,
});
personality_enum!(Courtship {
    Neutral,
    Amorous,
    Proper,
});
personality_enum!(Transparency {
    Neutral,
    Open,
    Guarded,
});
personality_enum!(SelfKnowledge {
    Neutral,
    Introspective,
    SelfDeceiving,
});
personality_enum!(Inclination {
    Men,
    Either,
    Women,
    Neither,
});
personality_enum!(Presentation {
    Man,
    Ambiguous,
    Woman,
});

impl Presentation {
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::Man => "man",
            Self::Ambiguous => "ambiguous",
            Self::Woman => "woman",
        }
    }

    pub const fn stable_variant_id(self) -> &'static str {
        match self {
            Self::Man => "Man",
            Self::Ambiguous => "Ambiguous",
            Self::Woman => "Woman",
        }
    }
}
personality_enum!(Sex { Female, Male });

impl Sex {
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }

    pub const fn stable_variant_id(self) -> &'static str {
        match self {
            Self::Female => "Female",
            Self::Male => "Male",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
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
    pub inclination: Inclination,
    pub presentation: Presentation,
    pub sex: Sex,
}

impl Personality {
    pub const fn neutral() -> Self {
        Self {
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
            inclination: Inclination::Women,
            presentation: Presentation::Man,
            sex: Sex::Male,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_personality_has_no_non_neutral_axes() {
        assert_eq!(Personality::neutral().non_neutral_count(), 0);
    }

    #[test]
    fn personality_vocabulary_uses_canonical_snake_case() {
        assert_eq!(
            serde_json::to_string(&SelfKnowledge::SelfDeceiving).unwrap(),
            "\"self_deceiving\""
        );
    }
}
