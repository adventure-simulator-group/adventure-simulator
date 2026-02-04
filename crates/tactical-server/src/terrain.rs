use adventure_simulator_core::prelude::*;
use bevy::platform::hash::RandomState;
use noiz::prelude::*;
use std::hash::{BuildHasher, Hasher};

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

    pub fn generate(self, width: usize, height: usize, depth: usize) -> SceneTerrain {
        let mut noise = Noise::from(LayeredNoise::new(
            NormedByDerivative::<f32, EuclideanLength, PeakDerivativeContribution>::default()
                .with_falloff(0.3),
            Persistence(0.6),
            FractalLayers {
                layer:
                    Octave::<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients, true>>::default(
                    ),
                lacunarity: 2.1,
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
}
