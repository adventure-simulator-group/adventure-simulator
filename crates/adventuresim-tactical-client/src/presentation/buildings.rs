use adventuresim_building_generator::{
    BuildingLodLevel, BuildingLodMaterial, BuildingProgram, LodMesh, compile_building_collision,
    compile_building_detail, compile_building_lod, compile_static_building_detail, generate,
};
use bevy::ecs::hierarchy::ChildSpawnerCommands;

use super::*;

mod materials;
pub(crate) use materials::TacticalBuildingMaterials;
pub(in crate::presentation) use materials::setup_tactical_building_materials;

const DETAIL_LOD_END_START_METRES: f32 = 55.0;
const DETAIL_LOD_END_END_METRES: f32 = 70.0;
const FACADE_LOD_END_START_METRES: f32 = 150.0;
const FACADE_LOD_END_END_METRES: f32 = 175.0;

#[derive(Component)]
pub(crate) struct DistantCityBuildingPresentation;

#[derive(Component)]
pub(crate) struct PresentedBuildingMesh {
    pub(crate) scope: BuildingPresentationScope,
    pub(crate) level: BuildingRenderLevel,
    pub(crate) material: BuildingLodMaterial,
    pub(crate) triangles: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildingRenderLevel {
    Lod0,
    Lod1,
    Lod2,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildingPresentationScope {
    Playable,
    DistantCity,
}

#[derive(Clone)]
struct CompiledBuildingBatch {
    material: BuildingLodMaterial,
    mesh: Handle<Mesh>,
    triangles: usize,
}

#[derive(Clone)]
struct CompiledBuildingLevels {
    program: BuildingProgram,
    dynamic_openings: bool,
    floor_offset_metres: f32,
    lod0: Vec<CompiledBuildingBatch>,
    lod1: Vec<CompiledBuildingBatch>,
    lod2: Vec<CompiledBuildingBatch>,
}

#[derive(Default, Resource)]
pub(in crate::presentation) struct TacticalBuildingMeshCache(Vec<CompiledBuildingLevels>);

pub(in crate::presentation) fn on_scene_building_added(
    event: On<Add, SceneBuilding>,
    mut commands: Commands,
    buildings: Query<&SceneBuilding>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
    mut cache: ResMut<TacticalBuildingMeshCache>,
) -> Result {
    let building = buildings.get(event.entity)?;
    let compiled = cached_building_levels(&mut cache, &building.program, true, &mut meshes)?;
    commands
        .entity(event.entity)
        .insert(Visibility::default())
        .with_children(|parent| {
            spawn_building_levels(
                parent,
                building.id,
                &compiled,
                BuildingPresentationScope::Playable,
                &materials,
            );
        });
    Ok(())
}

pub(in crate::presentation) fn on_scene_vista_buildings(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    existing: Query<Entity, With<DistantCityBuildingPresentation>>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
    mut cache: ResMut<TacticalBuildingMeshCache>,
) -> Result {
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for placement in &bundle.distant_buildings {
        let compiled =
            cached_building_levels(&mut cache, &placement.program(), false, &mut meshes)?;
        commands
            .spawn((
                Name::new(format!("Distant city building {}", placement.id)),
                DistantCityBuildingPresentation,
                Visibility::default(),
                Transform::from_xyz(
                    placement.centre_metres.x,
                    placement.base_elevation_metres + compiled.floor_offset_metres,
                    placement.centre_metres.y,
                )
                .with_rotation(Quat::from_rotation_y(placement.orientation.yaw_radians())),
            ))
            .with_children(|parent| {
                spawn_building_levels(
                    parent,
                    placement.id,
                    &compiled,
                    BuildingPresentationScope::DistantCity,
                    &materials,
                );
            });
    }
    Ok(())
}

fn cached_building_levels(
    cache: &mut TacticalBuildingMeshCache,
    program: &BuildingProgram,
    dynamic_openings: bool,
    meshes: &mut Assets<Mesh>,
) -> Result<CompiledBuildingLevels> {
    if let Some(compiled) = cache.0.iter().find(|compiled| {
        compiled.program == *program && compiled.dynamic_openings == dynamic_openings
    }) {
        return Ok(compiled.clone());
    }

    let plan = generate(program)?;
    let collision = compile_building_collision(&plan);
    let local_origin = collision.bounds.centre();
    let floor_offset_metres = local_origin.y - collision.bounds.min.y;
    let detail = if dynamic_openings {
        compile_static_building_detail(&plan)
    } else {
        compile_building_detail(&plan)
    };
    let facade = compile_building_lod(&plan, BuildingLodLevel::Facade);
    let shell = compile_building_lod(&plan, BuildingLodLevel::Shell);
    let compile_batches = |source: &[LodMesh], meshes: &mut Assets<Mesh>| {
        source
            .iter()
            .map(|batch| CompiledBuildingBatch {
                material: batch.material,
                mesh: meshes.add(building_mesh(batch, local_origin)),
                triangles: batch.indices.len() / 3,
            })
            .collect()
    };
    let compiled = CompiledBuildingLevels {
        program: program.clone(),
        dynamic_openings,
        floor_offset_metres,
        lod0: compile_batches(&detail.meshes, meshes),
        lod1: compile_batches(&facade.meshes, meshes),
        lod2: compile_batches(&shell.meshes, meshes),
    };
    cache.0.push(compiled.clone());
    Ok(compiled)
}

fn spawn_building_levels(
    parent: &mut ChildSpawnerCommands,
    building_id: u64,
    compiled: &CompiledBuildingLevels,
    scope: BuildingPresentationScope,
    materials: &TacticalBuildingMaterials,
) {
    for (level, batches) in [
        (BuildingRenderLevel::Lod0, &compiled.lod0),
        (BuildingRenderLevel::Lod1, &compiled.lod1),
        (BuildingRenderLevel::Lod2, &compiled.lod2),
    ] {
        for batch in batches {
            parent.spawn((
                Name::new(format!("Building {:?} {:?}", level, batch.material)),
                PresentedBuildingMesh {
                    scope,
                    level,
                    material: batch.material,
                    triangles: batch.triangles,
                },
                Mesh3d(batch.mesh.clone()),
                MeshMaterial3d(materials.get_for_building(building_id, batch.material)),
                building_lod_visibility(level),
            ));
        }
    }
}

pub(super) fn building_lod_visibility(level: BuildingRenderLevel) -> VisibilityRange {
    match level {
        BuildingRenderLevel::Lod0 => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: DETAIL_LOD_END_START_METRES..DETAIL_LOD_END_END_METRES,
            use_aabb: false,
        },
        BuildingRenderLevel::Lod1 => VisibilityRange {
            start_margin: DETAIL_LOD_END_START_METRES..DETAIL_LOD_END_END_METRES,
            end_margin: FACADE_LOD_END_START_METRES..FACADE_LOD_END_END_METRES,
            use_aabb: false,
        },
        BuildingRenderLevel::Lod2 => VisibilityRange {
            start_margin: FACADE_LOD_END_START_METRES..FACADE_LOD_END_END_METRES,
            // The generated settlement is already finite. Keeping its cheapest
            // shell visible avoids cutting the far half of a city from elevated
            // or exterior viewpoints.
            end_margin: f32::MAX..f32::MAX,
            use_aabb: false,
        },
    }
}

fn building_mesh(batch: &LodMesh, local_origin: Vec3) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        batch
            .vertices
            .iter()
            .map(|vertex| (vertex.position - local_origin).to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        batch
            .vertices
            .iter()
            .map(|vertex| vertex.normal.to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        batch
            .vertices
            .iter()
            .map(|vertex| vertex.uv.to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(batch.indices.clone()));
    if matches!(
        batch.material,
        BuildingLodMaterial::Wall(_)
            | BuildingLodMaterial::CrownMasonry
            | BuildingLodMaterial::Roof(_)
            | BuildingLodMaterial::Timber
            | BuildingLodMaterial::InteriorTimber
            | BuildingLodMaterial::Iron
            | BuildingLodMaterial::InteriorPlaster
            | BuildingLodMaterial::Floor
            | BuildingLodMaterial::Glass
    ) {
        mesh.generate_tangents()
            .expect("interior building UVs must support tangent-space normal maps");
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_lod_has_no_artificial_distance_cutoff() {
        let visibility = building_lod_visibility(BuildingRenderLevel::Lod2);
        assert_eq!(visibility.end_margin, f32::MAX..f32::MAX);
    }
}
