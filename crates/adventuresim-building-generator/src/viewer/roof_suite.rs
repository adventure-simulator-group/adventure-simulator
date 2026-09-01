#[derive(Clone, Debug, Deserialize)]
struct RoofSuiteManifest {
    fixture: String,
    view: String,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    roof_graph_hash: String,
    roof_render_item_count: usize,
    roof_render_multiset_hash: String,
    rendered_roof_item_count: usize,
    rendered_roof_hash: String,
    focused_roof_item_ids: Vec<u64>,
    visible_focused_roof_item_count: usize,
    section_removed_roof_item_ids: Vec<u64>,
    section_annotation_visible: bool,
    roof_drainage_network_ids: Vec<u64>,
    roof_drainage_channel_ids: Vec<u64>,
    roof_drainage_outlet_ids: Vec<u64>,
    roof_drainage_route_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    validation_passed: bool,
}

const ROOF_PROOF_SLUGS: [&str; 50] = [
    "roof-gable-exterior",
    "roof-gable-interior",
    "roof-gable-top",
    "roof-gable-cutaway",
    "roof-gable-drainage",
    "roof-gable-low-pitch",
    "roof-gable-mid-pitch",
    "roof-gable-high-pitch",
    "roof-hip-halfhip-exterior",
    "roof-hip-halfhip-top",
    "roof-hip-halfhip-underside",
    "roof-l-valley-exterior",
    "roof-l-valley-top",
    "roof-l-valley-underside",
    "roof-l-valley-drainage",
    "roof-courtyard-valleys-top",
    "roof-dormer-gabled-exterior",
    "roof-dormer-gabled-interior",
    "roof-dormer-gabled-top",
    "roof-dormer-gabled-cutaway",
    "roof-dormer-gabled-drainage",
    "roof-dormer-shed-exterior",
    "roof-dormer-shed-interior",
    "roof-dormer-shed-top",
    "roof-dormer-shed-cutaway",
    "roof-dormer-shed-drainage",
    "roof-cross-gable-exterior",
    "roof-cross-gable-top",
    "roof-cross-gable-underside",
    "roof-cross-gable-drainage",
    "roof-abutment-wall-exterior",
    "roof-abutment-wall-top",
    "roof-abutment-wall-cutaway",
    "roof-abutment-wall-drainage",
    "roof-abutment-tower-exterior",
    "roof-abutment-tower-top",
    "roof-abutment-tower-cutaway",
    "roof-abutment-tower-drainage",
    "roof-round-tower-exterior",
    "roof-round-tower-top",
    "roof-round-tower-cutaway",
    "roof-round-tower-drainage",
    "roof-pavilion-exterior",
    "roof-pavilion-top",
    "roof-pavilion-cutaway",
    "roof-pavilion-drainage",
    "roof-cathedral-exterior",
    "roof-cathedral-top",
    "roof-cathedral-cutaway",
    "roof-cathedral-drainage",
];

const ROOF_REGRESSION_FIXTURES: [&str; 9] = [
    "town-house",
    "hall-house",
    "fachwerk-cottage",
    "fachwerk-merchant-house",
    "renaissance-town-hall",
    "cathedral",
    "castle-gatehouse",
    "courtyard-castle",
    "walled-keep",
];

pub(crate) fn validate_roof_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut expected = ROOF_PROOF_SLUGS
        .iter()
        .map(|slug| (*slug, *slug))
        .collect::<Vec<_>>();
    let regression_names = ROOF_REGRESSION_FIXTURES
        .iter()
        .map(|fixture| (format!("roof-{fixture}-regression"), "exterior".to_owned()))
        .collect::<Vec<_>>();
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
    if actual_count != expected.len() + regression_names.len() {
        return Err(format!(
            "expected exactly 59 roof manifests, found {actual_count}"
        ));
    }
    let mut records = Vec::new();
    for (basename, view) in expected.drain(..) {
        let path = directory.join(format!("{basename}.capture.json"));
        let manifest: RoofSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((basename.to_owned(), view.to_owned(), true, manifest));
    }
    for (basename, view) in regression_names {
        let path = directory.join(format!("{basename}.capture.json"));
        let manifest: RoofSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((basename, view, false, manifest));
    }
    validate_roof_suite_records(&records)
}

fn validate_roof_suite_records(
    records: &[(String, String, bool, RoofSuiteManifest)],
) -> Result<(), String> {
    if records.len() != 59 {
        return Err(format!("expected 59 roof records, found {}", records.len()));
    }
    let first = &records[0].3;
    let mut fixture_hashes = std::collections::HashMap::new();
    let mut pitch_state_hashes = std::collections::HashSet::new();
    for (basename, expected_view, focused, manifest) in records {
        if manifest.view != *expected_view || !manifest.validation_passed {
            return Err(format!("{basename} has invalid view or failed capture QA"));
        }
        if manifest.resolver_schema_version != first.resolver_schema_version
            || manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!("{basename} comes from a mixed source build"));
        }
        if manifest.roof_render_item_count != manifest.rendered_roof_item_count
            || manifest.roof_render_multiset_hash != manifest.rendered_roof_hash
        {
            return Err(format!("{basename} has roof renderer correspondence drift"));
        }
        if *focused
            && (manifest.focused_roof_item_ids.is_empty()
                || manifest.visible_focused_roof_item_count
                    + manifest.section_removed_roof_item_ids.len()
                    != manifest.focused_roof_item_ids.len()
                || !manifest.section_annotation_visible)
        {
            return Err(format!(
                "{basename} lacks exact visible focused roof authority"
            ));
        }
        if manifest.roof_graph_hash.is_empty() {
            return Err(format!("{basename} lacks roof graph hash"));
        }
        if basename.ends_with("-drainage")
            && (manifest.roof_drainage_network_ids.is_empty()
                || manifest.roof_drainage_channel_ids.is_empty()
                || manifest.roof_drainage_outlet_ids.is_empty()
                || manifest.roof_drainage_route_ids.is_empty()
                || manifest.focused_resolved_void_ids.is_empty())
        {
            return Err(format!(
                "{basename} lacks exact focused face-channel-outlet drainage authority"
            ));
        }
        let pitch_state = basename.contains("-low-pitch")
            || basename.contains("-mid-pitch")
            || basename.contains("-high-pitch");
        let demonstrator_state = pitch_state || basename.contains("roof-round-tower-");
        if pitch_state {
            pitch_state_hashes.insert(manifest.roof_graph_hash.clone());
        }
        if demonstrator_state {
            continue;
        } else if let Some((plan_hash, roof_hash)) = fixture_hashes.get(&manifest.fixture) {
            if plan_hash != &manifest.plan_hash || roof_hash != &manifest.roof_graph_hash {
                return Err(format!(
                    "{basename} has fixture-inconsistent plan/roof hash"
                ));
            }
        } else {
            fixture_hashes.insert(
                manifest.fixture.clone(),
                (manifest.plan_hash.clone(), manifest.roof_graph_hash.clone()),
            );
        }
    }
    if pitch_state_hashes.len() != 3 {
        return Err("low/mid/high pitch handles did not produce three roof graphs".to_owned());
    }
    Ok(())
}
