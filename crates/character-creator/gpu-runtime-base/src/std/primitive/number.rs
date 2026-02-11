#[derive(Debug, Clone, Copy, Default)]
pub struct Number;

impl Number {
    pub fn new(value: f64) -> f64 {
        value
    }

    pub fn add(a: f64, b: f64) -> f64 {
        a + b
    }
}
