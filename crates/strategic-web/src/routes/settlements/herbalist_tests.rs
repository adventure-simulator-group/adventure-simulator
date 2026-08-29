#[cfg(test)]
mod herbalist_tests {
    use super::living_party_members;
    use crate::spacetimedb::CharacterView;

    fn member(id: u64, alive: bool) -> CharacterView {
        CharacterView {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    #[test]
    fn travel_forecasts_only_include_living_party_members() {
        let members = [member(1, true), member(2, false), member(3, true)];

        let living = living_party_members(&members);

        assert_eq!(
            living.iter().map(|member| member.id).collect::<Vec<_>>(),
            [1, 3]
        );
    }
}
