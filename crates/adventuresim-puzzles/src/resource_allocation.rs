use fabelgeist_determinism::SplitMix64;
use serde::{Deserialize, Serialize};

use crate::shuffle;

const RESOURCE_ALLOCATION_GENERATION_DOMAIN: u64 = 0x7265_736f_7572_6365;

pub const RESOURCE_ALLOCATION_RULES_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JourneyHazard {
    Darkness,
    Chasm,
    Thirst,
    Wounds,
    Glamour,
    Cold,
}

impl JourneyHazard {
    pub const ALL: [Self; 6] = [
        Self::Darkness,
        Self::Chasm,
        Self::Thirst,
        Self::Wounds,
        Self::Glamour,
        Self::Cold,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Darkness => "lightless ground",
            Self::Chasm => "a broken crossing",
            Self::Thirst => "a waterless march",
            Self::Wounds => "untended wounds",
            Self::Glamour => "deceiving glamour",
            Self::Cold => "bitter cold",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProvisionId {
    Torch,
    Rope,
    WaterSkin,
    Salve,
    IronBell,
    WoolCloak,
    ClimbingStaff,
}

impl ProvisionId {
    pub const ALL: [Self; 7] = [
        Self::Torch,
        Self::Rope,
        Self::WaterSkin,
        Self::Salve,
        Self::IronBell,
        Self::WoolCloak,
        Self::ClimbingStaff,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Torch => "Pitch torch",
            Self::Rope => "Coiled rope",
            Self::WaterSkin => "Full waterskin",
            Self::Salve => "Surgeon's salve",
            Self::IronBell => "Cold-iron bell",
            Self::WoolCloak => "Wool cloak",
            Self::ClimbingStaff => "Ash climbing staff",
        }
    }

    pub const fn weight(self) -> u8 {
        match self {
            Self::Torch | Self::IronBell => 1,
            Self::Salve | Self::ClimbingStaff => 2,
            Self::Rope | Self::WaterSkin | Self::WoolCloak => 3,
        }
    }

    pub const fn readiness(self) -> u8 {
        match self {
            Self::Torch => 3,
            Self::Rope => 5,
            Self::WaterSkin => 4,
            Self::Salve => 5,
            Self::IronBell => 4,
            Self::WoolCloak => 4,
            Self::ClimbingStaff => 3,
        }
    }

    pub const fn protections(self) -> &'static [JourneyHazard] {
        match self {
            Self::Torch => &[JourneyHazard::Darkness, JourneyHazard::Cold],
            Self::Rope => &[JourneyHazard::Chasm],
            Self::WaterSkin => &[JourneyHazard::Thirst],
            Self::Salve => &[JourneyHazard::Wounds],
            Self::IronBell => &[JourneyHazard::Darkness, JourneyHazard::Glamour],
            Self::WoolCloak => &[JourneyHazard::Cold, JourneyHazard::Wounds],
            Self::ClimbingStaff => &[JourneyHazard::Chasm, JourneyHazard::Glamour],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAllocationSpec {
    pub provision_count: u8,
    pub hazard_count: u8,
    pub capacity: u8,
    pub require_irredundant_hazards: bool,
    pub require_capacity_to_matter: bool,
}

impl Default for ResourceAllocationSpec {
    fn default() -> Self {
        Self {
            provision_count: 6,
            hazard_count: 3,
            capacity: 7,
            require_irredundant_hazards: true,
            require_capacity_to_matter: true,
        }
    }
}

impl ResourceAllocationSpec {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !(4..=ProvisionId::ALL.len() as u8).contains(&self.provision_count) {
            return Err("resource puzzle must offer between four and seven provisions");
        }
        if !(2..=4).contains(&self.hazard_count) {
            return Err("resource puzzle must present between two and four hazards");
        }
        if !(3..=14).contains(&self.capacity) {
            return Err("resource-puzzle capacity must be between three and fourteen");
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvisionProfile {
    pub id: ProvisionId,
    pub weight: u8,
    pub readiness: u8,
    pub protections: Vec<JourneyHazard>,
}

impl From<ProvisionId> for ProvisionProfile {
    fn from(id: ProvisionId) -> Self {
        Self {
            id,
            weight: id.weight(),
            readiness: id.readiness(),
            protections: id.protections().to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAllocationPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub spec: ResourceAllocationSpec,
    pub provisions: Vec<ProvisionId>,
    pub hazards: Vec<JourneyHazard>,
    solution: Vec<ProvisionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAllocationProjection {
    pub rules_version: u16,
    pub capacity: u8,
    pub provisions: Vec<ProvisionProfile>,
    pub hazards: Vec<JourneyHazard>,
}

impl ResourceAllocationPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_with_spec(seed, ResourceAllocationSpec::default())
            .expect("standard resource-allocation specification is valid")
    }

    pub fn generate_with_spec(
        seed: u64,
        spec: ResourceAllocationSpec,
    ) -> Result<Self, &'static str> {
        let spec = spec.validate()?;
        let mut rng = SplitMix64::new(seed ^ RESOURCE_ALLOCATION_GENERATION_DOMAIN);
        for _ in 0..512 {
            let mut provisions = ProvisionId::ALL.to_vec();
            let mut hazards = JourneyHazard::ALL.to_vec();
            shuffle(&mut provisions, &mut rng);
            shuffle(&mut hazards, &mut rng);
            provisions.truncate(usize::from(spec.provision_count));
            hazards.truncate(usize::from(spec.hazard_count));
            provisions.sort_unstable();
            hazards.sort_unstable();

            let optimal = allocation_optimal_packs(&provisions, &hazards, spec.capacity);
            if optimal.len() != 1 {
                continue;
            }
            if spec.require_irredundant_hazards
                && (0..hazards.len()).any(|removed| {
                    let reduced = hazards
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, hazard)| (index != removed).then_some(hazard))
                        .collect::<Vec<_>>();
                    allocation_optimal_packs(&provisions, &reduced, spec.capacity) == optimal
                })
            {
                continue;
            }
            if spec.require_capacity_to_matter
                && allocation_optimal_packs(&provisions, &hazards, u8::MAX) == optimal
            {
                continue;
            }
            let puzzle = Self {
                rules_version: RESOURCE_ALLOCATION_RULES_VERSION,
                seed,
                spec,
                provisions,
                hazards,
                solution: optimal.into_iter().next().unwrap(),
            };
            puzzle.validate()?;
            return Ok(puzzle);
        }
        Err("could not generate a unique resource allocation for this specification")
    }

    pub fn projection(&self) -> ResourceAllocationProjection {
        ResourceAllocationProjection {
            rules_version: self.rules_version,
            capacity: self.spec.capacity,
            provisions: self.provisions.iter().copied().map(Into::into).collect(),
            hazards: self.hazards.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.spec.validate()?;
        if self.rules_version != RESOURCE_ALLOCATION_RULES_VERSION {
            return Err("unsupported resource-allocation rules version");
        }
        if self.provisions.len() != usize::from(self.spec.provision_count)
            || self.hazards.len() != usize::from(self.spec.hazard_count)
            || !strictly_sorted_unique(&self.provisions)
            || !strictly_sorted_unique(&self.hazards)
        {
            return Err("resource-allocation dimensions violate their generation spec");
        }
        let optimal = allocation_optimal_packs(&self.provisions, &self.hazards, self.spec.capacity);
        if optimal != vec![self.solution.clone()] {
            return Err("resource-allocation facts do not prove the private optimum");
        }
        if self.spec.require_irredundant_hazards
            && (0..self.hazards.len()).any(|removed| {
                let reduced = self
                    .hazards
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, hazard)| (index != removed).then_some(hazard))
                    .collect::<Vec<_>>();
                allocation_optimal_packs(&self.provisions, &reduced, self.spec.capacity) == optimal
            })
        {
            return Err("resource-allocation puzzle contains a redundant hazard");
        }
        if self.spec.require_capacity_to_matter
            && allocation_optimal_packs(&self.provisions, &self.hazards, u8::MAX) == optimal
        {
            return Err("resource-allocation capacity does not affect the optimum");
        }
        Ok(())
    }

    pub fn check(&self, provisions: &[ProvisionId]) -> Result<bool, &'static str> {
        let mut answer = provisions.to_vec();
        answer.sort_unstable();
        if !strictly_sorted_unique(&answer)
            || answer.iter().any(|item| !self.provisions.contains(item))
        {
            return Err("resource-allocation answer contains invalid provisions");
        }
        Ok(answer == self.solution)
    }
}

pub fn allocation_legal_packs(
    provisions: &[ProvisionId],
    hazards: &[JourneyHazard],
    capacity: u8,
) -> Vec<Vec<ProvisionId>> {
    (0_u16..(1_u16 << provisions.len()))
        .filter_map(|mask| {
            let pack = provisions
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, item)| (mask & (1 << index) != 0).then_some(item))
                .collect::<Vec<_>>();
            let weight = pack
                .iter()
                .map(|item| u16::from(item.weight()))
                .sum::<u16>();
            (weight <= u16::from(capacity)
                && hazards
                    .iter()
                    .all(|hazard| pack.iter().any(|item| item.protections().contains(hazard))))
            .then_some(pack)
        })
        .collect()
}

pub fn allocation_optimal_packs(
    provisions: &[ProvisionId],
    hazards: &[JourneyHazard],
    capacity: u8,
) -> Vec<Vec<ProvisionId>> {
    let legal = allocation_legal_packs(provisions, hazards, capacity);
    let quality = |pack: &[ProvisionId]| {
        let readiness = pack
            .iter()
            .map(|item| u16::from(item.readiness()))
            .sum::<u16>();
        let weight = pack
            .iter()
            .map(|item| u16::from(item.weight()))
            .sum::<u16>();
        (
            readiness,
            std::cmp::Reverse(weight),
            std::cmp::Reverse(pack.len()),
        )
    };
    let Some(best) = legal.iter().map(|pack| quality(pack)).max() else {
        return Vec::new();
    };
    legal
        .into_iter()
        .filter(|pack| quality(pack) == best)
        .collect()
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_allocations_have_one_auditable_optimum() {
        for seed in 0..1_000 {
            let puzzle = ResourceAllocationPuzzle::generate(seed);
            puzzle.validate().unwrap();
            let projection = serde_json::to_string(&puzzle.projection()).unwrap();
            assert!(!projection.contains("solution"));
        }
    }
}
