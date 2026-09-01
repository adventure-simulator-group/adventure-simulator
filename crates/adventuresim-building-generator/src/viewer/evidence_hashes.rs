fn stable_evidence_hash(bytes: &[u8]) -> String {
    format!("fnv1a64:{:016x}", stable_u64(bytes))
}

fn stable_u64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn resolved_item_multiset_hash(items: impl IntoIterator<Item = (u64, u64)>) -> String {
    let mut items = items.into_iter().collect::<Vec<_>>();
    items.sort_unstable();
    stable_evidence_hash(&serde_json::to_vec(&items).expect("serialize resolved item fingerprints"))
}

fn source_revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn source_dirty_fingerprint() -> String {
    stable_evidence_hash(BUILDING_GENERATOR_SOURCE.concat().as_bytes())
}
