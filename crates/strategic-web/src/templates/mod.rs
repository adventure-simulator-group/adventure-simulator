//! Maud HTML templates

use std::sync::atomic::{AtomicU64, Ordering};

/// Fresh opaque request identity for one rendered form. A browser transport
/// retry resubmits the same hidden value; a rerender receives a new value.
pub(crate) fn fresh_request_token(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos:x}-{sequence:x}")
}

mod components;
mod inventory_browser;
mod layout;

pub mod challenge;
pub mod character;
pub mod investigation;
pub mod mission;
pub mod quest;
pub mod recruitment;
pub mod settlement;

pub use components::*;
pub use layout::*;

#[cfg(test)]
mod request_token_tests {
    use super::fresh_request_token;

    #[test]
    fn each_rendered_attempt_receives_a_fresh_opaque_token() {
        let first = fresh_request_token("treatment");
        let second = fresh_request_token("treatment");
        assert_ne!(first, second);
        assert!(first.starts_with("treatment-"));
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        );
    }
}
