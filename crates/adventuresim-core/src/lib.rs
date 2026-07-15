pub mod attribute;
pub mod body;
pub mod capability;
pub mod combat;
pub mod composite;
pub mod equipment;
pub mod essential;
pub mod morale;
pub mod skill;
#[doc(hidden)]
pub mod stub;

pub mod prelude {
    pub use crate::attribute::*;
    pub use crate::body::*;
    pub use crate::capability::*;
    pub use crate::combat::*;
    pub use crate::composite::PlayerInfo;
    pub use crate::equipment::*;
    pub use crate::essential::*;
    pub use crate::morale::*;
    pub use crate::skill::*;
}
