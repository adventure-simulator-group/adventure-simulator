//! Signed opaque browser sessions backed by server-side character grants.

use std::{
    fmt,
    future::Future,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{routes::AppState, spacetimedb::sql_string_literal};

pub const SESSION_COOKIE: &str = "adventuresim_session";
const SESSION_VERSION: &str = "v2";
const SESSION_ID_BYTES: usize = 32;
const OWNER_DOMAIN: &[u8] = b"adventuresim/browser-session-owner/v1\0";
const SESSION_LIFETIME_SECONDS: u64 = 60 * 60 * 24 * 30;
const SESSION_FUTURE_SKEW_SECONDS: u64 = 5 * 60;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct SessionCodec {
    secret: [u8; 32],
    secure_cookie: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionCodecError {
    #[error("STRATEGIC_SESSION_SECRET must be exactly 32 base64url bytes")]
    InvalidSecret,
    #[error("secure random session generation failed")]
    Random,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuedSession {
    pub token: String,
    pub owner_key: String,
}

impl SessionCodec {
    pub fn from_base64url(secret: &str, secure_cookie: bool) -> Result<Self, SessionCodecError> {
        let decoded = URL_SAFE_NO_PAD
            .decode(secret)
            .map_err(|_| SessionCodecError::InvalidSecret)?;
        let secret: [u8; 32] = decoded
            .try_into()
            .map_err(|_| SessionCodecError::InvalidSecret)?;
        Ok(Self {
            secret,
            secure_cookie,
        })
    }

    pub fn issue(&self) -> Result<IssuedSession, SessionCodecError> {
        let mut id = [0u8; SESSION_ID_BYTES];
        getrandom::fill(&mut id).map_err(|_| SessionCodecError::Random)?;
        let issued_at = unix_seconds();
        Ok(self.issue_at(id, issued_at))
    }

    fn issue_at(&self, id: [u8; SESSION_ID_BYTES], issued_at: u64) -> IssuedSession {
        let encoded_id = URL_SAFE_NO_PAD.encode(id);
        let signed = format!("{SESSION_VERSION}.{encoded_id}.{issued_at}");
        let signature = self.sign(signed.as_bytes());
        IssuedSession {
            token: format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)),
            owner_key: owner_key(&id),
        }
    }

    /// Verify the HMAC before accepting the opaque ID. `verify_slice` performs
    /// a constant-time MAC comparison.
    pub fn verify(&self, token: &str) -> Option<String> {
        self.verify_at(token, unix_seconds())
    }

    fn verify_at(&self, token: &str, now: u64) -> Option<String> {
        let mut parts = token.split('.');
        let version = parts.next()?;
        let encoded_id = parts.next()?;
        let encoded_issued_at = parts.next()?;
        let encoded_signature = parts.next()?;
        if version != SESSION_VERSION || parts.next().is_some() {
            return None;
        }
        let id = URL_SAFE_NO_PAD.decode(encoded_id).ok()?;
        let id: [u8; SESSION_ID_BYTES] = id.try_into().ok()?;
        let issued_at = encoded_issued_at.parse::<u64>().ok()?;
        let signature = URL_SAFE_NO_PAD.decode(encoded_signature).ok()?;
        let signed = format!("{version}.{encoded_id}.{encoded_issued_at}");
        let mut mac = HmacSha256::new_from_slice(&self.secret).ok()?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&signature).ok()?;
        if issued_at > now.saturating_add(SESSION_FUTURE_SKEW_SECONDS)
            || now.saturating_sub(issued_at) > SESSION_LIFETIME_SECONDS
        {
            return None;
        }
        Some(owner_key(&id))
    }

    fn sign(&self, value: &[u8]) -> [u8; 32] {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC-SHA256 accepts 32-byte keys");
        mac.update(value);
        mac.finalize().into_bytes().into()
    }

    pub fn set_cookie_value(&self, token: &str) -> String {
        let secure = if self.secure_cookie { "; Secure" } else { "" };
        format!(
            "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
            SESSION_LIFETIME_SECONDS, secure
        )
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn owner_key(id: &[u8; SESSION_ID_BYTES]) -> String {
    let mut hash = Sha256::new();
    hash.update(OWNER_DOMAIN);
    hash.update(id);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

#[derive(Clone, Debug, Default)]
pub struct Session {
    owner_key: Option<String>,
    token: Option<String>,
    character_id: Option<CharacterId>,
    character_ids: Vec<CharacterId>,
}

impl Session {
    pub fn character_id_u64(&self) -> Option<u64> {
        self.character_id.map(CharacterId::get)
    }

    pub fn character_ids(&self) -> Vec<u64> {
        self.character_ids
            .iter()
            .copied()
            .map(CharacterId::get)
            .collect()
    }

    pub(crate) fn owner_key(&self) -> Option<&str> {
        self.owner_key.as_deref()
    }

    pub(crate) fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize)]
struct BackendBrowserCharacterAccess {
    owner_key: String,
    character_id: u64,
    selected: bool,
}

pub struct SessionUnavailable;

impl IntoResponse for SessionUnavailable {
    fn into_response(self) -> Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Browser session data is unavailable",
        )
            .into_response()
    }
}

impl FromRequestParts<AppState> for Session {
    type Rejection = SessionUnavailable;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let jar = CookieJar::from_request_parts(parts, state)
                .await
                .unwrap_or_default();
            let Some(token) = jar
                .get(SESSION_COOKIE)
                .map(|cookie| cookie.value().to_owned())
            else {
                return Ok(Session::default());
            };
            let Some(owner_key) = state.session_codec.verify(&token) else {
                // A malformed, expired, or forged cookie has no authority.
                return Ok(Session::default());
            };
            let rows = state
                .db
                .query::<BackendBrowserCharacterAccess>(&format!(
                    "SELECT * FROM backend_browser_character_access WHERE owner_key = {}",
                    sql_string_literal(&owner_key)
                ))
                .await
                .map_err(|error| {
                    tracing::error!(%error, "failed to resolve browser character grants");
                    SessionUnavailable
                })?;
            if rows.iter().any(|row| row.owner_key != owner_key) {
                tracing::error!("browser character access projection returned a foreign owner");
                return Err(SessionUnavailable);
            }
            let character_id = rows
                .iter()
                .find(|row| row.selected)
                .map(|row| CharacterId(row.character_id));
            let character_ids = rows
                .into_iter()
                .map(|row| CharacterId(row.character_id))
                .collect();
            Ok(Session {
                owner_key: Some(owner_key),
                token: Some(token),
                character_id,
                character_ids,
            })
        }
    }
}

pub fn redirect_with_session_cookie(
    codec: &SessionCodec,
    token: Option<&str>,
    redirect_to: &str,
) -> Response {
    let mut headers = HeaderMap::new();
    if let Some(token) = token
        && let Ok(cookie) = HeaderValue::from_str(&codec.set_cookie_value(token))
    {
        headers.append(header::SET_COOKIE, cookie);
    }
    (headers, Redirect::to(redirect_to)).into_response()
}

/// Clearing a character selection keeps the opaque browser identity and all
/// grants. The caller must clear the server-side selection first.
pub fn clear_character_cookie(redirect_to: &str) -> Response {
    Redirect::to(redirect_to).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_codec(secret_byte: u8, secure: bool) -> SessionCodec {
        SessionCodec::from_base64url(&URL_SAFE_NO_PAD.encode([secret_byte; 32]), secure).unwrap()
    }

    #[test]
    fn signed_tokens_round_trip_without_character_ids() {
        let codec = test_codec(7, false);
        let issued = codec.issue().unwrap();
        assert_eq!(codec.verify(&issued.token), Some(issued.owner_key));
        assert!(!issued.token.contains("character"));
        assert_eq!(issued.token.split('.').count(), 4);
    }

    #[test]
    fn tampering_and_different_secrets_are_rejected() {
        let codec = test_codec(7, false);
        let issued = codec.issue().unwrap();
        let mut tampered = issued.token.clone().into_bytes();
        let last = tampered.len() - 1;
        tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
        assert!(
            codec
                .verify(std::str::from_utf8(&tampered).unwrap())
                .is_none()
        );
        assert!(test_codec(8, false).verify(&issued.token).is_none());
    }

    #[test]
    fn secret_length_and_cookie_flags_fail_closed() {
        assert!(SessionCodec::from_base64url(&URL_SAFE_NO_PAD.encode([0u8; 31]), true).is_err());
        let issued = test_codec(1, true).issue().unwrap();
        let cookie = test_codec(1, true).set_cookie_value(&issued.token);
        assert!(cookie.starts_with("adventuresim_session=v2."));
        assert!(cookie.contains("; HttpOnly;"));
        assert!(cookie.contains("; SameSite=Lax;"));
        assert!(cookie.ends_with("; Secure"));
        assert!(
            !test_codec(1, false)
                .set_cookie_value(&issued.token)
                .contains("; Secure")
        );
    }

    #[test]
    fn sessions_expire_server_side_and_reject_excessive_future_skew() {
        let codec = test_codec(7, false);
        let issued = codec.issue_at([9; SESSION_ID_BYTES], 10_000);
        assert_eq!(
            codec.verify_at(&issued.token, 10_000 + SESSION_LIFETIME_SECONDS),
            Some(issued.owner_key.clone())
        );
        assert!(
            codec
                .verify_at(&issued.token, 10_001 + SESSION_LIFETIME_SECONDS)
                .is_none()
        );
        assert!(
            codec
                .verify_at(&issued.token, 10_000 - SESSION_FUTURE_SKEW_SECONDS)
                .is_some()
        );
        assert!(
            codec
                .verify_at(&issued.token, 9_999 - SESSION_FUTURE_SKEW_SECONDS)
                .is_none()
        );
    }
}
