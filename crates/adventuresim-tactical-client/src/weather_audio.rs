use adventuresim_tactical_core::prelude::*;
use adventuresim_world_schema::UnitBasisPoints;
use bevy::audio::{AudioSink, AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;

use crate::{audio_config::TacticalAudioConfig, player::ClientPlayer};

pub struct WeatherAudioPlugin;

impl Plugin for WeatherAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_rain_layers)
            .add_systems(Update, update_rain_layers);
    }
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
enum RainAudioLayer {
    Surface(RainSurface),
    HeavyStorm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RainSurface {
    SoftGround,
    Foliage,
    HardGround,
    Water,
}

impl RainSurface {
    const ALL: [Self; 4] = [
        Self::SoftGround,
        Self::Foliage,
        Self::HardGround,
        Self::Water,
    ];

    fn asset_path(self) -> &'static str {
        match self {
            Self::SoftGround => "audio/weather/rain/soft-ground.ogg",
            Self::Foliage => "audio/weather/rain/foliage.ogg",
            Self::HardGround => "audio/weather/rain/hard-ground.ogg",
            Self::Water => "audio/weather/rain/water.ogg",
        }
    }

    fn at(surface: Option<GroundSurface>) -> Self {
        let Some(surface) = surface else {
            return Self::SoftGround;
        };
        match (surface.cover, surface.substrate) {
            (GroundCover::TallGrass | GroundCover::LeafLitter | GroundCover::Reeds, _) => {
                Self::Foliage
            }
            (GroundCover::LooseStone, _) => Self::HardGround,
            (_, GroundSubstrate::Water) => Self::Water,
            (_, GroundSubstrate::Stone | GroundSubstrate::Gravel | GroundSubstrate::Road) => {
                Self::HardGround
            }
            (_, GroundSubstrate::Soil | GroundSubstrate::Mud) => Self::SoftGround,
        }
    }
}

fn spawn_rain_layers(mut commands: Commands, asset_server: Res<AssetServer>) {
    for surface in RainSurface::ALL {
        spawn_rain_layer(
            &mut commands,
            &asset_server,
            RainAudioLayer::Surface(surface),
            surface.asset_path(),
        );
    }
    spawn_rain_layer(
        &mut commands,
        &asset_server,
        RainAudioLayer::HeavyStorm,
        "audio/weather/rain/rain-heavy.ogg",
    );
}

fn spawn_rain_layer(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layer: RainAudioLayer,
    path: &'static str,
) {
    commands.spawn((
        Name::new(format!("tactical-rain-{layer:?}")),
        layer,
        AudioPlayer::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::SILENT,
            ..default()
        },
    ));
}

fn update_rain_layers(
    time: Res<Time>,
    environments: Query<&SceneEnvironment>,
    grounds: Query<&SceneGround>,
    listeners: Query<&GlobalTransform, With<ClientPlayer>>,
    mut layers: Query<(&RainAudioLayer, &mut AudioSink)>,
    config: Res<TacticalAudioConfig>,
) {
    let weather = environments
        .iter()
        .next()
        .map(|environment| environment.weather);
    let intensity = weather
        .filter(|weather| weather.precipitation == Precipitation::Rain)
        .map(|weather| UnitBasisPoints::saturating(weather.intensity_bps).as_unit_f32())
        .unwrap_or_default();
    let listener_position = listeners.iter().next().map(GlobalTransform::translation);
    let surface = RainSurface::at(listener_position.and_then(|position| {
        grounds
            .iter()
            .next()
            .and_then(|ground| ground.ground_at(position.xz()))
    }));
    let (surface_volume, storm_volume) = rain_mix(intensity, &config);
    let response = 1.0 - (-config.weather.volume_response_per_second * time.delta_secs()).exp();
    for (layer, mut sink) in &mut layers {
        let target = match layer {
            RainAudioLayer::Surface(candidate) if *candidate == surface => surface_volume,
            RainAudioLayer::Surface(_) => 0.0,
            RainAudioLayer::HeavyStorm => storm_volume,
        };
        let volume = sink.volume().to_linear().lerp(target, response);
        sink.set_volume(Volume::Linear(volume));
    }
}

fn rain_mix(intensity: f32, config: &TacticalAudioConfig) -> (f32, f32) {
    let intensity = intensity.clamp(0.0, 1.0);
    let weather = &config.weather;
    let storm = ((intensity - weather.heavy_rain_start_fraction)
        / (1.0 - weather.heavy_rain_start_fraction))
        .clamp(0.0, 1.0);
    let storm = storm * storm * (3.0 - 2.0 * storm);
    (
        weather.rain_surface_maximum_relative_volume
            * intensity.powf(weather.surface_intensity_exponent)
            * (1.0 - storm * weather.storm_surface_ducking),
        weather.rain_storm_maximum_relative_volume * storm,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TacticalAudioConfig {
        TacticalAudioConfig::parse(include_str!("../../../assets/config/tactical-audio.yaml"))
            .unwrap()
    }

    fn surface(substrate: GroundSubstrate, cover: GroundCover) -> GroundSurface {
        GroundSurface {
            substrate,
            cover,
            ..default()
        }
    }

    #[test]
    fn cover_and_substrate_select_distinct_rain_beds() {
        assert_eq!(
            RainSurface::at(Some(surface(GroundSubstrate::Soil, GroundCover::TallGrass))),
            RainSurface::Foliage
        );
        assert_eq!(
            RainSurface::at(Some(surface(GroundSubstrate::Road, GroundCover::Bare))),
            RainSurface::HardGround
        );
        assert_eq!(
            RainSurface::at(Some(surface(GroundSubstrate::Water, GroundCover::Bare))),
            RainSurface::Water
        );
    }

    #[test]
    fn heavy_storm_layer_enters_only_above_its_threshold() {
        let config = config();
        assert_eq!(rain_mix(0.0, &config), (0.0, 0.0));
        assert_eq!(
            rain_mix(config.weather.heavy_rain_start_fraction, &config).1,
            0.0
        );
        assert!(rain_mix(1.0, &config).1 > rain_mix(0.8, &config).1);
        assert!(rain_mix(1.0, &config).0 > 0.0);
    }
}
