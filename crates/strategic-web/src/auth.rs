//! Auth middleware stub
//!
//! TODO: Implement proper authentication with OIDC
//!
//! This module provides placeholder auth middleware.
//! In production, integrate with your identity provider.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// Placeholder session data
#[derive(Clone, Debug)]
pub struct Session {
    pub user_id: Option<String>,
    pub character_id: Option<String>,
}

/// Auth middleware - currently a no-op placeholder
///
/// TODO: Implement proper authentication:
/// - Verify JWT/session tokens
/// - Load user data from identity provider
/// - Reject unauthorized requests for protected routes
pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    // Placeholder: allow all requests
    // In production:
    // 1. Extract auth token from cookie/header
    // 2. Validate token with identity provider
    // 3. Attach session data to request extensions
    // 4. Reject if auth required but not provided

    Ok(next.run(request).await)
}

/// Require authentication - rejects if not authenticated
///
/// TODO: Implement actual auth check
pub async fn require_auth(request: Request, next: Next) -> Result<Response, StatusCode> {
    // Placeholder: allow all requests
    // In production:
    // 1. Check for valid session in request extensions
    // 2. Return 401 Unauthorized if not authenticated
    // 3. Return 403 Forbidden if authenticated but lacking permissions

    Ok(next.run(request).await)
}
