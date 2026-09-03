//! Compact exact-containment windows around existing source sampling anchors.
//! A window is never inferred from a settlement's previously sampled lithology.

use super::*;
use adventuresim_world_schema::{
    CompiledWorld, MAX_GEOLOGIC_WINDOWS, MappedGeologicWindow, TerrainFeature,
};
use geo::{Coord, Rect};

const WINDOW_RADII_METRES: [f64; 5] = [1000.0, 500.0, 250.0, 125.0, 100.0];

pub(crate) fn enrich(mut world: CompiledWorld, path: &Path) -> Result<CompiledWorld> {
    let map = GeologyMap::open(path)?;
    let projection = GeologyProjection::new()?;
    let mut windows = Vec::new();
    for settlement in &world.settlements {
        let point = projection.project(settlement.latitude, settlement.longitude)?;
        if let Some(window) = map.contained_window(point, &settlement.id)? {
            windows.push(TerrainFeature::MappedGeology(window));
        }
    }
    if windows.len() > MAX_GEOLOGIC_WINDOWS {
        return Err(Error::Validation(
            "too many mapped geological windows".into(),
        ));
    }
    world.terrain_features.extend(windows);
    world.terrain_features.sort_by(|a, b| a.id().cmp(b.id()));
    Ok(world)
}

impl GeologyMap {
    pub(super) fn contained_window(
        &self,
        point: Point<f64>,
        anchor: &str,
    ) -> Result<Option<MappedGeologicWindow>> {
        for feature in self.candidates(point)? {
            let geometry =
                GpkgWkb(&feature.geometry)
                    .to_geo()
                    .map_err(|source| Error::InvalidField {
                        path: self.path.clone(),
                        field: "geom",
                        value: feature.fid.to_string(),
                        message: source.to_string(),
                    })?;
            if !geometry_contains(&geometry, &point) {
                continue;
            }
            let SurfaceGeology::Mapped(profile) = feature.into_profile(&self.path)? else {
                continue;
            };
            let GeologicLithologyEvidence::Mapped(lithology) = profile.setting.lithology else {
                return Ok(None);
            };
            for radius in WINDOW_RADII_METRES {
                let bounds = [
                    (point.x() - radius).ceil() as i32,
                    (point.y() - radius).ceil() as i32,
                    (point.x() + radius).floor() as i32,
                    (point.y() + radius).floor() as i32,
                ];
                let rectangle = Rect::new(
                    Coord {
                        x: f64::from(bounds[0]),
                        y: f64::from(bounds[1]),
                    },
                    Coord {
                        x: f64::from(bounds[2]),
                        y: f64::from(bounds[3]),
                    },
                )
                .to_polygon();
                if contains_window(&geometry, &rectangle) {
                    return Ok(Some(MappedGeologicWindow {
                        id: format!("egdi-window:{anchor}"),
                        unit: profile.unit,
                        lithology,
                        bounds_metres: bounds,
                    }));
                }
            }
            return Ok(None);
        }
        Ok(None)
    }
}

fn contains_window(geometry: &Geometry<f64>, window: &geo::Polygon<f64>) -> bool {
    match geometry {
        Geometry::Polygon(polygon) => polygon.contains(window),
        Geometry::MultiPolygon(polygons) => {
            polygons.0.iter().any(|polygon| polygon.contains(window))
        }
        Geometry::GeometryCollection(collection) => collection
            .iter()
            .any(|geometry| contains_window(geometry, window)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Polygon, polygon};

    #[test]
    fn coverage_rejects_holes_and_concave_boundaries_even_when_corners_are_inside() {
        let exterior = polygon![(x:0.,y:0.),(x:10.,y:0.),(x:10.,y:10.),(x:0.,y:10.),(x:0.,y:0.)];
        let hole = polygon![(x:4.,y:4.),(x:6.,y:4.),(x:6.,y:6.),(x:4.,y:6.),(x:4.,y:4.)];
        let geometry = Geometry::Polygon(Polygon::new(
            exterior.exterior().clone(),
            vec![hole.exterior().clone()],
        ));
        let window = Rect::new(Coord { x: 2., y: 2. }, Coord { x: 8., y: 8. }).to_polygon();
        assert!(!contains_window(&geometry, &window));
        let safe = Rect::new(Coord { x: 1., y: 1. }, Coord { x: 3., y: 3. }).to_polygon();
        assert!(contains_window(&geometry, &safe));
    }
}
