use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! authority_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err("authority IDs must not be empty");
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

authority_id!(MissionId);
authority_id!(BattleId);
authority_id!(HostileGroupId);
authority_id!(OutcomeSourceId);

/// A mission may be deliberately unbound (for example, a random encounter).
/// Only the bound form is eligible to defeat a persistent hostile group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostileGroupBinding {
    Unbound,
    Bound(HostileGroupId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategicBattleVictor {
    Allies,
    Enemies,
    Stalemate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_ids_reject_empty_values() {
        assert!(MissionId::new("").is_err());
        assert_eq!(BattleId::new("battle:1").unwrap().as_str(), "battle:1");
    }

    #[test]
    fn unbound_encounters_cannot_alias_a_hostile_group() {
        let random = HostileGroupBinding::Unbound;
        let bound = HostileGroupBinding::Bound(HostileGroupId::new("group:1").unwrap());
        assert_ne!(random, bound);
    }
}
