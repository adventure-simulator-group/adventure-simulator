use std::{collections::BTreeMap, fs, path::Path};

use super::Package;

const WIDTH: f64 = 1_200.0;
const HEIGHT: f64 = 800.0;

struct Geometry {
    land: String,
    ferry: String,
    water: String,
    elevation: BTreeMap<i16, String>,
    contours: BTreeMap<i16, String>,
    forest: BTreeMap<(String, u8), String>,
    forest_coverage: String,
}

pub(super) fn write(path: &Path, package: &Package) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, build(package))?;
    Ok(())
}

pub(super) fn build(package: &Package) -> String {
    let geometry = geometry(package);
    let mut output = String::with_capacity(
        geometry.land.len()
            + geometry.ferry.len()
            + geometry.water.len()
            + geometry.forest_coverage.len()
            + geometry.elevation.values().map(String::len).sum::<usize>()
            + geometry.contours.values().map(String::len).sum::<usize>()
            + geometry.forest.values().map(String::len).sum::<usize>()
            + 4_096,
    );
    output.push_str(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 800"><metadata>Viabundus Pre-modern Street Map 2 (CC-BY-SA-4.0), Copernicus DEM GLO-30, and Copernicus HRL Forests 2018; generalized for Adventure Simulator.</metadata><style>
.map-land{fill:var(--map-land,#e6e1cb)}
.map-elevation{stroke:none}.map-elevation-50{fill:var(--map-height-50,#ddd5ba)}.map-elevation-100{fill:var(--map-height-100,#d4c9aa)}.map-elevation-250{fill:var(--map-height-250,#c9b999)}.map-elevation-500{fill:var(--map-height-500,#baa686)}.map-elevation-1000{fill:var(--map-height-1000,#aa9277)}.map-elevation-1500{fill:var(--map-height-1500,#987f6c)}.map-elevation-2000{fill:var(--map-height-2000,#826b60)}
.map-water{fill:var(--map-water,#b8c9c5);stroke:color-mix(in srgb,var(--map-water,#b8c9c5) 70%,#222);stroke-width:.7}
.map-forest{fill:var(--map-forest,#73806a);stroke:none;opacity:.48}.map-forest-conifer{fill:var(--map-forest-conifer,#586a5d)}.map-forest-mixed{fill:color-mix(in srgb,var(--map-forest,#73806a) 55%,var(--map-forest-conifer,#586a5d))}.map-forest-density-1{opacity:.26}.map-forest-density-2{opacity:.43}.map-forest-density-3{opacity:.62}
.map-forest-coverage{fill:none;stroke:color-mix(in srgb,var(--map-forest,#73806a) 62%,transparent);stroke-width:.7;stroke-dasharray:2 2;vector-effect:non-scaling-stroke}
.map-contour{fill:none;stroke:color-mix(in srgb,var(--map-contour,#665b49) 62%,transparent);stroke-width:.48;stroke-linecap:round;vector-effect:non-scaling-stroke}.map-contour-500,.map-contour-1000,.map-contour-1500,.map-contour-2000{stroke:var(--map-contour,#665b49);stroke-width:.72}
.map-road{fill:none;stroke:var(--map-road,#6e654f);stroke-width:1.3;stroke-linecap:round;stroke-linejoin:round;vector-effect:non-scaling-stroke}.map-road-ferry{stroke:var(--map-ferry,#625f58);stroke-dasharray:4 3}
</style>"#,
    );
    output.push_str(&format!(
        "<g id=\"strategic-map-world-v1\" data-package-sha256=\"{}\">",
        package.package_sha256
    ));
    output.push_str("<rect class=\"map-land\" x=\"0\" y=\"0\" width=\"1200\" height=\"800\"/>");
    for (band, path) in &geometry.elevation {
        output.push_str(&format!(
            "<path class=\"map-elevation map-elevation-{band}\" d=\"{path}\"/>"
        ));
    }
    output.push_str(&format!(
        "<path class=\"map-water\" d=\"{}\" fill-rule=\"evenodd\"/>",
        geometry.water
    ));
    for ((kind, density), path) in &geometry.forest {
        output.push_str(&format!(
            "<path class=\"map-forest map-forest-{kind} map-forest-density-{density}\" d=\"{path}\"/>"
        ));
    }
    output.push_str(&format!(
        "<path class=\"map-forest-coverage\" d=\"{}\"/>",
        geometry.forest_coverage
    ));
    for (elevation, path) in &geometry.contours {
        output.push_str(&format!(
            "<path class=\"map-contour map-contour-{elevation}\" d=\"{path}\"/>"
        ));
    }
    output.push_str(&format!(
        "<path class=\"map-road map-road-land\" d=\"{}\"/><path class=\"map-road map-road-ferry\" d=\"{}\"/>",
        geometry.land, geometry.ferry
    ));
    output.push_str("</g></svg>\n");
    output
}

fn geometry(package: &Package) -> Geometry {
    let mut geometry = Geometry {
        land: String::new(),
        ferry: String::new(),
        water: String::new(),
        elevation: BTreeMap::new(),
        contours: BTreeMap::new(),
        forest: BTreeMap::new(),
        forest_coverage: String::new(),
    };
    for cell in &package.elevation.cells {
        append_bounds(
            geometry.elevation.entry(cell.band_m).or_default(),
            cell.bounds,
            package.bounds,
        );
    }
    for line in &package.elevation.contours {
        append_path(
            geometry.contours.entry(line.elevation_m).or_default(),
            &line.points,
            false,
            package.bounds,
        );
    }
    for region in &package.forest.regions {
        append_bounds(
            geometry
                .forest
                .entry((region.kind.clone(), region.density))
                .or_default(),
            region.bounds,
            package.bounds,
        );
    }
    for bounds in &package.forest.coverage {
        append_bounds(&mut geometry.forest_coverage, *bounds, package.bounds);
    }
    for line in &package.roads {
        append_source_path(
            if line.kind == "ferry" {
                &mut geometry.ferry
            } else {
                &mut geometry.land
            },
            &line.points,
            false,
            package.bounds,
        );
    }
    for ring in &package.water {
        append_source_path(&mut geometry.water, ring, true, package.bounds);
    }
    geometry
}

fn append_bounds(output: &mut String, [west, south, east, north]: [f64; 4], bounds: [f64; 4]) {
    append_path(
        output,
        &[[west, north], [east, north], [east, south], [west, south]],
        true,
        bounds,
    );
}

fn append_path(output: &mut String, points: &[[f64; 2]], close: bool, bounds: [f64; 4]) {
    append_coordinates(output, points.iter().copied(), close, bounds);
}

fn append_source_path(output: &mut String, points: &[super::Point], close: bool, bounds: [f64; 4]) {
    append_coordinates(output, points.iter().map(|point| point.0), close, bounds);
}

fn append_coordinates(
    output: &mut String,
    points: impl Iterator<Item = [f64; 2]>,
    close: bool,
    bounds: [f64; 4],
) {
    for (index, point) in points.enumerate() {
        let (x, y) = project(point[0], point[1], bounds);
        output.push_str(if index == 0 { "M" } else { "L" });
        output.push_str(&format!("{x:.2},{y:.2}"));
    }
    if close {
        output.push('Z');
    }
}

fn project(longitude: f64, latitude: f64, [west, south, east, north]: [f64; 4]) -> (f64, f64) {
    let x = ((longitude - west) / (east - west) * WIDTH).clamp(0.0, WIDTH);
    let y = ((north - latitude) / (north - south) * HEIGHT).clamp(0.0, HEIGHT);
    (x, y)
}
