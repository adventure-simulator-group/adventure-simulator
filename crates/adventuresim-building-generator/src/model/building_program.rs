use super::*;

/// High-level input recipe for procedural building generation.
///
/// The recipe is intentionally allowed to describe combinations that cannot be
/// built. The public [`crate::generate`] boundary is the validator: every
/// successful result has passed the complete structural audit, while an
/// unbuildable recipe returns [`crate::GenerationError`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingProgram {
    pub archetype: BuildingArchetype,
    pub seed: u64,
    pub footprint: Footprint,
    pub storey_height_metres: f32,
    pub storeys: Vec<StoreyProgram>,
    pub vertical_connections: Vec<VerticalConnectionRequirement>,
    pub wall_style: WallStyle,
    pub timber_frame_style: Option<TimberFrameStyle>,
    pub upper_storey_projection_metres: f32,
    pub roof_pitch_degrees: f32,
    /// Optional explicit kernel demonstrator used by deterministic proof plans.
    /// Curated archetypes leave this unset.
    #[serde(default)]
    pub roof_demonstrator: Option<RoofKind>,
    /// Present only when a church-specific structural program, rather than
    /// the generic room allocator, is authoritative.
    #[serde(default)]
    pub church_program: Option<ChurchProgram>,
}

impl BuildingProgram {
    pub fn fixture(archetype: BuildingArchetype, seed: u64) -> Self {
        use RoomKind::*;

        match archetype {
            BuildingArchetype::TownHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 6,
                    depth: 10,
                },
                storey_height_metres: 3.0,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Shop, 18).exterior().beside(Workshop),
                            RoomRequirement::new(EntranceHall, 8)
                                .exterior()
                                .beside(StairHall),
                            RoomRequirement::new(Workshop, 15).beside(Storage),
                            RoomRequirement::new(Storage, 8),
                            RoomRequirement::new(StairHall, 8),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(CommonRoom, 22)
                                .exterior()
                                .beside(Kitchen),
                            RoomRequirement::new(Kitchen, 12).exterior().beside(Pantry),
                            RoomRequirement::new(Pantry, 6),
                            RoomRequirement::new(Bedchamber, 13).exterior(),
                            RoomRequirement::new(StairHall, 7),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::StraightStair {
                    lowest_storey: 0,
                    highest_storey: 1,
                    landing_room: StairHall,
                }],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::LateMedieval),
                upper_storey_projection_metres: 0.22,
                roof_pitch_degrees: 55.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::HallHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 9,
                    depth: 13,
                },
                storey_height_metres: 3.3,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(GreatHall, 52)
                            .exterior()
                            .beside(Kitchen),
                        RoomRequirement::new(EntranceHall, 14).exterior(),
                        RoomRequirement::new(Kitchen, 20).exterior().beside(Pantry),
                        RoomRequirement::new(Pantry, 10),
                        RoomRequirement::new(Storage, 15).exterior(),
                    ],
                }],
                vertical_connections: Vec::new(),
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::NorthernCloseStudded),
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 50.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::FachwerkCottage => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle { width: 7, depth: 8 },
                storey_height_metres: 2.8,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(CommonRoom, 18)
                            .exterior()
                            .beside(Kitchen),
                        RoomRequirement::new(Kitchen, 10).exterior().beside(Pantry),
                        RoomRequirement::new(Pantry, 5),
                        RoomRequirement::new(Bedchamber, 12).exterior(),
                        RoomRequirement::new(EntranceHall, 7).exterior(),
                    ],
                }],
                vertical_connections: Vec::new(),
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::NorthernCloseStudded),
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 53.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::FachwerkMerchantHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 8,
                    depth: 11,
                },
                storey_height_metres: 3.0,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Shop, 24).exterior().beside(Workshop),
                            RoomRequirement::new(EntranceHall, 10)
                                .exterior()
                                .beside(StairHall),
                            RoomRequirement::new(Workshop, 22).beside(Storage),
                            RoomRequirement::new(Storage, 16),
                            RoomRequirement::new(StairHall, 16),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(CommonRoom, 30)
                                .exterior()
                                .beside(Kitchen),
                            RoomRequirement::new(Kitchen, 18).exterior().beside(Pantry),
                            RoomRequirement::new(Pantry, 8),
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(StairHall, 12),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Gallery, 26).exterior(),
                            RoomRequirement::new(Bedchamber, 24).exterior(),
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Storage, 10),
                            RoomRequirement::new(StairHall, 8),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::StraightStair {
                    lowest_storey: 0,
                    highest_storey: 2,
                    landing_room: StairHall,
                }],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::EarlyModernOrnate),
                upper_storey_projection_metres: 0.28,
                roof_pitch_degrees: 57.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::RenaissanceTownHall => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 14,
                    depth: 10,
                },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 30).exterior(),
                            RoomRequirement::new(GreatHall, 48).exterior(),
                            RoomRequirement::new(Shop, 24).exterior(),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 20),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 54).exterior(),
                            RoomRequirement::new(Gallery, 34).exterior(),
                            RoomRequirement::new(Chapel, 20).exterior(),
                            RoomRequirement::new(Storage, 14),
                            RoomRequirement::new(StairHall, 18),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::StraightStair {
                    lowest_storey: 0,
                    highest_storey: 1,
                    landing_room: StairHall,
                }],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::EarlyModernOrnate),
                upper_storey_projection_metres: 0.24,
                roof_pitch_degrees: 54.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::Cathedral => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 28,
                    depth: 14,
                },
                storey_height_metres: 5.8,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(Nave, 190).exterior().beside(Chancel),
                        RoomRequirement::new(Chancel, 70).exterior().beside(Nave),
                        RoomRequirement::new(Chapel, 32).exterior(),
                        RoomRequirement::new(Sacristy, 24)
                            .exterior()
                            .beside(Chancel),
                        RoomRequirement::new(EntranceHall, 20).exterior(),
                    ],
                }],
                vertical_connections: Vec::new(),
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 58.0,
                roof_demonstrator: None,
                church_program: Some(ChurchProgram::URBAN_BRICK_BASILICA),
            },
            BuildingArchetype::CastleGatehouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 10,
                    depth: 6,
                },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Passage, 18).exterior(),
                            RoomRequirement::new(Guardroom, 18)
                                .exterior()
                                .beside(Passage),
                            RoomRequirement::new(Armoury, 12).beside(Guardroom),
                            RoomRequirement::new(StairHall, 12),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 24).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 10),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::TowerSpiral {
                    lowest_storey: 0,
                    highest_storey: 1,
                }],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 48.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::CourtyardCastle => Self {
                archetype,
                seed,
                footprint: Footprint::Courtyard {
                    width: 18,
                    depth: 16,
                    wing: 4,
                    gate_width: 4,
                },
                storey_height_metres: 3.5,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Passage, 24).exterior(),
                            RoomRequirement::new(GreatHall, 55).exterior(),
                            RoomRequirement::new(Kitchen, 30).exterior(),
                            RoomRequirement::new(Guardroom, 35).exterior(),
                            RoomRequirement::new(Armoury, 24),
                            RoomRequirement::new(Storage, 35),
                            RoomRequirement::new(StairHall, 25),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Gallery, 50).exterior(),
                            RoomRequirement::new(GreatHall, 55).exterior(),
                            RoomRequirement::new(Chapel, 28).exterior(),
                            RoomRequirement::new(Bedchamber, 34).exterior(),
                            RoomRequirement::new(Guardroom, 30).exterior(),
                            RoomRequirement::new(StairHall, 25),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::TowerSpiral {
                    lowest_storey: 0,
                    highest_storey: 1,
                }],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 52.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::WalledKeep => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle { width: 9, depth: 8 },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 14).exterior(),
                            RoomRequirement::new(Guardroom, 18).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 28).exterior(),
                            RoomRequirement::new(Kitchen, 12).exterior(),
                            RoomRequirement::new(Guardroom, 12).exterior(),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 14),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::TowerSpiral {
                    lowest_storey: 0,
                    highest_storey: 2,
                }],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 0.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::ArtilleryRondelCastle => Self {
                archetype,
                seed,
                // The room-grid footprint is the retained older keep. The
                // independent ArtilleryCastleAssembly owns the much larger
                // 36 x 30 m clear court and retrofit enceinte around it.
                footprint: Footprint::Rectangle { width: 9, depth: 8 },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 14).exterior(),
                            RoomRequirement::new(Guardroom, 18).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 28).exterior(),
                            RoomRequirement::new(Kitchen, 12).exterior(),
                            RoomRequirement::new(Guardroom, 12).exterior(),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 14),
                        ],
                    },
                ],
                vertical_connections: vec![VerticalConnectionRequirement::TowerSpiral {
                    lowest_storey: 0,
                    highest_storey: 2,
                }],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 0.0,
                roof_demonstrator: None,
                church_program: None,
            },
        }
    }
}

pub const BUILDING_DOCUMENT_SCHEMA_VERSION: u32 = 3;
