const fn roof_proof_slug(view: RoofProofView) -> &'static str {
    match view {
        RoofProofView::RoofGableExterior => "roof-gable-exterior",
        RoofProofView::RoofGableInterior => "roof-gable-interior",
        RoofProofView::RoofGableTop => "roof-gable-top",
        RoofProofView::RoofGableCutaway => "roof-gable-cutaway",
        RoofProofView::RoofGableDrainage => "roof-gable-drainage",
        RoofProofView::RoofGableLowPitch => "roof-gable-low-pitch",
        RoofProofView::RoofGableMidPitch => "roof-gable-mid-pitch",
        RoofProofView::RoofGableHighPitch => "roof-gable-high-pitch",
        RoofProofView::RoofHipHalfhipExterior => "roof-hip-halfhip-exterior",
        RoofProofView::RoofHipHalfhipTop => "roof-hip-halfhip-top",
        RoofProofView::RoofHipHalfhipUnderside => "roof-hip-halfhip-underside",
        RoofProofView::RoofLValleyExterior => "roof-l-valley-exterior",
        RoofProofView::RoofLValleyTop => "roof-l-valley-top",
        RoofProofView::RoofLValleyUnderside => "roof-l-valley-underside",
        RoofProofView::RoofLValleyDrainage => "roof-l-valley-drainage",
        RoofProofView::RoofCourtyardValleysTop => "roof-courtyard-valleys-top",
        RoofProofView::RoofDormerGabledExterior => "roof-dormer-gabled-exterior",
        RoofProofView::RoofDormerGabledInterior => "roof-dormer-gabled-interior",
        RoofProofView::RoofDormerGabledTop => "roof-dormer-gabled-top",
        RoofProofView::RoofDormerGabledCutaway => "roof-dormer-gabled-cutaway",
        RoofProofView::RoofDormerGabledDrainage => "roof-dormer-gabled-drainage",
        RoofProofView::RoofDormerShedExterior => "roof-dormer-shed-exterior",
        RoofProofView::RoofDormerShedInterior => "roof-dormer-shed-interior",
        RoofProofView::RoofDormerShedTop => "roof-dormer-shed-top",
        RoofProofView::RoofDormerShedCutaway => "roof-dormer-shed-cutaway",
        RoofProofView::RoofDormerShedDrainage => "roof-dormer-shed-drainage",
        RoofProofView::RoofCrossGableExterior => "roof-cross-gable-exterior",
        RoofProofView::RoofCrossGableTop => "roof-cross-gable-top",
        RoofProofView::RoofCrossGableUnderside => "roof-cross-gable-underside",
        RoofProofView::RoofCrossGableDrainage => "roof-cross-gable-drainage",
        RoofProofView::RoofAbutmentWallExterior => "roof-abutment-wall-exterior",
        RoofProofView::RoofAbutmentWallTop => "roof-abutment-wall-top",
        RoofProofView::RoofAbutmentWallCutaway => "roof-abutment-wall-cutaway",
        RoofProofView::RoofAbutmentWallDrainage => "roof-abutment-wall-drainage",
        RoofProofView::RoofAbutmentTowerExterior => "roof-abutment-tower-exterior",
        RoofProofView::RoofAbutmentTowerTop => "roof-abutment-tower-top",
        RoofProofView::RoofAbutmentTowerCutaway => "roof-abutment-tower-cutaway",
        RoofProofView::RoofAbutmentTowerDrainage => "roof-abutment-tower-drainage",
        RoofProofView::RoofRoundTowerExterior => "roof-round-tower-exterior",
        RoofProofView::RoofRoundTowerTop => "roof-round-tower-top",
        RoofProofView::RoofRoundTowerCutaway => "roof-round-tower-cutaway",
        RoofProofView::RoofRoundTowerDrainage => "roof-round-tower-drainage",
        RoofProofView::RoofPavilionExterior => "roof-pavilion-exterior",
        RoofProofView::RoofPavilionTop => "roof-pavilion-top",
        RoofProofView::RoofPavilionCutaway => "roof-pavilion-cutaway",
        RoofProofView::RoofPavilionDrainage => "roof-pavilion-drainage",
        RoofProofView::RoofCathedralExterior => "roof-cathedral-exterior",
        RoofProofView::RoofCathedralTop => "roof-cathedral-top",
        RoofProofView::RoofCathedralCutaway => "roof-cathedral-cutaway",
        RoofProofView::RoofCathedralDrainage => "roof-cathedral-drainage",
    }
}

fn roof_proof_assembly_indices(plan: &BuildingPlan, view: RoofProofView) -> Vec<usize> {
    let child_kind = if matches!(
        view,
        RoofProofView::RoofDormerGabledExterior
            | RoofProofView::RoofDormerGabledInterior
            | RoofProofView::RoofDormerGabledTop
            | RoofProofView::RoofDormerGabledCutaway
            | RoofProofView::RoofDormerGabledDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::GabledDormer)
    } else if matches!(
        view,
        RoofProofView::RoofDormerShedExterior
            | RoofProofView::RoofDormerShedInterior
            | RoofProofView::RoofDormerShedTop
            | RoofProofView::RoofDormerShedCutaway
            | RoofProofView::RoofDormerShedDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::ShedDormer)
    } else if matches!(
        view,
        RoofProofView::RoofCrossGableExterior
            | RoofProofView::RoofCrossGableTop
            | RoofProofView::RoofCrossGableUnderside
            | RoofProofView::RoofCrossGableDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::CrossGable)
    } else {
        None
    };
    if let Some(kind) = child_kind
        && let Some(child_id) = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.kind == kind)
            .map(|child| child.child)
    {
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                (roof.id == child_id || roof.children.iter().any(|child| child.child == child_id))
                    .then_some(index)
            })
            .collect();
    }
    if matches!(
        view,
        RoofProofView::RoofLValleyExterior
            | RoofProofView::RoofLValleyTop
            | RoofProofView::RoofLValleyUnderside
            | RoofProofView::RoofLValleyDrainage
            | RoofProofView::RoofCourtyardValleysTop
            | RoofProofView::RoofCathedralExterior
            | RoofProofView::RoofCathedralTop
            | RoofProofView::RoofCathedralCutaway
            | RoofProofView::RoofCathedralDrainage
    ) {
        return (0..plan.roof_assemblies.len()).collect();
    }
    if matches!(
        view,
        RoofProofView::RoofAbutmentTowerExterior
            | RoofProofView::RoofAbutmentTowerTop
            | RoofProofView::RoofAbutmentTowerCutaway
            | RoofProofView::RoofAbutmentTowerDrainage
    ) {
        let tower_child = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.kind == adventuresim_building_generator::RoofChildKind::Tower)
            .map(|child| child.child);
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                (Some(roof.id) == tower_child
                    || tower_child
                        .is_some_and(|child| roof.children.iter().any(|link| link.child == child)))
                .then_some(index)
            })
            .collect();
    }
    if matches!(
        view,
        RoofProofView::RoofAbutmentWallExterior
            | RoofProofView::RoofAbutmentWallTop
            | RoofProofView::RoofAbutmentWallCutaway
            | RoofProofView::RoofAbutmentWallDrainage
    ) {
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                roof.edges
                    .iter()
                    .any(|edge| {
                        edge.kind == adventuresim_building_generator::RoofEdgeKind::WallAbutment
                    })
                    .then_some(index)
            })
            .collect();
    }
    let predicate = |roof: &RoofAssembly| {
        if matches!(
            view,
            RoofProofView::RoofHipHalfhipExterior
                | RoofProofView::RoofHipHalfhipTop
                | RoofProofView::RoofHipHalfhipUnderside
        ) {
            matches!(roof.kind, RoofKind::Hip | RoofKind::HalfHip)
        } else if matches!(
            view,
            RoofProofView::RoofRoundTowerExterior
                | RoofProofView::RoofRoundTowerTop
                | RoofProofView::RoofRoundTowerCutaway
                | RoofProofView::RoofRoundTowerDrainage
        ) {
            roof.kind == RoofKind::Conical
        } else if matches!(
            view,
            RoofProofView::RoofPavilionExterior
                | RoofProofView::RoofPavilionTop
                | RoofProofView::RoofPavilionCutaway
                | RoofProofView::RoofPavilionDrainage
        ) {
            roof.kind == RoofKind::Pavilion
        } else {
            roof.kind == RoofKind::Gable && roof.parent.is_none()
        }
    };
    plan.roof_assemblies
        .iter()
        .enumerate()
        .find_map(|(index, roof)| predicate(roof).then_some(vec![index]))
        .unwrap_or_default()
}

fn roof_proof_sectioned(view: RoofProofView) -> bool {
    let slug = roof_proof_slug(view);
    slug.ends_with("-interior") || slug.ends_with("-cutaway") || slug.ends_with("-underside")
}
