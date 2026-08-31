use std::collections::BTreeSet;
#[cfg(not(target_family = "wasm"))]
use std::path::Path;

use serde::Deserialize;

use crate::starting_character::StartingSlot;

const ENEMY_FIXTURE_VERSION: u32 = 1;
const MAX_ENEMY_FIXTURE_BYTES: u64 = 16 * 1024;
const MAX_ENEMY_NAME_BYTES: usize = 64;
const MAX_LOADOUT_ITEMS: usize = 64;

/// A versioned standalone tactical roster. Environment inputs are intentionally
/// absent: callers choose terrain and enemies through independent files.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TacticalEnemyFixture {
    version: u32,
    enemies: Vec<TacticalEnemySpec>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TacticalEnemySpec {
    pub name: String,
    pub behavior: TacticalEnemyBehavior,
    pub add_basic_clothing: bool,
    pub loadout: Vec<TacticalEnemyItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TacticalEnemyBehavior {
    Passive,
    StandardCombat,
    AlwaysBlockWithoutFacing,
    AlwaysDodge,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TacticalEnemyItem {
    pub item_id: String,
    pub slot: TacticalEnemySlot,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TacticalEnemySlot {
    LeftHand,
    RightHand,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
    Head,
    Chest,
    Stomach,
}

impl From<TacticalEnemySlot> for StartingSlot {
    fn from(value: TacticalEnemySlot) -> Self {
        match value {
            TacticalEnemySlot::LeftHand => Self::LeftHand,
            TacticalEnemySlot::RightHand => Self::RightHand,
            TacticalEnemySlot::LeftArm => Self::LeftArm,
            TacticalEnemySlot::RightArm => Self::RightArm,
            TacticalEnemySlot::LeftLeg => Self::LeftLeg,
            TacticalEnemySlot::RightLeg => Self::RightLeg,
            TacticalEnemySlot::LeftFoot => Self::LeftFoot,
            TacticalEnemySlot::RightFoot => Self::RightFoot,
            TacticalEnemySlot::Head => Self::Head,
            TacticalEnemySlot::Chest => Self::Chest,
            TacticalEnemySlot::Stomach => Self::Stomach,
        }
    }
}

impl TacticalEnemyFixture {
    pub fn parse(yaml: &str) -> Result<Self, String> {
        if yaml.is_empty() || yaml.len() as u64 > MAX_ENEMY_FIXTURE_BYTES {
            return Err("enemy fixture must contain between 1 byte and 16 KiB".into());
        }
        let fixture: Self = serde_saphyr::from_str(yaml)
            .map_err(|error| format!("enemy fixture is not valid YAML: {error}"))?;
        fixture.validate()?;
        Ok(fixture)
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn load(path: &Path) -> Result<Self, String> {
        let length = std::fs::metadata(path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?
            .len();
        if length == 0 || length > MAX_ENEMY_FIXTURE_BYTES {
            return Err("enemy fixture must contain between 1 byte and 16 KiB".into());
        }
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Self::parse(&yaml).map_err(|error| format!("{}: {error}", path.display()))
    }

    #[must_use]
    pub fn enemies(&self) -> &[TacticalEnemySpec] {
        &self.enemies
    }

    #[must_use]
    pub fn enemy_count(&self) -> u32 {
        u32::try_from(self.enemies.len()).expect("validated enemy fixtures fit in u32")
    }

    #[must_use]
    pub fn enemy_named(&self, name: &str) -> Option<&TacticalEnemySpec> {
        self.enemies.iter().find(|enemy| enemy.name == name)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != ENEMY_FIXTURE_VERSION {
            return Err(format!(
                "enemy fixture version must be {ENEMY_FIXTURE_VERSION}, got {}",
                self.version
            ));
        }
        if self.enemies.is_empty()
            || self.enemies.len() > crate::threat_escalation::MAX_MOB_COUNT as usize
        {
            return Err(format!(
                "enemy fixture must define between 1 and {} enemies",
                crate::threat_escalation::MAX_MOB_COUNT
            ));
        }
        let mut names = BTreeSet::new();
        for enemy in &self.enemies {
            if enemy.name.trim() != enemy.name
                || enemy.name.is_empty()
                || enemy.name.len() > MAX_ENEMY_NAME_BYTES
            {
                return Err(format!(
                    "enemy names must contain 1 through {MAX_ENEMY_NAME_BYTES} bytes without surrounding whitespace"
                ));
            }
            if !names.insert(enemy.name.as_str()) {
                return Err(format!("enemy name '{}' is duplicated", enemy.name));
            }
            if enemy.loadout.len() > MAX_LOADOUT_ITEMS {
                return Err(format!(
                    "enemy '{}' has more than {MAX_LOADOUT_ITEMS} loadout items",
                    enemy.name
                ));
            }
            for item in &enemy.loadout {
                if item.item_id.is_empty()
                    || !item.item_id.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                {
                    return Err(format!(
                        "enemy '{}' has invalid item ID '{}'",
                        enemy.name, item.item_id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enemy_fixture_parses_a_declarative_roster() {
        let fixture = TacticalEnemyFixture::parse(
            r#"
version: 1
enemies:
  - name: Dodger
    behavior: always_dodge
    add_basic_clothing: true
    loadout:
      - item_id: morion
        slot: head
"#,
        )
        .unwrap();

        assert_eq!(fixture.enemies().len(), 1);
        assert_eq!(
            fixture.enemy_named("Dodger").unwrap().behavior,
            TacticalEnemyBehavior::AlwaysDodge
        );
        assert_eq!(
            StartingSlot::from(fixture.enemies()[0].loadout[0].slot),
            StartingSlot::Head
        );
    }

    #[test]
    fn enemy_fixture_rejects_duplicate_names_and_unknown_fields() {
        let duplicate = r#"
version: 1
enemies:
  - &enemy
    name: Duplicate
    behavior: passive
    add_basic_clothing: false
    loadout: []
  - *enemy
"#;
        assert!(TacticalEnemyFixture::parse(duplicate).is_err());

        let unknown = r#"
version: 1
environment: dense-woodland
enemies: []
"#;
        assert!(TacticalEnemyFixture::parse(unknown).is_err());
    }

    #[test]
    fn committed_enemy_fixtures_are_valid_and_keep_animation_roster_in_yaml() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tactical-enemies");
        for name in ["passive-bandit.yaml", "standard-bandit.yaml"] {
            let fixture = TacticalEnemyFixture::load(&root.join(name)).unwrap();
            assert_eq!(fixture.enemies().len(), 1);
        }
        let animation = TacticalEnemyFixture::load(&root.join("animation-demo.yaml")).unwrap();
        assert_eq!(animation.enemies().len(), 5);
        assert_eq!(
            animation.enemy_named("Munition Dodger").unwrap().behavior,
            TacticalEnemyBehavior::AlwaysDodge
        );
    }
}
