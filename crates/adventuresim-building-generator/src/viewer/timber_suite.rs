fn timber_proof_specs() -> Vec<(String, BuildingArchetype, ViewerView)> {
    let mut specs = Vec::new();
    for archetype in TIMBER_ARCHETYPES {
        for view in [
            ViewerView::TimberWholeExterior,
            ViewerView::TimberFrameFacade,
            ViewerView::TimberRegistrationCut,
            ViewerView::TimberSupportLoad,
            ViewerView::TimberProgramDetail,
        ] {
            let suffix = timber_proof_suffix(view).expect("timber view suffix");
            specs.push((
                format!("timber-{}-{suffix}", archetype.slug()),
                archetype,
                view,
            ));
        }
    }
    specs.extend([
        (
            "timber-opening-bay-exterior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBayExterior,
        ),
        (
            "timber-opening-bay-interior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBayInterior,
        ),
        (
            "timber-opening-bay-section".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBaySection,
        ),
        (
            "timber-joint-close".to_owned(),
            BuildingArchetype::TownHouse,
            ViewerView::TimberJointClose,
        ),
        (
            "timber-jetty-exterior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyExterior,
        ),
        (
            "timber-jetty-underside".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyUnderside,
        ),
        (
            "timber-jetty-load".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyLoad,
        ),
        (
            "timber-gable-roof-bearing".to_owned(),
            BuildingArchetype::FachwerkCottage,
            ViewerView::TimberGableRoofBearing,
        ),
        (
            "timber-dormer-trimmer".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberDormerTrimmer,
        ),
        (
            "timber-townhall-masonry-junction".to_owned(),
            BuildingArchetype::RenaissanceTownHall,
            ViewerView::TimberTownHallJunction,
        ),
    ]);
    specs
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TimberSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    resolved_geometry_hash: String,
    timber_program_hash: String,
    timber_program: Option<String>,
    timber_assembly_id: Option<u64>,
    timber_member_ids: Vec<u64>,
    timber_joint_ids: Vec<u64>,
    timber_node_ids: Vec<u64>,
    timber_focused_roles: Vec<String>,
    timber_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    timber_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    timber_target_component_ids: Vec<String>,
    timber_focus_interface_ids: Vec<u64>,
    timber_required_roles: Vec<String>,
    timber_cut_plane: Option<[f32; 4]>,
    timber_removed_target_item_ids: Vec<u64>,
    timber_legend_visible: bool,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_roof_item_ids: Vec<u64>,
    section_removed_item_ids: Vec<u64>,
    visible_focused_resolved_item_count: usize,
    focused_bounds_fraction: [f32; 4],
    section_cut_applied: bool,
    section_annotation_visible: bool,
    pixel_hash: String,
    plan_audit_issue_count: usize,
    validation_passed: bool,
}

pub(crate) fn validate_timber_suite(directory: &std::path::Path) -> Result<(), String> {
    let specs = timber_proof_specs();
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
    if actual_count != specs.len() {
        return Err(format!(
            "expected exactly {} timber manifests, found {actual_count}",
            specs.len()
        ));
    }
    let mut records = Vec::new();
    for (slug, archetype, view) in specs {
        let path = directory.join(format!("{slug}.capture.json"));
        let manifest: TimberSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((slug, archetype, view, manifest));
    }
    validate_timber_suite_records(&records)
}

pub(crate) fn validate_artillery_suite(directory: &std::path::Path) -> Result<(), String> {
    const VIEWS: [&str; 20] = [
        "artillery-whole-exterior",
        "artillery-whole-courtyard",
        "artillery-whole-top",
        "artillery-whole-longitudinal-cut",
        "artillery-whole-transverse-cut",
        "artillery-trace-plan",
        "artillery-curtain-section",
        "artillery-curtain-terreplein",
        "artillery-rondel-exterior",
        "artillery-rondel-casemate",
        "artillery-rondel-cutaway",
        "artillery-rondel-top",
        "artillery-gate-approach",
        "artillery-gate-interior",
        "artillery-bridge-deployed",
        "artillery-bridge-denied",
        "artillery-circulation",
        "artillery-drainage",
        "artillery-support-dag",
        "artillery-fire-plan",
    ];
    validate_compact_evidence_suite(
        directory,
        &VIEWS
            .iter()
            .map(|name| (*name, "artillery-rondel-castle"))
            .collect::<Vec<_>>(),
        true,
    )
}

pub(crate) fn validate_final_building_suite(directory: &std::path::Path) -> Result<(), String> {
    const SPECS: [(&str, &str); 10] = [
        ("final-town-house-regression", "town-house"),
        ("final-hall-house-regression", "hall-house"),
        ("final-fachwerk-cottage-regression", "fachwerk-cottage"),
        (
            "final-fachwerk-merchant-regression",
            "fachwerk-merchant-house",
        ),
        (
            "final-renaissance-town-hall-regression",
            "renaissance-town-hall",
        ),
        ("final-cathedral-regression", "cathedral"),
        ("final-castle-gatehouse-regression", "castle-gatehouse"),
        ("final-courtyard-castle-regression", "courtyard-castle"),
        ("final-walled-keep-regression", "walled-keep"),
        (
            "final-artillery-rondel-castle-regression",
            "artillery-rondel-castle",
        ),
    ];
    validate_compact_evidence_suite(directory, &SPECS, false)
}

fn validate_compact_evidence_suite(
    directory: &std::path::Path,
    specs: &[(&str, &str)],
    artillery: bool,
) -> Result<(), String> {
    let mut revision = None::<String>;
    let mut dirty = None::<String>;
    let mut pixels = std::collections::HashSet::new();
    let mut ordinary_plan = None::<String>;
    for (stem, fixture) in specs {
        let capture_path = directory.join(format!("{stem}.capture.json"));
        let png = directory.join(format!("{stem}.png"));
        let plan = directory.join(format!("{stem}.plan.json"));
        if !png.is_file() || !plan.is_file() {
            return Err(format!("{stem} lacks PNG or plan evidence"));
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&capture_path)
                .map_err(|error| format!("{}: {error}", capture_path.display()))?,
        )
        .map_err(|error| format!("{}: {error}", capture_path.display()))?;
        let string = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let section = value
            .get("section_cut_applied")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let section_correspondence = section
            && value
                .get("section_removed_item_ids")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ids| !ids.is_empty());
        if string("fixture") != *fixture
            || value
                .get("plan_audit_issue_count")
                .and_then(serde_json::Value::as_u64)
                != Some(0)
            || value
                .get("mesh_integrity_issue_count")
                .and_then(serde_json::Value::as_u64)
                != Some(0)
            || value
                .get("resolver_schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(2)
            || (!section_correspondence
                && string("resolved_solid_multiset_hash") != string("rendered_geometry_hash"))
        {
            return Err(format!(
                "{stem} fails fixture/audit/schema/render correspondence"
            ));
        }
        for (slot, current) in [
            (&mut revision, string("source_revision")),
            (&mut dirty, string("source_dirty_fingerprint")),
        ] {
            if slot.as_ref().is_some_and(|expected| expected != &current) {
                return Err(format!("{stem} came from a mixed source build"));
            }
            *slot = Some(current);
        }
        if !pixels.insert(string("pixel_hash")) {
            return Err(format!("{stem} duplicates another proof image"));
        }
        if artillery && *stem != "artillery-bridge-denied" {
            let hash = string("plan_hash");
            if ordinary_plan
                .as_ref()
                .is_some_and(|expected| expected != &hash)
            {
                return Err(format!("{stem} has a mixed artillery plan hash"));
            }
            ordinary_plan = Some(hash);
            if value
                .get("focused_resolved_item_ids")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|ids| ids.is_empty())
            {
                return Err(format!("{stem} lacks exact focused artillery IDs"));
            }
            let bounds = value
                .get("focused_bounds_fraction")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("{stem} lacks focused bounds"))?;
            if bounds.len() != 4
                || bounds[2].as_f64().unwrap_or(0.0) - bounds[0].as_f64().unwrap_or(1.0) < 0.12
                || bounds[3].as_f64().unwrap_or(0.0) - bounds[1].as_f64().unwrap_or(1.0) < 0.12
            {
                return Err(format!("{stem} focused authority is too small to inspect"));
            }
            let roles = value
                .get("artillery_focused_roles")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<std::collections::HashSet<_>>()
                })
                .unwrap_or_default();
            let required: &[&str] = match *stem {
                "artillery-curtain-section" => &[
                    "ArtilleryRevetment",
                    "ArtilleryEarthCore",
                    "ArtilleryRetainingWall",
                    "ArtilleryTerreplein",
                ],
                "artillery-rondel-casemate" | "artillery-rondel-cutaway" => &[
                    "ArtilleryEarthCore",
                    "ArtilleryCasemateFloor",
                    "ArtilleryCasemateRoof",
                    "WeaponMount",
                ],
                "artillery-gate-interior" => &[
                    "ArtilleryGateMechanism",
                    "ArtilleryCasemateFloor",
                    "ArtilleryCasemateRoof",
                    "OpeningClosure",
                ],
                _ => &[],
            };
            if required.iter().any(|role| !roles.contains(role)) {
                return Err(format!("{stem} does not focus all required physical roles"));
            }
            let role_bounds = value
                .get("artillery_role_bounds_fraction")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| format!("{stem} lacks per-role projected bounds"))?;
            for role in required {
                let bounds = role_bounds
                    .get(*role)
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| format!("{stem} does not visibly project {role}"))?;
                if bounds.len() != 4
                    || bounds[2].as_f64().unwrap_or(0.0) - bounds[0].as_f64().unwrap_or(1.0) < 0.01
                    || bounds[3].as_f64().unwrap_or(0.0) - bounds[1].as_f64().unwrap_or(1.0) < 0.01
                    || bounds[0].as_f64().unwrap_or(-1.0) < -0.05
                    || bounds[1].as_f64().unwrap_or(-1.0) < -0.05
                    || bounds[2].as_f64().unwrap_or(2.0) > 1.05
                    || bounds[3].as_f64().unwrap_or(2.0) > 1.05
                {
                    return Err(format!(
                        "{stem} projects {role} outside a readable proof area"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_timber_suite_records(
    records: &[(String, BuildingArchetype, ViewerView, TimberSuiteManifest)],
) -> Result<(), String> {
    if records.len() != 35 {
        return Err(format!(
            "expected 35 timber proofs, found {}",
            records.len()
        ));
    }
    let first = &records[0].3;
    let mut fixture_hashes = std::collections::HashMap::<String, (String, String, String)>::new();
    let mut pixels = std::collections::HashSet::new();
    for (slug, archetype, view, manifest) in records {
        let expected_view = timber_proof_suffix(*view).expect("timber proof suffix");
        if manifest.fixture != archetype.slug()
            || manifest.view != expected_view
            || manifest.seed != 47
            || manifest.resolver_schema_version != 2
            || manifest.plan_audit_issue_count != 0
            || !manifest.validation_passed
        {
            return Err(format!(
                "{slug} violates its fixture/view validation contract"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!("{slug} comes from mixed or stale source authority"));
        }
        if let Some((plan_hash, geometry_hash, frame_hash)) = fixture_hashes.get(&manifest.fixture)
        {
            if plan_hash != &manifest.plan_hash
                || geometry_hash != &manifest.resolved_geometry_hash
                || frame_hash != &manifest.timber_program_hash
            {
                return Err(format!("{slug} is fixture-inconsistent"));
            }
        } else {
            fixture_hashes.insert(
                manifest.fixture.clone(),
                (
                    manifest.plan_hash.clone(),
                    manifest.resolved_geometry_hash.clone(),
                    manifest.timber_program_hash.clone(),
                ),
            );
        }
        if manifest.timber_program.is_none()
            || manifest.timber_assembly_id.is_none()
            || manifest.timber_member_ids.len() < 20
            || manifest.timber_joint_ids.len() < 12
            || manifest.timber_node_ids.len() < 12
            || manifest.focused_resolved_item_ids.is_empty()
            || manifest.visible_focused_resolved_item_count
                + manifest.timber_removed_target_item_ids.len()
                != manifest.focused_resolved_item_ids.len()
            || manifest.timber_target_component_ids.len() != 1
            || !manifest.timber_target_component_ids[0].starts_with("timber:")
            || !manifest.timber_target_component_ids[0].contains('/')
            || manifest.timber_focus_interface_ids.is_empty()
            || !manifest
                .timber_removed_target_item_ids
                .iter()
                .all(|id| manifest.section_removed_item_ids.contains(id))
            || !manifest.timber_legend_visible
            || manifest.timber_required_roles.is_empty()
            || manifest.timber_focused_roles.is_empty()
            || !manifest.section_annotation_visible
            || manifest.timber_required_roles.iter().any(|role| {
                !manifest
                    .timber_focused_roles
                    .iter()
                    .any(|found| found == role)
                    || manifest
                        .timber_role_item_ids
                        .get(role)
                        .is_none_or(Vec::is_empty)
                    || manifest
                        .timber_role_bounds_fraction
                        .get(role)
                        .is_none_or(|bounds| {
                            bounds[0] < 0.0
                                || bounds[1] < 0.0
                                || bounds[2] > 1.0
                                || bounds[3] > 1.0
                                || (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]) < 0.0004
                        })
            })
        {
            return Err(format!(
                "{slug} lacks exact frame IDs, roles, focus, or legend"
            ));
        }
        if matches!(
            view,
            ViewerView::TimberOpeningBayExterior
                | ViewerView::TimberOpeningBayInterior
                | ViewerView::TimberOpeningBaySection
        ) && (manifest.focused_resolved_void_ids.len() != 1
            || manifest
                .timber_role_item_ids
                .get("WallHost")
                .is_none_or(Vec::is_empty))
        {
            return Err(format!(
                "{slug} does not prove both exact opening void and Gefach cells"
            ));
        }
        if *view == ViewerView::TimberJointClose
            && (manifest
                .timber_focused_roles
                .iter()
                .any(|role| role == "WallHost")
                || manifest.timber_focus_interface_ids.len() < 2)
        {
            return Err(format!(
                "{slug} hides its participant contact behind enclosure geometry"
            ));
        }
        if *view == ViewerView::TimberGableRoofBearing && manifest.focused_roof_item_ids.is_empty()
        {
            return Err(format!("{slug} omits the exact Stage 4 roof face"));
        }
        let expects_cut = timber_section_proof(*view);
        if expects_cut != manifest.section_cut_applied
            || expects_cut != manifest.timber_cut_plane.is_some()
        {
            return Err(format!("{slug} lacks its exact declared cut state"));
        }
        let bounds = manifest.focused_bounds_fraction;
        if bounds[0] < 0.0
            || bounds[1] < 0.0
            || bounds[2] > 1.0
            || bounds[3] > 1.0
            || bounds[2] - bounds[0] < 0.12
            || bounds[3] - bounds[1] < 0.20
        {
            return Err(format!("{slug} target is clipped or too small"));
        }
        if manifest.pixel_hash.is_empty() || !pixels.insert(manifest.pixel_hash.clone()) {
            return Err(format!("{slug} lacks unique pixel evidence"));
        }
    }
    Ok(())
}
