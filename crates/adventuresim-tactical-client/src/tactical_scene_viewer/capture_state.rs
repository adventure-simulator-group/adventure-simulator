use std::{collections::BTreeSet, path::PathBuf};

use adventuresim_tactical_core::prelude::{SceneSource, WeatherSnapshot};
use bevy::prelude::{Entity, Resource, Vec3};

use super::{
    manifest::{CaptureRecord, RepairSummary, TerrainSummary},
    view_specs::CaptureViewSpec,
};
use crate::presentation::TreeLeafRepresentation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureReadback {
    Prime,
    Warmup,
    Screenshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CapturePhase {
    Configure,
    Settling {
        frames: u32,
        prime_readbacks: u8,
    },
    Readback {
        view: usize,
        prime_readbacks: u8,
        kind: CaptureReadback,
    },
}

impl CapturePhase {
    pub(super) fn settling(self) -> Option<(u32, u8)> {
        match self {
            Self::Settling {
                frames,
                prime_readbacks,
            } => Some((frames, prime_readbacks)),
            Self::Configure | Self::Readback { .. } => None,
        }
    }

    pub(super) fn readback_in_flight(self) -> bool {
        matches!(self, Self::Readback { .. })
    }
}

#[derive(Resource)]
pub(super) struct CaptureState {
    pub(super) fixture: String,
    pub(super) input_path: PathBuf,
    pub(super) output: PathBuf,
    pub(super) digest: String,
    pub(super) seed: u64,
    pub(super) absolute_minute: u64,
    pub(super) latitude_microdegrees: i32,
    pub(super) longitude_microdegrees: i32,
    pub(super) canopy_bps: u16,
    pub(super) generation_version: u16,
    pub(super) scene_source: SceneSource,
    pub(super) weather: WeatherSnapshot,
    pub(super) repairs: RepairSummary,
    pub(super) terrain: TerrainSummary,
    pub(super) expected_trees: usize,
    pub(super) expected_rocks: usize,
    pub(super) expects_grass: bool,
    pub(super) vista_lods_supplied: usize,
    pub(super) vista_diameter_metres: f32,
    pub(super) vista_minimum_metres: f32,
    pub(super) vista_peak_metres: f32,
    pub(super) vista_relief_metres: f32,
    pub(super) peak_target: Vec3,
    pub(super) valley_target: Vec3,
    pub(super) obstacle_focus: Vec3,
    pub(super) tree_focus: Option<Vec3>,
    pub(super) rock_focus: Option<Vec3>,
    pub(super) debris_focus: Option<Vec3>,
    pub(super) debris_camera: Option<Vec3>,
    pub(super) debris_leaf_distance_metres: Option<f32>,
    pub(super) debris_twig_distance_metres: Option<f32>,
    pub(super) tree_leaf_focus: Option<Vec3>,
    pub(super) tree_leaf_camera: Option<Vec3>,
    pub(super) tree_focus_entity: Option<Entity>,
    pub(super) tree_review_entities: Vec<Entity>,
    pub(super) tree_review_leaf_entities: Vec<(Entity, TreeLeafRepresentation)>,
    pub(super) ground_eye_position: Vec3,
    pub(super) ground_eye_target: Vec3,
    pub(super) settle_frames: u32,
    pub(super) tree_review_azimuth_degrees: f32,
    pub(super) profile: String,
    pub(super) requested_views: Vec<String>,
    pub(super) views: Vec<CaptureViewSpec>,
    pub(super) view: usize,
    pub(super) phase: CapturePhase,
    pub(super) lighting_luminance_samples: Vec<f32>,
    pub(super) captures: Vec<CaptureRecord>,
    pub(super) recursive_lods_observed: BTreeSet<(u8, u8)>,
    pub(super) recursive_aggregate_lods_observed: BTreeSet<u8>,
}

pub(super) fn foreground_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else { return 0 };
    let Some(background) = data.get(..4) else {
        return 0;
    };
    let mut pixels = 0usize;
    let mut foreground = 0usize;
    for pixel in data.as_chunks::<4>().0 {
        pixels += 1;
        let difference = pixel[..3]
            .iter()
            .zip(&background[..3])
            .map(|(left, right)| left.abs_diff(*right) as u16)
            .sum::<u16>();
        foreground += usize::from(difference >= 12);
    }
    foreground
        .checked_mul(10_000)
        .and_then(|value| value.checked_div(pixels))
        .unwrap_or(0)
        .min(10_000) as u16
}

pub(super) fn mean_luminance(data: Option<&[u8]>) -> f32 {
    let Some(data) = data else { return f32::NAN };
    let pixels = data.as_chunks::<4>().0;
    if pixels.is_empty() {
        return f32::NAN;
    }
    let total = pixels
        .iter()
        .map(|pixel| {
            f64::from(pixel[0]) * 0.2126
                + f64::from(pixel[1]) * 0.7152
                + f64::from(pixel[2]) * 0.0722
        })
        .sum::<f64>();
    (total / pixels.len() as f64) as f32
}

pub(super) fn luminance_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

pub(super) fn lighting_samples_stable(samples: &[f32]) -> bool {
    if samples.len() < 2 || samples.iter().any(|sample| !sample.is_finite()) {
        return false;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    luminance_delta(samples) <= (mean * 0.02).max(1.5)
}

pub(super) fn foliage_detail_pixel_bps(data: Option<&[u8]>, width: u32, height: u32) -> u16 {
    if width == 0 || height == 0 {
        return 0;
    }
    let Some(data) = data else { return 0 };
    let row_bytes = width as usize * 4;
    if data.len() < row_bytes * height as usize {
        return 0;
    }
    let mut compared = 0usize;
    let mut detailed = 0usize;
    for y in height as usize / 3..height as usize {
        let row = &data[y * row_bytes..(y + 1) * row_bytes];
        for pair in row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(row[4..].as_chunks::<4>().0)
        {
            compared += 1;
            let difference = pair.0[..3]
                .iter()
                .zip(&pair.1[..3])
                .map(|(left, right)| left.abs_diff(*right) as u16)
                .sum::<u16>();
            detailed += usize::from(difference >= 4);
        }
    }
    detailed
        .checked_mul(10_000)
        .and_then(|value| value.checked_div(compared))
        .unwrap_or(0)
        .min(10_000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_phase_encodes_readback_exclusivity() {
        assert!(!CapturePhase::Configure.readback_in_flight());
        assert!(
            !CapturePhase::Settling {
                frames: 2,
                prime_readbacks: 1
            }
            .readback_in_flight()
        );
        assert!(
            CapturePhase::Readback {
                view: 3,
                prime_readbacks: 2,
                kind: CaptureReadback::Screenshot
            }
            .readback_in_flight()
        );
    }

    #[test]
    fn foliage_metric_rejects_zero_sized_images() {
        assert_eq!(foliage_detail_pixel_bps(Some(&[]), 0, 1), 0);
        assert_eq!(foliage_detail_pixel_bps(Some(&[]), 1, 0), 0);
    }
}
