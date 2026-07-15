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
pub const THEME_COOKIE: &str = "theme";
pub const DEFAULT_THEME: Theme = Theme::FrakturNocturne;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    FrakturTexturina,
    FrakturNocturne,
    DarkArcanum,
    NorthernFrost,
    VerdantChronicle,
    ImperialCrimson,
}

impl Theme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FrakturTexturina => "fraktur-texturina",
            Self::FrakturNocturne => "fraktur-nocturne",
            Self::DarkArcanum => "dark-arcanum",
            Self::NorthernFrost => "northern-frost",
            Self::VerdantChronicle => "verdant-chronicle",
            Self::ImperialCrimson => "imperial-crimson",
        }
    }
}

impl FromStr for Theme {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fraktur-texturina" => Ok(Self::FrakturTexturina),
            "fraktur-nocturne" => Ok(Self::FrakturNocturne),
            "dark-arcanum" => Ok(Self::DarkArcanum),
            "northern-frost" => Ok(Self::NorthernFrost),
            "verdant-chronicle" => Ok(Self::VerdantChronicle),
            "imperial-crimson" => Ok(Self::ImperialCrimson),
            _ => Err(()),
        }
    }
}

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
    theme: Theme,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            character_id: None,
            theme: DEFAULT_THEME,
        }
    }
}

impl Session {
    pub fn character_id_u64(&self) -> Option<u64> {
        self.character_id.map(CharacterId::get)
    }

    pub fn theme(&self) -> &str {
        self.theme.as_str()
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
            let theme = jar
                .get(THEME_COOKIE)
                .and_then(|c| c.value().parse().ok())
                .unwrap_or(DEFAULT_THEME);

            Ok(Session {
                character_id,
                theme,
            })
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

/// Response that sets the theme cookie and redirects
pub fn set_theme_cookie(theme: Theme, redirect_to: &str) -> Response {
    let cookie = format!(
        "{}={}; Path=/; SameSite=Lax; Max-Age={}",
        THEME_COOKIE,
        theme.as_str(),
        60 * 60 * 24 * 365 // 1 year
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
    fn theme_is_parsed_once_into_a_closed_set() {
        assert_eq!("dark-arcanum".parse(), Ok(Theme::DarkArcanum));
        assert!("../../bad.css".parse::<Theme>().is_err());
    }
    #[test]
    fn character_id_rejects_non_numeric_cookie_values() {
        assert!("123".parse::<CharacterId>().is_ok());
        assert!("not-an-id".parse::<CharacterId>().is_err());
    }
}
