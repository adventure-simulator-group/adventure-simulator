#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/policy_and_discovery.rs");
    include!("tests/travel.rs");
    include!("tests/recovery_and_cases.rs");
    include!("tests/failure_security.rs");
    include!("tests/configuration_and_medical.rs");
    include!("tests/control_policy.rs");
}
