use super::*;
use adventuresim_tactical_core::prelude::SceneEnvironmentFixture;
use bevy::{
    color::ColorToComponents,
    prelude::{App, Update},
};

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

#[test]
fn understory_density_preserves_sparse_woods_and_caps_dense_biomes() {
    assert!((understory_scatter_chance(0.35, 0.03, 0.0) - 0.191).abs() < 0.000_01);
    assert_eq!(understory_scatter_chance(0.9, 0.05, 0.0), 0.24);
    assert_eq!(understory_scatter_chance(0.1, 0.95, 0.0), 0.24);
    assert_eq!(understory_scatter_chance(0.0, 0.0, 0.0), 0.0);
    assert_eq!(understory_scatter_chance(0.0, 0.0, 1.0), 0.08);
}

#[test]
fn grass_density_favors_open_meadow_and_thins_under_closed_canopy() {
    assert_eq!(grass_scatter_density(0.0, 0.0, 0.0, 0.0), 0.98);
    assert!((grass_scatter_density(0.35, 0.0, 0.0, 0.0) - 0.6475).abs() < 0.000_01);
    assert_eq!(grass_scatter_density(0.9, 0.0, 0.0, 0.0), 0.25);
    assert_eq!(grass_scatter_density(0.0, 1.0, 0.0, 0.0), 0.25);
}

#[test]
fn terminal_grass_pigment_compensates_for_foliage_optical_darkening() {
    let environment = SceneEnvironmentFixture::TemperateHills.snapshot("terminal-grass-pigment");
    let blade = grass_pigment(&environment).0.to_linear().to_f32_array();
    let terminal = grass_terminal_pigment(&environment)
        .to_linear()
        .to_f32_array();
    for (channel, expected) in [0.22, 0.25, 0.05].into_iter().enumerate() {
        assert!((terminal[channel] / blade[channel] - expected).abs() < 0.000_01);
    }
}

#[test]
fn foliage_uses_hardware_multisample_coverage() {
    assert_eq!(
        foliage_material(0.3, true).alpha_mode(),
        AlphaMode::AlphaToCoverage
    );
}
