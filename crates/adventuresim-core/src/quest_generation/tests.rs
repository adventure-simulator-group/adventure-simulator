#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/testimony.rs");
    include!("tests/routes_and_evidence.rs");
    include!("tests/solver_and_generation.rs");
    include!("tests/projection_and_limits.rs");
}
