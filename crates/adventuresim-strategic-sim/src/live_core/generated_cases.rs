impl LiveRunner {
    pub(super) fn owned_open_generated_cases(&self, character_id: u64) -> Vec<(String, String)> {
        stable_owned_open_cases(
            character_id,
            self.connection
                .db
                .backend_investigation_cases()
                .iter()
                .map(|row| (row.owner_character_id, row.case_id, row.subject, row.status)),
        )
    }

    pub(super) fn generated_case_status(&self, character_id: u64, case_id: &str) -> Option<String> {
        self.connection
            .db
            .backend_investigation_cases()
            .iter()
            .find(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| row.status)
    }

    pub(super) fn observe_generated_case_intake(
        &mut self,
        agent: u32,
        owner_character_id: u64,
        case_id: &str,
        subject: &str,
        source: &str,
    ) -> bool {
        let key = (owner_character_id, case_id.to_owned());
        if !self.generated_seen_cases.insert(key) {
            return false;
        }
        self.metrics.generated_case_intakes = self.metrics.generated_case_intakes.saturating_add(1);
        self.metrics.quests_attempted = self.metrics.quests_attempted.saturating_add(1);
        if source == "owner_projection_continuation" {
            self.metrics.generated_case_continuations =
                self.metrics.generated_case_continuations.saturating_add(1);
        }
        let party_id = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|character| character.id == owner_character_id)
            .and_then(|character| character.party_id)
            .unwrap_or_default();
        self.event(
            agent,
            CoreLoopEventKind::GeneratedCaseIntake,
            format!(
                "owner={owner_character_id};party={};case={};subject={};source={}",
                bounded_event_field(&party_id),
                bounded_event_field(case_id),
                bounded_event_field(subject),
                bounded_event_field(source),
            ),
        );
        true
    }

    pub(super) fn observe_generated_case_transition(
        &mut self,
        agent: u32,
        character_id: u64,
        case_id: &str,
        title: &str,
        immediately_after_own_action: bool,
    ) {
        let key = (character_id, case_id.to_owned());
        if self.generated_terminal_cases.contains(&key) {
            return;
        }
        let attribution = generated_closure_attribution(
            "open",
            self.generated_case_status(character_id, case_id).as_deref(),
            immediately_after_own_action,
        );
        match attribution {
            GeneratedClosureAttribution::StillOpen => {}
            GeneratedClosureAttribution::OwnImmediateTransition => {
                self.generated_terminal_cases.insert(key);
                self.metrics.generated_quests_completed += 1;
                self.metrics.quests_completed += 1;
                let party_id = self
                    .connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|character| character.id == character_id)
                    .and_then(|character| character.party_id)
                    .unwrap_or_default();
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedQuestCompleted,
                    format!(
                        "party={};case={};subject={};attribution=own_immediate_transition",
                        bounded_event_field(&party_id),
                        bounded_event_field(case_id),
                        bounded_event_field(title)
                    ),
                );
            }
            GeneratedClosureAttribution::ExternalTransition => {
                self.generated_terminal_cases.insert(key);
                self.metrics.generated_quests_closed_externally += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedQuestClosedExternally,
                    format!(
                        "case={};subject={};attribution=external_transition",
                        bounded_event_field(case_id),
                        bounded_event_field(title)
                    ),
                );
            }
        }
    }

    pub(super) fn observe_external_generated_closures(&mut self) {
        let tracked = self
            .generated_seen_cases
            .iter()
            .map(|(owner, case_id)| (case_id.clone(), *owner))
            .collect::<Vec<_>>();
        for (case_id, owner) in tracked {
            let Some(agent) = self.character_ids.iter().position(|id| *id == owner) else {
                continue;
            };
            let title = self
                .connection
                .db
                .backend_investigation_cases()
                .iter()
                .find(|row| row.owner_character_id == owner && row.case_id == case_id)
                .map_or_else(|| "Unlabelled problem".into(), |row| row.subject);
            self.observe_generated_case_transition(agent as u32, owner, &case_id, &title, false);
        }
    }

    pub(super) fn visible_npc_candidates(
        &self,
        character_id: u64,
        preferred_name: Option<&str>,
        preferred_location: Option<&str>,
    ) -> Vec<PublicNpcCandidate> {
        let Some(character) = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
        else {
            return Vec::new();
        };
        let Some(settlement_id) = character.current_settlement_id else {
            return Vec::new();
        };
        let Some(settlement) = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)
        else {
            return Vec::new();
        };
        let Some(economy) = public_settlement_economy_profile(&settlement.economy) else {
            return Vec::new();
        };
        let has_keep = matches!(
            settlement.category,
            SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
        );
        let minute = self
            .connection
            .db
            .backend_character_times()
            .iter()
            .find(|row| row.character_id == character_id)
            .map_or(720, |row| row.minutes);
        let candidates = self
            .connection
            .db
            .settlement_resident_presence()
            .iter()
            .filter(|presence| {
                presence.settlement_id == settlement_id
                    && npc_is_publicly_present(presence.start_minute, presence.end_minute, minute)
            })
            .filter_map(|presence| {
                self.connection
                    .db
                    .backend_settlement_residents()
                    .iter()
                    .find(|npc| {
                        npc.character_id == presence.character_id
                            && npc.home_settlement_id == settlement_id
                    })
                    .map(|npc| PublicNpcCandidate {
                        resident_character_id: npc.character_id,
                        name: npc.name,
                        profession: npc.profession,
                        conversation_id: npc.conversation_id,
                        location_id: presence.location_id,
                    })
            })
            .collect();
        let candidates =
            retain_navigable_public_npc_candidates(candidates, &economy, has_keep, &settlement_id);
        stable_public_npc_candidates(candidates, preferred_name, preferred_location)
    }

    pub(super) fn start_public_dialogue(
        &mut self,
        character_id: u64,
        cycle: u32,
        candidate: &PublicNpcCandidate,
        purpose: &str,
    ) -> Result<String, String> {
        self.dialogue_nonce = self.dialogue_nonce.saturating_add(1);
        let session_id = format!(
            "dialogue:{character_id}:sim-{cycle}-{}-{purpose}",
            self.dialogue_nonce
        );
        let result = reducer_call!(self, "start_dialogue", |cb| self
            .connection
            .reducers
            .start_dialogue_then(
                character_id,
                session_id.clone(),
                candidate.conversation_id.clone(),
                candidate.resident_character_id.to_string(),
                candidate.location_id.clone(),
                adventuresim_dialogue::CATALOG_DIGEST.to_owned(),
                cb,
            ));
        self.call(result)?;
        let session_is_owned = self
            .connection
            .db
            .backend_dialogue_sessions()
            .iter()
            .any(|row| row.id == session_id && row.owner_character_id == character_id);
        if !session_is_owned {
            return Err("dialogue reducer completed without an owner-scoped session".into());
        }
        Ok(session_id)
    }

    pub(super) fn official_world_minute(&self) -> u64 {
        self.connection
            .db
            .world_clock()
            .iter()
            .map(|clock| clock.official_minutes)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn public_discovery_fingerprint(
        &self,
        character_id: u64,
        official_minute: u64,
        candidates: &[PublicNpcCandidate],
    ) -> (PublicDiscoveryFingerprint, usize, &'static str) {
        let settlement_id = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|row| row.current_settlement_id)
            .unwrap_or_default();
        let mut contacts = candidates
            .iter()
            .map(public_discovery_contact_identity)
            .collect::<Vec<_>>();
        contacts.sort();
        let mut active_symptoms = self
            .connection
            .db
            .local_problem_symptom()
            .iter()
            .filter(|symptom| {
                symptom.settlement_id == settlement_id
                    && symptom.active_from <= official_minute
                    && official_minute < symptom.active_until
            })
            .map(|symptom| {
                (
                    symptom.symptom,
                    symptom.public_summary,
                    symptom.active_from,
                    symptom.active_until,
                )
            })
            .collect::<Vec<_>>();
        active_symptoms.sort();
        let oldest_age = active_symptoms
            .iter()
            .map(|(_, _, active_from, _)| official_minute.saturating_sub(*active_from))
            .max();
        let active_symptom_count = active_symptoms.len();
        (
            PublicDiscoveryFingerprint {
                settlement_id,
                contacts,
                active_symptoms,
            },
            active_symptom_count,
            public_symptom_age_bucket(oldest_age),
        )
    }

    pub(super) fn discover_generated_case(
        &mut self,
        character_id: u64,
        agent: u32,
        cycle: u32,
    ) -> Result<GeneratedDiscoveryOutcome, String> {
        let before = self
            .connection
            .db
            .backend_investigation_cases()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.status == "open")
            .map(|row| row.case_id)
            .collect::<HashSet<_>>();
        let before_referrals = self
            .connection
            .db
            .backend_investigation_leads()
            .iter()
            .filter(|row| row.owner_character_id == character_id)
            .map(PublicDiscoveryReferral::from)
            .map(|lead| (lead.lead_id.clone(), lead))
            .collect::<HashMap<_, _>>();
        let candidates = self.visible_npc_candidates(character_id, None, None);
        let visible_candidate_count = candidates.len();
        let official_minute = self.official_world_minute();
        let (public_fingerprint, active_symptom_count, oldest_symptom_age_bucket) =
            self.public_discovery_fingerprint(character_id, official_minute, &candidates);
        let previous_contact = public_discovery_previous_contact(
            self.generated_discovery_backoff.get(&character_id),
            &public_fingerprint,
        );
        let candidate = stable_discovery_action_candidate(candidates, previous_contact);
        let location_class = discovery_location_class(candidate.as_ref());
        let public_backoff = self
            .generated_discovery_backoff
            .get(&character_id)
            .is_some_and(|backoff| {
                public_discovery_backoff_active(backoff, &public_fingerprint, official_minute)
            });
        if public_backoff {
            self.metrics.generated_discovery_public_backoff_suppressions = self
                .metrics
                .generated_discovery_public_backoff_suppressions
                .saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class={location_class};owner_open_case_count={};public_backoff=true;result=suppressed;reason=unchanged_public_state",
                    public_count_bucket(active_symptom_count),
                    visible_candidate_count.min(32),
                    before.len().min(32),
                ),
            );
            return Ok(GeneratedDiscoveryOutcome::PublicBackoff);
        }
        self.generated_discovery_backoff.remove(&character_id);

        let Some(candidate) = candidate else {
            self.metrics.generated_discovery_decisions_unproductive = self
                .metrics
                .generated_discovery_decisions_unproductive
                .saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class=none;owner_open_case_count={};public_backoff=false;dialogue_success=false;session_success=false;new_open_cases=0;rumor_delivered=false;result=unproductive;reason=no_visible_contacts;fallback=no_visible_contacts;activity_fallback=true",
                    public_count_bucket(active_symptom_count),
                    visible_candidate_count.min(32),
                    before.len().min(32),
                ),
            );
            return Ok(GeneratedDiscoveryOutcome::NoVisibleContacts);
        };

        self.metrics.generated_discovery_actions_attempted = self
            .metrics
            .generated_discovery_actions_attempted
            .saturating_add(1);
        self.event(
            agent,
            CoreLoopEventKind::GeneratedDiscoveryAttempt,
            format!(
                "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class={location_class};owner_open_case_count={};public_backoff=false",
                public_count_bucket(active_symptom_count),
                visible_candidate_count.min(32),
                before.len().min(32),
            ),
        );
        if let Err(error) = self.start_public_dialogue(character_id, cycle, &candidate, "discover")
        {
            let dialogue_succeeded =
                error == "dialogue reducer completed without an owner-scoped session";
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class={location_class};owner_open_case_count={};public_backoff=false;dialogue_success={dialogue_succeeded};session_success=false;new_open_cases=0;rumor_delivered=false;result=failed;reason={};fallback=none;activity_fallback=false",
                    public_count_bucket(active_symptom_count),
                    visible_candidate_count.min(32),
                    before.len().min(32),
                    if dialogue_succeeded {
                        "session_projection_missing"
                    } else {
                        "dialogue_failed"
                    },
                ),
            );
            return Err(if dialogue_succeeded {
                "start_discovery_dialogue failed: owner-scoped dialogue session unavailable".into()
            } else {
                "start_discovery_dialogue failed: public discovery contact failed".into()
            });
        }

        // The owner-scoped open-case projection is the public postcondition of
        // receiving a generated rumor. It avoids inspecting private delivery
        // receipts or generation eligibility.
        let mut after = self.owned_open_generated_cases(character_id);
        let mut discovered = after
            .iter()
            .filter(|(case_id, _)| !before.contains(case_id))
            .cloned()
            .collect::<Vec<_>>();
        if discovered.is_empty()
            && let Some(referral) = new_or_updated_public_discovery_referral(
                character_id,
                &before_referrals,
                self.connection
                    .db
                    .backend_investigation_leads()
                    .iter()
                    .map(PublicDiscoveryReferral::from),
            )
        {
            let preferred_location = if referral.current_learned_location.is_empty() {
                &referral.expected_location
            } else {
                &referral.current_learned_location
            };
            if self.try_generated_dialogue_topic(
                character_id,
                agent,
                cycle,
                &referral.case_id,
                &referral.summary,
                &["referred-testimony"],
                Some(&referral.witness_name),
                Some(preferred_location),
            )? {
                after = self.owned_open_generated_cases(character_id);
                discovered = after
                    .iter()
                    .filter(|(case_id, _)| !before.contains(case_id))
                    .cloned()
                    .collect();
            }
        }
        discovered.sort();
        let new_open_cases = discovered.len();
        if let Some((case_id, subject)) = discovered.into_iter().next() {
            self.metrics.generated_discovery_actions_fruitful = self
                .metrics
                .generated_discovery_actions_fruitful
                .saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class={location_class};owner_open_case_count={};public_backoff=false;dialogue_success=true;session_success=true;new_open_cases={};rumor_delivered=true;result=fruitful;reason=rumor_delivered;fallback=none;activity_fallback=false",
                    public_count_bucket(active_symptom_count),
                    visible_candidate_count.min(32),
                    after.len().min(32),
                    new_open_cases.min(32),
                ),
            );
            self.generated_discovery_backoff.remove(&character_id);
            self.observe_generated_case_intake(
                agent,
                character_id,
                &case_id,
                &subject,
                "dialogue_rumor",
            );
            self.metrics.generated_quests_discovered += 1;
            self.metrics.generated_unique_party_cases_discovered += 1;
            let party_id = self
                .connection
                .db
                .backend_characters()
                .iter()
                .find(|character| character.id == character_id)
                .and_then(|character| character.party_id)
                .unwrap_or_default();
            self.event(
                agent,
                CoreLoopEventKind::GeneratedQuestDiscovered,
                format!(
                    "party={};case={};subject={};npc={};location={}",
                    bounded_event_field(&party_id),
                    bounded_event_field(&case_id),
                    bounded_event_field(&subject),
                    bounded_event_field(&candidate.name),
                    bounded_event_field(&candidate.location_id)
                ),
            );
            return Ok(GeneratedDiscoveryOutcome::Discovered);
        }

        self.metrics.generated_discovery_decisions_unproductive = self
            .metrics
            .generated_discovery_decisions_unproductive
            .saturating_add(1);
        self.event(
            agent,
            CoreLoopEventKind::GeneratedDiscoveryResult,
            format!(
                "official_minute={official_minute};active_symptom_count={};oldest_symptom_age_bucket={oldest_symptom_age_bucket};visible_candidate_count={};location_class={location_class};owner_open_case_count={};public_backoff=false;dialogue_success=true;session_success=true;new_open_cases=0;rumor_delivered=false;result=unproductive;reason=no_public_rumor_available;fallback=no_public_rumor_available;activity_fallback=true",
                public_count_bucket(active_symptom_count),
                visible_candidate_count.min(32),
                after.len().min(32),
            ),
        );
        self.generated_discovery_backoff.insert(
            character_id,
            PublicDiscoveryBackoff {
                fingerprint: public_fingerprint,
                last_contact: public_discovery_contact_identity(&candidate),
                retry_at: official_minute.saturating_add(PUBLIC_DISCOVERY_BACKOFF_MINUTES),
            },
        );
        Ok(GeneratedDiscoveryOutcome::NoPublicRumor)
    }

    pub(super) fn try_generated_dialogue_topic(
        &mut self,
        character_id: u64,
        agent: u32,
        cycle: u32,
        case_id: &str,
        subject: &str,
        topics: &[&str],
        preferred_name: Option<&str>,
        preferred_location: Option<&str>,
    ) -> Result<bool, String> {
        let mut candidates =
            self.visible_npc_candidates(character_id, preferred_name, preferred_location);
        if let Some(name) = preferred_name {
            candidates.retain(|candidate| candidate.name.eq_ignore_ascii_case(name));
        }
        for candidate in candidates.into_iter().take(8) {
            let contact = public_discovery_contact_identity(&candidate);
            let public_before_dialogue =
                self.public_dialogue_progress_fingerprint(character_id, case_id);
            if topics.iter().all(|topic_id| {
                let key = PublicDialogueAttemptKey {
                    owner_character_id: character_id,
                    case_id: case_id.to_owned(),
                    topic_id: (*topic_id).to_owned(),
                    contact: contact.clone(),
                };
                !public_dialogue_topic_attempt_allowed(
                    self.generated_dialogue_no_progress.get(&key),
                    &public_before_dialogue,
                )
            }) {
                continue;
            }
            let session_id = self.start_public_dialogue(character_id, cycle, &candidate, "case")?;
            let mut options = self
                .connection
                .db
                .backend_dialogue_topic_options()
                .iter()
                .filter(|row| {
                    row.owner_character_id == character_id
                        && row.session_id == session_id
                        && topics.contains(&row.topic_id.as_str())
                        && row.public_case_id == case_id
                })
                .collect::<Vec<_>>();
            options.sort_by_key(|row| (row.topic_id.clone(), row.id.clone()));
            let Some(option) = options.into_iter().next() else {
                continue;
            };
            let session = self
                .connection
                .db
                .backend_dialogue_sessions()
                .iter()
                .find(|row| row.owner_character_id == character_id && row.id == session_id)
                .ok_or("projected dialogue session disappeared")?;
            let action_id = format!("sim-topic-{cycle}-{}", self.sequence.saturating_add(1));
            let topic_id = option.topic_id.clone();
            let attempt_key = PublicDialogueAttemptKey {
                owner_character_id: character_id,
                case_id: case_id.to_owned(),
                topic_id: topic_id.clone(),
                contact,
            };
            let public_before = self.public_dialogue_progress_fingerprint(character_id, case_id);
            if !public_dialogue_topic_attempt_allowed(
                self.generated_dialogue_no_progress.get(&attempt_key),
                &public_before,
            ) {
                continue;
            }
            let result = reducer_call!(self, "choose_dialogue_topic", |cb| self
                .connection
                .reducers
                .choose_dialogue_topic_then(
                    character_id,
                    session_id.clone(),
                    topic_id.clone(),
                    action_id.clone(),
                    session.revision,
                    session.catalog_revision.clone(),
                    cb,
                ));
            self.call(result)?;
            let public_after = self.public_dialogue_progress_fingerprint(character_id, case_id);
            if !public_dialogue_topic_made_progress(&public_before, &public_after) {
                self.generated_dialogue_no_progress
                    .insert(attempt_key, public_before);
                continue;
            }
            self.generated_dialogue_no_progress.remove(&attempt_key);
            if topic_id == "referred-testimony" {
                self.metrics.generated_witness_dialogues += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedWitnessDialogue,
                    format!(
                        "case={};subject={};npc={};location={};topic={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&candidate.name),
                        bounded_event_field(&candidate.location_id),
                        bounded_event_field(&topic_id)
                    ),
                );
            } else {
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedInvestigationAction,
                    format!(
                        "case={};subject={};npc={};location={};topic={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&candidate.name),
                        bounded_event_field(&candidate.location_id),
                        bounded_event_field(&topic_id)
                    ),
                );
            }
            self.observe_generated_case_transition(agent, character_id, case_id, subject, true);
            return Ok(true);
        }
        Ok(false)
    }

    fn public_dialogue_progress_fingerprint(
        &self,
        character_id: u64,
        case_id: &str,
    ) -> PublicDialogueProgressFingerprint {
        let mut cases = self
            .connection
            .db
            .backend_investigation_cases()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| (row.case_id, row.status, row.latest_update_at))
            .collect::<Vec<_>>();
        cases.sort();
        let mut leads = self
            .connection
            .db
            .backend_investigation_leads()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| {
                (
                    row.lead_id,
                    row.recorded_at,
                    row.summary,
                    row.witness_name,
                    row.corrected_by,
                    row.expected_location,
                    row.current_learned_location,
                )
            })
            .collect::<Vec<_>>();
        leads.sort();
        let mut actions = self
            .connection
            .db
            .backend_investigation_actions()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| {
                (
                    row.action_id,
                    row.expected_version,
                    row.available,
                    row.can_travel_to_required_site,
                    row.unavailable_reason_code,
                    row.wait_minutes,
                )
            })
            .collect::<Vec<_>>();
        actions.sort();
        let mut outcomes = self
            .connection
            .db
            .backend_investigation_action_outcomes()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| (row.outcome_id, row.action_id, row.recorded_at))
            .collect::<Vec<_>>();
        outcomes.sort();
        let mut sites = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| {
                (
                    row.case_site_id,
                    row.knowledge_stage,
                    row.tracked,
                    row.case_resolved,
                    row.combat_available,
                )
            })
            .collect::<Vec<_>>();
        sites.sort();
        PublicDialogueProgressFingerprint {
            cases,
            leads,
            actions,
            outcomes,
            sites,
        }
    }

    pub(super) fn generated_actor_ready_after_time(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        case_id: &str,
    ) -> Result<bool, String> {
        self.synchronize_generated_party_for_action(party_id, owner_character_id, case_id, 0)
    }

    pub(super) fn synchronize_generated_party_for_action(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        case_id: &str,
        cycle: u32,
    ) -> Result<bool, String> {
        let Some((current_leader, leader_agent)) = self.current_leader(party_id) else {
            return Ok(false);
        };
        if current_leader != owner_character_id {
            return Ok(false);
        }
        let result = reducer_call!(self, "synchronize_party_for_activity", |cb| self
            .connection
            .reducers
            .synchronize_party_for_activity_then(owner_character_id, cb));
        self.call(result)?;
        self.observe_deaths();
        if self.current_leader(party_id).map(|(leader, _)| leader) != Some(owner_character_id) {
            return Ok(false);
        }
        let mut party_medically_ready = true;
        for party_agent in self.party_agents(owner_character_id)? {
            if !self.ensure_medically_safe(party_agent)? {
                party_medically_ready = false;
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    party_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "generated_case={};cycle={cycle};reason=not_ready_after_party_clock_sync",
                        bounded_event_field(case_id)
                    ),
                );
                continue;
            }
            self.maintain_equipment(party_agent)?;
        }
        // Recovery and maintenance advance individual clocks. Re-align after
        // those actions so the preflight does not reject its own care work as
        // persistent party clock skew.
        let result = reducer_call!(self, "resynchronize_party_after_generated_preflight", |cb| self
            .connection
            .reducers
            .synchronize_party_for_activity_then(owner_character_id, cb));
        self.call(result)?;
        self.observe_deaths();
        if !party_medically_ready {
            return Ok(false);
        }
        if self
            .refreshed_safe_party_for_owner(party_id, owner_character_id)?
            .is_none()
        {
            return Ok(false);
        }
        let mut living_member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id)
            .filter(|member| {
                self.connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|character| character.id == member.character_id)
                    .is_some_and(|character| character.alive)
            })
            .map(|member| member.character_id)
            .collect::<Vec<_>>();
        living_member_ids.sort_unstable();
        let aligned = public_party_clocks_aligned(
            &living_member_ids,
            self.connection
                .db
                .backend_character_times()
                .iter()
                .map(|time| (time.character_id, time.minutes)),
        );
        if !aligned {
            self.event(
                leader_agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "generated_case={};cycle={cycle};reason=public_party_clock_skew",
                    bounded_event_field(case_id)
                ),
            );
        }
        Ok(aligned)
    }

    pub(super) fn refreshed_safe_party_for_owner(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
    ) -> Result<Option<(u32, Party)>, String> {
        self.observe_deaths();
        let Some((current_leader, current_agent)) = self.current_leader(party_id) else {
            return Ok(None);
        };
        if current_leader != owner_character_id {
            return Ok(None);
        }
        let party_agents = self.party_agents(current_leader)?;
        if !self.unsafe_party_agents(&party_agents).is_empty() {
            return Ok(None);
        }
        let party = self.party_for(current_leader)?;
        if party.id != party_id {
            return Ok(None);
        }
        Ok(Some((current_agent, party)))
    }

    pub(super) fn emit_generated_investigation_attempt(
        &mut self,
        party_id: &str,
        character_id: u64,
        agent: u32,
        case_id: &str,
        subject: &str,
        action: &BackendInvestigationAction,
        attempt: &str,
    ) -> Result<(), String> {
        let actor_time = self
            .connection
            .db
            .backend_character_times()
            .iter()
            .find(|row| row.character_id == character_id)
            .map(|row| row.minutes)
            .ok_or("projected investigation actor clock is unavailable")?;
        let party_member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.character_id)
            .collect::<Vec<_>>();
        let mut party_times = party_member_ids
            .iter()
            .map(|member_id| {
                self.connection
                    .db
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == *member_id)
                    .map(|row| row.minutes)
                    .ok_or("projected investigation party clock is unavailable")
            })
            .collect::<Result<Vec<_>, _>>()?;
        party_times.sort_unstable();
        let party_time_min = party_times
            .first()
            .copied()
            .ok_or("projected investigation party clock is unavailable")?;
        let party_time_max = party_times
            .last()
            .copied()
            .ok_or("projected investigation party clock is unavailable")?;
        let reason_code = if action.unavailable_reason_code.is_empty() {
            "none"
        } else {
            &action.unavailable_reason_code
        };
        self.event(
            agent,
            CoreLoopEventKind::GeneratedInvestigationAttempt,
            format!(
                "case={};subject={};action={};method={};summary={};attempt={};expected_version={};available={};unavailable_reason_code={};wait_minutes={};actor_time={actor_time};party_time_min={party_time_min};party_time_max={party_time_max}",
                bounded_event_field(case_id),
                bounded_event_field(subject),
                bounded_event_field(&action.action_id),
                bounded_event_field(&action.method),
                bounded_event_field(&action.summary),
                bounded_event_field(attempt),
                action.expected_version,
                action.available,
                bounded_event_field(reason_code),
                action.wait_minutes,
            ),
        );
        Ok(())
    }

    pub(super) fn wait_for_generated_investigation_window(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        agent: u32,
        case_id: &str,
        action_id: &str,
        wait_minutes: u32,
    ) -> Result<bool, String> {
        let wait_minutes = projected_investigation_wait_minutes("night_window", wait_minutes)
            .ok_or("projected investigation wait hint was invalid")?;
        let at_settlement = self
            .party_for(owner_character_id)?
            .current_settlement_id
            .is_some();
        let settlement_venue = if at_settlement && wait_minutes >= 60 {
            self.settlement_activity_venue(owner_character_id, 0)?
        } else {
            None
        };
        if at_settlement && wait_minutes >= 60 && settlement_venue.is_none() {
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "generated_case={};action={};reason=insufficient_visible_resources;wait_minutes={wait_minutes}",
                    bounded_event_field(case_id),
                    bounded_event_field(action_id),
                ),
            );
            return Ok(false);
        }
        let wait_mode = if let Some(venue) = settlement_venue {
            let result = reducer_call!(self, "wait_for_investigation_window_settlement", |cb| {
                self.connection.reducers.rest_at_settlement_hours_then(
                    owner_character_id,
                    u64::from(wait_minutes),
                    venue.at_inn(),
                    cb,
                )
            });
            self.call(result)?;
            if venue.at_inn() {
                "settlement_inn"
            } else {
                "settlement_temple"
            }
        } else {
            let shelter = self.rest_at_camp_with_party_shelter(
                owner_character_id,
                u64::from(wait_minutes),
                "wait_for_investigation_window_camp",
            )?;
            if matches!(shelter, FieldShelter::Tent) {
                "field_tent"
            } else {
                "field_bivouac"
            }
        };
        self.metrics.generated_investigation_waits += 1;
        self.metrics.generated_investigation_wait_minutes = self
            .metrics
            .generated_investigation_wait_minutes
            .saturating_add(u64::from(wait_minutes));
        self.event(
            agent,
            CoreLoopEventKind::GeneratedInvestigationWait,
            format!(
                "case={};action={};reason=night_window;wait_minutes={wait_minutes};mode={wait_mode}",
                bounded_event_field(case_id),
                bounded_event_field(action_id),
            ),
        );
        self.generated_actor_ready_after_time(party_id, owner_character_id, case_id)
    }

    pub(super) fn return_completed_generated_party_to_origin(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        case_id: &str,
    ) -> Result<bool, String> {
        let Some(occupied_site_id) = self
            .party_by_id(party_id)?
            .current_case_site_id
            .map(|site| site.value)
        else {
            return Ok(true);
        };
        let pin = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .find(|pin| {
                occupied_case_pin_matches(
                    owner_character_id,
                    case_id,
                    &occupied_site_id,
                    pin.owner_character_id,
                    &pin.case_id,
                    &pin.case_site_id,
                )
            })
            .ok_or("completed generated case site has no exact owner-scoped return pin")?;
        let Some((current_leader, current_agent)) = self.current_leader(party_id) else {
            return Ok(false);
        };
        let settlement_id = pin.origin_settlement_id.clone();
        let result = reducer_call!(self, "return_completed_generated_case", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(current_leader, settlement_id.clone(), cb,));
        self.call(result)?;
        self.event(
            current_agent,
            CoreLoopEventKind::Travel,
            format!("generated_case={case_id};case_completed=true;return_started={settlement_id}"),
        );
        let journey_outcome = self.travel_camps(party_id)?;
        self.observe_deaths();
        if journey_outcome == JourneyTravelOutcome::Completed {
            self.event(
                current_agent,
                CoreLoopEventKind::Travel,
                format!("generated_case={case_id};return_completed={settlement_id}"),
            );
        }
        Ok(journey_outcome == JourneyTravelOutcome::Completed)
    }

    pub(super) fn advance_generated_case(
        &mut self,
        party_id: &str,
        character_id: u64,
        agent: u32,
        cycle: u32,
        case_id: &str,
        subject: &str,
    ) -> Result<bool, String> {
        if !self.synchronize_generated_party_for_action(party_id, character_id, case_id, cycle)? {
            return Ok(false);
        }
        let defeat_key = (character_id, case_id.to_owned());
        let preflight_fingerprint = self.public_party_combat_fingerprint(party_id);
        let public_combat_available =
            self.connection
                .db
                .backend_case_site_pins()
                .iter()
                .any(|pin| {
                    pin.owner_character_id == character_id
                        && pin.case_id == case_id
                        && pin.combat_available
                });
        if generated_defeat_decision(
            public_combat_available,
            self.generated_defeat_fingerprints.get(&defeat_key),
            &preflight_fingerprint,
        ) == GeneratedDefeatDecision::SuppressUnchanged
        {
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "generated_case={};reason=unchanged_defeated_threat;phase=preflight;public_fingerprint_members={}",
                    bounded_event_field(case_id),
                    preflight_fingerprint.members.len(),
                ),
            );
            return Ok(false);
        }
        for _ in 0..MAX_GENERATED_CASE_STEPS_PER_CYCLE {
            if self.generated_case_status(character_id, case_id).as_deref() != Some("open") {
                return self.return_completed_generated_party_to_origin(
                    party_id,
                    character_id,
                    case_id,
                );
            }
            let at_settlement = self
                .party_for(character_id)?
                .current_settlement_id
                .is_some();
            let mut actions = self
                .connection
                .db
                .backend_investigation_actions()
                .iter()
                .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
                .collect::<Vec<_>>();
            let profile = &self.profiles[agent as usize];
            sort_generated_actions(profile, &mut actions);
            if let Some(action) = actions.iter().find(|row| row.available).cloned() {
                let known_outcomes = self
                    .connection
                    .db
                    .backend_investigation_action_outcomes()
                    .iter()
                    .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
                    .map(|row| row.outcome_id)
                    .collect::<HashSet<_>>();
                self.emit_generated_investigation_attempt(
                    party_id,
                    character_id,
                    agent,
                    case_id,
                    subject,
                    &action,
                    "initial",
                )?;
                let result = reducer_call!(self, "perform_investigation_action", |cb| self
                    .connection
                    .reducers
                    .perform_investigation_action_then(
                        character_id,
                        action.action_id.clone(),
                        action.method.clone(),
                        action.expected_version,
                        cb,
                    ));
                self.call(result)?;
                let mut outcomes = self
                    .connection
                    .db
                    .backend_investigation_action_outcomes()
                    .iter()
                    .filter(|row| {
                        row.owner_character_id == character_id
                            && row.case_id == case_id
                            && row.action_id == action.action_id
                            && !known_outcomes.contains(&row.outcome_id)
                    })
                    .collect::<Vec<_>>();
                outcomes.sort_by_key(|row| (row.recorded_at, row.outcome_id.clone()));
                let wording = outcomes
                    .last()
                    .map_or("No new public outcome wording", |row| row.wording.as_str());
                self.metrics.generated_investigation_actions += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedInvestigationAction,
                    format!(
                        "case={};subject={};action={};method={};summary={};outcome={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&action.action_id),
                        bounded_event_field(&action.method),
                        bounded_event_field(&action.summary),
                        bounded_event_field(wording)
                    ),
                );
                self.observe_generated_case_transition(agent, character_id, case_id, subject, true);
                if self.generated_case_status(character_id, case_id).as_deref() != Some("open") {
                    return self.return_completed_generated_party_to_origin(
                        party_id,
                        character_id,
                        case_id,
                    );
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            if let Some(action) = actions.iter().find(|row| row.can_travel_to_required_site) {
                let funnel_key = (character_id, case_id.to_owned());
                if self.generated_exact_site_cases.insert(funnel_key.clone()) {
                    self.metrics.generated_exact_site_ready += 1;
                }
                let pin = self
                    .connection
                    .db
                    .backend_case_site_pins()
                    .iter()
                    .find(|pin| {
                        pin.owner_character_id == character_id
                            && pin.case_id == case_id
                            && pin.case_site_id == action.required_case_site_id
                    })
                    .ok_or("projected action travel had no exact owner-scoped site pin")?;
                let site_id = pin.case_site_id.clone();
                let distance_m = pin.distance_m;
                let readiness = if self
                    .party_for(character_id)?
                    .current_settlement_id
                    .is_some()
                {
                    self.prepare_party_for_departure(party_id, character_id, agent)?
                } else {
                    self.validate_party_departure_readiness(party_id)
                };
                if let DepartureReadiness::Deferred(reason) = readiness {
                    self.event(
                        agent,
                        CoreLoopEventKind::QuestSuppressed,
                        format!(
                            "generated_case={};reason={reason};phase=survival_readiness",
                            bounded_event_field(case_id)
                        ),
                    );
                    return Ok(false);
                }
                if matches!(
                    self.provision_case_site_journey(
                        party_id,
                        character_id,
                        agent,
                        case_id,
                        distance_m,
                    )?,
                    TravelProvisionDecision::Deferred(_)
                ) {
                    return Ok(false);
                }
                let result = reducer_call!(self, "travel_to_generated_case_site", |cb| self
                    .connection
                    .reducers
                    .travel_to_case_site_then(
                        character_id,
                        CaseSiteId {
                            value: site_id.clone(),
                        },
                        cb,
                    ));
                self.call(result)?;
                self.event(
                    agent,
                    CoreLoopEventKind::Travel,
                    format!("generated_case={case_id};outbound={site_id}"),
                );
                let journey_outcome = self.travel_camps(party_id)?;
                if journey_outcome != JourneyTravelOutcome::Completed {
                    return Ok(false);
                }
                if self.generated_traveled_cases.insert(funnel_key) {
                    self.metrics.generated_case_site_traveled += 1;
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            if let Some((action, wait_minutes)) = actions.iter().find_map(|action| {
                projected_investigation_wait_minutes(
                    &action.unavailable_reason_code,
                    action.wait_minutes,
                )
                .map(|wait_minutes| (action, wait_minutes))
            }) {
                if !self.wait_for_generated_investigation_window(
                    party_id,
                    character_id,
                    agent,
                    case_id,
                    &action.action_id,
                    wait_minutes,
                )? {
                    return Ok(false);
                }
                // Rest may clip at a disease/injury boundary or synchronize a
                // lagging member. Re-read the projected action and its exact
                // expected version before attempting it.
                continue;
            }
            if at_settlement {
                let witness = self
                    .connection
                    .db
                    .backend_investigation_leads()
                    .iter()
                    .filter(|row| {
                        row.owner_character_id == character_id
                            && row.case_id == case_id
                            && !row.witness_name.is_empty()
                            && row.corrected_by.is_empty()
                    })
                    .max_by_key(|row| (row.recorded_at, row.lead_id.clone()));
                if let Some(witness) = witness
                    && self.try_generated_dialogue_topic(
                        character_id,
                        agent,
                        cycle,
                        case_id,
                        subject,
                        &["referred-testimony"],
                        Some(&witness.witness_name),
                        Some(if witness.current_learned_location.is_empty() {
                            &witness.expected_location
                        } else {
                            &witness.current_learned_location
                        }),
                    )?
                {
                    continue;
                }
                if self.try_generated_dialogue_topic(
                    character_id,
                    agent,
                    cycle,
                    case_id,
                    subject,
                    &["return-recovered-property", "expose-false-account"],
                    None,
                    None,
                )? {
                    continue;
                }
            }
            let party = self.party_for(character_id)?;
            let occupied_site_id = party.current_case_site_id.map(|site| site.value);
            let pin = occupied_site_id.as_deref().and_then(|occupied_site_id| {
                self.connection
                    .db
                    .backend_case_site_pins()
                    .iter()
                    .find(|pin| {
                        occupied_case_pin_matches(
                            character_id,
                            case_id,
                            occupied_site_id,
                            pin.owner_character_id,
                            &pin.case_id,
                            &pin.case_site_id,
                        )
                    })
            });
            if let Some(pin) = pin {
                if pin.combat_available {
                    let defeat_key = (character_id, case_id.to_owned());
                    let combat_fingerprint = self.public_party_combat_fingerprint(party_id);
                    if generated_defeat_decision(
                        true,
                        self.generated_defeat_fingerprints.get(&defeat_key),
                        &combat_fingerprint,
                    ) == GeneratedDefeatDecision::SuppressUnchanged
                    {
                        self.event(
                            agent,
                            CoreLoopEventKind::QuestSuppressed,
                            format!(
                                "generated_case={};reason=unchanged_defeated_threat;public_fingerprint_members={}",
                                bounded_event_field(case_id),
                                combat_fingerprint.members.len(),
                            ),
                        );
                        let settlement_id = pin.origin_settlement_id.clone();
                        let result =
                            reducer_call!(self, "generated_unchanged_defeat_retreat", |cb| self
                                .connection
                                .reducers
                                .travel_to_settlement_then(
                                    character_id,
                                    settlement_id.clone(),
                                    cb
                                ));
                        self.call(result)?;
                        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                            return Ok(false);
                        }
                        return Ok(false);
                    }
                    let mission_id = format!(
                        "mission:sim-generated:{party_id}:{}:{}",
                        pin.case_site_id, self.sequence
                    );
                    let battle_id = format!("battle:{mission_id}");
                    let result = reducer_call!(self, "autoresolve_generated_mission", |cb| self
                        .connection
                        .reducers
                        .autoresolve_mission_then(character_id, mission_id.clone(), cb));
                    self.call(result)?;
                    let public_binding =
                        self.connection.db.backend_case_battles().iter().any(|row| {
                            row.owner_character_id == character_id
                                && row.public_case_id == case_id
                                && row.party_id == party_id
                                && row.battle_id == battle_id
                                && row.mission_id == mission_id
                                && row.case_site_id.value == pin.case_site_id
                        });
                    if !public_binding {
                        return Err(
                            "generated autoresolve had no public case-battle binding".into()
                        );
                    }
                    if self
                        .connection
                        .db
                        .battle_result()
                        .iter()
                        .any(|row| row.battle_id == battle_id)
                    {
                        self.event(
                            agent,
                            CoreLoopEventKind::AutoresolveVictory,
                            format!("generated_case={case_id};battle={battle_id}"),
                        );
                    } else {
                        self.metrics.defeats += 1;
                        self.generated_defeat_fingerprints
                            .insert(defeat_key, combat_fingerprint);
                        self.event(
                            agent,
                            CoreLoopEventKind::AutoresolveDefeat,
                            format!("generated_case={case_id};battle={battle_id}"),
                        );
                        let settlement_id = pin.origin_settlement_id.clone();
                        let result =
                            reducer_call!(self, "generated_defeat_retreat_to_settlement", |cb| {
                                self.connection.reducers.travel_to_settlement_then(
                                    character_id,
                                    settlement_id.clone(),
                                    cb,
                                )
                            });
                        self.call(result)?;
                        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                            return Ok(false);
                        }
                        self.observe_deaths();
                        if let Some((current_leader, _)) = self.current_leader(party_id) {
                            for party_agent in self.party_agents(current_leader)? {
                                self.ensure_medically_safe(party_agent)?;
                            }
                        }
                        return Ok(false);
                    }
                    self.generated_defeat_fingerprints
                        .remove(&(character_id, case_id.to_owned()));
                    self.observe_generated_case_transition(
                        agent,
                        character_id,
                        case_id,
                        subject,
                        true,
                    );
                    if self.generated_case_status(character_id, case_id).as_deref() != Some("open")
                    {
                        return self.return_completed_generated_party_to_origin(
                            party_id,
                            character_id,
                            case_id,
                        );
                    }
                    if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                        return Ok(false);
                    }
                    continue;
                }
                let settlement_id = pin.origin_settlement_id.clone();
                let result = reducer_call!(self, "return_from_generated_case_site", |cb| self
                    .connection
                    .reducers
                    .travel_to_settlement_then(character_id, settlement_id.clone(), cb,));
                self.call(result)?;
                self.event(
                    agent,
                    CoreLoopEventKind::Travel,
                    format!("generated_case={case_id};return={settlement_id}"),
                );
                if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                    return Ok(false);
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            return Ok(false);
        }
        Ok(false)
    }

    pub(super) fn turn_in_ready_direct_contract(
        &mut self,
        party_id: &str,
        leader: u64,
        leader_agent: u32,
        quest: &BackendContract,
    ) -> Result<(), String> {
        let party = self.party_by_id(party_id)?;
        let publicly_ready = party.current_settlement_id.as_deref()
            == Some(quest.settlement_id.as_str())
            && party.current_case_site_id.is_none()
            && party.camp_destination.is_none()
            && party.active_contract_id.as_deref() == Some(quest.id.as_str())
            && self
                .connection
                .db
                .backend_contracts()
                .iter()
                .any(|contract| {
                    contract.id == quest.id
                        && contract.status == ContractStatus::ReadyToReport
                        && contract.accepted_by.as_deref() == Some(party_id)
                });
        if !publicly_ready {
            self.event(
                leader_agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "quest={};reason=direct_contract_report_arrival_not_proven",
                    bounded_event_field(&quest.id)
                ),
            );
            return Ok(());
        }
        let result = reducer_call!(self, "interact_report_contract", |cb| self
            .connection
            .reducers
            .simulate_contract_issuer_interaction_then(
                leader,
                quest.id.clone(),
                ContractInteractionStage::Report,
                cb,
            ));
        self.call(result)?;
        let result = reducer_call!(self, "turn_in_quest", |cb| self
            .connection
            .reducers
            .report_contract_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.quests_completed += 1;
        self.metrics.direct_contracts_completed += 1;
        self.event(
            leader_agent,
            CoreLoopEventKind::TurnIn,
            format!(
                "party={};quest={}",
                bounded_event_field(&party_id),
                bounded_event_field(&quest.id)
            ),
        );
        Ok(())
    }
}
