#[derive(Clone, Debug, Deserialize)]
struct CrownSuiteManifest {
    fixture: String,
    view: String,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    validation_passed: bool,
}

const CROWN_PROOF_SUITE: [(&str, &str, &str); 9] = [
    (
        "crown-straight-exterior",
        "courtyard-castle",
        "crown-straight-exterior",
    ),
    (
        "crown-straight-interior",
        "courtyard-castle",
        "crown-straight-interior",
    ),
    (
        "crown-corner-exterior",
        "walled-keep",
        "crown-corner-exterior",
    ),
    (
        "crown-corner-interior",
        "walled-keep",
        "crown-corner-interior",
    ),
    (
        "crown-gate-tower-exterior",
        "walled-keep",
        "crown-tower-exterior",
    ),
    ("crown-gate-tower-top", "walled-keep", "crown-tower-top"),
    (
        "crown-gate-tower-cutaway",
        "walled-keep",
        "crown-tower-cutaway",
    ),
    ("crown-courtyard-regression", "courtyard-castle", "exterior"),
    ("crown-walled-keep-regression", "walled-keep", "exterior"),
];

fn validate_crown_suite_records(records: &[(&str, CrownSuiteManifest)]) -> Result<(), String> {
    if records.len() != CROWN_PROOF_SUITE.len() {
        return Err(format!(
            "expected {} crown proof manifests, found {}",
            CROWN_PROOF_SUITE.len(),
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixtures = std::collections::HashMap::<&str, (&str, &str)>::new();
    for ((actual_name, manifest), (expected_name, expected_fixture, expected_view)) in
        records.iter().zip(CROWN_PROOF_SUITE)
    {
        if *actual_name != expected_name
            || manifest.fixture != expected_fixture
            || manifest.view != expected_view
        {
            return Err(format!(
                "proof {actual_name} does not match expected {expected_name}/{expected_fixture}/{expected_view}"
            ));
        }
        if !manifest.validation_passed || manifest.resolver_schema_version != 2 {
            return Err(format!(
                "proof {actual_name} is invalid or not resolver schema 2"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "proof {actual_name} comes from a mixed source build"
            ));
        }
        if let Some((plan_hash, resolved_hash)) = fixtures.get(expected_fixture) {
            if *plan_hash != manifest.plan_hash || *resolved_hash != manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "proof {actual_name} disagrees with its fixture plan/resolved hash"
                ));
            }
        } else {
            fixtures.insert(
                expected_fixture,
                (&manifest.plan_hash, &manifest.resolved_geometry_hash),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_crown_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut owned = Vec::with_capacity(CROWN_PROOF_SUITE.len());
    for (basename, _, _) in CROWN_PROOF_SUITE {
        let path = directory.join(format!("{basename}.capture.json"));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest = serde_json::from_slice::<CrownSuiteManifest>(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        owned.push((basename, manifest));
    }
    validate_crown_suite_records(&owned)
}
