/// Equal-value historical denominations used by the northern-German 1544
/// setting. Exchange-rate gameplay intentionally remains deferred.
pub const CURRENCY_IDS: [&str; 6] = [
    "rhenish_gulden",
    "lubeck_mark",
    "hamburg_mark",
    "saxon_thaler",
    "brandenburg_groschen",
    "danish_mark",
];

pub fn is_currency_id(item_id: &str) -> bool {
    CURRENCY_IDS.contains(&item_id)
}

pub fn currency_name(item_id: &str) -> Option<&'static str> {
    Some(match item_id {
        "rhenish_gulden" => "Rhenish gulden",
        "lubeck_mark" => "Lübeck mark",
        "hamburg_mark" => "Hamburg mark",
        "saxon_thaler" => "Saxon thaler",
        "brandenburg_groschen" => "Brandenburg groschen",
        "danish_mark" => "Danish mark",
        _ => return None,
    })
}

/// Stable FNV-1a assignment. Unlike `DefaultHasher`, this mapping is stable
/// across Rust and toolchain releases.
pub fn assigned_currency_id(settlement_id: &str) -> &'static str {
    let hash = settlement_id
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    CURRENCY_IDS[hash as usize % CURRENCY_IDS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_and_assignment_are_total_and_stable() {
        assert!(CURRENCY_IDS.iter().all(|id| currency_name(id).is_some()));
        assert_eq!(
            assigned_currency_id("viabundus-123"),
            assigned_currency_id("viabundus-123")
        );
        assert!(is_currency_id(assigned_currency_id("viabundus-123")));
    }
}
