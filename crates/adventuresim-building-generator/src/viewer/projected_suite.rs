#[derive(Clone, Debug, Deserialize)]
struct ProjectedSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    focus_kind: Option<String>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_projected_ray_count: usize,
    projected_defense_kind: Option<String>,
    projected_defense_deployment: Option<String>,
    projected_tactical_target: Option<String>,
    validation_passed: bool,
}

#[derive(Clone, Copy)]
struct ProjectedProofExpectation {
    basename: &'static str,
    fixture: &'static str,
    view: &'static str,
    seed: u64,
    kind: Option<&'static str>,
    deployment: Option<&'static str>,
}

const PROJECTED_PROOF_SUITE: [ProjectedProofExpectation; 23] = [
    ProjectedProofExpectation {
        basename: "machicolation-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-longitudinal",
        fixture: "castle-gatehouse",
        view: "projected-longitudinal",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-sockets",
        fixture: "castle-gatehouse",
        view: "projected-sockets",
        seed: 42,
        kind: Some("hoarding"),
        deployment: Some("sockets_only"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-longitudinal",
        fixture: "castle-gatehouse",
        view: "projected-longitudinal",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-flank",
        fixture: "castle-gatehouse",
        view: "projected-flank",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "projected-castle-gatehouse-regression",
        fixture: "castle-gatehouse",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
    ProjectedProofExpectation {
        basename: "projected-courtyard-regression",
        fixture: "courtyard-castle",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
    ProjectedProofExpectation {
        basename: "projected-walled-keep-regression",
        fixture: "walled-keep",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
];

fn validate_projected_suite_records(
    records: &[(&str, ProjectedSuiteManifest)],
) -> Result<(), String> {
    if records.len() != PROJECTED_PROOF_SUITE.len() {
        return Err(format!(
            "expected {} projected-defense manifests, found {}",
            PROJECTED_PROOF_SUITE.len(),
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixtures = std::collections::HashMap::<(&str, u64), (&str, &str)>::new();
    for ((actual_name, manifest), expected) in records.iter().zip(PROJECTED_PROOF_SUITE) {
        if *actual_name != expected.basename
            || manifest.fixture != expected.fixture
            || manifest.view != expected.view
            || manifest.seed != expected.seed
            || manifest.projected_defense_kind.as_deref() != expected.kind
            || manifest.projected_defense_deployment.as_deref() != expected.deployment
        {
            return Err(format!(
                "projected proof {actual_name} does not match its expected fixture/view/seed/state"
            ));
        }
        if !manifest.validation_passed || manifest.resolver_schema_version != 2 {
            return Err(format!(
                "projected proof {actual_name} is invalid or not resolver schema 2"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "projected proof {actual_name} comes from a mixed source build"
            ));
        }
        if expected.kind.is_some()
            && (manifest.focus_kind.as_deref() != Some("resolved_projected")
                || manifest.focused_resolved_item_ids.is_empty()
                || manifest.focused_resolved_void_ids.is_empty()
                    && expected.deployment != Some("sockets_only")
                || manifest.focused_projected_ray_count == 0
                    && expected.deployment != Some("sockets_only")
                || manifest.projected_tactical_target.is_none())
        {
            return Err(format!(
                "projected proof {actual_name} lacks exact assembly IDs, voids, rays, or tactical target"
            ));
        }
        let fixture_key = (expected.fixture, expected.seed);
        if let Some((plan_hash, resolved_hash)) = fixtures.get(&fixture_key) {
            if *plan_hash != manifest.plan_hash || *resolved_hash != manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "projected proof {actual_name} disagrees with its fixture/seed hashes"
                ));
            }
        } else {
            fixtures.insert(
                fixture_key,
                (&manifest.plan_hash, &manifest.resolved_geometry_hash),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_projected_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut owned = Vec::with_capacity(PROJECTED_PROOF_SUITE.len());
    for expected in PROJECTED_PROOF_SUITE {
        let path = directory.join(format!("{}.capture.json", expected.basename));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest = serde_json::from_slice::<ProjectedSuiteManifest>(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        owned.push((expected.basename, manifest));
    }
    validate_projected_suite_records(&owned)
}
