use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NumberCos;

impl NumberCos {
    pub fn eval(x: f64) -> f64 {
        x.cos()
    }
}
