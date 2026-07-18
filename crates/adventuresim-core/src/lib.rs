pub mod activity;
pub mod attribute;
pub mod autoresolve;
pub mod body;
pub mod capability;
pub mod combat;
pub mod composite;
pub mod equipment;
pub mod essential;
pub mod morale;
pub mod provisioning;
pub mod skill;
pub mod strategic_schedule;
pub mod strategic_time;
#[doc(hidden)]
pub mod stub;

pub mod prelude {
    pub use crate::activity::*;
    pub use crate::attribute::*;
    pub use crate::autoresolve::*;
    pub use crate::body::*;
    pub use crate::capability::*;
    pub use crate::combat::*;
    pub use crate::composite::PlayerInfo;
    pub use crate::equipment::*;
    pub use crate::essential::*;
    pub use crate::morale::*;
    pub use crate::provisioning::*;
    pub use crate::skill::*;
    pub use crate::strategic_schedule::*;
    pub use crate::strategic_time::*;
}
