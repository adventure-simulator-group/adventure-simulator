/// A physical urban dwelling class with the exact generated footprint used to pack frontages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CityHouseClass {
    Cottage,
    CraftTownHouse,
    HallHouse,
    MerchantHouse,
}

impl CityHouseClass {
    pub const ALL: [Self; 4] = [
        Self::Cottage,
        Self::CraftTownHouse,
        Self::HallHouse,
        Self::MerchantHouse,
    ];

    pub fn frontage_width_metres(self) -> f32 {
        match self {
            Self::Cottage => 10.5,
            Self::CraftTownHouse => 9.0,
            Self::HallHouse => 13.5,
            Self::MerchantHouse => 12.0,
        }
    }

    pub fn depth_metres(self) -> f32 {
        match self {
            Self::Cottage => 12.0,
            Self::CraftTownHouse => 15.0,
            Self::HallHouse => 19.5,
            Self::MerchantHouse => 16.5,
        }
    }

    pub fn resident_capacity(self) -> u32 {
        match self {
            Self::Cottage => 6,
            Self::CraftTownHouse => 13,
            Self::HallHouse => 16,
            Self::MerchantHouse => 30,
        }
    }
}
