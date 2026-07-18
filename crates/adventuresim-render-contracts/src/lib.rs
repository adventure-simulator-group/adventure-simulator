//! Stable, Bevy-free contracts shared by offline map compilation, SSR, and the WASM renderer.

use serde::{Deserialize, Serialize};

pub const RENDER_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ContractError {
    #[error("renderer schema {0} is unsupported (expected {RENDER_SCHEMA_VERSION})")]
    Version(u32),
    #[error("{0}")]
    Invalid(&'static str),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}
impl Bounds {
    pub fn validate(self) -> Result<(), ContractError> {
        if [self.min.x, self.min.y, self.max.x, self.max.y]
            .iter()
            .all(|v| v.is_finite())
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
        {
            Ok(())
        } else {
            Err(ContractError::Invalid(
                "map bounds must be finite and ordered",
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceNotice {
    pub name: String,
    pub canonical_url: String,
    pub required_notices: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapManifest {
    pub renderer_schema: u32,
    pub world_schema: u32,
    pub artifact_id: String,
    pub manifest_digest: String,
    pub package_hash: String,
    pub package_url: String,
    pub paper_map_url: String,
    pub bounds: Bounds,
    pub sources: Vec<SourceNotice>,
}
impl MapManifest {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.renderer_schema)?;
        self.bounds.validate()?;
        if self.artifact_id.len() != 64
            || self.manifest_digest.len() != 64
            || self.package_hash.len() != 64
        {
            return Err(ContractError::Invalid(
                "artifact identities must be 64-character hashes",
            ));
        }
        if self.package_url.is_empty() || self.paper_map_url.is_empty() {
            return Err(ContractError::Invalid("artifact URLs are required"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementFeature {
    pub id: String,
    pub name: String,
    pub point: Point,
    pub population_level: i32,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoadFeature {
    pub id: String,
    pub from: Point,
    pub to: Point,
    pub ferry: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapPackage {
    pub renderer_schema: u32,
    pub bounds: Bounds,
    pub settlements: Vec<SettlementFeature>,
    pub roads: Vec<RoadFeature>,
}
impl MapPackage {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.renderer_schema)?;
        self.bounds.validate()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MarkerKind {
    Destination,
    SelectedDestination,
    Party,
    ActiveQuest,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapMarker {
    pub id: String,
    pub label: String,
    pub point: Point,
    pub kind: MarkerKind,
    pub href: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapOverlay {
    pub renderer_schema: u32,
    pub focus: Point,
    pub markers: Vec<MapMarker>,
    pub selected_route: Vec<Point>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    Cylinder { radius: f32, height: f32 },
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimationName {
    Idle,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneActor {
    pub id: String,
    pub label: String,
    pub color_rgb: [u8; 3],
    pub primitive: Primitive,
    pub animation: AnimationName,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDescriptor {
    pub renderer_schema: u32,
    pub actors: Vec<SceneActor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartupMode {
    StrategicMap {
        package_url: Option<String>,
        package: MapPackage,
        overlay: MapOverlay,
    },
    StrategicScene {
        scene: SceneDescriptor,
    },
    Tactical {
        player_id: u64,
        server_url: String,
    },
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupConfig {
    pub renderer_schema: u32,
    pub canvas_selector: String,
    pub startup: StartupMode,
}
impl StartupConfig {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.renderer_schema)?;
        if self.canvas_selector.is_empty() {
            return Err(ContractError::Invalid("canvas selector is required"));
        }
        let nested = match &self.startup {
            StartupMode::StrategicMap { overlay, .. } => overlay.renderer_schema,
            StartupMode::StrategicScene { scene } => scene.renderer_schema,
            StartupMode::Tactical { .. } => RENDER_SCHEMA_VERSION,
        };
        validate_version(nested)
    }
}
pub fn validate_version(version: u32) -> Result<(), ContractError> {
    if version == RENDER_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ContractError::Version(version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_round_trip_and_version_validation() {
        let value = StartupConfig {
            renderer_schema: 1,
            canvas_selector: "#map".into(),
            startup: StartupMode::StrategicScene {
                scene: SceneDescriptor {
                    renderer_schema: 1,
                    actors: vec![SceneActor {
                        id: "7".into(),
                        label: "Ada".into(),
                        color_rgb: [1, 2, 3],
                        primitive: Primitive::Cylinder {
                            radius: 0.4,
                            height: 1.8,
                        },
                        animation: AnimationName::Idle,
                    }],
                },
            },
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<StartupConfig>(&json).unwrap(), value);
        assert_eq!(value.validate(), Ok(()));
        let mut incompatible = value;
        incompatible.renderer_schema = 2;
        assert_eq!(incompatible.validate(), Err(ContractError::Version(2)));
    }
    #[test]
    fn bounds_reject_nan_and_inversion() {
        assert!(
            Bounds {
                min: Point { x: 1., y: 0. },
                max: Point { x: 0., y: f64::NAN }
            }
            .validate()
            .is_err()
        );
    }
}
