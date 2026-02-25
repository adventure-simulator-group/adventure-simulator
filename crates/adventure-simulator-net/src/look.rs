use adventure_simulator_core::prelude::*;
use bevy::prelude::*;
use lightyear::prelude::input::InputRegistryExt;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct AdventureSimulatorCharacterLookPlugin;

impl Plugin for AdventureSimulatorCharacterLookPlugin {
    fn build(&self, app: &mut App) {
        app.register_input_action::<CharacterLookUpdate>();
        app.register_required_components::<Player, CharacterLook>();

        #[cfg(feature = "client")]
        {
            app.register_required_components_with::<ControlledPlayer, _>(|| {
                lightyear::prelude::ComponentReplicationOverrides::<CharacterLook>::default()
                    .disable_all()
            });

            app.add_systems(PostUpdate, send_character_look_update)
                .add_observer(on_add_controlled_player_mock_character_look);
        }

        #[cfg(feature = "server")]
        {
            app.add_observer(on_character_look_received);
        }
    }
}

/// Custom input action to send client-side player camera state to the
/// server so it will be used for physics and replicating to other players.
///
/// [`CharacterLook`] is replicated, but for the player's client, it is
/// disabled -- since camera is expected to be client-authoritative in this case.
///
/// **NOTE**: There is a `RotateCamera` input from `bevy_ahoy`, but we can't
/// really use it, because it accumulate errors which results in an invalid
/// movement direction.
#[derive(Serialize, Deserialize, InputAction, Clone)]
#[action_output(Vec2)]
struct CharacterLookUpdate;

#[cfg(feature = "client")]
fn on_add_controlled_player_mock_character_look(
    event: On<Add, ControlledPlayer>,
    mut commands: Commands,
) {
    commands
        .entity(event.entity)
        .insert(actions!(Player[(Action::<CharacterLookUpdate>::new())]));
}

#[cfg(feature = "client")]
fn send_character_look_update(
    mut commands: Commands,
    q_changed: Query<(Entity, &CharacterLook, &Actions<Player>), Changed<CharacterLook>>,
    q_update_action: Query<Entity, With<Action<CharacterLookUpdate>>>,
) {
    for (entity, look, actions) in &q_changed {
        let Some(action) = q_update_action.iter_many(actions.iter()).next() else {
            warn!(
                ?entity,
                "CharacterLookUpdate can't be sent: no actions found"
            );
            continue;
        };

        commands.entity(action).insert(ActionMock::once(
            ActionState::Fired,
            Vec2::new(look.yaw, look.pitch),
        ));
    }
}

#[cfg(feature = "server")]
fn on_character_look_received(
    event: On<Fire<CharacterLookUpdate>>,
    mut q_look: Query<(&mut CharacterLook, &mut Transform)>,
) -> Result {
    let (mut look, mut transform) = q_look.get_mut(event.context)?;
    look.yaw = event.value.x;
    look.pitch = event.value.y;
    transform.rotation = Quat::from_rotation_y(look.yaw + std::f32::consts::PI);

    Ok(())
}
