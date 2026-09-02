use super::*;

pub(in crate::tactical_scene_viewer) const INTERIOR_REVIEW_VIEWS: [CaptureViewSpec; 11] = [
    v!(
        "warmup",
        "Building-interior render-pipeline warmup",
        CapturePose::BuildingInterior { camera: 0 },
        72.0,
        100
    )
    .warmup(),
    v!(
        "town-house-ground",
        "Town house ground-storey interior",
        CapturePose::BuildingInterior { camera: 0 },
        72.0,
        100
    ),
    v!(
        "town-house-upper",
        "Town house upper-storey interior",
        CapturePose::BuildingInterior { camera: 1 },
        72.0,
        100
    ),
    v!(
        "hall-house-ground",
        "Hall house ground-storey interior",
        CapturePose::BuildingInterior { camera: 2 },
        72.0,
        100
    ),
    v!(
        "hall-house-plaster-grazing",
        "Hall house plaster under neutral grazing review light",
        CapturePose::BuildingInterior { camera: 8 },
        62.0,
        100
    )
    .plaster_grazing_light(),
    v!(
        "merchant-house-partition",
        "Merchant house framed internal partition and doorway junction",
        CapturePose::BuildingInterior { camera: 9 },
        70.0,
        100
    ),
    v!(
        "hall-house-upper",
        "Hall house alternate interior angle",
        CapturePose::BuildingInterior { camera: 3 },
        72.0,
        100
    ),
    v!(
        "cottage-ground",
        "Fachwerk cottage ground-storey interior",
        CapturePose::BuildingInterior { camera: 4 },
        72.0,
        100
    ),
    v!(
        "cottage-upper",
        "Fachwerk cottage upper-storey interior",
        CapturePose::BuildingInterior { camera: 5 },
        72.0,
        100
    ),
    v!(
        "merchant-house-ground",
        "Merchant house ground-storey interior",
        CapturePose::BuildingInterior { camera: 6 },
        72.0,
        100
    ),
    v!(
        "merchant-house-upper",
        "Merchant house upper-storey interior",
        CapturePose::BuildingInterior { camera: 7 },
        72.0,
        100
    ),
];
