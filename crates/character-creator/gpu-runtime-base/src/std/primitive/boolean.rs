#[derive(Debug, Clone, Copy, Default)]
pub struct Boolean;


impl Boolean {
    pub fn new(value: bool) -> bool {
        value
    }

    pub fn not(value: bool) -> bool {
        !value
    }

    pub fn and(a: bool, b: bool) -> bool {
        a && b
    }

    pub fn or(a: bool, b: bool) -> bool {
        a || b
    }

    pub fn xor(a: bool, b: bool) -> bool {
        a ^ b
    }
}
