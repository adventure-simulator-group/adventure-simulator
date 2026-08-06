use super::*;

pub(super) fn request_animation_packs(
    catalog: Res<AnimationPackCatalog>,
    asset_server: Res<AssetServer>,
    mut runtime: ResMut<AnimationRuntime>,
) {
    runtime.requested_base = Some(asset_server.load(animation_asset_path(BIPED_BASE_GLB)));
    for (pack_id, pack) in &catalog.packs {
        for (motion_id, source) in &pack.motions {
            runtime.requested_motions.insert(
                (pack_id.clone(), motion_id.clone()),
                asset_server.load(animation_asset_path(&source.path)),
            );
        }
    }
    if let Err(error) = runtime.library.validate_structure() {
        error!(
            ?error,
            "Rejected structurally invalid animation pack library"
        );
    }
}

pub(super) fn collect_loaded_packs(
    catalog: Res<AnimationPackCatalog>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut runtime: ResMut<AnimationRuntime>,
) {
    let mut changed = false;
    if !runtime.base_processed && !runtime.base_failed {
        let Some(handle) = runtime.requested_base.as_ref() else {
            return;
        };
        if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
            runtime.base_failed = true;
            warn!(path = BIPED_BASE_GLB, "Authored base rig is unavailable");
        } else if let Some(gltf) = gltfs.get(handle) {
            runtime.base_scene = gltf
                .default_scene
                .clone()
                .or_else(|| gltf.scenes.first().cloned());
            if runtime.base_scene.is_none() {
                runtime.base_failed = true;
                warn!(path = BIPED_BASE_GLB, "Authored base rig has no scene");
            } else {
                runtime.base_processed = true;
            }
        }
    }

    let requested = runtime
        .requested_motions
        .iter()
        .map(|(key, handle)| (key.clone(), handle.clone()))
        .collect::<Vec<_>>();
    for (key, handle) in requested {
        if runtime.processed_motions.contains(&key) || runtime.unavailable_motions.contains(&key) {
            continue;
        }
        if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        let Some(gltf) = gltfs.get(&handle) else {
            continue;
        };
        let Some(clip_handle) = sole_animation(&gltf.animations) else {
            warn!(
                pack = key.0,
                motion = key.1,
                count = gltf.animations.len(),
                "Motion must contain exactly one animation"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        };
        let Some(clip) = clips.get(clip_handle) else {
            continue;
        };
        if runtime.canonical_targets.is_empty() {
            continue;
        }
        if !clip_targets_match_base(clip, &runtime.canonical_targets) {
            warn!(
                pack = key.0,
                motion = key.1,
                "Motion targets are incompatible with the base rig"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        let Some(source) = catalog
            .packs
            .get(&key.0)
            .and_then(|pack| pack.motions.get(&key.1))
        else {
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        };
        if !frame_fits_clip(source.last_frame, clip.duration()) {
            warn!(
                pack = key.0,
                motion = key.1,
                last_frame = source.last_frame,
                duration = clip.duration(),
                "Motion is shorter than its catalog frames"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        runtime
            .clip_handles
            .insert(key.clone(), clip_handle.clone());
        runtime.processed_motions.insert(key);
        changed = true;
    }

    if !changed {
        return;
    }
    runtime.library = AnimationPackLibrary::default();
    for (pack_id, pack) in &catalog.packs {
        let available = pack
            .poses
            .iter()
            .filter(|(_, anchor)| {
                runtime
                    .processed_motions
                    .contains(&(pack_id.clone(), anchor.motion.clone()))
            })
            .map(|(&pose, _)| pose)
            .collect();
        if let Err(error) = runtime.library.insert(AnimationPack {
            id: pack_id.clone(),
            skeleton_family: pack.skeleton_family.clone(),
            fallback: pack.fallback.clone(),
            clips: available,
        }) {
            error!(?error, pack = pack_id, "Rejected animation pack");
        }
    }
    let ordered = runtime
        .clip_handles
        .iter()
        .map(|(key, handle)| (key.clone(), handle.clone()))
        .collect::<Vec<_>>();
    let mut graph = AnimationGraph::new();
    runtime.clips = ordered
        .into_iter()
        .map(|(key, handle)| {
            let node = graph.add_clip(handle.clone(), 1.0, graph.root);
            let pack = &catalog.packs[&key.0];
            let anchor_motion = key.1.strip_suffix("_mirrored").unwrap_or(&key.1);
            let anchor_frames = pack
                .poses
                .values()
                .filter(|anchor| anchor.motion == anchor_motion)
                .map(|anchor| anchor.frame)
                .chain(
                    pack.references
                        .get(anchor_motion)
                        .into_iter()
                        .flatten()
                        .map(|reference| reference.frame),
                )
                .collect::<BTreeSet<_>>();
            let anchor_nodes = anchor_frames
                .into_iter()
                .map(|frame| (frame, graph.add_clip(handle.clone(), 1.0, graph.root)))
                .collect();
            (key, LoadedClip { node, anchor_nodes })
        })
        .collect();
    runtime.graph = Some(graphs.add(graph));
    runtime.revision = runtime.revision.wrapping_add(1);
}

pub(super) fn sole_animation(
    animations: &[Handle<AnimationClip>],
) -> Option<&Handle<AnimationClip>> {
    (animations.len() == 1).then(|| &animations[0])
}

pub(super) fn frame_fits_clip(frame: u16, duration: f32) -> bool {
    frame_seconds(frame) <= duration + 0.5 / ANIMATION_FPS
}

pub(super) fn frame_seconds(frame: u16) -> f32 {
    frame as f32 / ANIMATION_FPS
}

pub(super) fn clip_targets_match_base(
    clip: &AnimationClip,
    canonical_targets: &HashSet<AnimationTargetId>,
) -> bool {
    targets_match_base(clip.curves().keys(), canonical_targets)
}

pub(super) fn targets_match_base<'a>(
    mut targets: impl Iterator<Item = &'a AnimationTargetId>,
    canonical_targets: &HashSet<AnimationTargetId>,
) -> bool {
    let Some(first) = targets.next() else {
        return false;
    };
    canonical_targets.contains(first) && targets.all(|target| canonical_targets.contains(target))
}

pub(super) fn attach_loaded_rig_scenes(
    mut commands: Commands,
    runtime: Res<AnimationRuntime>,
    players: Query<(Entity, &SkeletonState, Has<AnimationRigAttached>), With<Player>>,
) {
    for (player, _skeleton, attached) in &players {
        if attached {
            continue;
        }
        let Some(scene) = runtime.base_scene.as_ref() else {
            continue;
        };
        commands.entity(player).with_children(|parent| {
            parent.spawn((
                Name::new("Authored animation rig"),
                AnimationRigScene(player),
                SceneRoot(scene.clone()),
                Transform::from_xyz(0.0, PLAYER_VISUAL_Y_OFFSET, 0.0),
                Visibility::Hidden,
            ));
        });
        commands
            .entity(player)
            .insert((AnimationPlayback::default(), AnimationRigAttached));
    }
}

/// A zero-animation base glTF receives none of Bevy's usual animation wiring.
/// Recreate the loader's stable name-path target IDs so independently exported
/// motion files can animate the canonical `Skeleton` hierarchy.
pub(super) fn establish_animation_targets(
    mut commands: Commands,
    mut runtime: ResMut<AnimationRuntime>,
    roots: Query<(Entity, &AnimationRigScene), Without<RigAnimationTargetsBound>>,
    children: Query<&Children>,
    names: Query<&Name>,
    existing_players: Query<(), With<AnimationPlayer>>,
) {
    for (rig_root, owner) in &roots {
        let Some(skeleton_root) = find_named_descendant(rig_root, "Skeleton", &children, &names)
        else {
            continue;
        };
        let player = descendants_including(skeleton_root, &children)
            .into_iter()
            .find(|entity| existing_players.contains(*entity))
            .unwrap_or(skeleton_root);
        commands.entity(player).insert((
            AnimationPlayer::default(),
            AnimationPlayerOwner(owner.0),
            AnimationGraphRevision::default(),
        ));
        bind_animation_target_paths(
            &mut commands,
            skeleton_root,
            player,
            Vec::new(),
            &children,
            &names,
            &mut runtime.canonical_targets,
        );
        commands.entity(rig_root).insert(RigAnimationTargetsBound);
    }
}

pub(super) fn descendants_including(root: Entity, children: &Query<&Children>) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        found.push(entity);
        if let Ok(entity_children) = children.get(entity) {
            pending.extend(entity_children.iter());
        }
    }
    found
}

pub(super) fn find_named_descendant(
    root: Entity,
    target: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    descendants_including(root, children)
        .into_iter()
        .find(|entity| names.get(*entity).is_ok_and(|name| name.as_str() == target))
}

pub(super) fn bind_animation_target_paths(
    commands: &mut Commands,
    entity: Entity,
    player: Entity,
    mut path: Vec<Name>,
    children: &Query<&Children>,
    names: &Query<&Name>,
    canonical_targets: &mut HashSet<AnimationTargetId>,
) {
    let Ok(name) = names.get(entity) else {
        return;
    };
    path.push(name.clone());
    let target = AnimationTargetId::from_names(path.iter());
    canonical_targets.insert(target);
    commands.entity(entity).insert((target, AnimatedBy(player)));
    if let Ok(entity_children) = children.get(entity) {
        for child in entity_children.iter() {
            bind_animation_target_paths(
                commands,
                child,
                player,
                path.clone(),
                children,
                names,
                canonical_targets,
            );
        }
    }
}

pub(super) fn identify_animation_players(
    mut commands: Commands,
    added: Query<Entity, Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    roots: Query<&AnimationRigScene>,
) {
    for player in &added {
        let mut current = player;
        for _ in 0..64 {
            if let Ok(root) = roots.get(current) {
                commands.entity(player).insert((
                    AnimationPlayerOwner(root.0),
                    AnimationGraphRevision::default(),
                ));
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}
