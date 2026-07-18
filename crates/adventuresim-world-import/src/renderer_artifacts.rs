//! Deterministic, distributable whole-world map artifacts.
//!
//! Travel edges currently contain endpoints, not polylines, so roads are intentionally
//! rendered as straight topology segments. Regional packages/raster tiles are follow-ups.

use crate::{Error, Result};
use adventuresim_render_contracts::{
    Bounds, MapManifest, MapPackage, Point, RENDER_SCHEMA_VERSION, RoadFeature, SettlementFeature,
    SourceNotice,
};
use adventuresim_world_schema::{CompiledWorld, TravelRoute};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq)]
pub struct RendererArtifactPaths {
    pub manifest: PathBuf,
    pub package: PathBuf,
    pub paper_map: PathBuf,
}

pub fn build(
    world: &CompiledWorld,
    world_artifact_id: &str,
    output_dir: &Path,
) -> Result<RendererArtifactPaths> {
    let mut nodes: BTreeMap<u64, Point> = world
        .nodes
        .iter()
        .map(|node| {
            (
                node.id,
                Point {
                    x: node.longitude,
                    y: node.latitude,
                },
            )
        })
        .collect();
    let mut settlements: Vec<_> = world
        .settlements
        .iter()
        .map(|s| SettlementFeature {
            id: s.id.clone(),
            name: s.name.clone(),
            point: Point {
                x: s.longitude,
                y: s.latitude,
            },
            population_level: s.population_level,
        })
        .collect();
    settlements.sort_by(|a, b| a.id.cmp(&b.id));
    // Demo/import-light worlds can have settlements without the corresponding node list.
    for settlement in &world.settlements {
        nodes.entry(settlement.source_node_id).or_insert(Point {
            x: settlement.longitude,
            y: settlement.latitude,
        });
    }
    let mut roads: Vec<_> = world
        .edges
        .iter()
        .filter_map(|edge| {
            Some(RoadFeature {
                id: edge.id.to_string(),
                from: *nodes.get(&edge.from_node_id)?,
                to: *nodes.get(&edge.to_node_id)?,
                ferry: matches!(edge.route, TravelRoute::Ferry(_)),
            })
        })
        .collect();
    roads.sort_by(|a, b| a.id.cmp(&b.id));
    let points: Vec<_> = settlements
        .iter()
        .map(|s| s.point)
        .chain(roads.iter().flat_map(|r| [r.from, r.to]))
        .collect();
    let bounds = bounds(&points).unwrap_or(Bounds {
        min: Point { x: 9.5, y: 53.5 },
        max: Point { x: 11.0, y: 54.5 },
    });
    let package = MapPackage {
        renderer_schema: RENDER_SCHEMA_VERSION,
        bounds,
        settlements,
        roads,
    };
    package
        .validate()
        .map_err(|e| Error::Validation(e.to_string()))?;
    let mut package_bytes = serde_json::to_vec(&package)?;
    package_bytes.push(b'\n');
    let package_hash = blake3::hash(&package_bytes).to_hex().to_string();
    let package_name = format!("map-{package_hash}.json");
    let source_notices: Vec<_> = world
        .metadata
        .sources
        .iter()
        .map(|source| SourceNotice {
            name: source.name.clone(),
            canonical_url: source.canonical_url.clone(),
            required_notices: source.required_notices.clone(),
        })
        .collect();
    let svg = paper_svg(&package, &source_notices);
    let svg_hash = blake3::hash(svg.as_bytes()).to_hex().to_string();
    let svg_name = format!("paper-map-{svg_hash}.svg");
    let manifest = MapManifest {
        renderer_schema: RENDER_SCHEMA_VERSION,
        world_schema: world.metadata.schema_version,
        artifact_id: world_artifact_id.into(),
        manifest_digest: world.metadata.manifest_digest.clone(),
        package_hash,
        package_url: format!("/tactical/map/{package_name}"),
        paper_map_url: format!("/tactical/map/{svg_name}"),
        bounds,
        sources: source_notices,
    };
    manifest
        .validate()
        .map_err(|e| Error::Validation(e.to_string()))?;
    std::fs::create_dir_all(output_dir)?;
    let paths = RendererArtifactPaths {
        manifest: output_dir.join("manifest.json"),
        package: output_dir.join(package_name),
        paper_map: output_dir.join(svg_name),
    };
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    std::fs::write(&paths.package, package_bytes)?;
    std::fs::write(&paths.paper_map, svg)?;
    std::fs::write(&paths.manifest, manifest_bytes)?;
    Ok(paths)
}

fn bounds(points: &[Point]) -> Option<Bounds> {
    let first = *points.first()?;
    Some(points.iter().skip(1).fold(
        Bounds {
            min: first,
            max: first,
        },
        |mut b, p| {
            b.min.x = b.min.x.min(p.x);
            b.min.y = b.min.y.min(p.y);
            b.max.x = b.max.x.max(p.x);
            b.max.y = b.max.y.max(p.y);
            b
        },
    ))
}

fn paper_svg(package: &MapPackage, sources: &[SourceNotice]) -> String {
    let w = 1000.;
    let h = 650.;
    let dx = (package.bounds.max.x - package.bounds.min.x).max(0.001);
    let dy = (package.bounds.max.y - package.bounds.min.y).max(0.001);
    let map = |p: Point| {
        (
            (p.x - package.bounds.min.x) / dx * w,
            h - (p.y - package.bounds.min.y) / dy * h,
        )
    };
    let mut out = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"-30 -30 1060 710\" role=\"img\" aria-labelledby=\"title desc\"><title id=\"title\">Adventure Simulator world map</title><desc id=\"desc\">A topology map of settlements and straight road segments.</desc><rect x=\"-30\" y=\"-30\" width=\"1060\" height=\"710\" fill=\"#d9c79c\"/>",
    );
    for r in &package.roads {
        let (a, b) = (map(r.from), map(r.to));
        let dash = if r.ferry {
            " stroke-dasharray=\"8 8\""
        } else {
            ""
        };
        let _ = write!(
            out,
            "<path d=\"M{:.1},{:.1} L{:.1},{:.1}\" stroke=\"#594b35\" stroke-width=\"2\"{} fill=\"none\"/>",
            a.0, a.1, b.0, b.1, dash
        );
    }
    for s in &package.settlements {
        let p = map(s.point);
        let name = html_escape::encode_double_quoted_attribute(&s.name);
        let _ = write!(
            out,
            "<g><circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{}\" fill=\"#7b1e1e\"/><text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" fill=\"#211b13\">{}</text></g>",
            p.0,
            p.1,
            3 + s.population_level.max(0),
            p.0 + 9.,
            p.1 - 7.,
            name
        );
    }
    out.push_str("<metadata>");
    for source in sources {
        out.push_str("<source><name>");
        out.push_str(&html_escape::encode_text(&source.name));
        out.push_str("</name><url>");
        out.push_str(&html_escape::encode_text(&source.canonical_url));
        out.push_str("</url>");
        for notice in &source.required_notices {
            out.push_str("<notice>");
            out.push_str(&html_escape::encode_text(notice));
            out.push_str("</notice>");
        }
        out.push_str("</source>");
    }
    out.push_str("</metadata></svg>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_are_ordered() {
        let b = bounds(&[Point { x: 2., y: -1. }, Point { x: -3., y: 5. }]).unwrap();
        assert_eq!(b.min, Point { x: -3., y: -1. });
        assert_eq!(b.max, Point { x: 2., y: 5. });
    }

    #[test]
    fn paper_svg_contains_escaped_standalone_source_attribution() {
        let package = MapPackage {
            renderer_schema: RENDER_SCHEMA_VERSION,
            bounds: Bounds {
                min: Point { x: 0., y: 0. },
                max: Point { x: 1., y: 1. },
            },
            settlements: vec![],
            roads: vec![],
        };
        let svg = paper_svg(
            &package,
            &[SourceNotice {
                name: "Atlas & Archive".into(),
                canonical_url: "https://example.test/?a=1&b=2".into(),
                required_notices: vec!["Credit <required>".into()],
            }],
        );
        assert!(svg.contains("Atlas &amp; Archive"));
        assert!(svg.contains("https://example.test/?a=1&amp;b=2"));
        assert!(svg.contains("Credit &lt;required&gt;"));
    }
}
