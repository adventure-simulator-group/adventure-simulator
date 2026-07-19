//! Session management via cookies

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use std::future::Future;
use std::{fmt, str::FromStr};

pub const CHARACTER_COOKIE: &str = "character_id";

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
}

impl Default for Session {
    fn default() -> Self {
        Self { character_id: None }
    }
}

impl Session {
    pub fn character_id_u64(&self) -> Option<u64> {
        self.character_id.map(CharacterId::get)
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
            Ok(Session { character_id })
        }
    }
}

/// Response that sets the character cookie and redirects
pub fn set_character_cookie(character_id: &str, redirect_to: &str) -> Response {
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        CHARACTER_COOKIE,
        character_id,
        60 * 60 * 24 * 30 // 30 days
    );

    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to(redirect_to),
    )
        .into_response()
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
}
