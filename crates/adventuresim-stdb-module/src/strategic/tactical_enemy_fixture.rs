/// Apply a standalone tactical roster through ordinary strategic equipment
/// operations before the transient tactical server claims the mission.
fn parse_tactical_enemy_fixture(
    yaml: &str,
) -> Result<adventuresim_core::tactical_fixture::TacticalEnemyFixture, String> {
    adventuresim_core::tactical_fixture::TacticalEnemyFixture::parse(yaml)
}

fn configure_tactical_enemy_fixture(
    ctx: &ReducerContext,
    hostile_group_id: &str,
    fixture: &adventuresim_core::tactical_fixture::TacticalEnemyFixture,
) -> Result<(), String> {
    let enemy_ids = crate::world_actor::context_character_ids(ctx, hostile_group_id);
    if enemy_ids.len() != fixture.enemies().len() {
        return Err(format!(
            "Enemy fixture requires exactly {} enemies, got {}",
            fixture.enemies().len(),
            enemy_ids.len()
        ));
    }
    for (character_id, enemy) in enemy_ids.into_iter().zip(fixture.enemies()) {
        let mut character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or("Enemy fixture character disappeared")?;
        character.name.clone_from(&enemy.name);
        ctx.db.character().id().update(character);

        let loadout = enemy
            .loadout
            .iter()
            .map(|item| (item.item_id.as_str(), item.slot.into()))
            .collect::<Vec<_>>();
        crate::character::replace_development_loadout(ctx, character_id, &loadout)?;
        if enemy.add_basic_clothing {
            crate::character::add_and_equip_basic_clothing(ctx, character_id)?;
        }
    }
    Ok(())
}
