// Party-facing settlement behavior.
//
// These ordered fragments share one private module scope because the handlers
// exchange request forms and presentation state. The settlement facade only
// imports the handlers and projections consumed outside this domain.

include!("location_personal.rs");
include!("containers.rs");
include!("cooking.rs");
include!("ingredient_preparation.rs");
include!("training_activity.rs");
include!("inventory_medical.rs");
include!("social.rs");
include!("transfers.rs");
