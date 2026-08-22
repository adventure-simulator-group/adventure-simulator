use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NumberPi;

impl NumberPi {
    pub fn eval() -> f64 {
        std::f64::consts::PI
    }
}
