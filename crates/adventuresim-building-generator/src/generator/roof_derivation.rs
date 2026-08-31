fn derive_roofs(program: &BuildingProgram) -> Vec<RoofPiece> {
    let (width, depth) = program.footprint.dimensions();
    let size = Vec2::new(
        f32::from(width) * CELL_SIZE_METRES,
        f32::from(depth) * CELL_SIZE_METRES,
    );
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match (program.archetype, program.footprint) {
        (BuildingArchetype::TownHouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.45,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::HallHouse, _) => vec![RoofPiece {
            kind: RoofKind::HalfHip,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.65,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::FachwerkCottage, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.5,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::FachwerkMerchantHouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.55,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::RenaissanceTownHall, _) => vec![RoofPiece {
            kind: RoofKind::HalfHip,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.65,
            gable_profile: GableProfile::Stepped,
        }],
        (BuildingArchetype::Cathedral, _) => vec![
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(21.15, 10.5),
                size: Vec2::new(31.50, 6.0),
                base_height_metres: 11.5,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.55,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Shed,
                // The high edge seats on the south clerestory exterior face
                // at z=7.125 rather than passing through to its interior side.
                centre: Vec2::new(14.05, 5.5875),
                size: Vec2::new(16.40, 2.175),
                base_height_metres: 7.0,
                pitch_degrees: 28.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.45,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Shed,
                // Mirrored north aisle: high edge seats at z=13.875.
                centre: Vec2::new(14.05, 15.4125),
                size: Vec2::new(16.40, 2.175),
                base_height_metres: 7.0,
                pitch_degrees: 28.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.45,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(25.65, 10.5),
                size: Vec2::new(4.5, 18.0),
                base_height_metres: 11.5,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::Z,
                eave_metres: 0.48,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Pavilion,
                centre: Vec2::new(39.15, 10.5),
                size: Vec2::new(8.8, 8.8),
                base_height_metres: 11.5,
                pitch_degrees: 52.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.40,
                gable_profile: GableProfile::Plain,
            },
        ],
        (BuildingArchetype::CastleGatehouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5 + Vec2::Y * 0.5,
            // The accepted gatehouse roof is the buildable central volume
            // between the two 3 m-radius flanking towers. It may abut their
            // shells, but it must not continue behind/through either tower.
            size: Vec2::new(size.x - 6.8, size.y - 1.0),
            base_height_metres: top - 0.45,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.35,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::CourtyardCastle, Footprint::Courtyard { wing, .. }) => {
            let wing_metres = f32::from(wing) * CELL_SIZE_METRES;
            // Keep the roof eaves behind the fighting circuit. The wall walk occupies
            // the outer 1.25 m of each wing and needs additional shoulder clearance.
            // The 3 m-radius corner towers own their full junction envelope.
            // Keep wing roofs beyond that envelope as well as behind the
            // fighting walk; this avoids unbuildable roof/gutter stubs through
            // the cylindrical shells.
            let outer_clearance = 3.2;
            let inner_clearance = 0.4;
            let transverse_span = wing_metres - outer_clearance - inner_clearance;
            vec![
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(size.x * 0.5, outer_clearance + transverse_span * 0.5),
                    size: Vec2::new(size.x - outer_clearance * 2.0, transverse_span),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Stepped,
                },
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(
                        size.x * 0.5,
                        size.y - outer_clearance - transverse_span * 0.5,
                    ),
                    size: Vec2::new(size.x - outer_clearance * 2.0, transverse_span),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Curved,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(outer_clearance + transverse_span * 0.5, size.y * 0.5),
                    size: Vec2::new(
                        transverse_span,
                        size.y - 2.0 * (outer_clearance + transverse_span),
                    ),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(
                        size.x - outer_clearance - transverse_span * 0.5,
                        size.y * 0.5,
                    ),
                    size: Vec2::new(
                        transverse_span,
                        size.y - 2.0 * (outer_clearance + transverse_span),
                    ),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
            ]
        }
        (BuildingArchetype::WalledKeep, _) => Vec::new(),
        _ => Vec::new(),
    }
}

fn derive_roof_dormers(program: &BuildingProgram) -> Vec<RoofDormer> {
    if program.roof_demonstrator == Some(RoofKind::Gable) {
        return Vec::new();
    }
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    let front_roof_inset = match program.footprint {
        Footprint::Courtyard { wing, .. } => f32::from(wing) * CELL_SIZE_METRES * 0.72,
        Footprint::Rectangle { .. } => 0.0,
    };
    let dormer = |centre, facing, kind, profile| RoofDormer {
        centre,
        base_height_metres: top + 1.15,
        width_metres: 2.15,
        depth_metres: 1.85,
        height_metres: 1.75,
        facing,
        kind,
        gable_profile: profile,
    };
    match program.archetype {
        BuildingArchetype::TownHouse => vec![dormer(
            Vec2::new(width, depth * 0.58),
            Direction::East,
            DormerKind::Gabled,
            GableProfile::Plain,
        )],
        BuildingArchetype::HallHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.36),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.64),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::FachwerkCottage => vec![
            dormer(
                Vec2::new(width, depth * 0.38),
                Direction::East,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.66),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::FachwerkMerchantHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.38),
                Direction::East,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.68),
                Direction::East,
                DormerKind::Hipped,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(0.0, depth * 0.52),
                Direction::West,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::RenaissanceTownHall => vec![
            dormer(
                Vec2::new(width * 0.22, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
            dormer(
                Vec2::new(width * 0.78, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
        BuildingArchetype::Cathedral => Vec::new(),
        BuildingArchetype::CastleGatehouse => Vec::new(),
        BuildingArchetype::CourtyardCastle => vec![
            dormer(
                Vec2::new(width * 0.3, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Stepped,
            ),
            dormer(
                Vec2::new(width * 0.7, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
        BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle => Vec::new(),
    }
}
