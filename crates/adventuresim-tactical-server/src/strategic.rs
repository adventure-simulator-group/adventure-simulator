use std::time::Duration;

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::Resource;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone)]
pub struct StrategicApi {
    client: Client,
    base_url: String,
    mission_id: String,
}

impl StrategicApi {
    pub fn new(base_url: String, mission_id: String) -> std::result::Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|error| error.to_string())?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            mission_id,
        })
    }

    pub fn mark_ready(&self, addr: String, cert_digest: String) -> std::result::Result<(), String> {
        self.post_empty(
            &format!("/internal/missions/{}/ready", self.mission_id),
            &MissionReadyPayload { addr, cert_digest },
        )
    }

    pub fn load_player(&self, character_id: u64) -> std::result::Result<ConnectedPlayer, String> {
        let player = self.get_json(&format!(
            "/internal/missions/{}/players/{character_id}/loadout",
            self.mission_id
        ))?;
        self.post_empty(
            &format!(
                "/internal/missions/{}/players/{character_id}/enter",
                self.mission_id
            ),
            &EmptyPayload {},
        )?;
        Ok(player)
    }

    pub fn leave_player(&self, character_id: u64) -> std::result::Result<(), String> {
        self.post_empty(
            &format!(
                "/internal/missions/{}/players/{character_id}/leave",
                self.mission_id
            ),
            &EmptyPayload {},
        )
    }

    pub fn commit_result(&self, success: bool, xp_gained: i32) -> std::result::Result<(), String> {
        self.post_empty(
            &format!("/internal/missions/{}/result", self.mission_id),
            &MissionResultPayload {
                success,
                xp_gained: i64::from(xp_gained),
            },
        )
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> std::result::Result<T, String> {
        let response = self
            .client
            .get(self.url(path))
            .send()
            .map_err(|error| error.to_string())?;
        let response = Self::check_response(response)?;
        response.json().map_err(|error| error.to_string())
    }

    fn post_empty<T: Serialize>(&self, path: &str, payload: &T) -> std::result::Result<(), String> {
        let response = self
            .client
            .post(self.url(path))
            .json(payload)
            .send()
            .map_err(|error| error.to_string())?;
        Self::check_response(response).map(|_| ())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn check_response(
        response: reqwest::blocking::Response,
    ) -> std::result::Result<reqwest::blocking::Response, String> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().unwrap_or_default();
        Err(format!("{status}: {body}"))
    }
}

#[derive(Serialize)]
struct EmptyPayload {}

#[derive(Serialize)]
struct MissionReadyPayload {
    addr: String,
    cert_digest: String,
}

#[derive(Serialize)]
struct MissionResultPayload {
    success: bool,
    xp_gained: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectedPlayer {
    pub character: TacticalCharacter,
    pub items: Vec<ConnectedPlayerItem>,
    pub skills: Skills,
    pub stats: Stats,
    pub attrs: Attributes,
    pub limbs: Limbs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TacticalCharacter {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectedPlayerItem {
    pub quantity: u32,
    pub item: TacticalItem,
    pub equipped: Option<ItemSlot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TacticalItem {
    pub id: String,
    pub weight: f32,
    pub slot: ItemSlot,
    pub kind: ItemKind,
    pub accuracy: f32,
    pub block: f32,
    pub dodge: f32,
    pub coverage: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum ItemKind {
    Simple,
    Weapon,
    Armor,
    Shield,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum ItemSlot {
    None,
    LeftHolding,
    RightHolding,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    AnyHolding,
    AnyArm,
    AnyLeg,
}
