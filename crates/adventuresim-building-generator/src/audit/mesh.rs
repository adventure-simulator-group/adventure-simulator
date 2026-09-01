#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeshAuditReport {
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    pub inconsistent_winding_edges: usize,
    pub degenerate_triangles: usize,
    pub inverted_winding: bool,
}

impl MeshAuditReport {
    pub const fn passes_closed_solid(self) -> bool {
        self.boundary_edges == 0
            && self.nonmanifold_edges == 0
            && self.inconsistent_winding_edges == 0
            && self.degenerate_triangles == 0
            && !self.inverted_winding
    }
}

pub fn audit_triangle_mesh(positions: &[[f32; 3]], indices: &[u32]) -> MeshAuditReport {
    type Point = [i64; 3];
    let quantize = |position: [f32; 3]| -> Point {
        position.map(|component| (component * 10_000.0).round() as i64)
    };
    let points = positions.iter().copied().map(quantize).collect::<Vec<_>>();
    let mut edges: BTreeMap<(Point, Point), (usize, i32)> = BTreeMap::new();
    let mut report = MeshAuditReport::default();
    let mut signed_volume_x6 = 0.0_f64;
    let (triangles, remainder) = indices.as_chunks::<3>();
    report.degenerate_triangles += usize::from(!remainder.is_empty());
    for triangle in triangles {
        let (Some(&a), Some(&b), Some(&c)) = (
            points.get(triangle[0] as usize),
            points.get(triangle[1] as usize),
            points.get(triangle[2] as usize),
        ) else {
            report.degenerate_triangles += 1;
            continue;
        };
        if a == b || b == c || c == a {
            report.degenerate_triangles += 1;
            continue;
        }
        let af = a.map(|component| component as f64);
        let bf = b.map(|component| component as f64);
        let cf = c.map(|component| component as f64);
        signed_volume_x6 += af[0] * (bf[1] * cf[2] - bf[2] * cf[1])
            + af[1] * (bf[2] * cf[0] - bf[0] * cf[2])
            + af[2] * (bf[0] * cf[1] - bf[1] * cf[0]);
        for (from, to) in [(a, b), (b, c), (c, a)] {
            let (key, direction) = if from < to {
                ((from, to), 1)
            } else {
                ((to, from), -1)
            };
            let edge = edges.entry(key).or_default();
            edge.0 += 1;
            edge.1 += direction;
        }
    }
    for (count, winding) in edges.into_values() {
        match count {
            1 => report.boundary_edges += 1,
            2 if winding != 0 => report.inconsistent_winding_edges += 1,
            2 => {}
            _ => report.nonmanifold_edges += 1,
        }
    }
    report.inverted_winding = signed_volume_x6 < -0.5;
    report
}
