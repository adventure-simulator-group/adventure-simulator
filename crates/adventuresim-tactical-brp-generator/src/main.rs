//! Discovers every BRP-queryable reflect type (anything with
//! `ReflectComponent`/`ReflectResource` data registered) linked in from
//! `adventuresim-tactical-server`/`-client` via their `remote-types`
//! feature (see each crate's `src/lib.rs`), and prints them as typed Python
//! classes for `scripts/tactical_brp.py` - see `just generate-brp-types`.
//!
//! Discovery is dynamic: `reflect_auto_register` (enabled workspace-wide)
//! populates `AppTypeRegistry` for every linked-in `#[derive(Reflect)]`
//! type automatically as soon as an `App` exists - no plugins need to be
//! added, nothing needs to run, no type needs to be named here by hand. New
//! reflected types show up the next time this is regenerated.

mod codegen;
mod resolve;

use bevy::app::App;
use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent, ReflectResource};

use resolve::{Resolver, StructKind};

// Only the linker needs most of these two crates, to pull in their
// `#[derive(Reflect)]` types - nothing from either is actually called,
// except `register_input_mock_types` (see its own doc comment): a few
// `bevy_enhanced_input` action components need type data registered by hand
// to be BRP-visible at all, matching what the real client also does.
use adventuresim_tactical_client::debug::register_input_mock_types;
#[expect(
    unused_imports,
    reason = "linking this crate registers its reflected types through macro-generated startup hooks"
)]
use adventuresim_tactical_server as _pull_in_server_reflect_types;

fn main() {
    let mut app = App::new();
    register_input_mock_types(&mut app);

    let registry = app.world().resource::<AppTypeRegistry>().read();

    let mut resolver = Resolver::new();
    for registration in registry.iter() {
        let has_component = registration.data::<ReflectComponent>().is_some();
        let has_resource = registration.data::<ReflectResource>().is_some();
        if !has_component && !has_resource {
            continue;
        }
        // A type registering as *both* `ReflectComponent` and
        // `ReflectResource` doesn't currently occur in practice -
        // `Component` is an arbitrary but harmless choice for it if it
        // ever does.
        let kind = if has_component {
            StructKind::Component
        } else {
            StructKind::Resource
        };
        let type_info = registration.type_info();
        resolver.resolve_top_level(type_info.type_path(), type_info, kind);
    }
    drop(registry);

    print!("{}", codegen::render(&resolver));
}
