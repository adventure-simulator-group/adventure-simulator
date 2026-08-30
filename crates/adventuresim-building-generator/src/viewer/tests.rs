#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_abi_produces_a_stable_player_build_snapshot() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let mut runtime = EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            Some(PlayerBuildDocument::empty()),
            None,
        );
        perform_editor_command(
            &mut runtime,
            EditorCommand::DrawWall {
                start_x_metres: 0.0,
                start_z_metres: 0.0,
                end_x_metres: CELL_SIZE_METRES,
                end_z_metres: 0.0,
                material: PlayerBuildMaterial::Stone,
                storey: 0,
            },
        );
        perform_editor_command(&mut runtime, EditorCommand::CycleWalls);
        let snapshot = editor_snapshot(&runtime);
        assert_eq!(
            runtime.player_build.as_ref().unwrap().assembly.storeys[0]
                .walls
                .len(),
            1
        );
        assert_eq!(snapshot.walls, WallVisibility::Cutaway);
        assert!(snapshot.error.is_none());
    }

    #[test]
    fn wall_draw_command_snaps_to_grid_and_spans_dragged_cells() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let mut runtime = EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            Some(PlayerBuildDocument::empty()),
            None,
        );
        perform_editor_command(
            &mut runtime,
            EditorCommand::DrawWall {
                start_x_metres: 0.1,
                start_z_metres: 0.2,
                end_x_metres: 4.6,
                end_z_metres: 0.3,
                material: PlayerBuildMaterial::Brick,
                storey: 0,
            },
        );
        let assembly = &runtime.player_build.as_ref().unwrap().assembly;
        assert_eq!(assembly.storeys[0].walls.len(), 3);
        assert!(
            assembly.storeys[0]
                .walls
                .iter()
                .all(|wall| wall.direction == Direction::North)
        );
        assert!(assembly.storeys[0].walls.iter().all(|wall| {
            assembly.wall_style_for(WallSelector {
                storey_level: 0,
                cell: wall.cell,
                direction: wall.direction,
            }) == WallStyle::Brick
        }));
    }

    #[test]
    fn floor_tiles_classify_freeform_wall_faces() {
        let wall = WallSegment {
            cell: Cell::new(0, 0),
            direction: Direction::North,
            inside_room: 0,
            outside_room: None,
        };
        let mut storey = adventuresim_building_generator::StoreyPlan {
            level: 0,
            rooms: Vec::new(),
            walls: vec![wall],
            openings: Vec::new(),
        };
        assert_eq!(freeform_wall_faces(&storey, wall).len(), 2);
        storey.rooms.push(adventuresim_building_generator::Room {
            id: 0,
            kind: adventuresim_building_generator::RoomKind::CommonRoom,
            cells: vec![Cell::new(0, 0)],
        });
        let exterior = freeform_wall_faces(&storey, wall);
        assert_eq!(exterior.len(), 1);
        assert!(exterior[0].exterior());
        storey.rooms[0].cells.push(Cell::new(0, 1));
        let interior = freeform_wall_faces(&storey, wall);
        assert_eq!(interior.len(), 1);
        assert!(!interior[0].exterior());
    }

    #[test]
    fn new_freeform_build_action_enables_construct_mode_and_save_path() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let mut runtime =
            EditorRuntime::new(document, plan, PathBuf::from("my-house.json"), None, None);
        perform_editor_action(&mut runtime, EditorUiAction::NewPlayerBuild);
        let player_build = runtime.player_build.as_ref().unwrap();
        assert_eq!(
            player_build.schema_version,
            PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION
        );
        assert!(player_build.assembly.storeys.is_empty());
        assert_eq!(runtime.mode, EditorMode::Construct);
        assert_eq!(
            runtime.player_build_path,
            Some(PathBuf::from("my-house-player-build.json"))
        );
        assert!(runtime.pending_player_rebuild);
        assert!(!runtime.show_generated_building);
        assert!(runtime.pending_rebuild);
    }

    #[test]
    fn detaching_replaces_the_generated_scene_at_its_shared_origin() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let expected_wall_count = plan
            .storeys
            .iter()
            .map(|storey| storey.walls.len())
            .sum::<usize>();
        let mut runtime =
            EditorRuntime::new(document, plan, PathBuf::from("my-house.json"), None, None);
        perform_editor_action(&mut runtime, EditorUiAction::DetachPlayerBuild);
        assert!(!runtime.show_generated_building);
        assert!(runtime.pending_rebuild);
        assert_eq!(
            runtime
                .player_build
                .as_ref()
                .unwrap()
                .assembly
                .storeys
                .iter()
                .map(|storey| storey.walls.len())
                .sum::<usize>(),
            expected_wall_count
        );
    }

    #[test]
    fn detached_assembly_renders_roofs_with_roof_visibility_targets() {
        let plan = generate_document(&BuildingDocument::fixture(BuildingArchetype::TownHouse, 42))
            .unwrap();
        assert!(!plan.roofs.is_empty());
        let document = PlayerBuildDocument::from_plan(&plan);
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        setup_player_build_scene(&mut world, &document);
        let mut roofs = world.query::<&EditorVisibilityTarget>();
        assert!(
            roofs
                .iter(&world)
                .any(|target| target.role == EditorVisibilityRole::Roof)
        );
    }

    #[test]
    fn player_build_visibility_changes_entity_components_for_hide_and_levels() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let player_build = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::DrawWall {
                start: adventuresim_building_generator::GridPoint::new(0, 0),
                end: adventuresim_building_generator::GridPoint::new(1, 0),
                storey: 0,
                style: WallStyle::TimberFrame,
            })
            .unwrap()
            .apply(PlayerBuildEdit::DrawWall {
                start: adventuresim_building_generator::GridPoint::new(0, 0),
                end: adventuresim_building_generator::GridPoint::new(1, 0),
                storey: 1,
                style: WallStyle::TimberFrame,
            })
            .unwrap();
        assert_eq!(player_build.assembly.storeys.len(), 2);
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.insert_resource(EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            Some(player_build.clone()),
            None,
        ));
        setup_player_build_scene(&mut world, &player_build);

        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Down;
            runtime.roof_visibility = RoofVisibility::Hide;
            runtime.active_storey = 0;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        let mut query = world.query::<(&EditorVisibilityTarget, &Visibility)>();
        let visibilities = query.iter(&world).collect::<Vec<_>>();
        assert!(!visibilities.is_empty());
        for (_, visibility) in visibilities {
            assert_eq!(
                *visibility,
                Visibility::Hidden,
                "the semantic wall and its fachwerk members should be hidden together"
            );
        }

        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Cutaway;
            runtime.roof_visibility = RoofVisibility::Ghost;
            runtime.active_storey = 1;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        let mut query = world.query::<(Entity, &EditorVisibilityTarget)>();
        let entities = query
            .iter(&world)
            .map(|(entity, target)| (entity, target.role))
            .collect::<Vec<_>>();
        assert!(!entities.is_empty());
        for (entity, _) in &entities {
            assert_eq!(
                *world.get::<Visibility>(*entity).unwrap(),
                Visibility::Visible,
                "Cutaway/Ghost leaves the wall and roof visible"
            );
        }
        let material_assets = world.resource::<Assets<StandardMaterial>>();
        for (entity, role) in entities {
            let base = &world.get::<EditorBaseMaterial>(entity).unwrap().0;
            let applied = &world
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0;
            assert_ne!(applied, base, "{role:?} should use a translucent material");
            let material = material_assets.get(applied).unwrap();
            assert_eq!(material.alpha_mode, AlphaMode::Blend);
            assert_eq!(material.base_color.to_srgba().alpha, 0.24);
        }
    }

    #[test]
    fn generated_editor_geometry_receives_the_same_visibility_components() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let wall_owner = plan.wall_assemblies.first().unwrap().owner.0;
        let roof_owner = plan.roof_assemblies.first().unwrap().owner.0;
        let mut world = World::new();
        world.init_resource::<Assets<StandardMaterial>>();
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let wall = world
            .spawn((
                Mesh3d(Handle::default()),
                MeshMaterial3d(material.clone()),
                GeometryOwner(wall_owner),
            ))
            .id();
        let roof = world
            .spawn((
                Mesh3d(Handle::default()),
                MeshMaterial3d(material),
                GeometryOwner(roof_owner),
                RoofRenderItem {
                    id: 1,
                    fingerprint: 1,
                    local_center: Vec3::ZERO,
                    local_half_size: Vec3::ONE,
                },
            ))
            .id();
        let floor_material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let upper_floor = world
            .spawn((
                Name::new("room floor"),
                Mesh3d(Handle::default()),
                MeshMaterial3d(floor_material),
                Transform::from_xyz(0.0, plan.storey_height_metres + 0.06, 0.0),
            ))
            .id();
        let frame_material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let upper_frame = world
            .spawn((
                Name::new("resolved timber frame member"),
                Mesh3d(Handle::default()),
                MeshMaterial3d(frame_material),
                Transform::from_xyz(0.0, plan.storey_height_metres * 1.5, 0.0),
            ))
            .id();
        configure_editor_scene(&mut world, &plan, false);
        assert_eq!(
            world.get::<EditorVisibilityTarget>(wall).unwrap().role,
            EditorVisibilityRole::Wall
        );
        assert_eq!(
            world.get::<EditorVisibilityTarget>(roof).unwrap().role,
            EditorVisibilityRole::Roof
        );
        assert_eq!(
            world
                .get::<EditorVisibilityTarget>(upper_floor)
                .unwrap()
                .role,
            EditorVisibilityRole::Floor
        );
        assert_eq!(
            world
                .get::<EditorVisibilityTarget>(upper_frame)
                .unwrap()
                .role,
            EditorVisibilityRole::Structure
        );
        world.insert_resource(EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            None,
            None,
        ));
        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Down;
            runtime.roof_visibility = RoofVisibility::Hide;
            runtime.active_storey = 0;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        assert_eq!(*world.get::<Visibility>(wall).unwrap(), Visibility::Hidden);
        assert_eq!(*world.get::<Visibility>(roof).unwrap(), Visibility::Hidden);
        assert_eq!(
            *world.get::<Visibility>(upper_floor).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *world.get::<Visibility>(upper_frame).unwrap(),
            Visibility::Hidden
        );

        // A rebuild can add geometry after the runtime change has already
        // been observed. Newly tagged entities must still inherit Ground's
        // current visibility state on the next update.
        world.clear_trackers();
        let late_material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let late_upper_floor = world
            .spawn((
                EditorVisibilityTarget {
                    storey: 1,
                    role: EditorVisibilityRole::Floor,
                },
                EditorBaseMaterial(late_material.clone()),
                EditorAppearanceIsTranslucent(false),
                MeshMaterial3d(late_material),
                Visibility::Visible,
            ))
            .id();
        world.run_system_once(update_editor_visibility).unwrap();
        assert_eq!(
            *world.get::<Visibility>(late_upper_floor).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn ground_level_hides_every_upper_mesh_in_the_real_fachwerk_editor_scene() {
        let document = BuildingDocument::fixture(BuildingArchetype::FachwerkMerchantHouse, 42);
        let plan = generate_document(&document).unwrap();
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        setup(
            &mut world,
            &plan,
            ViewerView::Exterior,
            ProjectedProofKind::Machicolation,
            None,
            SceneSetup::EditorBuilding,
        );
        configure_editor_scene(&mut world, &plan, false);
        world.insert_resource(EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            None,
            None,
        ));
        world.run_system_once(update_editor_visibility).unwrap();

        let mut query = world.query::<(&EditorVisibilityTarget, &Visibility, &Name)>();
        let upper = query
            .iter(&world)
            .filter(|(target, _, _)| target.storey > 0)
            .collect::<Vec<_>>();
        assert!(
            !upper.is_empty(),
            "the fixture should include visible upper-level editor geometry"
        );
        for (_, visibility, name) in upper {
            assert_eq!(
                *visibility,
                Visibility::Hidden,
                "upper mesh {} remained visible with Ground selected",
                name
            );
        }
    }

    #[test]
    fn timber_programme_and_detached_build_share_one_stair_authority() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let Stair::Straight {
            start,
            direction,
            run_metres,
            ..
        } = plan.stairs[0]
        else {
            panic!("town-house fixture must retain a straight editable stair");
        };
        let axis = match direction {
            Direction::North => Vec2::Y,
            Direction::East => Vec2::X,
            Direction::South => -Vec2::Y,
            Direction::West => -Vec2::X,
        };
        let timber = plan.timber_frame.as_ref().expect("town-house timber frame");
        let first_tread = timber.circulation.stair_solids[0];
        let first_tread = plan
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == first_tread)
            .expect("resolved first timber stair tread");
        assert!(
            (Vec2::new(first_tread.centre.x, first_tread.centre.z)
                - (start + axis * (run_metres / 18.0)))
                .length()
                < 0.02,
            "editable stair must start at the resolver-selected clear flight"
        );

        let mut programme_world = World::new();
        programme_world.init_resource::<Assets<Mesh>>();
        programme_world.init_resource::<Assets<StandardMaterial>>();
        setup(
            &mut programme_world,
            &plan,
            ViewerView::Exterior,
            ProjectedProofKind::Machicolation,
            None,
            SceneSetup::EditorBuilding,
        );
        let mut programme_names = programme_world.query::<&Name>();
        assert!(
            programme_names
                .iter(&programme_world)
                .all(|name| name.as_str() != "straight stair tread"),
            "programme must not render a second generic stair over its resolved timber flight"
        );

        let mut detached_world = World::new();
        detached_world.init_resource::<Assets<Mesh>>();
        detached_world.init_resource::<Assets<StandardMaterial>>();
        setup_player_build_scene(&mut detached_world, &PlayerBuildDocument::from_plan(&plan));
        let expected_uncut_upper_tiles = plan.storeys[1]
            .rooms
            .iter()
            .map(|room| room.cells.len())
            .sum::<usize>();
        let mut detached_floor_tiles = detached_world.query::<(&Name, &EditorVisibilityTarget)>();
        assert!(
            detached_floor_tiles
                .iter(&detached_world)
                .filter(|(name, target)| {
                    name.as_str() == "player build floor tile" && target.storey == 1
                })
                .count()
                > expected_uncut_upper_tiles,
            "the upper floor tile at the stair arrival must be split around an opening"
        );
        let mut detached_names = detached_world.query::<&Name>();
        assert!(
            detached_names
                .iter(&detached_world)
                .any(|name| name.as_str() == "straight stair tread"),
            "detached build must render the shared editable stair recipe"
        );
    }

    #[test]
    fn detached_stair_has_a_full_height_clear_arrival_opening() {
        let plan = generate_document(&BuildingDocument::fixture(BuildingArchetype::TownHouse, 42))
            .unwrap();
        let document = PlayerBuildDocument::from_plan(&plan);
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        setup_player_build_scene(&mut world, &document);

        let (width, depth) = document.assembly.footprint.dimensions();
        let origin = Vec2::new(
            -f32::from(width) * CELL_SIZE_METRES * 0.5,
            -f32::from(depth) * CELL_SIZE_METRES * 0.5,
        );
        let floors = world
            .query::<&PlayerBuildFloorPrism>()
            .iter(&world)
            .copied()
            .collect::<Vec<_>>();
        let mut blockers = Vec::new();
        for stair in document.assembly.stairs {
            let Stair::Straight {
                start,
                direction,
                base_height_metres,
                rise_metres,
                width_metres: _,
                tread_count,
                run_metres,
            } = stair
            else {
                continue;
            };
            let axis = direction_vector_2d(direction);
            let lateral = Vec2::new(-axis.y, axis.x);
            let run = run_metres;
            for tread in 1..tread_count {
                let progress = f32::from(tread) / f32::from(tread_count);
                let centre = start + origin + axis * (progress * run);
                let foot_y = base_height_metres + progress * rise_metres;
                // The same 0.90 m route width / 1.90 m clearance contract as
                // timber circulation, evaluated against detached ECS floors.
                let body_min = Vec3::new(
                    centre.x - lateral.x.abs() * 0.45 - axis.x.abs() * 0.30,
                    foot_y,
                    centre.y - lateral.y.abs() * 0.45 - axis.y.abs() * 0.30,
                );
                let body_max = Vec3::new(
                    centre.x + lateral.x.abs() * 0.45 + axis.x.abs() * 0.30,
                    foot_y + 1.90,
                    centre.y + lateral.y.abs() * 0.45 + axis.y.abs() * 0.30,
                );
                if let Some(floor) = floors.iter().find(|floor| {
                    body_min.x < floor.max.x
                        && body_max.x > floor.min.x
                        && body_min.y < floor.max.y
                        && body_max.y > floor.min.y
                        && body_min.z < floor.max.z
                        && body_max.z > floor.min.z
                }) {
                    blockers.push((tread, body_min, body_max, floor.min, floor.max));
                }
            }
        }
        assert!(
            blockers.is_empty(),
            "detached stair occupant volume intersects floor material: {blockers:?}"
        );
        let mut stringers = world.query_filtered::<&Transform, With<PlayerBuildStairStringer>>();
        let stringer_axes = stringers
            .iter(&world)
            .map(|transform| transform.rotation * Vec3::X)
            .collect::<Vec<_>>();
        assert!(!stringer_axes.is_empty(), "detached stair has stringers");
        assert!(
            stringer_axes.iter().all(|axis| axis.y > 0.0),
            "detached stair stringers must rise in the declared ascent direction: {stringer_axes:?}"
        );
    }

    #[test]
    fn detached_stair_landings_reach_real_room_doorways() {
        let plan = generate_document(&BuildingDocument::fixture(BuildingArchetype::TownHouse, 42))
            .unwrap();
        let document = PlayerBuildDocument::from_plan(&plan);
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        setup_player_build_scene(&mut world, &document);

        let (width, depth) = document.assembly.footprint.dimensions();
        let origin = Vec2::new(
            -f32::from(width) * CELL_SIZE_METRES * 0.5,
            -f32::from(depth) * CELL_SIZE_METRES * 0.5,
        );
        let mut wall_query = world.query::<(&PlayerBuildRenderPrism, &EditorVisibilityTarget)>();
        let walls = wall_query
            .iter(&world)
            .filter_map(|(prism, target)| {
                (target.role == EditorVisibilityRole::Wall).then_some((target.storey, *prism))
            })
            .collect::<Vec<_>>();
        let Stair::Straight {
            start,
            direction,
            run_metres,
            ..
        } = document.assembly.stairs[0]
        else {
            panic!("town-house fixture has a straight stair");
        };
        let axis = direction_vector_2d(direction);
        let run = run_metres;
        let cell_size = 0.10;
        let min = origin + Vec2::splat(0.05);
        let max = origin
            + Vec2::new(
                f32::from(width) * CELL_SIZE_METRES - 0.05,
                f32::from(depth) * CELL_SIZE_METRES - 0.05,
            );
        let index = |point: Vec2| {
            let local = ((point - min) / cell_size).round().as_ivec2();
            (local.x, local.y)
        };
        let point = |index: (i32, i32)| {
            min + Vec2::new(index.0 as f32 * cell_size, index.1 as f32 * cell_size)
        };
        for (level, landing) in [(0, start + origin), (1, start + origin + axis * run)] {
            let storey = document
                .assembly
                .storeys
                .iter()
                .find(|storey| storey.level == level)
                .expect("town-house has both stair landing storeys");
            let doorways = storey
                .openings
                .iter()
                .filter(|opening| opening.kind == OpeningKind::Door)
                .map(|opening| storey.walls[opening.wall].centre() + origin)
                .collect::<Vec<_>>();
            assert!(!doorways.is_empty(), "storey {level} has room doorways");
            let storey_walls = walls
                .iter()
                .filter_map(|(wall_level, wall)| {
                    (*wall_level == usize::from(level)).then_some(*wall)
                })
                .collect::<Vec<_>>();
            let floor_y = f32::from(level) * document.assembly.storey_height_metres + 0.01;
            let walkable = |position: Vec2| {
                position.cmpge(min).all()
                    && position.cmple(max).all()
                    && storey_walls.iter().all(|wall| {
                        let foot_min = Vec3::new(position.x - 0.45, floor_y, position.y - 0.15);
                        let foot_max =
                            Vec3::new(position.x + 0.45, floor_y + 1.90, position.y + 0.15);
                        foot_max.x <= wall.min.x
                            || foot_min.x >= wall.max.x
                            || foot_max.y <= wall.min.y
                            || foot_min.y >= wall.max.y
                            || foot_max.z <= wall.min.z
                            || foot_min.z >= wall.max.z
                    })
            };
            let landing_cell = index(landing);
            assert!(
                walkable(point(landing_cell)),
                "stair landing on storey {level} is inside a wall"
            );
            let mut seen = std::collections::HashSet::from([landing_cell]);
            let mut queue = std::collections::VecDeque::from([landing_cell]);
            while let Some(current) = queue.pop_front() {
                for offset in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let next = (current.0 + offset.0, current.1 + offset.1);
                    if seen.insert(next) && walkable(point(next)) {
                        queue.push_back(next);
                    }
                }
            }
            assert!(
                doorways.iter().any(|doorway| {
                    seen.iter()
                        .map(|cell| point(*cell))
                        .any(|position| position.distance(*doorway) <= cell_size * 1.5)
                }),
                "no 0.90 m-wide, 1.90 m-high route reaches a room doorway from the stair landing on storey {level}"
            );
        }
    }

    #[test]
    fn fixture_reconfiguration_preserves_camera_and_editor_environment() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let mut world = World::new();
        let camera = world
            .spawn((
                Camera3d::default(),
                Transform::from_xyz(12.0, 8.0, -10.0),
                PanOrbitCamera {
                    focus: Vec3::new(3.0, 2.0, 1.0),
                    target_focus: Vec3::new(3.0, 2.0, 1.0),
                    radius: Some(17.0),
                    target_radius: 17.0,
                    ..default()
                },
            ))
            .id();
        let environment = world
            .spawn((
                Mesh3d(Handle::default()),
                EditorEnvironmentEntity,
                Name::new("editor ground"),
            ))
            .id();
        let building = world.spawn(Mesh3d(Handle::default())).id();

        configure_editor_scene(&mut world, &plan, false);

        let orbit = world.get::<PanOrbitCamera>(camera).unwrap();
        assert_eq!(orbit.focus, Vec3::new(3.0, 2.0, 1.0));
        assert_eq!(orbit.radius, Some(17.0));
        assert!(world.get::<EditorBuildingEntity>(environment).is_none());
        assert!(world.get::<EditorBuildingEntity>(building).is_some());
    }

    #[test]
    fn editor_maps_resolved_owners_to_stable_individual_targets() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let (owner_targets, item_targets) = editor_owner_targets(&plan);

        for wall in &plan.wall_assemblies {
            if matches!(wall.source, WallSourceId::StoreyWall { .. }) {
                assert!(
                    matches!(
                        owner_targets.get(&wall.owner.0),
                        Some(EditorTarget::Wall(_))
                    ),
                    "storey wall owner {} must remain selectable, got {:?}",
                    wall.owner.0,
                    owner_targets.get(&wall.owner.0)
                );
            }
        }
        for opening in &plan.opening_assemblies {
            if matches!(opening.host_source, WallSourceId::StoreyWall { .. }) {
                assert!(
                    matches!(
                        item_targets.get(&opening.head_solid.0),
                        Some(EditorTarget::Opening(_))
                    ),
                    "opening head {} must remain selectable",
                    opening.head_solid.0
                );
            }
        }
        let frame = plan.timber_frame.as_ref().unwrap();
        let mut wall_grouped_members = 0;
        for member in &frame.members {
            match item_targets.get(&member.solid.0) {
                Some(EditorTarget::Wall(_)) => wall_grouped_members += 1,
                Some(EditorTarget::TimberMember(id)) if *id == member.id.0 => {}
                target => panic!("unexpected timber target for {}: {target:?}", member.id.0),
            }
        }
        assert!(
            wall_grouped_members > 0,
            "fachwerk bays should select their wall"
        );
    }

    #[test]
    fn splayed_jamb_mesh_is_a_closed_consistently_wound_solid() {
        for side in [-1, 1] {
            for exterior_depth_sign in [-1, 1] {
                let mesh = splayed_jamb_mesh(0.9, 3.4, 1.2, 0.18, 0.68, side, exterior_depth_sign);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected splayed-jamb vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "side={side}, exterior={exterior_depth_sign}: {report:?}"
                );
            }
        }
    }

    #[test]
    fn splayed_head_mesh_is_a_closed_consistently_wound_solid() {
        for exterior_depth_sign in [-1, 1] {
            let mesh = splayed_head_mesh(1.1, 0.82, 1.2, 0.48, 1.10, exterior_depth_sign);
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected splayed-head vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(
                report.passes_closed_solid(),
                "exterior={exterior_depth_sign}: {report:?}"
            );
        }
    }

    #[test]
    fn roof_face_meshes_remain_closed_after_authoritative_child_cuts() {
        for archetype in BuildingArchetype::ALL {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            for face in plan.roof_assemblies.iter().flat_map(|roof| &roof.faces) {
                let mesh = roof_face_prism_mesh(face);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected roof vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "{archetype:?} face {} cuts={:?}: {report:?}",
                    face.id.0,
                    face.cutouts
                );
            }
            for enclosure in plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.enclosure_faces)
            {
                let mesh = roof_enclosure_prism_mesh(enclosure);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected roof enclosure vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "{archetype:?} enclosure {}: {report:?}",
                    enclosure.id.0
                );
            }
        }
    }

    #[test]
    fn radial_tower_shell_mesh_is_closed_with_true_wall_thickness() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::WalledKeep, 42)).unwrap();
        let tower = plan.towers[0];
        let portals = plan
            .tower_portals
            .iter()
            .copied()
            .filter(|portal| portal.tower_index == 0)
            .collect::<Vec<_>>();
        let firing = plan
            .gate_defenses
            .iter()
            .flat_map(|defense| defense.firing_positions.iter())
            .filter(|position| position.tower_index == 0)
            .cloned()
            .collect::<Vec<_>>();
        for section in [false, true] {
            let mesh = tower_shell_mesh(tower, &portals, &firing, section);
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected radial-shell vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(
                report.passes_closed_solid(),
                "section={section}: {report:?}"
            );
        }
    }

    #[test]
    fn true_arch_spandrel_meshes_are_closed_and_consistently_wound() {
        for archetype in [
            BuildingArchetype::RenaissanceTownHall,
            BuildingArchetype::Cathedral,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            let opening =
                plan.opening_assemblies
                    .iter()
                    .find(|opening| {
                        matches!(
                    opening.profile,
                    adventuresim_building_generator::OpeningProfile::Segmental { .. }
                        | adventuresim_building_generator::OpeningProfile::PointedTwoCentred { .. }
                )
                    })
                    .unwrap();
            let solid = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == opening.head_solid)
                .unwrap();
            let (rise, radius) = match solid.shape {
                adventuresim_building_generator::ResolvedSolidShape::SegmentalArchRing {
                    rise_metres,
                    ..
                } => (rise_metres, None),
                adventuresim_building_generator::ResolvedSolidShape::PointedArchRing {
                    spring_height_metres,
                    apex_height_metres,
                    arc_radius_metres,
                    ..
                } => (
                    apex_height_metres - spring_height_metres,
                    Some(arc_radius_metres),
                ),
                _ => unreachable!(),
            };
            let mesh = arched_spandrel_mesh(
                solid.size.x.max(solid.size.z),
                solid.size.y,
                solid.size.x.min(solid.size.z),
                rise,
                radius,
            );
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected arch vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(report.passes_closed_solid(), "{archetype:?}: {report:?}");
        }
    }

    #[test]
    fn resolved_renderer_fingerprint_rejects_omission_duplication_and_transform_drift() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::CourtyardCastle,
            42,
        ))
        .unwrap();
        let fingerprints = |solids: &[adventuresim_building_generator::ResolvedSolid]| {
            resolved_item_multiset_hash(
                solids
                    .iter()
                    .map(|solid| (solid.id.0, stable_u64(&serde_json::to_vec(solid).unwrap()))),
            )
        };
        let expected = fingerprints(&plan.resolved_geometry.solids);
        assert_ne!(expected, fingerprints(&plan.resolved_geometry.solids[1..]));
        let mut duplicated = plan.resolved_geometry.solids.clone();
        duplicated.push(duplicated[0].clone());
        assert_ne!(expected, fingerprints(&duplicated));
        let mut moved = plan.resolved_geometry.solids.clone();
        moved[0].centre.x += 0.05;
        assert_ne!(expected, fingerprints(&moved));
        let mut resized = plan.resolved_geometry.solids.clone();
        resized[0].size.y += 0.05;
        assert_ne!(expected, fingerprints(&resized));
    }

    #[test]
    fn crown_proof_suite_rejects_mixed_build_and_fixture_hashes() {
        let records = || {
            CROWN_PROOF_SUITE
                .iter()
                .map(|(name, fixture, view)| {
                    (
                        *name,
                        CrownSuiteManifest {
                            fixture: (*fixture).to_owned(),
                            view: (*view).to_owned(),
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!("resolved-{fixture}"),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{fixture}"),
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_crown_suite_records(&records()).is_ok());

        let mut mixed_build = records();
        mixed_build[5].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_crown_suite_records(&mixed_build).is_err());

        let mut stale_fixture = records();
        stale_fixture[7].1.resolved_geometry_hash = "stale-resolved".to_owned();
        assert!(validate_crown_suite_records(&stale_fixture).is_err());
    }

    #[test]
    fn projected_proof_suite_requires_exact_state_ids_and_one_build() {
        let records = || {
            PROJECTED_PROOF_SUITE
                .iter()
                .map(|expected| {
                    (
                        expected.basename,
                        ProjectedSuiteManifest {
                            fixture: expected.fixture.to_owned(),
                            view: expected.view.to_owned(),
                            seed: expected.seed,
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!(
                                "resolved-{}-{}",
                                expected.fixture, expected.seed
                            ),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{}-{}", expected.fixture, expected.seed),
                            focus_kind: expected.kind.map(|_| "resolved_projected".to_owned()),
                            focused_resolved_item_ids: expected
                                .kind
                                .map_or_else(Vec::new, |_| vec![1]),
                            focused_resolved_void_ids: if expected.deployment
                                == Some("sockets_only")
                                || expected.kind.is_none()
                            {
                                Vec::new()
                            } else {
                                vec![2]
                            },
                            focused_projected_ray_count: if expected.deployment
                                == Some("sockets_only")
                                || expected.kind.is_none()
                            {
                                0
                            } else {
                                1
                            },
                            projected_defense_kind: expected.kind.map(str::to_owned),
                            projected_defense_deployment: expected.deployment.map(str::to_owned),
                            projected_tactical_target: expected
                                .kind
                                .map(|_| "named_target".to_owned()),
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_projected_suite_records(&records()).is_ok());

        let mut mixed_build = records();
        mixed_build[8].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_projected_suite_records(&mixed_build).is_err());

        let mut missing_exact_ids = records();
        missing_exact_ids[0].1.focused_resolved_void_ids.clear();
        assert!(validate_projected_suite_records(&missing_exact_ids).is_err());

        let mut stale_seed_state = records();
        stale_seed_state[10].1.seed = 42;
        assert!(validate_projected_suite_records(&stale_seed_state).is_err());
    }

    #[test]
    fn openings_proof_suite_requires_exact_triples_sections_and_one_build() {
        let records = || {
            OPENINGS_PROOF_SUITE
                .iter()
                .copied()
                .map(|expected| {
                    let focused =
                        expected.opening_profile.is_some() || expected.wall_section_kind.is_some();
                    let profile_serial = expected
                        .opening_profile
                        .map(|profile| stable_u64(profile.as_bytes()))
                        .unwrap_or_else(|| {
                            expected
                                .wall_section_kind
                                .map(|kind| stable_u64(kind.as_bytes()))
                                .unwrap_or(0)
                        });
                    (
                        expected,
                        OpeningsSuiteManifest {
                            fixture: expected.fixture.to_owned(),
                            view: expected.view.to_owned(),
                            seed: 42,
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!("resolved-{}", expected.fixture),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{}", expected.fixture),
                            opening_profile: expected.opening_profile.map(str::to_owned),
                            wall_section_kind: expected.wall_section_kind.map(str::to_owned),
                            focused_assembly_owner_id: focused.then_some(profile_serial as u32),
                            focused_resolved_item_ids: focused
                                .then_some(vec![profile_serial + 1])
                                .unwrap_or_default(),
                            focused_resolved_void_ids: expected
                                .opening_profile
                                .map(|_| vec![profile_serial + 2])
                                .unwrap_or_default(),
                            focused_resolved_geometry_hash: focused
                                .then(|| format!("focus-{profile_serial}")),
                            section_cut_applied: expected.section,
                            section_removed_item_ids: if expected.section
                                && expected.wall_section_kind != Some("round_tower_radial")
                            {
                                vec![profile_serial + 1]
                            } else {
                                Vec::new()
                            },
                            inside_label_visible: expected.section,
                            outside_label_visible: expected.section,
                            wall_thickness_metres: expected.section.then_some(0.5),
                            scale_figure_height_metres: expected.section.then_some(1.75),
                            scale_figure_visible: expected.section,
                            section_annotation: if expected.section {
                                format!(
                                    "wall=1 opening=2 profile={} thickness=0.50m",
                                    expected.opening_profile.unwrap_or("solid_section")
                                )
                            } else {
                                String::new()
                            },
                            section_annotation_visible: expected.section,
                            exterior_throat_bounds_fraction: if matches!(
                                expected.opening_profile,
                                Some("arrow_loop" | "gun_loop")
                            ) {
                                [0.30, 0.25, 0.42, 0.72]
                            } else {
                                [0.0; 4]
                            },
                            interior_mouth_bounds_fraction: if matches!(
                                expected.opening_profile,
                                Some("arrow_loop" | "gun_loop")
                            ) {
                                [0.48, 0.20, 0.68, 0.76]
                            } else {
                                [0.0; 4]
                            },
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_openings_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[4].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_openings_suite_records(&mixed).is_err());

        let mut triple_drift = records();
        triple_drift[1].1.focused_resolved_item_ids = vec![u64::MAX];
        assert!(validate_openings_suite_records(&triple_drift).is_err());

        let mut false_section = records();
        false_section[2].1.inside_label_visible = false;
        assert!(validate_openings_suite_records(&false_section).is_err());

        let mut uncut_ordinary_wall = records();
        uncut_ordinary_wall[15].1.section_removed_item_ids.clear();
        assert!(validate_openings_suite_records(&uncut_ordinary_wall).is_err());

        let mut stale_regression = records();
        stale_regression[19].1.focused_assembly_owner_id = Some(7);
        assert!(validate_openings_suite_records(&stale_regression).is_err());
    }

    #[test]
    fn roof_proof_suite_rejects_mixed_build_and_render_correspondence() {
        let records = || {
            ROOF_PROOF_SLUGS
                .iter()
                .map(|slug| ((*slug).to_owned(), (*slug).to_owned(), true))
                .chain(ROOF_REGRESSION_FIXTURES.iter().map(|fixture| {
                    (
                        format!("roof-{fixture}-regression"),
                        "exterior".to_owned(),
                        false,
                    )
                }))
                .map(|(basename, view, focused)| {
                    let graph_hash = if basename.contains("low-pitch") {
                        "roof-low"
                    } else if basename.contains("mid-pitch") {
                        "roof-mid"
                    } else if basename.contains("high-pitch") {
                        "roof-high"
                    } else {
                        "roof"
                    };
                    (
                        basename.clone(),
                        view.clone(),
                        focused,
                        RoofSuiteManifest {
                            fixture: basename,
                            view,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: "plan".to_owned(),
                            roof_graph_hash: graph_hash.to_owned(),
                            roof_render_item_count: 4,
                            roof_render_multiset_hash: "render".to_owned(),
                            rendered_roof_item_count: 4,
                            rendered_roof_hash: "render".to_owned(),
                            focused_roof_item_ids: focused.then_some(vec![1]).unwrap_or_default(),
                            visible_focused_roof_item_count: usize::from(focused),
                            section_removed_roof_item_ids: Vec::new(),
                            section_annotation_visible: focused,
                            roof_drainage_network_ids: vec![10],
                            roof_drainage_channel_ids: vec![11],
                            roof_drainage_outlet_ids: vec![12],
                            roof_drainage_route_ids: vec![13],
                            focused_resolved_void_ids: vec![12],
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_roof_suite_records(&records()).is_ok());
        let mut mixed = records();
        mixed[10].3.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_roof_suite_records(&mixed).is_err());
        let mut drift = records();
        drift[12].3.rendered_roof_hash = "wrong".to_owned();
        assert!(validate_roof_suite_records(&drift).is_err());
        let mut stale = records();
        stale[20].3.focused_roof_item_ids.clear();
        assert!(validate_roof_suite_records(&stale).is_err());
        let mut section = records();
        section[1].3.visible_focused_roof_item_count = 0;
        section[1].3.section_removed_roof_item_ids = vec![1];
        assert!(validate_roof_suite_records(&section).is_ok());
    }

    #[test]
    fn church_proof_suite_requires_one_authority_and_real_sections() {
        let records = || {
            CHURCH_PROOF_SLUGS
                .iter()
                .map(|slug| {
                    let section = slug.contains("cut")
                        || slug.ends_with("-interior")
                        || slug.ends_with("-section")
                        || slug.ends_with("-load")
                        || slug.ends_with("-vault")
                        || matches!(
                            *slug,
                            "church-tower-junction"
                                | "church-tower-stair"
                                | "church-tower-bell-underside"
                                | "church-tower-frame"
                                | "church-support-dag"
                        );
                    let focused_roles = vec![
                        "ChurchPier",
                        "ChurchArcade",
                        "ChurchVaultThrust",
                        "WallButtress",
                        "ChurchVaultShell",
                        "ChurchCrossingArch",
                        "WallHost",
                        "ChurchStairTread",
                        "Landing",
                        "ChurchGuard",
                        "ChurchBellFloor",
                        "ChurchBell",
                        "ChurchBellFrame",
                        "ChurchServiceLadder",
                        "RoofGutter",
                    ]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                    let target_suffix = if slug.starts_with("church-bay-") {
                        "/nave-bay:2"
                    } else if slug.starts_with("church-crossing-") {
                        "/crossing"
                    } else if slug.starts_with("church-choir-") {
                        "/choir-apse"
                    } else if slug.starts_with("church-tower-") {
                        "/west-tower"
                    } else if *slug == "church-drainage" {
                        "/roof-drainage"
                    } else if *slug == "church-support-dag" {
                        "/nave-bay:2/load-path"
                    } else {
                        "/whole"
                    };
                    (
                        *slug,
                        ChurchSuiteManifest {
                            fixture: "cathedral".to_owned(),
                            view: (*slug).to_owned(),
                            seed: 47,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: "plan-a".to_owned(),
                            resolved_geometry_hash: "resolved-a".to_owned(),
                            church_program_hash: "church-a".to_owned(),
                            church_bay_labels: ["N1", "N2", "N3", "N4", "X", "Q1", "Q2", "A5"]
                                .into_iter()
                                .map(str::to_owned)
                                .collect(),
                            church_support_node_ids: vec![1, 2],
                            church_opening_ids: (100..136).collect(),
                            church_focused_roles: focused_roles.clone(),
                            church_target_component_ids: vec![format!("church:1{target_suffix}")],
                            church_target_item_ids: vec![3, 4],
                            church_required_roles: Vec::new(),
                            church_cut_plane: section.then_some([0.0, 0.0, 1.0, -10.5]),
                            church_removed_target_item_ids: section
                                .then_some(vec![3])
                                .unwrap_or_default(),
                            church_legend_visible: true,
                            focused_bounds_fraction: [0.2, 0.2, 0.6, 0.7],
                            pixel_hash: format!("pixel-{slug}"),
                            focused_resolved_item_ids: vec![3, 4],
                            section_removed_item_ids: section
                                .then_some(vec![3])
                                .unwrap_or_default(),
                            visible_focused_resolved_item_count: 1,
                            section_cut_applied: section,
                            section_annotation_visible: section,
                            plan_audit_issue_count: 0,
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_church_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[7].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_church_suite_records(&mixed).is_err());

        let mut uncut = records();
        uncut[5].1.section_cut_applied = false;
        assert!(validate_church_suite_records(&uncut).is_err());

        let mut missing_bay = records();
        missing_bay[0].1.church_bay_labels.pop();
        assert!(validate_church_suite_records(&missing_bay).is_err());

        let mut duplicate_pixels = records();
        duplicate_pixels[10].1.pixel_hash = duplicate_pixels[9].1.pixel_hash.clone();
        assert!(validate_church_suite_records(&duplicate_pixels).is_err());

        let mut wrong_kind = records();
        wrong_kind[11].1.church_focused_roles = vec!["ChurchVaultShell".to_owned()];
        assert!(validate_church_suite_records(&wrong_kind).is_err());

        let mut generic_whole_substitution = records();
        generic_whole_substitution[11].1.church_target_component_ids =
            vec!["church:1/whole".to_owned()];
        assert!(validate_church_suite_records(&generic_whole_substitution).is_err());

        let mut tiny_target = records();
        tiny_target[12].1.focused_bounds_fraction = [0.49, 0.49, 0.51, 0.51];
        assert!(validate_church_suite_records(&tiny_target).is_err());

        let mut off_target_cut = records();
        off_target_cut[20].1.church_removed_target_item_ids = vec![u64::MAX];
        assert!(validate_church_suite_records(&off_target_cut).is_err());

        let mut missing_legend = records();
        missing_legend[28].1.church_legend_visible = false;
        assert!(validate_church_suite_records(&missing_legend).is_err());
    }

    #[test]
    fn timber_proof_suite_rejects_mixed_duplicate_and_unbound_evidence() {
        let records = || {
            timber_proof_specs()
                .into_iter()
                .enumerate()
                .map(|(index, (slug, archetype, view))| {
                    let section = timber_section_proof(view);
                    let fixture = archetype.slug().to_owned();
                    let opening = matches!(
                        view,
                        ViewerView::TimberOpeningBayExterior
                            | ViewerView::TimberOpeningBayInterior
                            | ViewerView::TimberOpeningBaySection
                    );
                    let roles = if opening {
                        vec!["FramePost".to_owned(), "WallHost".to_owned()]
                    } else {
                        vec!["FramePost".to_owned()]
                    };
                    let role_item_ids = roles
                        .iter()
                        .enumerate()
                        .map(|(role_index, role)| (role.clone(), vec![role_index as u64 + 1]))
                        .collect();
                    let role_bounds = roles
                        .iter()
                        .map(|role| (role.clone(), [0.25, 0.20, 0.65, 0.75]))
                        .collect();
                    (
                        slug.clone(),
                        archetype,
                        view,
                        TimberSuiteManifest {
                            fixture: fixture.clone(),
                            view: timber_proof_suffix(view).unwrap().to_owned(),
                            seed: 47,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{fixture}"),
                            resolved_geometry_hash: format!("geometry-{fixture}"),
                            timber_program_hash: format!("frame-{fixture}"),
                            timber_program: Some("program".to_owned()),
                            timber_assembly_id: Some(1),
                            timber_member_ids: (1..=20).collect(),
                            timber_joint_ids: (1..=12).collect(),
                            timber_node_ids: (1..=12).collect(),
                            timber_focused_roles: roles.clone(),
                            timber_role_item_ids: role_item_ids,
                            timber_role_bounds_fraction: role_bounds,
                            timber_target_component_ids: vec![format!("timber:1/{slug}")],
                            timber_focus_interface_ids: if view == ViewerView::TimberJointClose {
                                vec![41, 42]
                            } else {
                                vec![41]
                            },
                            timber_required_roles: roles,
                            timber_cut_plane: section.then_some([0.0, 0.0, 1.0, -2.0]),
                            timber_removed_target_item_ids: Vec::new(),
                            timber_legend_visible: true,
                            focused_resolved_item_ids: vec![1],
                            focused_resolved_void_ids: opening
                                .then_some(vec![88])
                                .unwrap_or_default(),
                            focused_roof_item_ids: (view == ViewerView::TimberGableRoofBearing)
                                .then_some(vec![77])
                                .unwrap_or_default(),
                            section_removed_item_ids: if section { vec![999] } else { Vec::new() },
                            visible_focused_resolved_item_count: 1,
                            focused_bounds_fraction: [0.25, 0.20, 0.65, 0.75],
                            section_cut_applied: section,
                            section_annotation_visible: true,
                            pixel_hash: format!("pixel-{index}"),
                            plan_audit_issue_count: 0,
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_timber_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[3].3.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_timber_suite_records(&mixed).is_err());

        let mut duplicate_pixel = records();
        duplicate_pixel[6].3.pixel_hash = duplicate_pixel[5].3.pixel_hash.clone();
        assert!(validate_timber_suite_records(&duplicate_pixel).is_err());

        let mut off_target = records();
        off_target[28].3.timber_target_component_ids = vec!["timber-whole".to_owned()];
        assert!(validate_timber_suite_records(&off_target).is_err());

        let mut missing_cut = records();
        let cut = missing_cut
            .iter_mut()
            .find(|record| timber_section_proof(record.2))
            .unwrap();
        cut.3.timber_cut_plane = None;
        assert!(validate_timber_suite_records(&missing_cut).is_err());

        let mut empty_roles = records();
        empty_roles[0].3.timber_required_roles.clear();
        assert!(validate_timber_suite_records(&empty_roles).is_err());

        let mut no_contact = records();
        no_contact[12].3.timber_focus_interface_ids.clear();
        assert!(validate_timber_suite_records(&no_contact).is_err());
    }
}
