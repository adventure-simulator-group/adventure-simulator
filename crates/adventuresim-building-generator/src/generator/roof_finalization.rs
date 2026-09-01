fn refit_roof_edge_treatments(
    assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) {
    // Tower/child clipping can shorten a verge after its treatment was first
    // resolved. Refit the authoritative treatment to the final typed edge;
    // retaining the pre-cut bar would create a detached rod across the cut.
    let mut orphan_treatments = HashSet::new();
    for assembly in assemblies {
        for treatment in geometry.solids.iter_mut().filter(|solid| {
            solid.owner == assembly.owner && solid.role == SolidRole::RoofEdgeTreatment
        }) {
            let pitch_cosine = treatment.longfall_radians.cos();
            let axis = Vec3::new(
                treatment.yaw_radians.cos() * pitch_cosine,
                treatment.longfall_radians.sin(),
                treatment.yaw_radians.sin() * pitch_cosine,
            );
            let endpoints = [
                treatment.centre - axis * treatment.size.x * 0.5,
                treatment.centre + axis * treatment.size.x * 0.5,
            ];
            let aligned = assembly.edges.iter().any(|edge| {
                if !matches!(
                    edge.kind,
                    RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
                ) {
                    return false;
                }
                let delta = edge.end - edge.start;
                let length_squared = delta.length_squared().max(0.000_001);
                treatment.size.x <= delta.length() + 0.03
                    && endpoints.iter().all(|point| {
                        let raw_t = (*point - edge.start).dot(delta) / length_squared;
                        let t = raw_t.clamp(0.0, 1.0);
                        point.distance(edge.start + delta * t) <= 0.075
                            && (-0.02..=1.02).contains(&raw_t)
                    })
            });
            if !aligned {
                orphan_treatments.insert(treatment.id);
            }
        }
        for edge in &mut assembly.edges {
            if edge
                .flashing
                .is_some_and(|id| orphan_treatments.contains(&id))
            {
                edge.flashing = None;
            }
        }
    }
    geometry
        .solids
        .retain(|solid| !orphan_treatments.contains(&solid.id));
    geometry.support_interfaces.retain(|interface| {
        !orphan_treatments.iter().any(|id| {
            interface.id == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
        })
    });
    for treatment in geometry
        .solids
        .iter()
        .filter(|solid| solid.role == SolidRole::RoofEdgeTreatment)
    {
        let interface_id =
            ResolvedItemId((0x9_u64 << 60) | (treatment.id.0 & 0x0FFF_FFFF_FFFF_FFFF));
        if let Some(interface) = geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
        {
            interface.bounds = ResolvedBounds {
                min: treatment.centre - Vec3::new(0.08, 0.025, 0.08),
                max: treatment.centre + Vec3::new(0.08, 0.025, 0.08),
            };
        }
    }
}

fn bind_roof_junctions(assemblies: &[RoofAssembly], geometry: &mut ResolvedGeometry) {
    let roof_owners = assemblies
        .iter()
        .map(|roof| roof.owner)
        .collect::<HashSet<_>>();
    let mut roof_bonds = Vec::new();
    for left in 0..geometry.solids.len() {
        for right in left + 1..geometry.solids.len() {
            let a = &geometry.solids[left];
            let b = &geometry.solids[right];
            if a.owner == b.owner
                || (!roof_owners.contains(&a.owner) && !roof_owners.contains(&b.owner))
            {
                continue;
            }
            let a_bounds = yaw_bounds(a);
            let b_bounds = yaw_bounds(b);
            let min = a_bounds.min.max(b_bounds.min);
            let max = a_bounds.max.min(b_bounds.max);
            let overlap = max - min;
            if overlap.min_element() > 0.001 {
                roof_bonds.push(JunctionBond {
                    id: ResolvedItemId((0x6_u64 << 60) | roof_bonds.len() as u64),
                    owners: [a.owner, b.owner],
                    bounds: ResolvedBounds {
                        min: min - Vec3::splat(0.01),
                        max: max + Vec3::splat(0.01),
                    },
                    minimum_interface_area_square_metres: 0.005,
                    maximum_penetration_metres: overlap.x.min(overlap.z).min(0.18),
                });
            }
        }
    }
    geometry.junction_bonds.extend(roof_bonds);
}

fn yaw_bounds(solid: &ResolvedSolid) -> ResolvedBounds {
    let cosine = solid.yaw_radians.cos().abs();
    let sine = solid.yaw_radians.sin().abs();
    let half = Vec3::new(
        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
        solid.size.y * 0.5,
        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
    );
    ResolvedBounds {
        min: solid.centre - half,
        max: solid.centre + half,
    }
}
