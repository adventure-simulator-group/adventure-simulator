//! Senescent material treatment for the shared pedunculate-oak leaf form.
//!
//! Drying changes pigment and relief, not the species-defining lobe layout.
//! The silhouette and venation therefore remain owned by `foliage` while this
//! module owns the independently reviewable dry-state response.

pub(super) fn relief(living_height: f32, u: f32, v: f32) -> f32 {
    let blade_y = 1.0 - v;
    let t = ((blade_y - 0.075) / 0.85).clamp(0.0, 1.0);
    let axis = 0.026 * (t - 0.42).powi(2) - 0.004;
    let transverse = ((u - 0.5 - axis) / 0.19).clamp(-1.0, 1.0);

    // Broad margin lift and low-amplitude puckering read as a dried lamina at
    // close range while keeping the shared molded-material silhouette intact.
    let edge_curl =
        transverse.abs().powf(2.4) * (0.055 + 0.018 * (t * core::f32::consts::TAU + 0.8).sin());
    let pucker = ((u * 23.0 + v * 17.0).sin() + (u * 11.0 - v * 29.0 + 0.6).cos()) * 0.006;
    (living_height * 0.82 + edge_curl + pucker).clamp(0.012, 0.34)
}

pub(super) fn albedo(
    blade: [u8; 3],
    vein: [u8; 3],
    back_blade: [u8; 3],
    is_vein: bool,
    _tissue_mottle: f32,
) -> ([u8; 3], [u8; 3]) {
    if is_vein {
        return (vein, vein.map(|channel| channel.saturating_add(11)));
    }

    (blade, back_blade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_relief_is_deterministic_and_bounded() {
        let first = relief(0.21, 0.18, 0.63);
        assert_eq!(first.to_bits(), relief(0.21, 0.18, 0.63).to_bits());
        for y in 0..=32 {
            for x in 0..=32 {
                let height = relief(0.21, x as f32 / 32.0, y as f32 / 32.0);
                assert!((0.012..=0.34).contains(&height));
            }
        }
    }
}
