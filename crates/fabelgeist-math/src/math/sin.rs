use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NumberSin;

impl NumberSin {
    pub fn eval(x: f64) -> f64 {
        x.sin()
    }
}
