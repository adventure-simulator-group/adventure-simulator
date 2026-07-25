use adventuresim_core::{
    local_problem::Scope,
    quest_catalog::{QUEST_CATALOG_DIGEST, catalog},
    quest_generation::{CATALOG_REVISION, GenerationContext, audit, generate, test_witnesses},
};

fn context(seed: u64, ordinal: u16) -> GenerationContext {
    GenerationContext {
        seed,
        observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
        observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
        settlement_id: "developer".into(),
        settlement_name: "Developer settlement".into(),
        scope: Scope::Settlement {
            settlement_id: "developer".into(),
        },
        ordinal,
        now_minute: 10_000,
        requested_family: None,
        witness_candidates: test_witnesses(),
    }
}
fn usage() -> ! {
    eprintln!(
        "questgen-check validate | explain <seed> [ordinal] | audit <count> | counterfactual <seed-a> <seed-b>"
    );
    std::process::exit(2)
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("validate") => {
            let catalog = catalog();
            for ordinal in 0..2 {
                generate(&context(0x187, ordinal)).unwrap_or_else(|e| panic!("{e:?}"));
            }
            println!(
                "catalog {CATALOG_REVISION}: valid ({} monsters, {} documents, digest {})",
                catalog.monsters().count(),
                catalog.documents.len(),
                QUEST_CATALOG_DIGEST
            );
        }
        Some("explain") => {
            let seed = args
                .get(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| usage());
            let ordinal = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &generate(&context(seed, ordinal)).unwrap_or_else(|e| panic!("{e:?}"))
                )
                .unwrap()
            );
        }
        Some("audit") => {
            let count = args
                .get(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| usage());
            println!("{}", serde_json::to_string_pretty(&audit(count)).unwrap());
        }
        Some("counterfactual") => {
            let a = args
                .get(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| usage());
            let b = args
                .get(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| usage());
            let left = generate(&context(a, 0)).unwrap();
            let right = generate(&context(b, 0)).unwrap();
            println!(
                "seed {a}: {:?} / {:?} / {:?}",
                left.family, left.cause, left.sites[0].kind
            );
            println!(
                "seed {b}: {:?} / {:?} / {:?}",
                right.family, right.cause, right.sites[0].kind
            );
        }
        _ => usage(),
    }
}
