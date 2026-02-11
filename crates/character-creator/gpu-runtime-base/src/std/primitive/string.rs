#[derive(Debug, Clone, Copy, Default)]
pub struct String;

impl String {
    pub fn new(value: std::string::String) -> std::string::String {
        value
    }

    pub fn append(all: Vec<std::string::String>) -> std::string::String {
        all.join("")
    }
}
