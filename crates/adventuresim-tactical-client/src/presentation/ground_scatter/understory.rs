use adventuresim_tactical_core::prelude::{
    GroundCover, GroundSubstrate, GroundSurface, SceneGround, SceneTerrain,
};
use bevy::{
    camera::visibility::VisibilityRange,
    prelude::{Commands, Mesh3d, MeshMaterial3d, Name, Vec2},
};
use fabelgeist_determinism::splitmix64;

use crate::presentation::unit_hash;

use super::{
    GroundScatterLayer, TreeLeafRepresentation, WoodyUnderstoryPresentationCache, foliage_transform,
};

const SHRUB_WOOD_END: std::ops::Range<f32> = 10.0..12.0;
const SHRUB_CAMBERED_LEAF_END: std::ops::Range<f32> = 6.0..8.0;
const SHRUB_MINIMAL_LEAF_CARD_END: std::ops::Range<f32> = 20.0..24.0;

fn shrub_wood_visibility() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: SHRUB_WOOD_END,
        use_aabb: false,
    }
}

fn shrub_cambered_leaf_visibility() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: SHRUB_CAMBERED_LEAF_END,
        use_aabb: true,
    }
}

fn shrub_minimal_leaf_card_visibility() -> VisibilityRange {
    VisibilityRange {
        start_margin: SHRUB_CAMBERED_LEAF_END,
        end_margin: SHRUB_MINIMAL_LEAF_CARD_END,
        use_aabb: true,
    }
}

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
                shrub_wood_visibility(),
                transform,
            ));
            commands.spawn((
                Name::new(format!("Shared {common_name} cambered leaves")),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(presentation.cambered_leaves.as_ref().unwrap().clone()),
                MeshMaterial3d(presentation.leaves.as_ref().unwrap().clone()),
                shrub_cambered_leaf_visibility(),
                transform,
            ));
            commands.spawn((
                Name::new(format!("Shared {common_name} minimal alpha-card leaves")),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(presentation.minimal_leaf_cards.as_ref().unwrap().clone()),
                MeshMaterial3d(presentation.leaves.as_ref().unwrap().clone()),
                shrub_minimal_leaf_card_visibility(),
                transform,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::{
        BLACKTHORN_PARAMETERS, COMMON_HAWTHORN_PARAMETERS, COMMON_HAZEL_PARAMETERS,
        procedural_oak_leaf_card_mesh, procedural_woody_plant_leaves,
        procedural_woody_plant_skeleton, procedural_woody_sparse_leaf_card_mesh,
    };
    use bevy::prelude::Mesh;

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

    #[test]
    fn shrub_lod_ranges_pin_the_close_detail_and_minimal_card_budget() {
        let wood = shrub_wood_visibility();
        let cambered = shrub_cambered_leaf_visibility();
        let cards = shrub_minimal_leaf_card_visibility();

        assert_eq!(cambered.end_margin, 6.0..8.0);
        assert_eq!(cards.start_margin, 6.0..8.0);
        assert_eq!(cards.end_margin, 20.0..24.0);
        assert_eq!(wood.end_margin, 10.0..12.0);

        // The detailed and minimal representations share one fade band, so
        // the crown has no uncovered interval during the handoff.
        assert_eq!(cambered.end_margin, cards.start_margin);
        // Wood ends beneath the minimal cards, and no physical shrub tier
        // remains after the 24-metre terminal margin.
        assert!(wood.end_margin.end < cards.end_margin.start);
    }

    #[test]
    fn minimal_shrub_cards_pin_the_per_species_geometry_budget() {
        let mut budgets = Vec::new();
        for parameters in [
            COMMON_HAZEL_PARAMETERS,
            BLACKTHORN_PARAMETERS,
            COMMON_HAWTHORN_PARAMETERS,
        ] {
            let branches = procedural_woody_plant_skeleton(42, 0.0, parameters);
            let leaves = procedural_woody_plant_leaves(42, &branches, 0.0, parameters);
            let full = procedural_oak_leaf_card_mesh(&leaves);
            let sparse = procedural_woody_sparse_leaf_card_mesh(&leaves);
            budgets.push((
                full.count_vertices(),
                full.indices().unwrap().len() / 3,
                sparse.count_vertices(),
                sparse.indices().unwrap().len() / 3,
            ));
            assert!(sparse.count_vertices() * 3 <= full.count_vertices());
        }

        assert_eq!(
            budgets,
            vec![
                // full vertices, full triangles, sparse vertices, sparse triangles
                (19_800, 9_900, 4_960, 2_480),
                (21_780, 10_890, 5_400, 2_700),
                (11_880, 5_940, 2_944, 1_472),
            ]
        );
    }

    #[test]
    fn minimal_shrub_cards_are_deterministic_when_source_order_changes() {
        let branches = procedural_woody_plant_skeleton(42, 0.0, COMMON_HAZEL_PARAMETERS);
        let leaves = procedural_woody_plant_leaves(42, &branches, 0.0, COMMON_HAZEL_PARAMETERS);
        let first = procedural_woody_sparse_leaf_card_mesh(&leaves);
        let mut reordered = leaves;
        reordered.reverse();
        let repeated = procedural_woody_sparse_leaf_card_mesh(&reordered);

        assert_eq!(first.count_vertices(), repeated.count_vertices());
        assert_eq!(first.indices(), repeated.indices());
        assert_eq!(
            first.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            first.attribute(Mesh::ATTRIBUTE_COLOR),
            repeated.attribute(Mesh::ATTRIBUTE_COLOR)
        );
    }
}
