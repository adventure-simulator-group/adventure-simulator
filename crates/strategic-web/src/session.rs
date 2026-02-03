//! Session management via cookies

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use std::future::Future;

pub const CHARACTER_COOKIE: &str = "character_id";

/// Current session extracted from cookies
#[derive(Clone, Debug, Default)]
pub struct Session {
    pub character_id: Option<String>,
}

impl Session {
    pub fn character_id(&self) -> Option<&str> {
        self.character_id.as_deref()
    }

    /// Returns true if a character is selected
    pub fn is_logged_in(&self) -> bool {
        self.character_id.is_some()
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
                .map(|c| c.value().to_string());

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
