//! Pure, deterministic mechanics for language-gated book study.

use crate::item_catalog_schema::{Book, BookTarget};
use crate::skill::Skill;

pub const READABLE_WRITTEN_RANK: f32 = 1.0;

pub const fn rank_band(book: &Book) -> (u8, u8) {
    (book.quality.saturating_sub(1), book.quality)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundedBookGain {
    pub accepted_effective_hours: f32,
    pub unused_real_hours: f32,
}

/// Return the authored ceiling for a target family.
pub fn maximum_book_rank(target: &BookTarget) -> Option<u8> {
    Some(match target {
        BookTarget::Written { .. } | BookTarget::Religion { .. } | BookTarget::Bestiary { .. } => 5,
        BookTarget::Skill { skill } if matches!(skill.as_str(), "physiology" | "herbalism") => 4,
        BookTarget::Terrain { .. } => 2,
        BookTarget::Skill { skill }
            if matches!(
                skill.as_str(),
                "surgery" | "cooking" | "tailoring" | "smithing" | "command" | "charm"
            ) =>
        {
            2
        }
        BookTarget::Skill { skill }
            if matches!(
                skill.as_str(),
                "polearm"
                    | "axe"
                    | "bludgeon"
                    | "sword"
                    | "knife"
                    | "bow"
                    | "crossbow"
                    | "firearm"
                    | "throw"
                    | "dodge"
                    | "block"
                    | "balance"
                    | "stealth"
            ) =>
        {
            1
        }
        _ => return None,
    })
}

pub fn book_shape_is_valid(book: &Book) -> bool {
    (1..=maximum_book_rank(&book.target).unwrap_or(0)).contains(&book.quality)
        && match &book.target {
            BookTarget::Terrain { terrain } => matches!(
                terrain.as_str(),
                "plains" | "forest" | "hills" | "wetlands" | "urban" | "snow"
            ),
            _ => true,
        }
}

pub fn written_rank(hours: f32, intelligence: f32) -> f32 {
    (hours.max(0.0) / 1_000.0).min(intelligence.clamp(0.0, 5.0))
}

pub fn reading_rate(medium_effective_rank: f32) -> f32 {
    (medium_effective_rank / 5.0).clamp(0.0, 1.0)
}

/// Award direct effective hours without applying the target aptitude's
/// training-speed multiplier. Aptitude remains an effective-rank cap.
///
/// `hours_for_rank` must describe the target leaf's direct-hours curve.
#[expect(
    clippy::too_many_arguments,
    reason = "the training boundary names each independent rule input explicitly"
)]
pub fn apply_bounded_book_training(
    direct_hours: &mut f32,
    effective_target_rank: f32,
    target_aptitude: f32,
    lower_rank: u8,
    upper_rank: u8,
    real_reading_hours: f32,
    medium_effective_rank: f32,
    hours_for_rank: impl Fn(f32) -> f32,
    project_effective_hours: impl Fn(f32) -> f32,
) -> BoundedBookGain {
    if !real_reading_hours.is_finite()
        || real_reading_hours <= 0.0
        || !effective_target_rank.is_finite()
        || !target_aptitude.is_finite()
        || !medium_effective_rank.is_finite()
        || medium_effective_rank < READABLE_WRITTEN_RANK
        || effective_target_rank + 0.000_01 < f32::from(lower_rank)
        || effective_target_rank >= f32::from(upper_rank)
        || target_aptitude <= f32::from(lower_rank)
    {
        return BoundedBookGain {
            unused_real_hours: real_reading_hours.max(0.0),
            ..Default::default()
        };
    }
    let rate = reading_rate(medium_effective_rank);
    let upper = f32::from(upper_rank).min(target_aptitude.clamp(0.0, 5.0));
    let effective_ceiling = hours_for_rank(upper);
    let current = direct_hours.max(0.0);
    if rate <= 0.0 || project_effective_hours(current) > effective_ceiling {
        return BoundedBookGain {
            unused_real_hours: real_reading_hours,
            ..Default::default()
        };
    }
    let desired = (current + real_reading_hours * rate).min(effective_ceiling.max(current));
    let accepted_direct = if project_effective_hours(desired) <= effective_ceiling {
        desired
    } else {
        let mut low = current;
        let mut high = desired;
        for _ in 0..64 {
            let middle = low + (high - low) * 0.5;
            if project_effective_hours(middle) <= effective_ceiling {
                low = middle;
            } else {
                high = middle;
            }
        }
        low
    };
    let accepted = (accepted_direct - current).max(0.0);
    *direct_hours = current + accepted;
    BoundedBookGain {
        accepted_effective_hours: accepted,
        unused_real_hours: if rate > 0.0 {
            (real_reading_hours - accepted / rate).max(0.0)
        } else {
            real_reading_hours
        },
    }
}

/// Integrate Written study with its changing correlated medium literacy and
/// its direct-zero target gate.
#[expect(
    clippy::too_many_arguments,
    reason = "the written-study boundary names each independent rule input explicitly"
)]
pub fn apply_written_book_training(
    hours: &mut adventuresim_world_schema::WrittenLanguageHours,
    medium: adventuresim_world_schema::WrittenLanguage,
    target: adventuresim_world_schema::WrittenLanguage,
    effective_target_rank: f32,
    intelligence: f32,
    lower_rank: u8,
    upper_rank: u8,
    real_reading_hours: f32,
) -> BoundedBookGain {
    let medium_effective = hours.effective(medium);
    let medium_rank = written_rank(medium_effective, intelligence);
    if !real_reading_hours.is_finite()
        || real_reading_hours <= 0.0
        || !effective_target_rank.is_finite()
        || !intelligence.is_finite()
        || medium_rank < READABLE_WRITTEN_RANK
        || effective_target_rank + 0.000_01 < f32::from(lower_rank)
        || effective_target_rank >= f32::from(upper_rank)
        || intelligence <= f32::from(lower_rank)
    {
        return BoundedBookGain {
            unused_real_hours: real_reading_hours.max(0.0),
            ..Default::default()
        };
    }
    let upper = f32::from(upper_rank).min(intelligence.clamp(0.0, 5.0));
    let effective_ceiling = upper * 1_000.0;
    let current = hours.direct(target).max(0.0);
    let baseline = *hours;
    let target_effective = |direct: f32| {
        let mut projected = baseline;
        *projected.direct_mut(target) = direct;
        projected.effective(target)
    };
    if target_effective(current) > effective_ceiling {
        return BoundedBookGain {
            unused_real_hours: real_reading_hours,
            ..Default::default()
        };
    }

    let coefficient = medium.correlation(target);
    let base = adventuresim_world_schema::WrittenLanguage::ALL
        .into_iter()
        .filter(|source| *source != target)
        .map(|source| hours.direct(source).max(0.0) * medium.correlation(source))
        .sum::<f32>();
    let medium_cap = intelligence.clamp(0.0, 5.0) * 1_000.0;
    let time_to_direct = |end: f32| {
        if end <= current {
            return 0.0;
        }
        let start_effective = (base + coefficient * current).min(medium_cap);
        if coefficient <= f32::EPSILON || start_effective >= medium_cap {
            return (end - current) * 5_000.0 / start_effective;
        }
        let saturation = ((medium_cap - base) / coefficient).max(current);
        let curved_end = end.min(saturation);
        let curved_delta = coefficient * (curved_end - current) / start_effective;
        let mut elapsed = 5_000.0 / coefficient * curved_delta.ln_1p();
        if end > saturation {
            elapsed += (end - saturation) * 5_000.0 / medium_cap;
        }
        elapsed
    };
    let direct_after_time = |elapsed: f32| {
        let start_effective = (base + coefficient * current).min(medium_cap);
        if coefficient <= f32::EPSILON || start_effective >= medium_cap {
            return current + elapsed * start_effective / 5_000.0;
        }
        let saturation = ((medium_cap - base) / coefficient).max(current);
        let curved_time = time_to_direct(saturation);
        if elapsed <= curved_time {
            current + start_effective / coefficient * (coefficient * elapsed / 5_000.0).exp_m1()
        } else {
            saturation + (elapsed - curved_time) * medium_cap / 5_000.0
        }
    };
    let desired = direct_after_time(real_reading_hours).min(effective_ceiling.max(current));
    let accepted_direct = if target_effective(desired) <= effective_ceiling {
        desired
    } else {
        let mut low = current;
        let mut high = desired;
        for _ in 0..64 {
            let middle = low + (high - low) * 0.5;
            if target_effective(middle) <= effective_ceiling {
                low = middle;
            } else {
                high = middle;
            }
        }
        low
    };
    let accepted = (accepted_direct - current).max(0.0);
    *hours.direct_mut(target) = current + accepted;
    let used_real_hours = time_to_direct(current + accepted);
    BoundedBookGain {
        accepted_effective_hours: accepted,
        unused_real_hours: (real_reading_hours - used_real_hours).max(0.0),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookCandidate<'a> {
    pub item_id: &'a str,
    pub book: &'a Book,
    pub personal: bool,
}

/// Personal books are considered before bookstore books. Duplicate inventory
/// copies are naturally collapsed by stable item ID.
pub fn select_candidate<'a>(
    candidates: impl IntoIterator<Item = BookCandidate<'a>>,
    useful: impl Fn(&Book) -> bool,
) -> Option<BookCandidate<'a>> {
    let mut values = candidates
        .into_iter()
        .filter(|candidate| useful(candidate.book))
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .personal
            .cmp(&left.personal)
            .then(left.book.quality.cmp(&right.book.quality))
            .then(left.item_id.cmp(right.item_id))
    });
    values.dedup_by(|left, right| left.item_id == right.item_id);
    values.into_iter().next()
}

pub fn ordinary_skill(target: &BookTarget) -> Option<Skill> {
    let BookTarget::Skill { skill } = target else {
        return None;
    };
    Some(match skill.as_str() {
        "physiology" => Skill::Physiology,
        "herbalism" => Skill::Herbalism,
        "surgery" => Skill::Surgery,
        "cooking" => Skill::Cooking,
        "tailoring" => Skill::Tailoring,
        "smithing" => Skill::Smithing,
        "command" => Skill::Command,
        "charm" => Skill::Charm,
        "polearm" => Skill::Polearm,
        "axe" => Skill::Axe,
        "bludgeon" => Skill::Bludgeon,
        "sword" => Skill::Sword,
        "knife" => Skill::Knife,
        "bow" => Skill::Bow,
        "crossbow" => Skill::Crossbow,
        "firearm" => Skill::Firearm,
        "throw" => Skill::Throw,
        "dodge" => Skill::Dodge,
        "block" => Skill::Block,
        "balance" => Skill::Balance,
        "stealth" => Skill::Stealth,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_catalog_schema::{Book, BookTarget};
    use adventuresim_world_schema::WrittenLanguage;

    fn book(lower: u8) -> Book {
        Book {
            medium: WrittenLanguage::German,
            target: BookTarget::Skill {
                skill: "cooking".into(),
            },
            quality: lower + 1,
            settlement_allowlist: Vec::new(),
        }
    }

    #[test]
    fn bounded_gain_requires_exact_prerequisite_clips_and_uses_literacy_rate() {
        let curve = |rank: f32| rank * 100.0;
        let mut hours = 100.0;
        let blocked =
            apply_bounded_book_training(&mut hours, 1.0, 5.0, 2, 3, 10.0, 5.0, curve, |d| d);
        assert_eq!(blocked.accepted_effective_hours, 0.0);
        let gain =
            apply_bounded_book_training(&mut hours, 1.0, 5.0, 1, 2, 1_000.0, 2.5, curve, |d| d);
        assert_eq!(hours, 200.0);
        assert_eq!(gain.accepted_effective_hours, 100.0);
        assert_eq!(gain.unused_real_hours, 800.0);
    }

    #[test]
    fn candidate_order_is_personal_then_band_then_stable_id_and_dedupes() {
        let one = book(0);
        let two = book(1);
        let selected = select_candidate(
            [
                BookCandidate {
                    item_id: "b",
                    book: &one,
                    personal: false,
                },
                BookCandidate {
                    item_id: "a",
                    book: &one,
                    personal: false,
                },
                BookCandidate {
                    item_id: "z",
                    book: &two,
                    personal: true,
                },
                BookCandidate {
                    item_id: "z",
                    book: &two,
                    personal: true,
                },
            ],
            |_| true,
        )
        .unwrap();
        assert_eq!(selected.item_id, "z");
    }

    #[test]
    fn correlated_effective_rank_counts_toward_the_books_upper_boundary() {
        let projection = |direct: f32| direct + 80.0_f32.min(direct);
        let mut bulk = 60.0;
        apply_bounded_book_training(
            &mut bulk,
            1.2,
            5.0,
            1,
            2,
            100.0,
            5.0,
            |rank| rank * 100.0,
            projection,
        );
        let mut partitioned = 60.0;
        for _ in 0..4 {
            let rank = projection(partitioned) / 100.0;
            apply_bounded_book_training(
                &mut partitioned,
                rank,
                5.0,
                1,
                2,
                25.0,
                5.0,
                |rank| rank * 100.0,
                projection,
            );
        }
        assert!((bulk - 120.0).abs() < 0.001);
        assert!((bulk - partitioned).abs() < 0.001);
        assert!(projection(bulk) <= 200.001);
    }

    #[test]
    fn written_bridges_are_bulk_chunk_equivalent_and_never_overshoot() {
        for target in [WrittenLanguage::Low, WrittenLanguage::Latin] {
            let initial = adventuresim_world_schema::WrittenLanguageHours {
                german: 1_000.0,
                ..Default::default()
            };
            let mut bulk_short = initial;
            apply_written_book_training(
                &mut bulk_short,
                WrittenLanguage::German,
                target,
                0.0,
                5.0,
                0,
                1,
                400.0,
            );
            let mut partitioned_short = initial;
            for _ in 0..4 {
                let rank = written_rank(partitioned_short.effective(target), 5.0);
                apply_written_book_training(
                    &mut partitioned_short,
                    WrittenLanguage::German,
                    target,
                    rank,
                    5.0,
                    0,
                    1,
                    100.0,
                );
            }
            assert!(
                (bulk_short.direct(target) - partitioned_short.direct(target)).abs() < 0.001,
                "{target:?}"
            );

            let mut bulk = initial;
            apply_written_book_training(
                &mut bulk,
                WrittenLanguage::German,
                target,
                0.0,
                5.0,
                0,
                1,
                10_000.0,
            );
            assert!(bulk.effective(target) <= 1_000.001, "{target:?}");
            assert!(
                (bulk.effective(target) - 1_000.0).abs() < 0.001,
                "{target:?}"
            );
        }
    }

    #[test]
    fn direct_zero_correlation_gate_cannot_jump_over_a_band() {
        let mut direct = 0.0;
        let gain = apply_bounded_book_training(
            &mut direct,
            0.0,
            5.0,
            0,
            1,
            100.0,
            5.0,
            |rank| rank * 100.0,
            |candidate| {
                if candidate <= 0.0 {
                    0.0
                } else {
                    candidate + 150.0
                }
            },
        );
        assert_eq!(direct, 0.0);
        assert_eq!(gain.accepted_effective_hours, 0.0);
        assert_eq!(gain.unused_real_hours, 100.0);
    }

    #[test]
    fn authored_family_caps_are_exact() {
        assert_eq!(
            maximum_book_rank(&BookTarget::Written {
                language: WrittenLanguage::Latin
            }),
            Some(5)
        );
        assert_eq!(
            maximum_book_rank(&BookTarget::Skill {
                skill: "physiology".into()
            }),
            Some(4)
        );
        assert_eq!(
            maximum_book_rank(&BookTarget::Skill {
                skill: "command".into()
            }),
            Some(2)
        );
        assert_eq!(
            maximum_book_rank(&BookTarget::Skill {
                skill: "sword".into()
            }),
            Some(1)
        );
        let cooking = book(1);
        assert_eq!(cooking.quality, 2);
        assert_eq!(rank_band(&cooking), (1, 2));
        assert!(book_shape_is_valid(&cooking));
        let mut excessive = cooking;
        excessive.quality = 3;
        assert!(!book_shape_is_valid(&excessive));
    }
}
