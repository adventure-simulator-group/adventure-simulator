use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum HorizontalAlignment {
    Left,
    #[default]
    Center,
    Right,
}
