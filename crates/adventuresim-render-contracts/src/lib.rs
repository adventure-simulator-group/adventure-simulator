//! Stable, Bevy-free contracts shared by offline map compilation, SSR, and the WASM renderer.

use serde::{Deserialize, Serialize};

pub const RENDER_SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 512;
pub const MAX_LABEL_BYTES: usize = 4_096;
pub const MAX_URL_BYTES: usize = 8_192;
pub const MAX_SETTLEMENTS: usize = 500_000;
pub const MAX_ROADS: usize = 2_000_000;
pub const MAX_MARKERS: usize = 250_000;
pub const MAX_ROUTE_POINTS: usize = 250_000;
pub const MAX_SCENE_ACTORS: usize = 25_000;
pub const MAX_SOURCES: usize = 10_000;
pub const MAX_SOURCE_NOTICES: usize = 100_000;

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

    pub fn contains(self, point: Point) -> bool {
        point.validate().is_ok()
            && point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

impl Point {
    pub fn validate(self) -> Result<(), ContractError> {
        if self.x.is_finite() && self.y.is_finite() {
            Ok(())
        } else {
            Err(ContractError::Invalid("map points must be finite"))
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
        if !valid_hash(&self.artifact_id)
            || !valid_hash(&self.manifest_digest)
            || !valid_hash(&self.package_hash)
        {
            return Err(ContractError::Invalid(
                "artifact identities must be 64-character hashes",
            ));
        }
        if !valid_url(&self.package_url) || !valid_url(&self.paper_map_url) {
            return Err(ContractError::Invalid("artifact URLs are required"));
        }
        if !self
            .package_url
            .ends_with(&format!("map-{}.json", self.package_hash))
        {
            return Err(ContractError::Invalid(
                "package URL must contain its content hash",
            ));
        }
        if self.sources.len() > MAX_SOURCES {
            return Err(ContractError::Invalid("manifest exceeds source limit"));
        }
        let mut notice_count = 0usize;
        for source in &self.sources {
            notice_count = notice_count.saturating_add(source.required_notices.len());
            if !valid_text(&source.name, MAX_LABEL_BYTES)
                || !valid_url(&source.canonical_url)
                || source
                    .required_notices
                    .iter()
                    .any(|notice| !valid_text(notice, 32_768))
            {
                return Err(ContractError::Invalid("invalid source attribution"));
            }
        }
        if notice_count > MAX_SOURCE_NOTICES {
            return Err(ContractError::Invalid(
                "manifest exceeds source notice limit",
            ));
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
        self.bounds.validate()?;
        if self.settlements.len() > MAX_SETTLEMENTS || self.roads.len() > MAX_ROADS {
            return Err(ContractError::Invalid(
                "map package exceeds collection limits",
            ));
        }
        for settlement in &self.settlements {
            if !valid_text(&settlement.id, MAX_ID_BYTES)
                || !valid_text(&settlement.name, MAX_LABEL_BYTES)
                || !self.bounds.contains(settlement.point)
            {
                return Err(ContractError::Invalid("invalid settlement feature"));
            }
        }
        for road in &self.roads {
            if !valid_text(&road.id, MAX_ID_BYTES)
                || !self.bounds.contains(road.from)
                || !self.bounds.contains(road.to)
            {
                return Err(ContractError::Invalid("invalid road feature"));
            }
        }
        Ok(())
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
impl MapOverlay {
    pub fn validate(&self, bounds: Bounds) -> Result<(), ContractError> {
        validate_version(self.renderer_schema)?;
        if self.markers.len() > MAX_MARKERS || self.selected_route.len() > MAX_ROUTE_POINTS {
            return Err(ContractError::Invalid(
                "map overlay exceeds collection limits",
            ));
        }
        if !bounds.contains(self.focus) {
            return Err(ContractError::Invalid(
                "map focus is outside package bounds",
            ));
        }
        for marker in &self.markers {
            if !valid_text(&marker.id, MAX_ID_BYTES)
                || !valid_text(&marker.label, MAX_LABEL_BYTES)
                || !bounds.contains(marker.point)
                || marker.href.as_deref().is_some_and(|url| !valid_url(url))
            {
                return Err(ContractError::Invalid("invalid map marker"));
            }
        }
        if self
            .selected_route
            .iter()
            .any(|point| !bounds.contains(*point))
        {
            return Err(ContractError::Invalid(
                "selected route is outside package bounds",
            ));
        }
        Ok(())
    }
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
impl SceneDescriptor {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.renderer_schema)?;
        if self.actors.len() > MAX_SCENE_ACTORS {
            return Err(ContractError::Invalid("scene exceeds actor limit"));
        }
        for actor in &self.actors {
            if !valid_text(&actor.id, MAX_ID_BYTES) || !valid_text(&actor.label, MAX_LABEL_BYTES) {
                return Err(ContractError::Invalid("invalid scene actor identity"));
            }
            match actor.primitive {
                Primitive::Cylinder { radius, height }
                    if radius.is_finite()
                        && height.is_finite()
                        && radius > 0.0
                        && height > 0.0
                        && radius <= 1_000.0
                        && height <= 1_000.0 => {}
                Primitive::Cylinder { .. } => {
                    return Err(ContractError::Invalid("invalid cylinder dimensions"));
                }
            }
        }
        Ok(())
    }
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
        if !valid_text(&self.canvas_selector, 1_024) {
            return Err(ContractError::Invalid("canvas selector is required"));
        }
        match &self.startup {
            StartupMode::StrategicMap {
                package_url,
                package,
                overlay,
            } => {
                if package_url.as_deref().is_some_and(|url| !valid_url(url)) {
                    return Err(ContractError::Invalid("invalid map manifest URL"));
                }
                package.validate()?;
                overlay.validate(package.bounds)
            }
            StartupMode::StrategicScene { scene } => scene.validate(),
            StartupMode::Tactical { server_url, .. } if valid_url(server_url) => Ok(()),
            StartupMode::Tactical { .. } => {
                Err(ContractError::Invalid("invalid tactical server URL"))
            }
        }
    }
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn valid_url(value: &str) -> bool {
    valid_text(value, MAX_URL_BYTES)
        && (value.starts_with('/') || value.contains("://") || value.contains(':'))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

    fn map_config() -> StartupConfig {
        let bounds = Bounds {
            min: Point { x: 0., y: 0. },
            max: Point { x: 10., y: 10. },
        };
        StartupConfig {
            renderer_schema: RENDER_SCHEMA_VERSION,
            canvas_selector: "#map".into(),
            startup: StartupMode::StrategicMap {
                package_url: Some("/tactical/map/manifest.json".into()),
                package: MapPackage {
                    renderer_schema: RENDER_SCHEMA_VERSION,
                    bounds,
                    settlements: vec![],
                    roads: vec![],
                },
                overlay: MapOverlay {
                    renderer_schema: RENDER_SCHEMA_VERSION,
                    focus: Point { x: 5., y: 5. },
                    markers: vec![],
                    selected_route: vec![],
                },
            },
        }
    }

    #[test]
    fn startup_recursively_rejects_nested_versions_and_nonfinite_primitives() {
        let mut config = map_config();
        let StartupMode::StrategicMap { package, .. } = &mut config.startup else {
            unreachable!()
        };
        package.renderer_schema = RENDER_SCHEMA_VERSION + 1;
        assert!(matches!(config.validate(), Err(ContractError::Version(_))));

        let scene = StartupConfig {
            renderer_schema: RENDER_SCHEMA_VERSION,
            canvas_selector: "#scene".into(),
            startup: StartupMode::StrategicScene {
                scene: SceneDescriptor {
                    renderer_schema: RENDER_SCHEMA_VERSION,
                    actors: vec![SceneActor {
                        id: "actor".into(),
                        label: "Actor".into(),
                        color_rgb: [0; 3],
                        primitive: Primitive::Cylinder {
                            radius: f32::NAN,
                            height: 1.0,
                        },
                        animation: AnimationName::Idle,
                    }],
                },
            },
        };
        assert!(scene.validate().is_err());
    }

    #[test]
    fn startup_rejects_oversized_payloads_and_out_of_bounds_points() {
        let mut config = map_config();
        let StartupMode::StrategicMap { overlay, .. } = &mut config.startup else {
            unreachable!()
        };
        overlay.selected_route = vec![Point { x: 5., y: 5. }; MAX_ROUTE_POINTS + 1];
        assert!(config.validate().is_err());

        let mut config = map_config();
        let StartupMode::StrategicMap { overlay, .. } = &mut config.startup else {
            unreachable!()
        };
        overlay.focus = Point { x: 11., y: 5. };
        assert!(config.validate().is_err());
    }
}
