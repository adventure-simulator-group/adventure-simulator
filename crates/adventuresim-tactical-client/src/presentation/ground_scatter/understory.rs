use adventuresim_tactical_core::prelude::{
    GroundCover, GroundSubstrate, GroundSurface, SceneGround, SceneTerrain,
};
use bevy::{
    camera::visibility::VisibilityRange,
    prelude::{Commands, Mesh3d, MeshMaterial3d, Name, Vec2},
};

use crate::presentation::{splitmix64, unit_hash};

use super::{
    GroundScatterLayer, TreeLeafRepresentation, WoodyUnderstoryPresentationCache, foliage_transform,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UnderstorySpecies {
    CommonHazel,
    Blackthorn,
    CommonHawthorn,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct UnderstoryHabitat {
    pub(super) canopy: f32,
    pub(super) wetland: f32,
    pub(super) cultivation: f32,
    pub(super) moisture: f32,
}

pub(super) fn select_species(hash: u64, habitat: UnderstoryHabitat) -> UnderstorySpecies {
    // This temporary habitat model intentionally uses only scene data already
    // available to the client. Hazel favors shaded, mesic woodland; blackthorn
    // favors brighter, drier scrub; hawthorn gains weight at open/cultivated
    // edges. A later world-data source can replace these weights without
    // changing species geometry or scatter identity.
    let dry = 1.0 - habitat.moisture;
    let open = 1.0 - habitat.canopy;
    let hazel = 0.25 + habitat.canopy * 1.55 + habitat.moisture * 0.35;
    let blackthorn = (0.2 + open * 0.82 + dry * 0.34 + habitat.cultivation * 0.22)
        * (1.0 - habitat.wetland * 0.55);
    let hawthorn =
        (0.24 + open * 0.48 + habitat.cultivation * 0.88) * (1.0 - habitat.wetland * 0.38);
    let total = hazel + blackthorn + hawthorn;
    let roll = unit_hash(splitmix64(hash ^ 0x5a8d_311c_42e7)) * total;
    if roll < hazel {
        UnderstorySpecies::CommonHazel
    } else if roll < hazel + blackthorn {
        UnderstorySpecies::Blackthorn
    } else {
        UnderstorySpecies::CommonHawthorn
    }
}

fn community_hash(base_seed: u64, x: i32, z: i32) -> u64 {
    // Four-by-four lattice communities produce roughly 13-metre thickets.
    // Species identity varies between communities, while each specimen keeps
    // its independent placement/rotation hash within the selected thicket.
    let community_x = x.div_euclid(4);
    let community_z = z.div_euclid(4);
    let cell = ((community_x as u32 as u64) << 32) | community_z as u32 as u64;
    splitmix64(base_seed ^ cell ^ 0xc011_00d5_7a1d)
}

fn community_density_multiplier(hash: u64) -> f32 {
    // Concentrate the same approximate population into legible thickets.
    // Dense cores, loose margins, and mostly open cells keep shrubs from
    // reading as evenly-spaced miniature trees across the whole landscape.
    let structure = unit_hash(splitmix64(hash ^ 0x7a11_c1ed_5eed));
    if structure < 0.25 {
        2.5
    } else if structure < 0.60 {
        1.0
    } else {
        0.15
    }
}

fn ground_allows_species(surface: GroundSurface, species: UnderstorySpecies) -> bool {
    if surface.substrate == GroundSubstrate::Water || surface.cover == GroundCover::Reeds {
        return false;
    }
    match surface.cover {
        // Hazel is a normal woodland-floor shrub, and hawthorn can enter
        // brighter gaps. Blackthorn remains biased to open scrub and edges.
        GroundCover::LeafLitter => species != UnderstorySpecies::Blackthorn,
        GroundCover::LooseStone => false,
        GroundCover::Bare | GroundCover::TallGrass => true,
        GroundCover::Reeds => false,
    }
}

/// One shrub site produced by the shared placement walk, consumed by both
/// the legacy per-entity renderer and the instanced batches.
pub(super) struct ShrubPlacement {
    pub(super) species: UnderstorySpecies,
    pub(super) world_x: f32,
    pub(super) world_z: f32,
    pub(super) hash: u64,
}

/// Deterministic shrub placement walk shared by both renderers, so the
/// instanced path reproduces the legacy thickets exactly.
pub(super) fn placements(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
    chance: f32,
    habitat: UnderstoryHabitat,
) -> Vec<ShrubPlacement> {
    let spacing = 3.2;
    let count_x = (terrain.width() / spacing).floor() as i32;
    let count_z = (terrain.depth() / spacing).floor() as i32;
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    let mut sites = Vec::new();
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell ^ 0xa04f_63d2_719b_e850);
            let community = community_hash(base_seed, x, z);
            let local_chance = (chance * community_density_multiplier(community)).min(0.82);
            if unit_hash(hash) >= local_chance {
                continue;
            }
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.72) * spacing;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.72) * spacing;
            let species = select_species(community, habitat);
            if ground
                .ground_at(Vec2::new(world_x, world_z))
                .is_none_or(|sample| !ground_allows_species(sample, species))
            {
                continue;
            }
            sites.push(ShrubPlacement {
                species,
                world_x,
                world_z,
                hash,
            });
        }
    }
    sites
}

#[cfg_attr(
    all(feature = "instanced-grass", not(target_family = "wasm")),
    allow(dead_code, reason = "legacy shrub renderer remains the wasm path")
)]
pub(super) fn spawn(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cache: &WoodyUnderstoryPresentationCache,
    base_seed: u64,
    chance: f32,
    habitat: UnderstoryHabitat,
) {
    for placement in placements(terrain, ground, base_seed, chance, habitat) {
        {
            let ShrubPlacement {
                species,
                world_x,
                world_z,
                hash,
            } = placement;
            let Some(transform) = foliage_transform(terrain, world_x, world_z, hash) else {
                continue;
            };
            let presentation = cache.presentation(species);
            let common_name = match species {
                UnderstorySpecies::CommonHazel => "common hazel",
                UnderstorySpecies::Blackthorn => "blackthorn",
                UnderstorySpecies::CommonHawthorn => "common hawthorn",
            };
            commands.spawn((
                Name::new(format!("Shared {common_name} shrub wood")),
                GroundScatterLayer::Understory,
                Mesh3d(presentation.branches.as_ref().unwrap().clone()),
                MeshMaterial3d(presentation.bark.as_ref().unwrap().clone()),
                // Past ~50 m a shrub reads as its alpha-card foliage mass;
                // the full woody skeleton previously rendered to 92 m and
                // dominated the understory's vertex cost (instance count in
                // a distance band grows with the radius squared).
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: 44.0..52.0,
                    use_aabb: true,
                },
                transform,
            ));
            commands.spawn((
                Name::new(format!("Shared {common_name} cambered leaves")),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(presentation.cambered_leaves.as_ref().unwrap().clone()),
                MeshMaterial3d(presentation.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: 26.0..34.0,
                    use_aabb: true,
                },
                transform,
            ));
            commands.spawn((
                Name::new(format!("Shared {common_name} alpha-card leaves")),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(presentation.leaf_cards.as_ref().unwrap().clone()),
                MeshMaterial3d(presentation.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 26.0..34.0,
                    end_margin: 84.0..96.0,
                    use_aabb: true,
                },
                transform,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn understory_species_form_coherent_four_by_four_communities() {
        let seed = 42;
        let habitat = UnderstoryHabitat {
            canopy: 0.45,
            wetland: 0.08,
            cultivation: 0.12,
            moisture: 0.55,
        };
        let expected = select_species(community_hash(seed, 0, 0), habitat);
        for z in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    select_species(community_hash(seed, x, z), habitat),
                    expected
                );
            }
        }
        assert_ne!(community_hash(seed, 0, 0), community_hash(seed, 4, 0));
    }

    #[test]
    fn shrub_communities_include_dense_thickets_and_open_relief() {
        let mut dense = 0;
        let mut open = 0;
        let mut mean = 0.0;
        for cell in 0..4_096_u64 {
            let multiplier = community_density_multiplier(splitmix64(cell));
            dense += usize::from(multiplier > 2.0);
            open += usize::from(multiplier < 0.2);
            mean += multiplier;
        }
        mean /= 4_096.0;
        assert!(dense > 800);
        assert!(open > 1_300);
        assert!((mean - 1.0).abs() < 0.08);
    }

    #[test]
    fn habitat_weights_shift_species_composition_without_excluding_any_preset() {
        let count = |habitat, species| {
            (0..4_096_u64)
                .filter(|seed| select_species(splitmix64(*seed), habitat) == species)
                .count()
        };
        let shaded = UnderstoryHabitat {
            canopy: 0.9,
            wetland: 0.05,
            cultivation: 0.0,
            moisture: 0.7,
        };
        let open_edge = UnderstoryHabitat {
            canopy: 0.1,
            wetland: 0.02,
            cultivation: 0.65,
            moisture: 0.35,
        };
        assert!(
            count(shaded, UnderstorySpecies::CommonHazel)
                > count(open_edge, UnderstorySpecies::CommonHazel)
        );
        assert!(
            count(open_edge, UnderstorySpecies::CommonHawthorn)
                > count(shaded, UnderstorySpecies::CommonHawthorn)
        );
        for species in [
            UnderstorySpecies::CommonHazel,
            UnderstorySpecies::Blackthorn,
            UnderstorySpecies::CommonHawthorn,
        ] {
            assert!(count(open_edge, species) > 0);
        }
    }

    #[test]
    fn woodland_leaf_litter_accepts_hazel_and_gap_hawthorn_but_not_blackthorn() {
        let litter = GroundSurface {
            substrate: GroundSubstrate::Soil,
            cover: GroundCover::LeafLitter,
            ..GroundSurface::default()
        };
        assert!(ground_allows_species(
            litter,
            UnderstorySpecies::CommonHazel
        ));
        assert!(ground_allows_species(
            litter,
            UnderstorySpecies::CommonHawthorn
        ));
        assert!(!ground_allows_species(
            litter,
            UnderstorySpecies::Blackthorn
        ));
        assert!(!ground_allows_species(
            GroundSurface {
                substrate: GroundSubstrate::Water,
                ..GroundSurface::default()
            },
            UnderstorySpecies::CommonHazel
        ));
    }
}
