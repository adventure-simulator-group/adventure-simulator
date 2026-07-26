//! Stable language types and deterministic playable-world inference.

use serde::{Deserialize, Serialize};

use crate::{PLAYABLE_BOUNDS, coordinates_in_bounds};

pub const LANGUAGE_DISTRIBUTION_TOTAL_BP: u16 = 10_000;
pub const YIDDISH_LOCAL_GERMAN_FLUENCY: f32 = 0.8;
pub const ORAL_FLUENCY_HOURS: f32 = 5_000.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LanguageDescriptor {
    pub english: &'static str,
    pub native: &'static str,
    pub monogram: &'static str,
    pub germanic_style: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum OralLanguage {
    EastCentral,
    WestCentral,
    Low,
    Yiddish,
    Latin,
    Romani,
    Elven,
    Dwarfish,
}

impl OralLanguage {
    pub const ALL: [Self; 8] = [
        Self::EastCentral,
        Self::WestCentral,
        Self::Low,
        Self::Yiddish,
        Self::Latin,
        Self::Romani,
        Self::Elven,
        Self::Dwarfish,
    ];

    pub const fn label(self) -> &'static str {
        self.descriptor().english
    }

    pub const fn descriptor(self) -> LanguageDescriptor {
        match self {
            Self::EastCentral => LanguageDescriptor {
                english: "East-central",
                native: "Ostmitteldeutsch",
                monogram: "E",
                germanic_style: true,
            },
            Self::WestCentral => LanguageDescriptor {
                english: "West-central",
                native: "Westmitteldeutsch",
                monogram: "W",
                germanic_style: true,
            },
            Self::Low => LanguageDescriptor {
                english: "Low",
                native: "Niederdeutsch",
                monogram: "L",
                germanic_style: true,
            },
            Self::Yiddish => LanguageDescriptor {
                english: "Yiddish",
                native: "ייִדיש",
                monogram: "Y",
                germanic_style: false,
            },
            Self::Latin => LanguageDescriptor {
                english: "Latin",
                native: "Latine",
                monogram: "L",
                germanic_style: false,
            },
            Self::Romani => LanguageDescriptor {
                english: "Romani",
                native: "Romani",
                monogram: "R",
                germanic_style: false,
            },
            Self::Elven => LanguageDescriptor {
                english: "Elven",
                native: "Elven",
                monogram: "E",
                germanic_style: false,
            },
            Self::Dwarfish => LanguageDescriptor {
                english: "Dwarfish",
                native: "Dwarfish",
                monogram: "D",
                germanic_style: false,
            },
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::EastCentral => 0,
            Self::WestCentral => 1,
            Self::Low => 2,
            Self::Yiddish => 3,
            Self::Latin => 4,
            Self::Romani => 5,
            Self::Elven => 6,
            Self::Dwarfish => 7,
        }
    }

    pub const fn correlation(self, other: Self) -> f32 {
        const C: [[f32; 8]; 8] = [
            [1.0, 0.70, 0.30, 0.55, 0.0, 0.0, 0.0, 0.0],
            [0.70, 1.0, 0.40, 0.55, 0.0, 0.0, 0.0, 0.0],
            [0.30, 0.40, 1.0, 0.35, 0.0, 0.0, 0.0, 0.0],
            [0.55, 0.55, 0.35, 1.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        ];
        C[self.index()][other.index()]
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct OralLanguageHours {
    pub east_central: f32,
    pub west_central: f32,
    pub low: f32,
    pub yiddish: f32,
    pub latin: f32,
    pub romani: f32,
    pub elven: f32,
    pub dwarfish: f32,
}

impl OralLanguageHours {
    pub fn direct(self, language: OralLanguage) -> f32 {
        match language {
            OralLanguage::EastCentral => self.east_central,
            OralLanguage::WestCentral => self.west_central,
            OralLanguage::Low => self.low,
            OralLanguage::Yiddish => self.yiddish,
            OralLanguage::Latin => self.latin,
            OralLanguage::Romani => self.romani,
            OralLanguage::Elven => self.elven,
            OralLanguage::Dwarfish => self.dwarfish,
        }
    }
    pub fn direct_mut(&mut self, language: OralLanguage) -> &mut f32 {
        match language {
            OralLanguage::EastCentral => &mut self.east_central,
            OralLanguage::WestCentral => &mut self.west_central,
            OralLanguage::Low => &mut self.low,
            OralLanguage::Yiddish => &mut self.yiddish,
            OralLanguage::Latin => &mut self.latin,
            OralLanguage::Romani => &mut self.romani,
            OralLanguage::Elven => &mut self.elven,
            OralLanguage::Dwarfish => &mut self.dwarfish,
        }
    }
    pub fn direct_values(self) -> impl Iterator<Item = (OralLanguage, f32)> {
        OralLanguage::ALL
            .into_iter()
            .map(move |l| (l, self.direct(l)))
    }
    pub fn direct_fields_valid(self, maximum: f32) -> bool {
        maximum.is_finite()
            && maximum >= 0.0
            && self
                .direct_values()
                .all(|(_, v)| v.is_finite() && (0.0..=maximum).contains(&v))
    }
    pub fn effective(self, language: OralLanguage) -> f32 {
        OralLanguage::ALL
            .into_iter()
            .map(|studied| self.direct(studied).max(0.0) * language.correlation(studied))
            .sum()
    }
    pub fn add_direct(&mut self, language: OralLanguage, hours: f32) {
        if hours.is_finite() && hours > 0.0 {
            let value = self.direct_mut(language);
            *value = if value.is_finite() {
                (*value + hours).min(ORAL_FLUENCY_HOURS)
            } else {
                hours.min(ORAL_FLUENCY_HOURS)
            };
        }
    }
}

pub fn best_common_oral_language(
    left: OralLanguageHours,
    right: OralLanguageHours,
) -> (OralLanguage, f32) {
    best_common_oral_language_capped(left, ORAL_FLUENCY_HOURS, right, ORAL_FLUENCY_HOURS)
}

pub fn best_common_oral_language_capped(
    left: OralLanguageHours,
    left_cap_hours: f32,
    right: OralLanguageHours,
    right_cap_hours: f32,
) -> (OralLanguage, f32) {
    OralLanguage::ALL
        .into_iter()
        .map(|language| {
            let coefficient = (left
                .effective(language)
                .min(left_cap_hours.max(0.0))
                .min(right.effective(language).min(right_cap_hours.max(0.0)))
                / ORAL_FLUENCY_HOURS)
                .clamp(0.0, 1.0);
            (language, coefficient)
        })
        .max_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then_with(|| b.0.index().cmp(&a.0.index()))
        })
        .unwrap_or((OralLanguage::EastCentral, 0.0))
}

pub fn language_scaled_effect(value: f32, shared_language: f32) -> f32 {
    if !value.is_finite() || !shared_language.is_finite() {
        return 0.0;
    }
    value * shared_language.clamp(0.0, 1.0)
}

/// O(L*n) best-counterpart choices from one immutable party snapshot.
pub fn party_common_oral_choices(
    speakers: &[(u64, OralLanguageHours)],
) -> Vec<(u64, OralLanguage, f32)> {
    let capped: Vec<_> = speakers
        .iter()
        .map(|(id, hours)| (*id, *hours, ORAL_FLUENCY_HOURS))
        .collect();
    party_common_oral_choices_capped(&capped)
}

pub fn party_common_oral_choices_capped(
    speakers: &[(u64, OralLanguageHours, f32)],
) -> Vec<(u64, OralLanguage, f32)> {
    let mut top = [[(0_u64, -1.0_f32); 2]; 8];
    for (id, hours, cap) in speakers {
        for language in OralLanguage::ALL {
            let candidate = (*id, hours.effective(language).min((*cap).max(0.0)));
            let values = &mut top[language.index()];
            if candidate.1 > values[0].1
                || (candidate.1 == values[0].1 && candidate.0 < values[0].0)
            {
                values[1] = values[0];
                values[0] = candidate;
            } else if candidate.1 > values[1].1
                || (candidate.1 == values[1].1 && candidate.0 < values[1].0)
            {
                values[1] = candidate;
            }
        }
    }
    speakers
        .iter()
        .map(|(id, own, cap)| {
            OralLanguage::ALL
                .into_iter()
                .map(|language| {
                    let values = top[language.index()];
                    let other = if values[0].0 != *id {
                        values[0].1
                    } else {
                        values[1].1
                    };
                    let coefficient = if other < 0.0 {
                        0.0
                    } else {
                        (own.effective(language).min((*cap).max(0.0)).min(other)
                            / ORAL_FLUENCY_HOURS)
                            .clamp(0.0, 1.0)
                    };
                    (language, coefficient)
                })
                .max_by(|a, b| {
                    a.1.total_cmp(&b.1)
                        .then_with(|| b.0.index().cmp(&a.0.index()))
                })
                .map(|(language, coefficient)| (*id, language, coefficient))
                .unwrap()
        })
        .collect()
}

pub fn party_oral_training_gains(
    speakers: &[(u64, OralLanguageHours)],
    elapsed_hours: f32,
) -> Vec<(u64, OralLanguage, f32)> {
    let hours = if elapsed_hours.is_finite() {
        elapsed_hours.max(0.0)
    } else {
        0.0
    };
    party_common_oral_choices(speakers)
        .into_iter()
        .map(|(id, language, coefficient)| (id, language, hours * coefficient))
        .collect()
}

pub fn party_oral_training_gains_capped(
    speakers: &[(u64, OralLanguageHours, f32)],
    elapsed_hours: f32,
) -> Vec<(u64, OralLanguage, f32)> {
    let hours = if elapsed_hours.is_finite() {
        elapsed_hours.max(0.0)
    } else {
        0.0
    };
    party_common_oral_choices_capped(speakers)
        .into_iter()
        .map(|(id, language, coefficient)| (id, language, hours * coefficient))
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum WrittenLanguage {
    German,
    Low,
    Latin,
    Hebrew,
    Yiddish,
    Elven,
    Dwarfish,
}

impl WrittenLanguage {
    pub const ALL: [Self; 7] = [
        Self::German,
        Self::Low,
        Self::Latin,
        Self::Hebrew,
        Self::Yiddish,
        Self::Elven,
        Self::Dwarfish,
    ];
    pub const fn index(self) -> usize {
        match self {
            Self::German => 0,
            Self::Low => 1,
            Self::Latin => 2,
            Self::Hebrew => 3,
            Self::Yiddish => 4,
            Self::Elven => 5,
            Self::Dwarfish => 6,
        }
    }
    pub const fn descriptor(self) -> LanguageDescriptor {
        match self {
            Self::German => LanguageDescriptor {
                english: "German",
                native: "Kanzleideutsch",
                monogram: "G",
                germanic_style: true,
            },
            Self::Low => LanguageDescriptor {
                english: "Low",
                native: "Niederdeutsch",
                monogram: "L",
                germanic_style: true,
            },
            Self::Latin => LanguageDescriptor {
                english: "Latin",
                native: "Latine",
                monogram: "L",
                germanic_style: false,
            },
            Self::Hebrew => LanguageDescriptor {
                english: "Hebrew",
                native: "עברית",
                monogram: "H",
                germanic_style: false,
            },
            Self::Yiddish => LanguageDescriptor {
                english: "Yiddish",
                native: "ייִדיש",
                monogram: "Y",
                germanic_style: false,
            },
            Self::Elven => LanguageDescriptor {
                english: "Elven",
                native: "Elven",
                monogram: "E",
                germanic_style: false,
            },
            Self::Dwarfish => LanguageDescriptor {
                english: "Dwarfish",
                native: "Dwarfish",
                monogram: "D",
                germanic_style: false,
            },
        }
    }
    pub const fn correlation(self, other: Self) -> f32 {
        const C: [[f32; 7]; 7] = [
            [1.0, 0.35, 0.05, 0.0, 0.0, 0.0, 0.0],
            [0.35, 1.0, 0.05, 0.0, 0.0, 0.0, 0.0],
            [0.05, 0.05, 1.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.45, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.45, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        ];
        C[self.index()][other.index()]
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct WrittenLanguageHours {
    pub german: f32,
    pub low: f32,
    pub latin: f32,
    pub hebrew: f32,
    pub yiddish: f32,
    pub elven: f32,
    pub dwarfish: f32,
}
impl WrittenLanguageHours {
    pub fn direct(self, l: WrittenLanguage) -> f32 {
        match l {
            WrittenLanguage::German => self.german,
            WrittenLanguage::Low => self.low,
            WrittenLanguage::Latin => self.latin,
            WrittenLanguage::Hebrew => self.hebrew,
            WrittenLanguage::Yiddish => self.yiddish,
            WrittenLanguage::Elven => self.elven,
            WrittenLanguage::Dwarfish => self.dwarfish,
        }
    }
    pub fn direct_mut(&mut self, l: WrittenLanguage) -> &mut f32 {
        match l {
            WrittenLanguage::German => &mut self.german,
            WrittenLanguage::Low => &mut self.low,
            WrittenLanguage::Latin => &mut self.latin,
            WrittenLanguage::Hebrew => &mut self.hebrew,
            WrittenLanguage::Yiddish => &mut self.yiddish,
            WrittenLanguage::Elven => &mut self.elven,
            WrittenLanguage::Dwarfish => &mut self.dwarfish,
        }
    }
    pub fn direct_fields_valid(self, maximum: f32) -> bool {
        maximum.is_finite()
            && maximum >= 0.0
            && WrittenLanguage::ALL.into_iter().all(|l| {
                let v = self.direct(l);
                v.is_finite() && (0.0..=maximum).contains(&v)
            })
    }
    pub fn effective(self, l: WrittenLanguage) -> f32 {
        if self.direct(l) <= 0.0 {
            return 0.0;
        }
        WrittenLanguage::ALL
            .into_iter()
            .map(|s| self.direct(s).max(0.0) * l.correlation(s))
            .sum()
    }
    pub fn add_direct(&mut self, l: WrittenLanguage, h: f32) {
        if h.is_finite() && h > 0.0 {
            let v = self.direct_mut(l);
            *v = if v.is_finite() {
                (*v + h).min(ORAL_FLUENCY_HOURS)
            } else {
                h.min(ORAL_FLUENCY_HOURS)
            };
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SettlementLanguageProfile {
    pub east_central_bp: u16,
    pub west_central_bp: u16,
    pub low_bp: u16,
    /// Per-person incidence; Yiddish speakers remain part of the vernacular distribution above.
    pub yiddish_incidence_bp: u16,
}
impl SettlementLanguageProfile {
    pub const fn german_total_bp(self) -> u32 {
        self.east_central_bp as u32 + self.west_central_bp as u32 + self.low_bp as u32
    }
    pub const fn is_valid(self) -> bool {
        self.german_total_bp() == LANGUAGE_DISTRIBUTION_TOTAL_BP as u32
            && self.yiddish_incidence_bp <= 1_000
    }
    pub const fn dominant_german(self) -> OralLanguage {
        if self.east_central_bp >= self.west_central_bp && self.east_central_bp >= self.low_bp {
            OralLanguage::EastCentral
        } else if self.west_central_bp >= self.low_bp {
            OralLanguage::WestCentral
        } else {
            OralLanguage::Low
        }
    }
}

/// Infer a settlement's vernacular mix without external linguistic GIS data.
/// The exact basis-point result is stable and intended to be replaced by curated data later.
pub fn infer_settlement_language_profile(
    longitude: f64,
    latitude: f64,
) -> Result<SettlementLanguageProfile, &'static str> {
    if !coordinates_in_bounds(longitude, latitude, PLAYABLE_BOUNDS) {
        return Err("coordinates must be finite and inside PLAYABLE_BOUNDS");
    }
    const WEST: i64 = 8_965_000;
    const SOUTH: i64 = 50_877_000;
    const EAST: i64 = 11_110_000;
    const NORTH: i64 = 52_211_000;
    let lon = (longitude * 1_000_000.0).round() as i64;
    let lat = (latitude * 1_000_000.0).round() as i64;
    let x_bp = ((lon - WEST) * 10_000 / (EAST - WEST)).clamp(0, 10_000);
    let y_bp = ((lat - SOUTH) * 10_000 / (NORTH - SOUTH)).clamp(0, 10_000);
    let northern = ((y_bp - 2_200) * 10_000 / 7_000).clamp(0, 10_000) as i128;
    let low = ((9_000_i128 * northern * northern + 50_000_000) / 100_000_000) as u16;
    let central = 10_000_u16 - low;
    let east_share_bp =
        (800 + 7_800 * x_bp / 10_000 + 1_800 * (10_000 - y_bp) / 10_000).clamp(500, 9_500);
    let east_central = ((i64::from(central) * east_share_bp + 5_000) / 10_000) as u16;
    let west_central = central - east_central;
    Ok(SettlementLanguageProfile {
        east_central_bp: east_central,
        west_central_bp: west_central,
        low_bp: low,
        yiddish_incidence_bp: 75,
    })
}

fn stable_mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

/// Initialize direct oral hours from the character's final settlement.
/// Yiddish is an individual deterministic incidence, never a settlement-wide replacement.
pub fn initial_oral_languages(
    profile: SettlementLanguageProfile,
    character_id: u64,
    npc: bool,
) -> OralLanguageHours {
    let roll = (stable_mix(character_id) % 10_000) as u16;
    let german = if roll < profile.east_central_bp {
        OralLanguage::EastCentral
    } else if roll
        < profile
            .east_central_bp
            .saturating_add(profile.west_central_bp)
    {
        OralLanguage::WestCentral
    } else {
        OralLanguage::Low
    };
    let yiddish = npc
        && (stable_mix(character_id ^ 0x5949_4444_4953_48) % 10_000)
            < u64::from(profile.yiddish_incidence_bp);
    let mut hours = OralLanguageHours::default();
    *hours.direct_mut(german) = if yiddish {
        ORAL_FLUENCY_HOURS
            * (YIDDISH_LOCAL_GERMAN_FLUENCY - OralLanguage::Yiddish.correlation(german)).max(0.0)
    } else {
        ORAL_FLUENCY_HOURS
    };
    if yiddish {
        hours.yiddish = ORAL_FLUENCY_HOURS;
    }
    hours
}

pub fn initial_character_languages(
    profile: SettlementLanguageProfile,
    character_id: u64,
    npc: bool,
) -> (OralLanguageHours, WrittenLanguageHours) {
    let oral = initial_oral_languages(profile, character_id, npc);
    let written = if oral.yiddish > 0.0 {
        WrittenLanguageHours {
            yiddish: 1_000.0,
            hebrew: 500.0,
            german: 500.0,
            ..Default::default()
        }
    } else {
        WrittenLanguageHours {
            german: 1_000.0,
            ..Default::default()
        }
    };
    (oral, written)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlations_are_identity_and_symmetric() {
        for a in OralLanguage::ALL {
            assert_eq!(a.correlation(a), 1.0);
            for b in OralLanguage::ALL {
                assert_eq!(a.correlation(b), b.correlation(a));
            }
        }
        for a in WrittenLanguage::ALL {
            assert_eq!(a.correlation(a), 1.0);
            for b in WrittenLanguage::ALL {
                assert_eq!(a.correlation(b), b.correlation(a));
            }
        }
    }
    #[test]
    fn inference_is_exact_and_geographic() {
        let sw = infer_settlement_language_profile(PLAYABLE_BOUNDS[0], PLAYABLE_BOUNDS[1]).unwrap();
        let ne = infer_settlement_language_profile(PLAYABLE_BOUNDS[2], PLAYABLE_BOUNDS[3]).unwrap();
        assert_eq!(sw.german_total_bp(), 10_000);
        assert_eq!(ne.german_total_bp(), 10_000);
        assert!(sw.west_central_bp > sw.east_central_bp);
        assert!(ne.low_bp > sw.low_bp);
        assert!(infer_settlement_language_profile(f64::NAN, 51.0).is_err());
        assert_eq!(
            sw,
            SettlementLanguageProfile {
                east_central_bp: 2_600,
                west_central_bp: 7_400,
                low_bp: 0,
                yiddish_incidence_bp: 75
            }
        );
        assert_eq!(
            ne,
            SettlementLanguageProfile {
                east_central_bp: 860,
                west_central_bp: 140,
                low_bp: 9_000,
                yiddish_incidence_bp: 75
            }
        );
    }
    #[test]
    fn common_language_ties_are_stable_and_nonrecursive() {
        let a = OralLanguageHours {
            east_central: 1000.0,
            ..Default::default()
        };
        let b = OralLanguageHours {
            west_central: 1000.0,
            ..Default::default()
        };
        let (language, coefficient) = best_common_oral_language(a, b);
        assert_eq!(language, OralLanguage::EastCentral);
        assert!((coefficient - 0.14).abs() < 1e-5);
    }

    #[test]
    fn common_oral_language_caps_direct_and_correlated_fluency() {
        let left = OralLanguageHours {
            east_central: 5_000.0,
            ..Default::default()
        };
        let right = OralLanguageHours {
            west_central: 5_000.0,
            ..Default::default()
        };
        let (_, low) = best_common_oral_language_capped(left, 1_000.0, right, 5_000.0);
        let (_, restored) = best_common_oral_language_capped(left, 4_000.0, right, 5_000.0);
        assert_eq!(low, 0.2);
        assert_eq!(restored, 0.7);

        let choices = party_common_oral_choices_capped(&[(1, left, 1_000.0), (2, right, 5_000.0)]);
        assert!(
            choices
                .iter()
                .all(|(_, _, coefficient)| *coefficient == 0.2)
        );
    }
    #[test]
    fn yiddish_people_are_bilingual_with_weaker_local_german() {
        let profile = SettlementLanguageProfile {
            east_central_bp: 10_000,
            west_central_bp: 0,
            low_bp: 0,
            yiddish_incidence_bp: 1_000,
        };
        let id = (0..10_000)
            .find(|id| initial_oral_languages(profile, *id, true).yiddish > 0.0)
            .unwrap();
        let hours = initial_oral_languages(profile, id, true);
        assert_eq!(hours.yiddish, ORAL_FLUENCY_HOURS);
        assert!(hours.east_central > 0.0);
        let gentile = initial_oral_languages(profile, id, false);
        let (_, shared) = best_common_oral_language(hours, gentile);
        assert!(
            (hours.effective(OralLanguage::EastCentral)
                - ORAL_FLUENCY_HOURS * YIDDISH_LOCAL_GERMAN_FLUENCY)
                .abs()
                < 0.01
        );
        assert!((shared - YIDDISH_LOCAL_GERMAN_FLUENCY).abs() < 0.001);
        assert_eq!(initial_oral_languages(profile, id, false).yiddish, 0.0);
    }
    #[test]
    fn descriptors_cover_every_stable_language() {
        for language in OralLanguage::ALL {
            let d = language.descriptor();
            assert!(!d.english.is_empty() && !d.native.is_empty() && !d.monogram.is_empty());
        }
        for language in WrittenLanguage::ALL {
            let d = language.descriptor();
            assert!(!d.english.is_empty() && !d.native.is_empty() && !d.monogram.is_empty());
        }
    }
    #[test]
    fn language_scaling_is_safe_for_missing_or_invalid_proficiency() {
        assert_eq!(language_scaled_effect(4.0, 0.0), 0.0);
        assert_eq!(language_scaled_effect(4.0, 0.5), 2.0);
        assert_eq!(language_scaled_effect(4.0, f32::NAN), 0.0);
        assert_eq!(
            best_common_oral_language(Default::default(), Default::default()),
            (OralLanguage::EastCentral, 0.0)
        );
    }
    #[test]
    fn party_choices_are_order_independent_and_snapshot_linear() {
        let speakers: Vec<_> = (0..64)
            .map(|id| {
                (
                    id,
                    if id % 2 == 0 {
                        OralLanguageHours {
                            east_central: 1000.0,
                            ..Default::default()
                        }
                    } else {
                        OralLanguageHours {
                            west_central: 1000.0,
                            ..Default::default()
                        }
                    },
                )
            })
            .collect();
        let mut reversed = speakers.clone();
        reversed.reverse();
        let mut a = party_common_oral_choices(&speakers);
        let mut b = party_common_oral_choices(&reversed);
        a.sort_by_key(|v| v.0);
        b.sort_by_key(|v| v.0);
        assert_eq!(a, b);
        let full = party_oral_training_gains(&speakers, 2.0);
        let half = party_oral_training_gains(&speakers, 1.0);
        for ((_, _, gain), (_, _, partial)) in full.iter().zip(half.iter()) {
            assert!((*gain - *partial * 2.0).abs() < f32::EPSILON);
        }
    }
    #[test]
    fn final_location_reinitializes_complete_bilingual_identity() {
        let origin = SettlementLanguageProfile {
            east_central_bp: 0,
            west_central_bp: 10_000,
            low_bp: 0,
            yiddish_incidence_bp: 0,
        };
        let final_place = SettlementLanguageProfile {
            east_central_bp: 10_000,
            west_central_bp: 0,
            low_bp: 0,
            yiddish_incidence_bp: 1_000,
        };
        let id = (0..10_000)
            .find(|id| {
                initial_character_languages(final_place, *id, true)
                    .0
                    .yiddish
                    > 0.0
            })
            .unwrap();
        let (_, origin_written) = initial_character_languages(origin, id, true);
        assert_eq!(origin_written.yiddish, 0.0);
        let (oral, written) = initial_character_languages(final_place, id, true);
        assert!(oral.east_central > 0.0 && oral.yiddish > 0.0);
        assert!(written.yiddish > 0.0 && written.hebrew > 0.0);
        assert_eq!(oral.west_central, 0.0);
    }
}
