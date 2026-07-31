#[cfg(test)]
mod social_notification_query_tests {
    use super::{
        SETTLEMENTS_SOURCE, SocialActionId, SocialDuration, social_action_blocked_by_actor,
        social_action_error_feedback, social_feedback,
    };
    use adventuresim_core::social::SocialActionKind;

    #[test]
    fn social_action_feedback_is_allowlisted_and_describes_cooldowns_and_results() {
        assert_eq!(
            social_action_error_feedback(
                "SpacetimeDB error: That approach needs time before it can be tried again"
            ),
            "cooldown"
        );
        assert_eq!(
            social_action_error_feedback("transport details that must not reach the browser"),
            "unavailable"
        );
        assert_eq!(
            social_feedback(Some("addressed")).unwrap().message,
            "This concern is addressed."
        );
        assert!(social_feedback(Some("made-up")).is_none());
    }

    #[test]
    fn casual_chat_forms_validate_stable_opaque_action_ids() {
        assert_eq!(SocialDuration::try_from(15).unwrap().minutes(), 15);
        assert_eq!(SocialDuration::try_from(480).unwrap().minutes(), 480);
        assert!(SocialDuration::try_from(14).is_err());
        assert!(SocialDuration::try_from(481).is_err());
        assert_eq!(
            SocialActionId::try_from("chat-19af-2".to_owned())
                .unwrap()
                .as_str(),
            "chat-19af-2"
        );
        assert!(SocialActionId::try_from(String::new()).is_err());
        assert!(SocialActionId::try_from("chat:19af".to_owned()).is_err());

        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn chat_with_party_member")
            .nth(1)
            .and_then(|tail| tail.split("fn social_action_error_feedback").next())
            .expect("party chat handler");
        assert!(handler.contains("json!(form.action_id.as_str())"));
        assert!(!handler.contains("SystemTime::now"));
    }

    #[test]
    fn party_rail_queries_current_party_sources_and_compact_addresses_only() {
        let source = SETTLEMENTS_SOURCE;
        let loader = source
            .split("pub(crate) async fn get_active_party_members")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) async fn soap_rest_preview").next())
            .expect("party member loader");
        assert!(loader.contains(
            "SELECT * FROM backend_character_morale_sources WHERE character_id = {}"
        ));
        assert!(
            !loader.contains(
                "query::<CharacterMoraleSource>(\"SELECT * FROM backend_character_morale_sources\")"
            )
        );
        assert!(loader.contains("SELECT * FROM backend_social_addresses WHERE actor_id = {}"));
        assert!(
            loader.contains("SELECT * FROM backend_automatic_social_chats WHERE actor_id = {}")
        );
        assert!(!loader.contains("backend_social_interactions"));
    }

    #[test]
    fn social_actor_action_visibility_uses_shared_policy_and_fails_closed() {
        assert!(social_action_blocked_by_actor(
            false,
            None,
            SocialActionKind::LightenMood
        ));
        let mut personality = crate::spacetimedb::CharacterPersonality::neutral(1);
        assert!(!social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::LightenMood
        ));
        personality.mirth = crate::spacetimedb::Mirth::Grave;
        assert!(social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::LightenMood
        ));
        assert!(!social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::Flirt
        ));
        personality.courtship = crate::spacetimedb::Courtship::Proper;
        assert!(social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::Flirt
        ));
        assert!(social_action_blocked_by_actor(
            true,
            None,
            SocialActionKind::Flirt
        ));
    }

    #[test]
    fn prayer_preview_uses_private_actor_study_and_fails_closed() {
        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn party_social")
            .nth(1)
            .and_then(|tail| tail.split("struct SocialActionForm").next())
            .expect("social dialog handler");
        assert!(handler.contains("private actor personality query failed closed"));
        assert!(handler.contains("private Religion knowledge query failed closed"));
        assert!(handler.contains("skills.religion_hours.direct(religion) <= 0.0"));
        assert!(!handler.contains("maximum_effective"));
        assert!(handler.contains("Their religion is unknown."));
        assert!(handler.contains("They profess no religion."));
    }
}
