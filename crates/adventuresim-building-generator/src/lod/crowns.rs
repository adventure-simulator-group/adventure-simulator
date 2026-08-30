use bevy::math::{Vec2, Vec3};

use super::{
    BuildingLod, BuildingLodLevel, BuildingLodMaterial, FacadeRun, FacadeRunPath,
    ROUND_LOD_SEGMENTS, append_facade_prism, direction_vector, plan_vertex,
};
use crate::{BuildingPlan, CrownAssembly, CrownPath, WallMaterialClass};

pub(super) fn append_crowns(lod: &mut BuildingLod, plan: &BuildingPlan) {
    for crown in &plan.crowns {
        match (lod.level, crown.path) {
            (
                BuildingLodLevel::Facade,
                CrownPath::Straight {
                    start,
                    end,
                    outward,
                },
            ) => append_geometric_crown(lod, crown, start, end, direction_vector(outward)),
            (
                _,
                CrownPath::Straight {
                    start,
                    end,
                    outward,
                },
            ) => append_masked_crown_strip(lod, crown, start, end, direction_vector(outward)),
            (
                _,
                CrownPath::Round {
                    centre,
                    radius_metres,
                    ..
                },
            ) => append_round_crown_mask(lod, crown, centre, radius_metres),
        }
    }
}

fn append_geometric_crown(
    lod: &mut BuildingLod,
    crown: &CrownAssembly,
    start: Vec2,
    end: Vec2,
    outward: Vec2,
) {
    let run = FacadeRun {
        material: WallMaterialClass::FortifiedMasonry,
        storey_level: 0,
        path: FacadeRunPath::Straight {
            start,
            end,
            outward,
        },
        base_elevation_metres: crown.base_height_metres,
        height_metres: crown.profile.breastwork_height_metres,
        thickness_metres: crown.profile.thickness_metres,
        source_walls: Vec::new(),
    };
    append_facade_prism(lod.mesh_mut(BuildingLodMaterial::CrownMasonry), &run);
    let length = start.distance(end);
    let tangent = (end - start).normalize_or_zero();
    let pitch = crown.profile.merlon_width_metres + crown.profile.crenel_width_metres;
    let count = (length / pitch).floor().max(1.0) as usize;
    let interval = length / count as f32;
    for index in 0..count {
        let centre = start + tangent * interval * (index as f32 + 0.5);
        let half_width = crown.profile.merlon_width_metres.min(interval) * 0.5;
        let merlon = FacadeRun {
            path: FacadeRunPath::Straight {
                start: centre - tangent * half_width,
                end: centre + tangent * half_width,
                outward,
            },
            base_elevation_metres: crown.base_height_metres
                + crown.profile.breastwork_height_metres,
            height_metres: crown.profile.merlon_height_metres,
            ..run.clone()
        };
        append_facade_prism(lod.mesh_mut(BuildingLodMaterial::CrownMasonry), &merlon);
    }
}

fn append_masked_crown_strip(
    lod: &mut BuildingLod,
    crown: &CrownAssembly,
    start: Vec2,
    end: Vec2,
    outward: Vec2,
) {
    let height = crown.profile.breastwork_height_metres + crown.profile.merlon_height_metres;
    let offset = outward * crown.profile.thickness_metres * 0.5;
    let bottom = crown.base_height_metres;
    let top = bottom + height;
    let repeats = start.distance(end)
        / (crown.profile.merlon_width_metres + crown.profile.crenel_width_metres);
    lod.mesh_mut(BuildingLodMaterial::CrownMask).push_quad(
        [
            plan_vertex(start + offset, bottom),
            plan_vertex(end + offset, bottom),
            plan_vertex(end + offset, top),
            plan_vertex(start + offset, top),
        ],
        Vec3::new(outward.x, 0.0, outward.y),
        [
            Vec2::new(0.0, 0.0),
            Vec2::new(repeats, 0.0),
            Vec2::new(repeats, 1.0),
            Vec2::new(0.0, 1.0),
        ],
    );
}

fn append_round_crown_mask(
    lod: &mut BuildingLod,
    crown: &CrownAssembly,
    centre: Vec2,
    radius: f32,
) {
    let height = crown.profile.breastwork_height_metres + crown.profile.merlon_height_metres;
    let pitch = crown.profile.merlon_width_metres + crown.profile.crenel_width_metres;
    for segment in 0..ROUND_LOD_SEGMENTS {
        let a = std::f32::consts::TAU * segment as f32 / ROUND_LOD_SEGMENTS as f32;
        let b = std::f32::consts::TAU * (segment + 1) as f32 / ROUND_LOD_SEGMENTS as f32;
        let radial_a = Vec2::from_angle(a);
        let radial_b = Vec2::from_angle(b);
        let bottom = crown.base_height_metres;
        let top = bottom + height;
        let normal = (radial_a + radial_b).normalize_or_zero();
        lod.mesh_mut(BuildingLodMaterial::CrownMask).push_quad(
            [
                plan_vertex(centre + radial_a * radius, bottom),
                plan_vertex(centre + radial_b * radius, bottom),
                plan_vertex(centre + radial_b * radius, top),
                plan_vertex(centre + radial_a * radius, top),
            ],
            Vec3::new(normal.x, 0.0, normal.y),
            [
                Vec2::new(radius * a / pitch, 0.0),
                Vec2::new(radius * b / pitch, 0.0),
                Vec2::new(radius * b / pitch, 1.0),
                Vec2::new(radius * a / pitch, 1.0),
            ],
        );
    }
}
