const fn church_proof_slug(view: ViewerView) -> Option<&'static str> {
    Some(match view {
        ViewerView::ChurchWholeWest => "church-whole-west",
        ViewerView::ChurchWholeEast => "church-whole-east",
        ViewerView::ChurchWholeNorth => "church-whole-north",
        ViewerView::ChurchWholeSouth => "church-whole-south",
        ViewerView::ChurchWholeTop => "church-whole-top",
        ViewerView::ChurchWholeLongitudinalCut => "church-whole-longitudinal-cut",
        ViewerView::ChurchWholeTransverseCut => "church-whole-transverse-cut",
        ViewerView::ChurchWholeRegression => "church-whole-regression",
        ViewerView::ChurchBayExterior => "church-bay-exterior",
        ViewerView::ChurchBayInterior => "church-bay-interior",
        ViewerView::ChurchBaySection => "church-bay-section",
        ViewerView::ChurchBayLoad => "church-bay-load",
        ViewerView::ChurchBayVault => "church-bay-vault",
        ViewerView::ChurchCrossingInterior => "church-crossing-interior",
        ViewerView::ChurchCrossingExterior => "church-crossing-exterior",
        ViewerView::ChurchCrossingTop => "church-crossing-top",
        ViewerView::ChurchCrossingCutLoad => "church-crossing-cut-load",
        ViewerView::ChurchChoirEast => "church-choir-east",
        ViewerView::ChurchChoirInterior => "church-choir-interior",
        ViewerView::ChurchChoirTop => "church-choir-top",
        ViewerView::ChurchChoirRadialSection => "church-choir-radial-section",
        ViewerView::ChurchTowerPortal => "church-tower-portal",
        ViewerView::ChurchTowerJunction => "church-tower-junction",
        ViewerView::ChurchTowerStair => "church-tower-stair",
        ViewerView::ChurchTowerBellUnderside => "church-tower-bell-underside",
        ViewerView::ChurchTowerFrame => "church-tower-frame",
        ViewerView::ChurchTowerLouvredExterior => "church-tower-louvred-exterior",
        ViewerView::ChurchTowerRoofDrain => "church-tower-roof-drain",
        ViewerView::ChurchDrainage => "church-drainage",
        ViewerView::ChurchSupportDag => "church-support-dag",
        _ => return None,
    })
}

fn church_section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ChurchWholeLongitudinalCut
            | ViewerView::ChurchWholeTransverseCut
            | ViewerView::ChurchBayInterior
            | ViewerView::ChurchBaySection
            | ViewerView::ChurchBayLoad
            | ViewerView::ChurchBayVault
            | ViewerView::ChurchCrossingInterior
            | ViewerView::ChurchCrossingCutLoad
            | ViewerView::ChurchChoirInterior
            | ViewerView::ChurchChoirRadialSection
            | ViewerView::ChurchTowerJunction
            | ViewerView::ChurchTowerStair
            | ViewerView::ChurchTowerBellUnderside
            | ViewerView::ChurchTowerFrame
            | ViewerView::ChurchSupportDag
    )
}

fn church_camera(plan: &BuildingPlan, view: ViewerView, origin: Vec2) -> Option<(Vec3, Vec3)> {
    let church = plan.church.as_ref()?;
    let point = |plan_x: f32, height: f32, plan_z: f32| {
        Vec3::new(plan_x + origin.x, height, plan_z + origin.y)
    };
    let whole = point(
        church.crossing_axis_metres - 5.0,
        8.0,
        church.tower.centre.y,
    );
    let tower_low = point(church.tower.centre.x, 3.5, church.tower.centre.y);
    let tower_mid = point(church.tower.centre.x, 10.5, church.tower.centre.y);
    let tower_high = point(church.tower.centre.x, 18.0, church.tower.centre.y);
    let bay = point(church.nave_axes_metres[1], 6.0, church.tower.centre.y);
    let crossing = point(church.crossing_axis_metres, 8.0, church.tower.centre.y);
    let choir_x = church
        .choir
        .bay_axes_metres
        .last()
        .copied()
        .unwrap_or(church.crossing_axis_metres + 8.0);
    let choir = point(choir_x + 2.5, 7.0, church.tower.centre.y);
    let (focus, offset) = match view {
        ViewerView::ChurchWholeWest => (whole, Vec3::new(-49.0, 17.0, -27.0)),
        ViewerView::ChurchWholeEast => (whole, Vec3::new(51.0, 18.0, 23.0)),
        ViewerView::ChurchWholeNorth => (whole, Vec3::new(7.0, 20.0, 50.0)),
        ViewerView::ChurchWholeSouth => (whole, Vec3::new(-7.0, 20.0, -50.0)),
        ViewerView::ChurchWholeRegression => (whole, Vec3::new(40.0, 24.0, -38.0)),
        ViewerView::ChurchWholeTop => (whole, Vec3::new(2.0, 65.0, -2.0)),
        ViewerView::ChurchWholeLongitudinalCut => (whole, Vec3::new(0.0, 16.0, -50.0)),
        ViewerView::ChurchWholeTransverseCut => (crossing, Vec3::new(44.0, 16.0, -5.0)),
        ViewerView::ChurchBayExterior => (bay, Vec3::new(-5.0, 12.0, -27.5)),
        ViewerView::ChurchBayInterior => (bay, Vec3::new(15.0, 10.5, -25.5)),
        ViewerView::ChurchBaySection => (bay, Vec3::new(13.0, 9.0, -24.0)),
        ViewerView::ChurchBayLoad => (bay, Vec3::new(20.0, 14.0, -28.0)),
        ViewerView::ChurchBayVault => (bay, Vec3::new(7.0, 20.0, -20.0)),
        ViewerView::ChurchCrossingInterior => (crossing, Vec3::new(18.0, 13.0, -29.0)),
        ViewerView::ChurchCrossingCutLoad => (crossing, Vec3::new(25.0, 18.0, -22.0)),
        ViewerView::ChurchCrossingExterior => (crossing, Vec3::new(-19.0, 14.0, -30.0)),
        ViewerView::ChurchCrossingTop => (crossing, Vec3::new(2.0, 38.0, -2.0)),
        ViewerView::ChurchChoirEast => (choir, Vec3::new(27.0, 13.0, 3.0)),
        ViewerView::ChurchChoirInterior => (choir, Vec3::new(-14.0, 14.0, -30.0)),
        ViewerView::ChurchChoirRadialSection => (choir, Vec3::new(22.0, 11.0, -2.0)),
        ViewerView::ChurchChoirTop => (choir, Vec3::new(0.5, 38.0, -0.5)),
        ViewerView::ChurchTowerPortal => (tower_low, Vec3::new(-24.0, 16.0, -24.0)),
        ViewerView::ChurchTowerLouvredExterior => (tower_high, Vec3::new(-18.0, 7.0, -18.0)),
        ViewerView::ChurchTowerJunction => (tower_low, Vec3::new(15.0, 9.0, -19.0)),
        ViewerView::ChurchTowerStair => (tower_mid, Vec3::new(25.0, 13.0, -28.0)),
        ViewerView::ChurchTowerBellUnderside => (tower_high, Vec3::new(11.0, -2.0, -10.0)),
        ViewerView::ChurchTowerFrame => (tower_high, Vec3::new(14.0, 3.5, -17.0)),
        ViewerView::ChurchTowerRoofDrain => (tower_high, Vec3::new(-8.0, 28.0, -24.0)),
        ViewerView::ChurchDrainage => (whole, Vec3::new(4.0, 48.0, -27.0)),
        ViewerView::ChurchSupportDag => (bay, Vec3::new(18.0, 13.0, -20.0)),
        _ => return None,
    };
    Some((focus + offset, focus))
}

fn church_target_component_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let prefix = format!("church:{}", church.id.0);
    let suffix = match view {
        ViewerView::ChurchBayExterior
        | ViewerView::ChurchBayInterior
        | ViewerView::ChurchBaySection
        | ViewerView::ChurchBayLoad
        | ViewerView::ChurchBayVault => "/nave-bay:2",
        ViewerView::ChurchCrossingInterior
        | ViewerView::ChurchCrossingExterior
        | ViewerView::ChurchCrossingTop
        | ViewerView::ChurchCrossingCutLoad => "/crossing",
        ViewerView::ChurchChoirEast
        | ViewerView::ChurchChoirInterior
        | ViewerView::ChurchChoirTop
        | ViewerView::ChurchChoirRadialSection => "/choir-apse",
        ViewerView::ChurchTowerPortal
        | ViewerView::ChurchTowerJunction
        | ViewerView::ChurchTowerStair
        | ViewerView::ChurchTowerBellUnderside
        | ViewerView::ChurchTowerFrame
        | ViewerView::ChurchTowerLouvredExterior
        | ViewerView::ChurchTowerRoofDrain => "/west-tower",
        ViewerView::ChurchDrainage => "/roof-drainage",
        ViewerView::ChurchSupportDag => "/nave-bay:2/load-path",
        _ => "/whole",
    };
    vec![format!("{prefix}{suffix}")]
}

fn church_required_roles(view: ViewerView) -> Vec<String> {
    let roles: &[&str] = match view {
        ViewerView::ChurchBaySection => &["ChurchPier", "ChurchArcade"],
        ViewerView::ChurchBayLoad | ViewerView::ChurchSupportDag => {
            &["ChurchVaultThrust", "WallButtress", "ChurchPier"]
        }
        ViewerView::ChurchBayVault => &["ChurchVaultShell", "ChurchVaultThrust"],
        ViewerView::ChurchCrossingCutLoad => {
            &["ChurchCrossingArch", "ChurchVaultThrust", "WallButtress"]
        }
        ViewerView::ChurchChoirInterior | ViewerView::ChurchChoirRadialSection => {
            &["ChurchVaultShell", "WallButtress", "WallHost"]
        }
        ViewerView::ChurchTowerStair => &["ChurchStairTread", "Landing", "ChurchGuard"],
        ViewerView::ChurchTowerBellUnderside => &["ChurchBellFloor", "ChurchBell"],
        ViewerView::ChurchTowerFrame => &["ChurchBellFrame", "ChurchBell", "ChurchServiceLadder"],
        ViewerView::ChurchTowerRoofDrain | ViewerView::ChurchDrainage => &["RoofGutter"],
        _ => &[],
    };
    roles.iter().map(|role| (*role).to_owned()).collect()
}

fn church_cut_plane(plan: &BuildingPlan, view: ViewerView) -> Option<[f32; 4]> {
    let church = plan.church.as_ref()?;
    if !church_section_proof(view) {
        return None;
    }
    if view == ViewerView::ChurchChoirRadialSection {
        let cut = church
            .choir
            .bay_axes_metres
            .last()
            .copied()
            .unwrap_or(church.crossing_axis_metres)
            + 5.0;
        Some([1.0, 0.0, 0.0, -cut])
    } else if matches!(
        view,
        ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
    ) {
        Some([1.0, 0.0, 0.0, -church.crossing_axis_metres])
    } else {
        Some([0.0, 0.0, 1.0, -church.tower.centre.y])
    }
}

fn church_section_removed_roof_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    if !church_section_proof(view) {
        return Vec::new();
    }
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let transverse = matches!(
        view,
        ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
    );
    let radial_cut = (view == ViewerView::ChurchChoirRadialSection).then(|| {
        church
            .choir
            .bay_axes_metres
            .last()
            .copied()
            .unwrap_or(church.crossing_axis_metres)
            + 5.0
    });
    plan.roof_assemblies
        .iter()
        .flat_map(|roof| &roof.faces)
        .filter(|face| {
            let centre =
                face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len().max(1) as f32;
            radial_cut.map_or_else(
                || {
                    (transverse && centre.x > church.crossing_axis_metres)
                        || (!transverse && centre.z < church.tower.centre.y)
                },
                |cut| centre.x > cut,
            )
        })
        .map(|face| face.id.0)
        .collect()
}
