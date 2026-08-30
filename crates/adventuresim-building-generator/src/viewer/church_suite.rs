const CHURCH_PROOF_SLUGS: [&str; 30] = [
    "church-whole-west",
    "church-whole-east",
    "church-whole-north",
    "church-whole-south",
    "church-whole-top",
    "church-whole-longitudinal-cut",
    "church-whole-transverse-cut",
    "church-whole-regression",
    "church-bay-exterior",
    "church-bay-interior",
    "church-bay-section",
    "church-bay-load",
    "church-bay-vault",
    "church-crossing-interior",
    "church-crossing-exterior",
    "church-crossing-top",
    "church-crossing-cut-load",
    "church-choir-east",
    "church-choir-interior",
    "church-choir-top",
    "church-choir-radial-section",
    "church-tower-portal",
    "church-tower-junction",
    "church-tower-stair",
    "church-tower-bell-underside",
    "church-tower-frame",
    "church-tower-louvred-exterior",
    "church-tower-roof-drain",
    "church-drainage",
    "church-support-dag",
];

#[derive(Clone, Debug, Deserialize)]
struct ChurchSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    resolved_geometry_hash: String,
    church_program_hash: String,
    church_bay_labels: Vec<String>,
    church_support_node_ids: Vec<u64>,
    church_opening_ids: Vec<u64>,
    church_focused_roles: Vec<String>,
    church_target_component_ids: Vec<String>,
    church_target_item_ids: Vec<u64>,
    church_required_roles: Vec<String>,
    church_cut_plane: Option<[f32; 4]>,
    church_removed_target_item_ids: Vec<u64>,
    church_legend_visible: bool,
    focused_bounds_fraction: [f32; 4],
    pixel_hash: String,
    focused_resolved_item_ids: Vec<u64>,
    section_removed_item_ids: Vec<u64>,
    visible_focused_resolved_item_count: usize,
    section_cut_applied: bool,
    section_annotation_visible: bool,
    plan_audit_issue_count: usize,
    validation_passed: bool,
}

pub(crate) fn validate_church_suite(directory: &std::path::Path) -> Result<(), String> {
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
    if actual_count != CHURCH_PROOF_SLUGS.len() {
        return Err(format!(
            "expected exactly {} church manifests, found {actual_count}",
            CHURCH_PROOF_SLUGS.len()
        ));
    }
    let mut records = Vec::new();
    for slug in CHURCH_PROOF_SLUGS {
        let path = directory.join(format!("{slug}.capture.json"));
        let manifest: ChurchSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((slug, manifest));
    }
    validate_church_suite_records(&records)
}

fn validate_church_suite_records(records: &[(&str, ChurchSuiteManifest)]) -> Result<(), String> {
    if records.len() != CHURCH_PROOF_SLUGS.len() {
        return Err(format!(
            "expected 30 church proofs, found {}",
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut pixel_hashes = std::collections::HashMap::<&str, &str>::new();
    for ((expected, manifest), slug) in records.iter().zip(CHURCH_PROOF_SLUGS) {
        if *expected != slug
            || manifest.fixture != "cathedral"
            || manifest.view != slug
            || manifest.seed != 47
            || manifest.resolver_schema_version != 2
            || !manifest.validation_passed
            || manifest.plan_audit_issue_count != 0
        {
            return Err(format!(
                "church proof {slug} violates its fixed fixture contract"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
            || manifest.plan_hash != first.plan_hash
            || manifest.resolved_geometry_hash != first.resolved_geometry_hash
            || manifest.church_program_hash != first.church_program_hash
        {
            return Err(format!(
                "church proof {slug} comes from mixed or stale authority"
            ));
        }
        if manifest.church_bay_labels.len() != 8
            || manifest.church_support_node_ids.is_empty()
            || manifest.church_opening_ids.len() < 30
            || manifest.focused_resolved_item_ids.is_empty()
            || manifest.visible_focused_resolved_item_count == 0
        {
            return Err(format!(
                "church proof {slug} lacks exact bay/support/opening focus IDs"
            ));
        }
        if manifest.church_target_component_ids.len() != 1
            || manifest.church_target_item_ids != manifest.focused_resolved_item_ids
            || manifest.church_target_item_ids.is_empty()
            || !manifest
                .church_removed_target_item_ids
                .iter()
                .all(|id| manifest.church_target_item_ids.contains(id))
            || manifest.church_removed_target_item_ids
                != manifest
                    .section_removed_item_ids
                    .iter()
                    .filter(|id| manifest.church_target_item_ids.contains(id))
                    .copied()
                    .collect::<Vec<_>>()
            || !manifest.church_legend_visible
        {
            return Err(format!(
                "church proof {slug} is not bound to its exact target/cut authority"
            ));
        }
        if manifest.pixel_hash.is_empty() {
            return Err(format!("church proof {slug} lacks a captured pixel hash"));
        }
        if let Some(previous) = pixel_hashes.insert(&manifest.pixel_hash, slug) {
            return Err(format!(
                "church proofs {previous} and {slug} are pixel-identical instead of proving distinct contracts"
            ));
        }
        let has_role = |role: &str| {
            manifest
                .church_focused_roles
                .iter()
                .any(|item| item == role)
        };
        let kind_roles_valid = match slug {
            "church-bay-section" => has_role("ChurchPier") && has_role("ChurchArcade"),
            "church-bay-load" | "church-support-dag" => {
                has_role("ChurchVaultThrust") && has_role("WallButtress")
            }
            "church-bay-vault" => has_role("ChurchVaultShell"),
            "church-tower-stair" => has_role("ChurchStairTread") && has_role("Landing"),
            "church-tower-bell-underside" => has_role("ChurchBellFloor") && has_role("ChurchBell"),
            "church-tower-frame" => has_role("ChurchBellFrame") && has_role("ChurchServiceLadder"),
            "church-tower-roof-drain" | "church-drainage" => {
                has_role("RoofGutter") || has_role("RoofEdgeTreatment")
            }
            _ => true,
        };
        if !kind_roles_valid
            || manifest
                .church_required_roles
                .iter()
                .any(|role| !has_role(role))
        {
            return Err(format!(
                "church proof {slug} lacks its kind-specific resolved roles"
            ));
        }
        let expects_section = slug.contains("cut")
            || slug.ends_with("-interior")
            || slug.ends_with("-section")
            || slug.ends_with("-load")
            || slug.ends_with("-vault")
            || matches!(
                slug,
                "church-tower-junction"
                    | "church-tower-stair"
                    | "church-tower-bell-underside"
                    | "church-tower-frame"
                    | "church-support-dag"
            );
        if expects_section
            && (!manifest.section_cut_applied
                || !manifest.section_annotation_visible
                || manifest.church_cut_plane.is_none())
        {
            return Err(format!(
                "church proof {slug} lacks its genuine cut/authority annotation"
            ));
        }
        if !expects_section && (manifest.section_cut_applied || manifest.church_cut_plane.is_some())
        {
            return Err(format!(
                "church proof {slug} applies a section cut outside its proof contract"
            ));
        }
        let target = &manifest.church_target_component_ids[0];
        let suffix_valid = if slug.starts_with("church-bay-") {
            target.ends_with("/nave-bay:2")
        } else if slug.starts_with("church-crossing-") {
            target.ends_with("/crossing")
        } else if slug.starts_with("church-choir-") {
            target.ends_with("/choir-apse")
        } else if slug.starts_with("church-tower-") {
            target.ends_with("/west-tower")
        } else if slug == "church-drainage" {
            target.ends_with("/roof-drainage")
        } else if slug == "church-support-dag" {
            target.ends_with("/nave-bay:2/load-path")
        } else {
            target.ends_with("/whole")
        };
        let bounds = manifest.focused_bounds_fraction;
        let target_area = (bounds[2] - bounds[0]).max(0.0) * (bounds[3] - bounds[1]).max(0.0);
        if !suffix_valid
            || bounds[0] < 0.0
            || bounds[1] < 0.0
            || bounds[2] > 1.0
            || bounds[3] > 1.0
            || target_area < 0.025
        {
            return Err(format!(
                "church proof {slug} is off-target or projects too little target area"
            ));
        }
    }
    Ok(())
}
