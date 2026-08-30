fn focus_name_matches(focus: Option<&str>, name: &str) -> bool {
    match focus {
        Some("gate_exterior") => {
            name.contains("gate guard chamber")
                || name.contains("gate leaf")
                || name.contains("portcullis")
                || name.contains("firing loop")
        }
        Some("gate_interior_section") => {
            name.contains("gate guard chamber")
                || name.contains("gate access")
                || name.contains("floor-level guard chamber door")
                || name.contains("gate leaf")
                || name.contains("portcullis")
        }
        Some("tower_portal") => {
            name.contains("tower entrance")
                || name.contains("portal landing")
                || name.contains("spiral stair")
                || name.contains("tower-top deck")
        }
        Some("resolved_crown") => name.contains("resolved crown owner"),
        Some("resolved_projected") => name.contains("resolved projected owner"),
        Some("resolved_roof") => name.contains("resolved roof"),
        Some("resolved_opening") => name.contains("resolved crown owner"),
        Some("resolved_wall_section") => {
            name.contains("resolved crown owner") || name.contains("architectural section")
        }
        Some("resolved_church_program") => {
            name.contains("resolved crown owner") || name.contains("resolved roof")
        }
        Some("resolved_timber_frame") => name.contains("resolved crown owner"),
        Some("artillery_assembly") => {
            name.contains("Artillery")
                || name.contains("DitchFloor")
                || name.contains("OpeningJamb")
                || name.contains("OpeningHead")
                || name.contains("WeaponMount")
        }
        None => true,
        _ => false,
    }
}

fn focus_requirements_met(
    focus: Option<&str>,
    visible_names: &[String],
    focused_tower_count: usize,
) -> bool {
    let count = |needle: &str| {
        visible_names
            .iter()
            .filter(|name| name.contains(needle))
            .count()
    };
    match focus {
        Some("gate_exterior") => {
            focused_tower_count >= 2
                && count("round tower shell with open firing loops") >= 2
                && count("closed heavy gate leaf") >= 2
                && count("portcullis vertical bar") >= 2
                && count("gate guard chamber") >= 4
                && count("outward firing opening") >= 1
        }
        Some("gate_interior_section") => {
            count("closed heavy gate leaf") >= 2
                && count("portcullis vertical bar") >= 2
                && count("gate guard chamber floor") >= 1
                && count("gate guard chamber access stair") >= 5
                && count("gate access top landing") >= 1
                && count("gate access bottom landing") >= 1
                && count("gate access support post") >= 4
                && count("gate access continuous edge guard") >= 4
                && count("gate access landing perimeter guard") >= 4
                && count("gate access masonry wall ledger") >= 1
                && count("gate access diagonal lateral brace") >= 6
                && count("floor-level guard chamber door") >= 1
                && count("floor around downward opening") >= 1
                && count("portcullis operating windlass") >= 1
        }
        Some("tower_portal") => {
            focused_tower_count == 1
                && count("tower entrance jamb") >= 2
                && count("portal landing") >= 1
                && count("spiral stair tread") >= 5
                && count("tower-top deck") >= 1
        }
        Some("resolved_crown") => {
            count("Breastwork") >= 1
                && count("Merlon") >= 1
                && count("Coping") >= 1
                && count("EdgeGuard") >= 1
        }
        Some("resolved_projected") => {
            count("GalleryFloor")
                + count("ProjectionSupport")
                + count("FrameMember")
                + count("BartizanShell")
                + count("DefenseHostWall")
                + count("CircuitWalk")
                + count("BeamJoist")
                + count("DrainageFloor")
                >= 3
        }
        Some("resolved_roof") => count("resolved roof") >= 1,
        Some("resolved_opening") => count("OpeningJamb") >= 1 && count("OpeningHead") >= 1,
        Some("resolved_wall_section") => {
            count("WallHost") >= 1 || count("architectural section") >= 3
        }
        // Exact target IDs/roles/cut bounds are validated independently for
        // church proofs.  An isolated two-pier bay or two-beam bell frame can
        // intentionally contain fewer than eight `Church*` meshes.
        Some("resolved_church_program") => count("Church") >= 1 || count("RoofGutter") >= 1,
        Some("resolved_timber_frame") => {
            count("FrameSill")
                + count("FramePost")
                + count("FramePlate")
                + count("FrameRail")
                + count("FrameBrace")
                + count("FrameJettyBeam")
                + count("FrameKnagge")
                + count("FrameFloor")
                + count("FrameJoist")
                + count("FrameGirder")
                + count("FrameGableMember")
                + count("FrameDormerTrimmer")
                + count("Landing")
                >= 2
        }
        Some("artillery_assembly") => {
            count("ArtilleryRevetment")
                + count("ArtilleryEarthCore")
                + count("ArtilleryRetainingWall")
                + count("ArtilleryTerreplein")
                + count("ArtilleryParapet")
                + count("ArtilleryBridge")
                + count("DitchFloor")
                >= 2
        }
        None => true,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LuminanceStats {
    median: u8,
    shadow: u8,
    separation: u8,
    dark_clipped_bps: u16,
    bright_clipped_bps: u16,
}

fn luminance_stats(
    data: Option<&[u8]>,
    region: Option<[f32; 4]>,
    region_margin: f32,
) -> LuminanceStats {
    let Some(data) = data else {
        return LuminanceStats::default();
    };
    let (pixels, _) = data.as_chunks::<4>();
    if pixels.is_empty() {
        return LuminanceStats::default();
    }
    // Close proof views measure their exact resolved-item screen bounds rather
    // than allowing the sky to dominate the quartiles. This is the named-stone
    // surface option in the screenshot QA contract; full views still sample
    // the complete frame.
    let [mut min_x, mut min_y, mut max_x, mut max_y] = region.unwrap_or([0.0, 0.0, 1.0, 1.0]);
    if region.is_some() {
        // Include the immediately supporting masonry so the sample contains a
        // key-facing plane and its cast-shadow/perpendicular return, not only
        // the four small fingerprint anchors.
        min_x = (min_x - region_margin).max(0.0);
        min_y = (min_y - region_margin).max(0.0);
        max_x = (max_x + region_margin).min(1.0);
        max_y = (max_y + region_margin).min(1.0);
    }
    let mut values = pixels
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let x = (*index % VIEW_WIDTH as usize) as f32 / VIEW_WIDTH as f32;
            let y = (*index / VIEW_WIDTH as usize) as f32 / VIEW_HEIGHT as f32;
            x >= min_x && x <= max_x && y >= min_y && y <= max_y
        })
        .map(|(_, pixel)| {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
            .round() as u8
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return LuminanceStats::default();
    }
    let dark = values.iter().filter(|&&value| value <= 5).count();
    let bright = values.iter().filter(|&&value| value >= 250).count();
    values.sort_unstable();
    let percentile = |percent: usize| values[(values.len() - 1) * percent / 100];
    let shadow = percentile(25);
    let key = percentile(75);
    LuminanceStats {
        median: ((u16::from(percentile(50)) * 100) / 255) as u8,
        shadow: ((u16::from(shadow) * 100) / 255) as u8,
        separation: ((u16::from(key.saturating_sub(shadow)) * 100) / 255) as u8,
        dark_clipped_bps: (dark.saturating_mul(10_000) / values.len()).min(10_000) as u16,
        bright_clipped_bps: (bright.saturating_mul(10_000) / values.len()).min(10_000) as u16,
    }
}

fn calibration_luminance_stats(data: Option<&[u8]>, bounds: [f32; 4]) -> LuminanceStats {
    let mut stats = luminance_stats(data, Some(bounds), 0.0);
    let [min_x, min_y, max_x, max_y] = bounds;
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    let mut patch_medians = [0_u8; 4];
    for (index, patch) in [
        [min_x, min_y, mid_x, mid_y],
        [mid_x, min_y, max_x, mid_y],
        [min_x, mid_y, mid_x, max_y],
        [mid_x, mid_y, max_x, max_y],
    ]
    .into_iter()
    .enumerate()
    {
        patch_medians[index] = luminance_stats(data, Some(patch), 0.0).median;
    }
    patch_medians.sort_unstable();
    let (calibration_shadow, calibration_span) = luminance_percentile_span(data, bounds, 5, 95);
    stats.shadow = patch_medians[0].min(calibration_shadow);
    stats.separation = patch_medians[3]
        .saturating_sub(patch_medians[0])
        .max(calibration_span);
    stats
}

fn luminance_percentile_span(
    data: Option<&[u8]>,
    bounds: [f32; 4],
    low_percent: usize,
    high_percent: usize,
) -> (u8, u8) {
    let Some(data) = data else {
        return (0, 0);
    };
    let (pixels, _) = data.as_chunks::<4>();
    let [min_x, min_y, max_x, max_y] = bounds;
    let mut values = pixels
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let x = (*index % VIEW_WIDTH as usize) as f32 / VIEW_WIDTH as f32;
            let y = (*index / VIEW_WIDTH as usize) as f32 / VIEW_HEIGHT as f32;
            x >= min_x && x <= max_x && y >= min_y && y <= max_y
        })
        .map(|(_, pixel)| {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
            .round() as u8
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return (0, 0);
    }
    values.sort_unstable();
    let percentile = |percent: usize| values[(values.len() - 1) * percent / 100];
    let low = percentile(low_percent);
    let high = percentile(high_percent);
    (
        ((u16::from(low) * 100) / 255) as u8,
        ((u16::from(high.saturating_sub(low)) * 100) / 255) as u8,
    )
}

fn subject_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else {
        return 0;
    };
    let (pixels, _) = data.as_chunks::<4>();
    let Some((reference, remaining)) = pixels.split_first() else {
        return 0;
    };
    let mut total = 1_usize;
    let mut different = 0_usize;
    for pixel in remaining {
        total += 1;
        if pixel[..3]
            .iter()
            .zip(&reference[..3])
            .any(|(channel, background)| channel.abs_diff(*background) > 8)
        {
            different += 1;
        }
    }
    (different.saturating_mul(10_000) / total).min(10_000) as u16
}
