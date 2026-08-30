#[derive(Clone, Debug, Deserialize)]
struct OpeningsSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    opening_profile: Option<String>,
    wall_section_kind: Option<String>,
    focused_assembly_owner_id: Option<u32>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_resolved_geometry_hash: Option<String>,
    section_cut_applied: bool,
    section_removed_item_ids: Vec<u64>,
    inside_label_visible: bool,
    outside_label_visible: bool,
    wall_thickness_metres: Option<f32>,
    scale_figure_height_metres: Option<f32>,
    scale_figure_visible: bool,
    section_annotation: String,
    section_annotation_visible: bool,
    exterior_throat_bounds_fraction: [f32; 4],
    interior_mouth_bounds_fraction: [f32; 4],
    validation_passed: bool,
}

#[derive(Clone, Copy)]
struct OpeningsProofExpectation {
    basename: &'static str,
    fixture: &'static str,
    view: &'static str,
    opening_profile: Option<&'static str>,
    wall_section_kind: Option<&'static str>,
    section: bool,
}

const OPENINGS_PROOF_SUITE: [OpeningsProofExpectation; 24] = [
    OpeningsProofExpectation {
        basename: "opening-rectangular-exterior",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-exterior",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-rectangular-interior",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-interior",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-rectangular-section",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-section",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-exterior",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-exterior",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-interior",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-interior",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-section",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-section",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-exterior",
        fixture: "cathedral",
        view: "opening-pointed-exterior",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-interior",
        fixture: "cathedral",
        view: "opening-pointed-interior",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-section",
        fixture: "cathedral",
        view: "opening-pointed-section",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-exterior",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-exterior",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-interior",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-interior",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-section",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-section",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-exterior",
        fixture: "walled-keep",
        view: "opening-gun-loop-exterior",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-interior",
        fixture: "walled-keep",
        view: "opening-gun-loop-interior",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-section",
        fixture: "walled-keep",
        view: "opening-gun-loop-section",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-timber-frame-section",
        fixture: "fachwerk-merchant-house",
        view: "wall-timber-frame-section",
        opening_profile: None,
        wall_section_kind: Some("timber_frame"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-civilian-masonry-section",
        fixture: "renaissance-town-hall",
        view: "wall-civilian-masonry-section",
        opening_profile: None,
        wall_section_kind: Some("civilian_masonry"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-cathedral-buttress-section",
        fixture: "cathedral",
        view: "wall-cathedral-buttress-section",
        opening_profile: None,
        wall_section_kind: Some("cathedral_buttress"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-round-tower-radial-section",
        fixture: "walled-keep",
        view: "wall-round-tower-radial-section",
        opening_profile: None,
        wall_section_kind: Some("round_tower_radial"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "openings-fachwerk-merchant-regression",
        fixture: "fachwerk-merchant-house",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-renaissance-town-hall-regression",
        fixture: "renaissance-town-hall",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-cathedral-regression",
        fixture: "cathedral",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-courtyard-castle-regression",
        fixture: "courtyard-castle",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-walled-keep-regression",
        fixture: "walled-keep",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
];

pub(crate) fn validate_openings_suite(directory: &std::path::Path) -> Result<(), String> {
    let actual_count = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".capture.json")
        })
        .count();
    if actual_count != OPENINGS_PROOF_SUITE.len() {
        return Err(format!(
            "expected exactly 24 proof manifests, found {actual_count}"
        ));
    }
    let mut records = Vec::new();
    for expected in OPENINGS_PROOF_SUITE {
        let path = directory.join(format!("{}.capture.json", expected.basename));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest: OpeningsSuiteManifest = serde_json::from_slice(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((expected, manifest));
    }
    validate_openings_suite_records(&records)
}

fn validate_openings_suite_records(
    records: &[(OpeningsProofExpectation, OpeningsSuiteManifest)],
) -> Result<(), String> {
    if records.len() != OPENINGS_PROOF_SUITE.len() {
        return Err(format!(
            "expected exactly 24 proof records, found {}",
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixture_hashes = std::collections::HashMap::new();
    let mut opening_focuses = std::collections::HashMap::new();
    for (expected, manifest) in records {
        if manifest.fixture != expected.fixture
            || manifest.view != expected.view
            || manifest.seed != 42
            || manifest.opening_profile.as_deref() != expected.opening_profile
            || manifest.wall_section_kind.as_deref() != expected.wall_section_kind
            || manifest.resolver_schema_version != 2
            || !manifest.validation_passed
        {
            return Err(format!(
                "proof {} violates its expectation",
                expected.basename
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "proof {} comes from a mixed source build",
                expected.basename
            ));
        }
        if let Some((plan_hash, geometry_hash)) = fixture_hashes.get(expected.fixture) {
            if plan_hash != &manifest.plan_hash || geometry_hash != &manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "proof {} has stale fixture hashes",
                    expected.basename
                ));
            }
        } else {
            fixture_hashes.insert(
                expected.fixture,
                (
                    manifest.plan_hash.clone(),
                    manifest.resolved_geometry_hash.clone(),
                ),
            );
        }
        let focused = expected.opening_profile.is_some() || expected.wall_section_kind.is_some();
        if focused
            && (manifest.focused_assembly_owner_id.is_none()
                || manifest.focused_resolved_item_ids.is_empty()
                || manifest.focused_resolved_geometry_hash.is_none()
                || (expected.opening_profile.is_some()
                    && manifest.focused_resolved_void_ids.is_empty()))
        {
            return Err(format!(
                "proof {} lacks exact focused geometry",
                expected.basename
            ));
        }
        if let Some(profile) = expected.opening_profile {
            let state = (
                manifest.focused_assembly_owner_id,
                manifest.focused_resolved_item_ids.clone(),
                manifest.focused_resolved_void_ids.clone(),
                manifest.focused_resolved_geometry_hash.clone(),
            );
            if let Some(previous) = opening_focuses.get(profile) {
                if previous != &state {
                    return Err(format!(
                        "proof {} drifts from its opening triple",
                        expected.basename
                    ));
                }
            } else {
                opening_focuses.insert(profile, state);
            }
        } else if !focused
            && (manifest.focused_assembly_owner_id.is_some()
                || !manifest.focused_resolved_item_ids.is_empty()
                || !manifest.focused_resolved_void_ids.is_empty()
                || manifest.focused_resolved_geometry_hash.is_some())
        {
            return Err(format!(
                "regression {} contains stale focused proof state",
                expected.basename
            ));
        }
        if expected.section
            && (!manifest.section_cut_applied
                || !manifest.inside_label_visible
                || !manifest.outside_label_visible
                || manifest
                    .wall_thickness_metres
                    .is_none_or(|value| value <= 0.0)
                || manifest.scale_figure_height_metres != Some(1.75)
                || !manifest.scale_figure_visible
                || !manifest.section_annotation_visible
                || !manifest.section_annotation.contains("wall=")
                || !manifest.section_annotation.contains("opening=")
                || !manifest.section_annotation.contains("profile=")
                || !manifest.section_annotation.contains("thickness=")
                || (expected.wall_section_kind != Some("round_tower_radial")
                    && manifest.section_removed_item_ids.is_empty())
                || manifest
                    .section_removed_item_ids
                    .iter()
                    .any(|id| !manifest.focused_resolved_item_ids.contains(id)))
        {
            return Err(format!(
                "proof {} is not a genuine labeled section",
                expected.basename
            ));
        }
        if matches!(expected.opening_profile, Some("arrow_loop" | "gun_loop")) {
            let valid_bounds = |bounds: [f32; 4]| {
                bounds[0] >= 0.0
                    && bounds[1] >= 0.0
                    && bounds[2] > bounds[0]
                    && bounds[3] > bounds[1]
                    && bounds[2] <= 1.0
                    && bounds[3] <= 1.0
            };
            if !valid_bounds(manifest.exterior_throat_bounds_fraction)
                || !valid_bounds(manifest.interior_mouth_bounds_fraction)
            {
                return Err(format!(
                    "proof {} does not project both military throat and mouth",
                    expected.basename
                ));
            }
        }
    }
    Ok(())
}
