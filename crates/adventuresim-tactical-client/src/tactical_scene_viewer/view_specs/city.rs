use super::*;

pub(in crate::tactical_scene_viewer) const CITY_REVIEW_VIEWS: [CaptureViewSpec; 8] = [
    v!(
        "warmup",
        "City material render-pipeline warmup",
        CapturePose::CityExterior { camera: 0 },
        68.0,
        100
    )
    .warmup()
    .vista(),
    v!(
        "facade-detail",
        "Close facade material and join review",
        CapturePose::CityExterior { camera: 0 },
        58.0,
        100
    )
    .vista(),
    v!(
        "street-context",
        "Eye-height street and adjacent-building review",
        CapturePose::CityExterior { camera: 1 },
        72.0,
        100
    )
    .vista(),
    v!(
        "playable-block-oblique",
        "Playable-block roof and facade oblique",
        CapturePose::CityExterior { camera: 2 },
        60.0,
        100
    )
    .vista(),
    v!(
        "neighbourhood-oblique",
        "Near and middle LOD neighbourhood oblique",
        CapturePose::CityExterior { camera: 3 },
        60.0,
        100
    )
    .vista(),
    v!(
        "city-edge",
        "City edge silhouette and middle-distance material review",
        CapturePose::CityExterior { camera: 4 },
        58.0,
        100
    )
    .vista(),
    v!(
        "whole-city-aerial",
        "Entire settlement aerial overview",
        CapturePose::CityExterior { camera: 5 },
        62.0,
        100
    )
    .vista(),
    v!(
        "whole-city-horizon",
        "Entire settlement distant skyline",
        CapturePose::CityExterior { camera: 6 },
        50.0,
        100
    )
    .vista(),
];
