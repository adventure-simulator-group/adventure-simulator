use bevy::platform::hash::RandomState;
use bevy::prelude::*;
use noiz::prelude::*;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};

use crate::scene::SceneTerrain;

/// Small replicated component carrying the parameters needed to regenerate terrain
/// deterministically on any peer, avoiding replication of the full heightmap.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
pub struct TerrainSeed {
    pub seed: u32,
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub period: f32,
    pub grid_scale: f32,
}

#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    pub seed: u32,
    pub period: f32,
    pub grid_scale: f32,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            period: 30.0,
            grid_scale: 1.0,
        }
    }

    pub fn from_hash(hash: impl std::hash::Hash) -> Self {
        let mut hasher = RandomState::default().build_hasher();
        hash.hash(&mut hasher);
        Self::new(hasher.finish() as u32)
    }

    pub fn generate(&self, width: usize, height: usize, depth: usize) -> SceneTerrain {
        let mut noise = Noise::from(LayeredNoise::new(
            Normed::<f32>::default(),
            Persistence(0.5),
            FractalLayers {
                layer: Octave::<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients>>::default(),
                lacunarity: 2.0,
                amount: 8,
            },
        ));
        noise.set_seed(self.seed);
        noise.set_period(self.period);

        SceneTerrain::new(width, depth, self.grid_scale, move |loc| {
            let normal: f32 = noise.sample(loc);
            normal * height as f32
        })
    }

    /// Generate terrain and return both the terrain data and a seed component for replication.
    pub fn generate_with_seed(
        &self,
        width: usize,
        height: usize,
        depth: usize,
    ) -> (SceneTerrain, TerrainSeed) {
        let terrain = self.generate(width, height, depth);
        let seed = TerrainSeed {
            seed: self.seed,
            width,
            height,
            depth,
            period: self.period,
            grid_scale: self.grid_scale,
        };
        (terrain, seed)
    }
}

impl TerrainSeed {
    /// Regenerate terrain from this seed.
    pub fn generate(&self) -> SceneTerrain {
        let generator = TerrainGenerator {
            seed: self.seed,
            period: self.period,
            grid_scale: self.grid_scale,
        };
        generator.generate(self.width, self.height, self.depth)
    }
}
