/// Stable roles used by the interactive animation combat laboratory. The
/// durable seed and transient tactical server both consume this definition so
/// fixture loadouts and runtime behavior cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationLabEnemyRole {
    Passive,
    ShieldBlocker,
    Dodger,
    DemiLancer,
}

impl AnimationLabEnemyRole {
    pub const ALL: [Self; 4] = [
        Self::Passive,
        Self::ShieldBlocker,
        Self::Dodger,
        Self::DemiLancer,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Passive => "Passive Bandit",
            Self::ShieldBlocker => "Shield Blocker",
            Self::Dodger => "Dodger",
            Self::DemiLancer => "Demi-lancer",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_lab_role_names_round_trip() {
        for role in AnimationLabEnemyRole::ALL {
            assert_eq!(AnimationLabEnemyRole::from_name(role.name()), Some(role));
        }
    }
}
