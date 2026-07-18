//! Shared validation for the one-run strategic simulation capability.

pub const SIM_BOOTSTRAP_TOKEN_ENV: &str = "ADVENTURESIM_SIM_BOOTSTRAP_TOKEN";
pub const SIM_BOOTSTRAP_TOKEN_HEX_LEN: usize = 64;

fn valid_compiled_token(token: &str) -> bool {
    token.len() == SIM_BOOTSTRAP_TOKEN_HEX_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Compare over the fixed expected length and fold the length mismatch into
/// the result. This avoids ordinary short-circuit string comparison while
/// keeping malformed caller input bounded.
fn constant_timeish_eq(expected: &[u8], presented: &[u8]) -> bool {
    let mut difference = expected.len() ^ presented.len();
    for (index, expected_byte) in expected.iter().enumerate() {
        difference |= usize::from(*expected_byte ^ presented.get(index).copied().unwrap_or(0));
    }
    difference == 0
}

/// A normal build passes `None` and can never authorize a claim, regardless of
/// database state or caller input.
pub fn simulation_bootstrap_authorized(compiled: Option<&str>, presented: &str) -> bool {
    let Some(expected) = compiled.filter(|token| valid_compiled_token(token)) else {
        return false;
    };
    constant_timeish_eq(expected.as_bytes(), presented.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_build_capability_is_unconditionally_disabled() {
        assert!(!simulation_bootstrap_authorized(
            None,
            &"a".repeat(SIM_BOOTSTRAP_TOKEN_HEX_LEN)
        ));
    }

    #[test]
    fn capability_requires_an_exact_high_entropy_shape_and_value() {
        let expected = "a".repeat(SIM_BOOTSTRAP_TOKEN_HEX_LEN);
        assert!(simulation_bootstrap_authorized(Some(&expected), &expected));
        assert!(!simulation_bootstrap_authorized(
            Some(&expected),
            &expected[..SIM_BOOTSTRAP_TOKEN_HEX_LEN - 1]
        ));
        assert!(!simulation_bootstrap_authorized(
            Some(&expected),
            &format!("{expected}a")
        ));
        assert!(!simulation_bootstrap_authorized(
            Some(&"z".repeat(SIM_BOOTSTRAP_TOKEN_HEX_LEN)),
            &"z".repeat(SIM_BOOTSTRAP_TOKEN_HEX_LEN)
        ));
    }
}
