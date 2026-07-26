//! Session management via cookies

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, header, request::Parts},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use std::collections::HashSet;
use std::future::Future;
use std::{fmt, str::FromStr};

pub const CHARACTER_COOKIE: &str = "character_id";
pub const CHARACTER_ROSTER_COOKIE: &str = "character_roster";
const MAX_REMEMBERED_CHARACTERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterId(u64);

impl CharacterId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
impl fmt::Display for CharacterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for CharacterId {
    type Err = std::num::ParseIntError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

/// Current session extracted from cookies
#[derive(Clone, Debug)]
pub struct Session {
    character_id: Option<CharacterId>,
    character_ids: Vec<CharacterId>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            character_id: None,
            character_ids: Vec::new(),
        }
    }
}

impl Session {
    pub fn character_id_u64(&self) -> Option<u64> {
        self.character_id.map(CharacterId::get)
    }

    pub fn character_ids(&self) -> Vec<u64> {
        let mut ids = self
            .character_ids
            .iter()
            .copied()
            .map(CharacterId::get)
            .collect::<Vec<_>>();
        if let Some(current) = self.character_id_u64()
            && !ids.contains(&current)
        {
            ids.push(current);
        }
        ids
    }
}

/// Extractor for session from cookies
impl<S> FromRequestParts<S> for Session
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let jar = CookieJar::from_request_parts(parts, state)
                .await
                .unwrap_or_default();

            let character_id = jar
                .get(CHARACTER_COOKIE)
                .and_then(|c| c.value().parse().ok());
            let character_ids = jar
                .get(CHARACTER_ROSTER_COOKIE)
                .map_or_else(Vec::new, |cookie| parse_character_ids(cookie.value()));
            Ok(Session {
                character_id,
                character_ids,
            })
        }
    }
}

fn parse_character_ids(value: &str) -> Vec<CharacterId> {
    let mut seen = HashSet::new();
    value
        .split('.')
        .filter_map(|value| value.parse::<CharacterId>().ok())
        .filter(|id| seen.insert(id.get()))
        .take(MAX_REMEMBERED_CHARACTERS)
        .collect()
}

/// Response that selects and remembers a character before redirecting.
pub fn set_character_cookie(
    character_id: u64,
    remembered_character_ids: &[u64],
    redirect_to: &str,
) -> Response {
    let current_cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        CHARACTER_COOKIE,
        character_id,
        60 * 60 * 24 * 30 // 30 days
    );
    let mut roster = remembered_character_ids
        .iter()
        .copied()
        .filter(|id| *id != character_id)
        .take(MAX_REMEMBERED_CHARACTERS.saturating_sub(1))
        .collect::<Vec<_>>();
    roster.push(character_id);
    let roster_cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        CHARACTER_ROSTER_COOKIE,
        roster
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("."),
        60 * 60 * 24 * 30
    );
    let mut headers = HeaderMap::new();
    for cookie in [current_cookie, roster_cookie] {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            headers.append(header::SET_COOKIE, value);
        }
    }

    (headers, Redirect::to(redirect_to)).into_response()
}

/// Response that clears the character cookie and redirects
pub fn clear_character_cookie(redirect_to: &str) -> Response {
    let cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        CHARACTER_COOKIE
    );

    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to(redirect_to),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_id_rejects_non_numeric_cookie_values() {
        assert!("123".parse::<CharacterId>().is_ok());
        assert!("not-an-id".parse::<CharacterId>().is_err());
    }

    #[test]
    fn remembered_character_ids_are_ordered_unique_and_bounded() {
        let value = (1_u64..=40)
            .chain([2, 3])
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(".");
        let ids = parse_character_ids(&value)
            .into_iter()
            .map(CharacterId::get)
            .collect::<Vec<_>>();
        assert_eq!(ids, (1_u64..=32).collect::<Vec<_>>());
    }

    #[test]
    fn selecting_a_character_sets_current_and_roster_cookies() {
        let response = set_character_cookie(9, &[7, 8], "/");
        let cookies = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>();
        assert_eq!(cookies.len(), 2);
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.starts_with("character_id=9;"))
        );
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.starts_with("character_roster=7.8.9;"))
        );
    }
}
