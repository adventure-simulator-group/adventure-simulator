fn replace_storey_wall_sources_inside_round_towers(
    towers: &[RoundTower],
    walls: &mut [crate::WallAssembly],
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let round_hosts = walls
        .iter()
        .filter_map(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                Some((tower_index, wall.owner, wall.host_solids.first().copied()?))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut removed_owners = std::collections::HashSet::new();
    let mut replaced_wall_ids = std::collections::HashSet::new();
    for wall in walls.iter_mut() {
        if !matches!(wall.source, crate::WallSourceId::StoreyWall { .. }) {
            continue;
        }
        let Some((_, replacement_owner, replacement_host)) =
            round_hosts.iter().find(|(index, _, _)| {
                let tower = towers[*index];
                wall.frame.origin.distance(tower.centre_metres())
                    <= tower.radius_metres() + CELL_SIZE_METRES * 0.5
            })
        else {
            continue;
        };
        removed_owners.insert(wall.owner);
        replaced_wall_ids.insert(wall.id);
        wall.replaced_by_owner = Some(*replacement_owner);
        wall.host_solids = vec![*replacement_host];
        wall.opening_ids.clear();
    }
    let removed_opening_owners = openings
        .iter()
        .filter(|opening| replaced_wall_ids.contains(&opening.host_wall))
        .map(|opening| opening.owner)
        .collect::<std::collections::HashSet<_>>();
    openings.retain(|opening| !replaced_wall_ids.contains(&opening.host_wall));
    let removed = |owner: GeometryOwnerId| {
        removed_owners.contains(&owner) || removed_opening_owners.contains(&owner)
    };
    geometry.solids.retain(|solid| !removed(solid.owner));
    geometry.surfaces.retain(|surface| !removed(surface.owner));
    geometry.voids.retain(|void| !removed(void.owner));
    geometry
        .support_interfaces
        .retain(|interface| !removed(interface.owner));
    geometry
        .junction_bonds
        .retain(|bond| !bond.owners.iter().any(|owner| removed(*owner)));
}

fn resolve_gatehouse_tower_chord_bonds(
    towers: &[RoundTower],
    defenses: &[ProjectedDefenseAssembly],
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers.iter().copied().enumerate() {
        let Some(round_wall) = walls.iter().find(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::RoundTower { tower_index: index } if index == tower_index
            )
        }) else {
            continue;
        };
        for (interface_index, interface) in tower.chord_interfaces().enumerate() {
            let toward = direction_vector(interface.toward_gate);
            let perpendicular = Vec2::new(-toward.y, toward.x);
            let radius = tower.radius_metres();
            let chord_offset = radius - interface.bearing_depth.metres();
            let half_chord = (radius * radius - chord_offset * chord_offset)
                .max(0.0)
                .sqrt();
            let point = tower.centre_metres() + toward * chord_offset;
            let defense = defenses.iter().find(|defense| {
                let ProjectedDefensePath::Linear { start, end, .. } = defense.path else {
                    return false;
                };
                let delta = end - start;
                let progress =
                    ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                point.distance(start + delta * progress) <= 0.08
            });
            let Some(defense) = defense else {
                continue;
            };
            let horizontal = toward.abs() * 0.035 + perpendicular.abs() * half_chord;
            for (slot, target_owner) in [defense.host_owner, defense.owner].into_iter().enumerate()
            {
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId(
                        (7_u64 << 60)
                            | (u64::from(round_wall.owner.0) << 20)
                            | ((interface_index as u64) << 4)
                            | (slot as u64 + 0x800),
                    ),
                    owners: [round_wall.owner, target_owner],
                    bounds: ResolvedBounds {
                        min: Vec3::new(point.x - horizontal.x, 0.0, point.y - horizontal.y),
                        max: Vec3::new(
                            point.x + horizontal.x,
                            tower.wall_height_metres,
                            point.y + horizontal.y,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 0.08,
                });
            }
        }
    }
}

fn resolve_storey_wall_corner_bonds(
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    let solids = geometry
        .solids
        .iter()
        .map(|solid| {
            (
                solid.id,
                solid.owner,
                solid.centre,
                solid.size,
                solid.role,
                solid.yaw_radians,
            )
        })
        .collect::<Vec<_>>();
    let mut serial = 0_u64;
    for (left_index, left) in walls.iter().enumerate() {
        for right in &walls[(left_index + 1)..] {
            if left.storey_level != right.storey_level
                || left.owner == right.owner
                || left.frame.tangent.dot(right.frame.tangent).abs() > 0.01
            {
                continue;
            }
            let left_ids = left.replaced_by_owner.map_or_else(
                || left.host_solids.clone(),
                |owner| {
                    solids
                        .iter()
                        .filter_map(|solid| (solid.1 == owner).then_some(solid.0))
                        .collect()
                },
            );
            let right_ids = right.replaced_by_owner.map_or_else(
                || right.host_solids.clone(),
                |owner| {
                    solids
                        .iter()
                        .filter_map(|solid| (solid.1 == owner).then_some(solid.0))
                        .collect()
                },
            );
            for left_id in &left_ids {
                let Some((_, left_owner, left_centre, left_size, left_role, left_yaw)) =
                    solids.iter().find(|solid| solid.0 == *left_id)
                else {
                    continue;
                };
                for right_id in &right_ids {
                    let Some((_, right_owner, right_centre, right_size, right_role, right_yaw)) =
                        solids.iter().find(|solid| solid.0 == *right_id)
                    else {
                        continue;
                    };
                    if !matches!(
                        left_role,
                        SolidRole::WallHost
                            | SolidRole::DefenseHostWall
                            | SolidRole::CircuitWalk
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                    ) || !matches!(
                        right_role,
                        SolidRole::WallHost
                            | SolidRole::DefenseHostWall
                            | SolidRole::CircuitWalk
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                    ) {
                        continue;
                    }
                    let aabb_half = |size: Vec3, yaw: f32| {
                        let cosine = yaw.cos().abs();
                        let sine = yaw.sin().abs();
                        Vec3::new(
                            (size.x * cosine + size.z * sine) * 0.5,
                            size.y * 0.5,
                            (size.x * sine + size.z * cosine) * 0.5,
                        )
                    };
                    let left_half = aabb_half(*left_size, *left_yaw);
                    let right_half = aabb_half(*right_size, *right_yaw);
                    let overlap_min = (*left_centre - left_half).max(*right_centre - right_half);
                    let overlap_max = (*left_centre + left_half).min(*right_centre + right_half);
                    let overlap = overlap_max - overlap_min;
                    if overlap.min_element() <= 0.025 {
                        continue;
                    }
                    let mut extents = [overlap.x, overlap.y, overlap.z];
                    extents.sort_by(f32::total_cmp);
                    geometry.junction_bonds.push(JunctionBond {
                        id: ResolvedItemId((8_u64 << 60) | serial),
                        owners: [*left_owner, *right_owner],
                        bounds: ResolvedBounds {
                            min: overlap_min,
                            max: overlap_max,
                        },
                        minimum_interface_area_square_metres: extents[1] * extents[2] * 0.90,
                        maximum_penetration_metres: overlap.x.min(overlap.z) + 0.005,
                    });
                    serial += 1;
                }
            }
        }
    }
    let wall_owners = walls
        .iter()
        .flat_map(|wall| [wall.owner, wall.replaced_by_owner.unwrap_or(wall.owner)])
        .collect::<HashSet<_>>();
    for (left_index, left) in solids.iter().enumerate() {
        for right in &solids[(left_index + 1)..] {
            if left.1 == right.1
                || (!wall_owners.contains(&left.1) && !wall_owners.contains(&right.1))
                || !matches!(
                    left.4,
                    SolidRole::WallHost
                        | SolidRole::DefenseHostWall
                        | SolidRole::CircuitWalk
                        | SolidRole::LoadBearing
                        | SolidRole::Breastwork
                        | SolidRole::WalkSurface
                        | SolidRole::DrainageChannel
                        | SolidRole::Landing
                        | SolidRole::DefenseHostButtress
                        | SolidRole::ProjectionSupport
                        | SolidRole::GalleryFloor
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
                || !matches!(
                    right.4,
                    SolidRole::WallHost
                        | SolidRole::DefenseHostWall
                        | SolidRole::CircuitWalk
                        | SolidRole::LoadBearing
                        | SolidRole::Breastwork
                        | SolidRole::WalkSurface
                        | SolidRole::DrainageChannel
                        | SolidRole::Landing
                        | SolidRole::DefenseHostButtress
                        | SolidRole::ProjectionSupport
                        | SolidRole::GalleryFloor
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
            {
                continue;
            }
            let aabb_half = |size: Vec3, yaw: f32| {
                let cosine = yaw.cos().abs();
                let sine = yaw.sin().abs();
                Vec3::new(
                    (size.x * cosine + size.z * sine) * 0.5,
                    size.y * 0.5,
                    (size.x * sine + size.z * cosine) * 0.5,
                )
            };
            let left_half = aabb_half(left.3, left.5);
            let right_half = aabb_half(right.3, right.5);
            let overlap_min = (left.2 - left_half).max(right.2 - right_half);
            let overlap_max = (left.2 + left_half).min(right.2 + right_half);
            let overlap = overlap_max - overlap_min;
            if overlap.min_element() <= 0.025
                || geometry.junction_bonds.iter().any(|bond| {
                    bond.owners.contains(&left.1)
                        && bond.owners.contains(&right.1)
                        && overlap_min
                            .cmpge(bond.bounds.min - Vec3::splat(0.025))
                            .all()
                        && overlap_max
                            .cmple(bond.bounds.max + Vec3::splat(0.025))
                            .all()
                })
            {
                continue;
            }
            let mut extents = [overlap.x, overlap.y, overlap.z];
            extents.sort_by(f32::total_cmp);
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId((8_u64 << 60) | serial),
                owners: [left.1, right.1],
                bounds: ResolvedBounds {
                    min: overlap_min,
                    max: overlap_max,
                },
                minimum_interface_area_square_metres: extents[1] * extents[2] * 0.90,
                maximum_penetration_metres: overlap.x.min(overlap.z) + 0.005,
            });
            serial += 1;
        }
    }
}
