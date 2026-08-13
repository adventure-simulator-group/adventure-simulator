use super::*;
use bevy::prelude::{App, Update};

#[test]
fn local_interactor_position_reaches_only_ground_foliage_materials() {
    let mut app = App::new();
    app.init_resource::<Time>();
    app.init_resource::<Assets<TacticalFoliageMaterial>>();
    app.init_resource::<GrassInteractionState>();
    app.add_systems(Update, update_grass_interaction);
    let (grass, crown) = {
        let mut materials = app
            .world_mut()
            .resource_mut::<Assets<TacticalFoliageMaterial>>();
        (
            materials.add(foliage_material(0.3, true)),
            materials.add(foliage_material(0.3, false)),
        )
    };
    app.world_mut().spawn((
        GrassInteractor,
        GlobalTransform::from_translation(Vec3::new(3.0, 1.0, -2.0)),
    ));
    app.update();

    let materials = app.world().resource::<Assets<TacticalFoliageMaterial>>();
    assert_eq!(
        materials.get(&grass).unwrap().interaction,
        Vec4::new(3.0, 1.0, -2.0, 1.35)
    );
    assert_eq!(materials.get(&crown).unwrap().interaction, Vec4::ZERO);
}
