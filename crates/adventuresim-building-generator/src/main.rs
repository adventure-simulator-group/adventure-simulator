//! Deterministic native viewer and screenshot harness for building prototypes.

#[cfg(not(target_family = "wasm"))]
mod viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use adventuresim_building_generator::BuildingArchetype;
#[cfg(not(target_family = "wasm"))]
use clap::{Parser, ValueEnum};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ViewerView {
    Exterior,
    Defenses,
    Cutaway,
    GateDetailExterior,
    GateDetailInterior,
    TowerPortalDetail,
    CrownStraightExterior,
    CrownStraightInterior,
    CrownCornerExterior,
    CrownCornerInterior,
    CrownTowerExterior,
    CrownTowerTop,
    CrownTowerCutaway,
    ProjectedExterior,
    ProjectedInterior,
    ProjectedUnderside,
    ProjectedTop,
    ProjectedLongitudinal,
    ProjectedSockets,
    ProjectedFlank,
    OpeningRectangularExterior,
    OpeningRectangularInterior,
    OpeningRectangularSection,
    OpeningSegmentalExterior,
    OpeningSegmentalInterior,
    OpeningSegmentalSection,
    OpeningPointedExterior,
    OpeningPointedInterior,
    OpeningPointedSection,
    OpeningArrowLoopExterior,
    OpeningArrowLoopInterior,
    OpeningArrowLoopSection,
    OpeningGunLoopExterior,
    OpeningGunLoopInterior,
    OpeningGunLoopSection,
    WallTimberFrameSection,
    WallCivilianMasonrySection,
    WallCathedralButtressSection,
    WallRoundTowerRadialSection,
    ChurchWholeWest,
    ChurchWholeEast,
    ChurchWholeNorth,
    ChurchWholeSouth,
    ChurchWholeTop,
    ChurchWholeLongitudinalCut,
    ChurchWholeTransverseCut,
    ChurchWholeRegression,
    ChurchBayExterior,
    ChurchBayInterior,
    ChurchBaySection,
    ChurchBayLoad,
    ChurchBayVault,
    ChurchCrossingInterior,
    ChurchCrossingExterior,
    ChurchCrossingTop,
    ChurchCrossingCutLoad,
    ChurchChoirEast,
    ChurchChoirInterior,
    ChurchChoirTop,
    ChurchChoirRadialSection,
    ChurchTowerPortal,
    ChurchTowerJunction,
    ChurchTowerStair,
    ChurchTowerBellUnderside,
    ChurchTowerFrame,
    ChurchTowerLouvredExterior,
    ChurchTowerRoofDrain,
    ChurchDrainage,
    ChurchSupportDag,
    TimberWholeExterior,
    TimberFrameFacade,
    TimberRegistrationCut,
    TimberSupportLoad,
    TimberProgramDetail,
    TimberOpeningBayExterior,
    TimberOpeningBayInterior,
    TimberOpeningBaySection,
    TimberJointClose,
    TimberJettyExterior,
    TimberJettyUnderside,
    TimberJettyLoad,
    TimberGableRoofBearing,
    TimberDormerTrimmer,
    TimberTownHallJunction,
    ArtilleryWholeExterior,
    ArtilleryWholeCourtyard,
    ArtilleryWholeTop,
    ArtilleryWholeLongitudinalCut,
    ArtilleryWholeTransverseCut,
    ArtilleryTracePlan,
    ArtilleryCurtainSection,
    ArtilleryCurtainTerreplein,
    ArtilleryRondelExterior,
    ArtilleryRondelCasemate,
    ArtilleryRondelCutaway,
    ArtilleryRondelTop,
    ArtilleryGateApproach,
    ArtilleryGateInterior,
    ArtilleryBridgeDeployed,
    ArtilleryBridgeDenied,
    ArtilleryCirculation,
    ArtilleryDrainage,
    ArtillerySupportDag,
    ArtilleryFirePlan,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ProjectedProofKind {
    Machicolation,
    Breteche,
    Hoarding,
    Bartizan,
}

#[cfg(not(target_family = "wasm"))]
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum RoofProofView {
    RoofGableExterior,
    RoofGableInterior,
    RoofGableTop,
    RoofGableCutaway,
    RoofGableDrainage,
    RoofGableLowPitch,
    RoofGableMidPitch,
    RoofGableHighPitch,
    RoofHipHalfhipExterior,
    RoofHipHalfhipTop,
    RoofHipHalfhipUnderside,
    RoofLValleyExterior,
    RoofLValleyTop,
    RoofLValleyUnderside,
    RoofLValleyDrainage,
    RoofCourtyardValleysTop,
    RoofDormerGabledExterior,
    RoofDormerGabledInterior,
    RoofDormerGabledTop,
    RoofDormerGabledCutaway,
    RoofDormerGabledDrainage,
    RoofDormerShedExterior,
    RoofDormerShedInterior,
    RoofDormerShedTop,
    RoofDormerShedCutaway,
    RoofDormerShedDrainage,
    RoofCrossGableExterior,
    RoofCrossGableTop,
    RoofCrossGableUnderside,
    RoofCrossGableDrainage,
    RoofAbutmentWallExterior,
    RoofAbutmentWallTop,
    RoofAbutmentWallCutaway,
    RoofAbutmentWallDrainage,
    RoofAbutmentTowerExterior,
    RoofAbutmentTowerTop,
    RoofAbutmentTowerCutaway,
    RoofAbutmentTowerDrainage,
    RoofRoundTowerExterior,
    RoofRoundTowerTop,
    RoofRoundTowerCutaway,
    RoofRoundTowerDrainage,
    RoofPavilionExterior,
    RoofPavilionTop,
    RoofPavilionCutaway,
    RoofPavilionDrainage,
    RoofCathedralExterior,
    RoofCathedralTop,
    RoofCathedralCutaway,
    RoofCathedralDrainage,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Render a deterministic procedural-building prototype"
)]
struct Args {
    /// Curated high-level building program to generate.
    #[arg(
        long,
        value_enum,
      required_unless_present_any = ["editor", "editor_script", "validate_crown_suite", "validate_projected_suite", "validate_openings_suite", "validate_roof_suite", "validate_church_suite", "validate_timber_suite", "validate_artillery_suite", "validate_final_building_suite"]
    )]
    fixture: Option<BuildingArchetype>,

    /// Validate that all nine crown proofs came from one current source build.
    #[arg(long, value_name = "DIRECTORY", conflicts_with = "fixture")]
    validate_crown_suite: Option<PathBuf>,

    /// Validate the projected-defense proof matrix and its exact resolved IDs.
    #[arg(
        long,
        value_name = "DIRECTORY",
        conflicts_with_all = ["fixture", "validate_crown_suite"]
    )]
    validate_projected_suite: Option<PathBuf>,

    /// Validate the Stage 3 wall/opening proof matrix and exact focused IDs.
    #[arg(
        long,
        value_name = "DIRECTORY",
        conflicts_with_all = ["fixture", "validate_crown_suite", "validate_projected_suite"]
    )]
    validate_openings_suite: Option<PathBuf>,

    /// Validate the Stage 4 roof proof matrix and exact roof/render hashes.
    #[arg(
        long,
        value_name = "DIRECTORY",
        conflicts_with_all = ["fixture", "validate_crown_suite", "validate_projected_suite", "validate_openings_suite"]
    )]
    validate_roof_suite: Option<PathBuf>,

    /// Validate the Stage 5 church-program proof matrix and exact authority.
    #[arg(
        long,
        value_name = "DIRECTORY",
        conflicts_with_all = ["fixture", "validate_crown_suite", "validate_projected_suite", "validate_openings_suite", "validate_roof_suite"]
    )]
    validate_church_suite: Option<PathBuf>,

    /// Validate the Stage 6 semantic timber-frame proof matrix.
    #[arg(
        long,
        value_name = "DIRECTORY",
        conflicts_with_all = ["fixture", "validate_crown_suite", "validate_projected_suite", "validate_openings_suite", "validate_roof_suite", "validate_church_suite"]
    )]
    validate_timber_suite: Option<PathBuf>,

    /// Validate the Stage 7 artillery-rondel proof matrix.
    #[arg(long, value_name = "DIRECTORY", conflicts_with = "fixture")]
    validate_artillery_suite: Option<PathBuf>,

    /// Validate the compact final ten-archetype regression matrix.
    #[arg(long, value_name = "DIRECTORY", conflicts_with = "fixture")]
    validate_final_building_suite: Option<PathBuf>,

    /// Deterministic Stage 4 roof-kernel proof preset.
    #[arg(long, value_enum)]
    roof_proof: Option<RoofProofView>,

    /// Full-building or deterministic close architectural inspection view.
    #[arg(long, value_enum, default_value_t = ViewerView::Exterior)]
    view: ViewerView,

    /// Deterministic generation seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// PNG output path. Omit to leave the interactive viewer open.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Open the semantic building editor instead of the passive viewer.
    #[arg(long, conflicts_with = "output")]
    editor: bool,

    /// Versioned editor document to load/save. Defaults to building-document.json.
    #[arg(long, value_name = "PATH", requires = "editor")]
    document: Option<PathBuf>,

    /// Freeform player-build document rendered over the editor fixture.
    #[arg(
        long,
        value_name = "PATH",
        requires = "editor",
        conflicts_with = "document"
    )]
    player_build_document: Option<PathBuf>,

    /// Execute a JSON editor-command script and print deterministic snapshots.
    #[arg(long, value_name = "PATH", conflicts_with = "editor")]
    editor_script: Option<PathBuf>,

    /// Frames allowed for render pipelines to settle before capture.
    #[arg(long, default_value_t = 240)]
    settle_frames: u32,

    /// Projected-defense assembly selected by projected proof views.
    #[arg(long, value_enum, default_value_t = ProjectedProofKind::Machicolation)]
    projected_kind: ProjectedProofKind,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    if let Some(script) = args.editor_script {
        match viewer::run_editor_script(&script) {
            Ok(snapshot) => println!("{snapshot}"),
            Err(error) => {
                eprintln!("editor script failed: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if let Some(directory) = args.validate_crown_suite {
        if let Err(error) = viewer::validate_crown_suite(&directory) {
            eprintln!("crown proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("crown proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_projected_suite {
        if let Err(error) = viewer::validate_projected_suite(&directory) {
            eprintln!("projected-defense proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!(
            "projected-defense proof suite valid: {}",
            directory.display()
        );
        return;
    }
    if let Some(directory) = args.validate_openings_suite {
        if let Err(error) = viewer::validate_openings_suite(&directory) {
            eprintln!("wall/opening proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("wall/opening proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_roof_suite {
        if let Err(error) = viewer::validate_roof_suite(&directory) {
            eprintln!("roof proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("roof proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_church_suite {
        if let Err(error) = viewer::validate_church_suite(&directory) {
            eprintln!("church proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("church proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_timber_suite {
        if let Err(error) = viewer::validate_timber_suite(&directory) {
            eprintln!("timber-frame proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("timber-frame proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_artillery_suite {
        if let Err(error) = viewer::validate_artillery_suite(&directory) {
            eprintln!("artillery proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("artillery proof suite valid: {}", directory.display());
        return;
    }
    if let Some(directory) = args.validate_final_building_suite {
        if let Err(error) = viewer::validate_final_building_suite(&directory) {
            eprintln!("final building proof suite invalid: {error}");
            std::process::exit(2);
        }
        println!("final building proof suite valid: {}", directory.display());
        return;
    }
    viewer::run(
        args.fixture.unwrap_or(BuildingArchetype::TownHouse),
        args.view,
        args.seed,
        args.output,
        args.settle_frames,
        args.projected_kind,
        args.roof_proof,
        args.editor,
        args.document,
        args.player_build_document,
    );
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("building-viewer is a native-only prototype");
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[test]
    fn editor_does_not_require_a_fixture_argument() {
        let args = Args::try_parse_from(["building-viewer", "--editor"]).unwrap();
        assert!(args.editor);
        assert_eq!(args.fixture, None);
    }

    #[test]
    fn editor_script_does_not_require_a_fixture_argument() {
        let args =
            Args::try_parse_from(["building-viewer", "--editor-script", "editor-actions.json"])
                .unwrap();
        assert_eq!(
            args.editor_script,
            Some(PathBuf::from("editor-actions.json"))
        );
        assert_eq!(args.fixture, None);
    }
}
