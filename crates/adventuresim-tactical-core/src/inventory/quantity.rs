use adventuresim_core::inventory_measurement::ItemQuantity;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Clone, Copy, Deref)]
#[serde(transparent)]
#[reflect(opaque)]
#[reflect(Component, PartialEq, Clone, Serialize, Deserialize)]
pub struct TacticalItemQuantity(pub ItemQuantity);

impl Default for TacticalItemQuantity {
    fn default() -> Self {
        Self(ItemQuantity::ONE)
    }
}

impl TacticalItemQuantity {
    pub const fn new(value: u32) -> Option<Self> {
        match ItemQuantity::new(value) {
            Some(quantity) => Some(Self(quantity)),
            None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}
