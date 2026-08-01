//! Romantic errantry quest frames and presenter-independent challenges.
//!
//! Errantry is deliberately a sibling of generated investigations: its
//! organizing principle is a chivalric purpose tested by ordered trials, not a
//! settlement case whose hidden cause must be discovered.

use serde::{Deserialize, Serialize};

mod puzzles;
pub use puzzles::*;

pub const ORDERED_SIGIL_RULES_VERSION: u16 = 2;
pub const ORDERED_SIGIL_COUNT: usize = 5;
pub const MAX_MINIMIZATION_SUBSETS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrantryPurpose {
    KeepVow,
    SeekBoon,
    Rescue,
    Pilgrimage,
    ProveWorth,
    Reconcile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrantryFrame {
    pub id: String,
    pub purpose: ErrantryPurpose,
    pub charge: String,
    pub trials: Vec<TrialBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBinding {
    pub order: u16,
    pub trial_id: String,
    pub challenge_id: Option<String>,
    pub site_id: String,
    pub kind: TrialKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrialKind {
    Combat,
    Social,
    Puzzle,
    Temptation,
    Ordeal,
}

/// A concrete advantage authored on the destination encounter. Preliminary
/// trials do not gate quest completion; instead they award material
/// countermeasures that suppress one of these defenses when the mission is
/// first bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FinaleDefenseKind {
    UnnaturalProwess,
    Reinforcements,
    PoisonedArms,
    ConcealedTrap,
    Glamour,
    SupernaturalArmor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CountermeasureKind {
    CapturedDispatch,
    Antidote,
    TrapWarning,
    ColdIronCharm,
    BlessedWeapon,
    RescuedAlly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalInsightKind {
    MustCloseToMelee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalInsight {
    pub kind: TacticalInsightKind,
    pub threat_id: String,
    pub finding: String,
    pub preparation: String,
}

/// Knowledge awarded by a trial must describe mechanics already consumed by
/// combat. It never changes the threat profile or applies a hidden modifier.
pub fn tactical_insight_for(threat_id: crate::bestiary::ThreatId) -> Option<TacticalInsight> {
    let profile = threat_id.profile();
    if profile.combat.ranged {
        return None;
    }
    let modeled = crate::bestiary::implemented_combat_lore(profile);
    if !modeled
        .weaknesses
        .iter()
        .any(|fact| fact == "Must close to melee range before attacking")
    {
        return None;
    }
    Some(TacticalInsight {
        kind: TacticalInsightKind::MustCloseToMelee,
        threat_id: threat_id.as_str().into(),
        finding: format!(
            "The {} carry no missile weapons and must close to melee range before striking.",
            profile.plural_name
        ),
        preparation: "Bring bows and arrows; archers can strike while these enemies close.".into(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialCountermeasure {
    pub kind: CountermeasureKind,
    pub source_challenge_id: String,
    pub item_id: String,
    pub counters: FinaleDefenseKind,
    pub enemy_scale_reduction_bps: u32,
    pub enemy_capability_multiplier_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedCountermeasure {
    pub kind: CountermeasureKind,
    pub source_challenge_id: String,
    pub item_id: String,
    pub countered_defense: FinaleDefenseKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinaleApproachResolution {
    pub authored_defenses: Vec<FinaleDefenseKind>,
    pub applied: Vec<AppliedCountermeasure>,
    pub unresolved_defenses: Vec<FinaleDefenseKind>,
    pub enemy_scale_reduction_bps: u32,
    pub enemy_capability_multiplier_bps: u32,
}

/// Resolves material countermeasures against a destination's authored
/// defenses. Only one source may suppress each defense; duplicate sources and
/// countermeasures for defenses the finale does not possess have no effect.
/// The result is sorted and deterministic so it can be snapshotted verbatim.
pub fn resolve_finale_approach(
    defenses: &[FinaleDefenseKind],
    countermeasures: &[MaterialCountermeasure],
) -> FinaleApproachResolution {
    let mut authored_defenses = defenses.to_vec();
    authored_defenses.sort();
    authored_defenses.dedup();

    let mut candidates = countermeasures.to_vec();
    candidates.sort_by(|left, right| {
        left.counters
            .cmp(&right.counters)
            .then(
                right
                    .enemy_scale_reduction_bps
                    .cmp(&left.enemy_scale_reduction_bps),
            )
            .then(
                left.enemy_capability_multiplier_bps
                    .cmp(&right.enemy_capability_multiplier_bps),
            )
            .then(left.kind.cmp(&right.kind))
            .then(left.source_challenge_id.cmp(&right.source_challenge_id))
    });
    let mut applied = Vec::new();
    let mut scale_reduction = 0_u32;
    let mut capability_multiplier = 10_000_u64;
    for defense in authored_defenses.iter().copied() {
        let Some(countermeasure) = candidates.iter().find(|item| item.counters == defense) else {
            continue;
        };
        scale_reduction = scale_reduction
            .saturating_add(countermeasure.enemy_scale_reduction_bps)
            .min(5_000);
        capability_multiplier = capability_multiplier.saturating_mul(u64::from(
            countermeasure
                .enemy_capability_multiplier_bps
                .clamp(5_000, 10_000),
        )) / 10_000;
        applied.push(AppliedCountermeasure {
            kind: countermeasure.kind,
            source_challenge_id: countermeasure.source_challenge_id.clone(),
            item_id: countermeasure.item_id.clone(),
            countered_defense: defense,
        });
    }
    let unresolved_defenses = authored_defenses
        .iter()
        .copied()
        .filter(|defense| {
            !applied
                .iter()
                .any(|item| item.countered_defense == *defense)
        })
        .collect();
    FinaleApproachResolution {
        authored_defenses,
        applied,
        unresolved_defenses,
        enemy_scale_reduction_bps: scale_reduction,
        enemy_capability_multiplier_bps: capability_multiplier.max(5_000) as u32,
    }
}

/// A camp's stable identity within one journey. Elapsed time is deliberately
/// absent: resting advances elapsed time without moving the camp.
pub fn journey_camp_identity_matches(
    journey_departure_minute: u64,
    completed_movement_minute: u64,
    camp_stop_minutes: &[u64],
    bound_departure_minute: u64,
    bound_movement_minute: u64,
) -> bool {
    journey_departure_minute == bound_departure_minute
        && completed_movement_minute == bound_movement_minute
        && camp_stop_minutes.contains(&bound_movement_minute)
}

pub fn rested_road_trial_camp_matches(
    journey_departure_minute: u64,
    completed_movement_minute: u64,
    completed_elapsed_minute: u64,
    camp_stop_minutes: &[u64],
    bound_departure_minute: u64,
    bound_movement_minute: u64,
    available_at_elapsed_minute: u64,
) -> bool {
    journey_camp_identity_matches(
        journey_departure_minute,
        completed_movement_minute,
        camp_stop_minutes,
        bound_departure_minute,
        bound_movement_minute,
    ) && completed_elapsed_minute >= available_at_elapsed_minute
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Sigil {
    Crown,
    Hart,
    Moon,
    Rose,
    Sword,
}

impl Sigil {
    pub const ALL: [Self; ORDERED_SIGIL_COUNT] =
        [Self::Crown, Self::Hart, Self::Moon, Self::Rose, Self::Sword];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Crown => "Crown",
            Self::Hart => "Hart",
            Self::Moon => "Moon",
            Self::Rose => "Rose",
            Self::Sword => "Sword",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderedSigilClue {
    Exact { sigil: Sigil, position: u8 },
    Before { first: Sigil, second: Sigil },
    Adjacent { first: Sigil, second: Sigil },
    NotAt { sigil: Sigil, position: u8 },
}

impl OrderedSigilClue {
    pub fn text(&self) -> String {
        match self {
            Self::Exact { sigil, position } => {
                format!("The {} belongs in place {}.", sigil.label(), position + 1)
            }
            Self::Before { first, second } => format!(
                "The {} stands somewhere before the {}.",
                first.label(),
                second.label()
            ),
            Self::Adjacent { first, second } => format!(
                "The {} and {} stand beside one another.",
                first.label(),
                second.label()
            ),
            Self::NotAt { sigil, position } => {
                format!(
                    "The {} does not belong in place {}.",
                    sigil.label(),
                    position + 1
                )
            }
        }
    }
}

/// Private deterministic replay authority. Never project this type to clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub solution: [Sigil; ORDERED_SIGIL_COUNT],
    pub clues: Vec<OrderedSigilClue>,
}

/// Observer-safe challenge truth. It intentionally lacks seed and solution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilProjection {
    pub rules_version: u16,
    pub sigils: [Sigil; ORDERED_SIGIL_COUNT],
    pub clues: Vec<OrderedSigilClue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilSubmission {
    pub expected_revision: u32,
    pub ordering: [Sigil; ORDERED_SIGIL_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionError {
    DuplicateSigil,
}

impl OrderedSigilPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_versioned(ORDERED_SIGIL_RULES_VERSION, seed)
            .expect("current ordered-sigil rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != ORDERED_SIGIL_RULES_VERSION {
            return Err("unsupported ordered-sigil rules version");
        }
        let mut rng = SplitMix64(seed ^ 0x6572_7261_6e74_7279);
        let mut solution = Sigil::ALL;
        for end in (1..solution.len()).rev() {
            let selected = (rng.next() as usize) % (end + 1);
            solution.swap(end, selected);
        }

        let (clues, _) = minimum_clues(&solution)?;
        let puzzle = Self {
            rules_version,
            seed,
            solution,
            clues,
        };
        puzzle.validate()?;
        Ok(puzzle)
    }

    pub fn projection(&self) -> OrderedSigilProjection {
        OrderedSigilProjection {
            rules_version: self.rules_version,
            sigils: Sigil::ALL,
            clues: self.clues.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rules_version != ORDERED_SIGIL_RULES_VERSION {
            return Err("unsupported ordered-sigil rules version");
        }
        if !is_permutation(&self.solution) {
            return Err("ordered-sigil solution is malformed");
        }
        if self.clues.iter().any(|clue| match clue {
            OrderedSigilClue::Exact { position, .. } | OrderedSigilClue::NotAt { position, .. } => {
                usize::from(*position) >= ORDERED_SIGIL_COUNT
            }
            OrderedSigilClue::Before { first, second }
            | OrderedSigilClue::Adjacent { first, second } => first == second,
        }) {
            return Err("ordered-sigil clue coordinates are malformed");
        }
        let found = solutions(&self.clues, 2);
        if found.len() != 1 || found[0] != self.solution {
            return Err("ordered-sigil clues do not prove the canonical solution");
        }
        Ok(())
    }

    pub fn check(&self, submission: &OrderedSigilSubmission) -> Result<bool, SubmissionError> {
        if !is_permutation(&submission.ordering) {
            return Err(SubmissionError::DuplicateSigil);
        }
        Ok(submission.ordering == self.solution)
    }
}

fn true_clue_pool(solution: &[Sigil; ORDERED_SIGIL_COUNT]) -> Vec<OrderedSigilClue> {
    let mut pool = Vec::with_capacity(39);
    // Canonical relational-first order is also the final deterministic
    // tie-break after cardinality, Exact count, and NotAt count.
    for left in 0..ORDERED_SIGIL_COUNT {
        for right in (left + 1)..ORDERED_SIGIL_COUNT {
            pool.push(OrderedSigilClue::Before {
                first: solution[left],
                second: solution[right],
            });
        }
    }
    for left in 0..(ORDERED_SIGIL_COUNT - 1) {
        pool.push(OrderedSigilClue::Adjacent {
            first: solution[left],
            second: solution[left + 1],
        });
    }
    for (position, sigil) in solution.iter().copied().enumerate() {
        for wrong in 0..ORDERED_SIGIL_COUNT {
            if wrong != position {
                pool.push(OrderedSigilClue::NotAt {
                    sigil,
                    position: wrong as u8,
                });
            }
        }
    }
    for (position, sigil) in solution.iter().copied().enumerate() {
        pool.push(OrderedSigilClue::Exact {
            sigil,
            position: position as u8,
        });
    }
    pool
}

fn all_orderings() -> Vec<[Sigil; ORDERED_SIGIL_COUNT]> {
    solutions(&[], usize::MAX)
}

fn minimum_clues(
    solution: &[Sigil; ORDERED_SIGIL_COUNT],
) -> Result<(Vec<OrderedSigilClue>, usize), &'static str> {
    let pool = true_clue_pool(solution);
    let orderings = all_orderings();
    let solution_index = orderings
        .iter()
        .position(|candidate| candidate == solution)
        .ok_or("ordered-sigil solution is absent from permutation space")?;
    let target = 1_u128 << solution_index;
    let full = (1_u128 << orderings.len()) - 1;
    let masks = pool
        .iter()
        .map(|clue| {
            orderings
                .iter()
                .enumerate()
                .fold(0_u128, |mask, (index, candidate)| {
                    mask | (u128::from(clue_holds(clue, candidate)) << index)
                })
        })
        .collect::<Vec<_>>();
    let mut evaluated = 0;
    for cardinality in 1..=4 {
        let mut chosen = Vec::with_capacity(cardinality);
        let mut best: Option<(usize, usize, Vec<usize>)> = None;
        search_minimum_subsets(
            0,
            cardinality,
            full,
            target,
            &pool,
            &masks,
            &mut chosen,
            &mut best,
            &mut evaluated,
        )?;
        if let Some((_, _, indices)) = best {
            return Ok((
                indices
                    .into_iter()
                    .map(|index| pool[index].clone())
                    .collect(),
                evaluated,
            ));
        }
    }
    Err("ordered-sigil four-clue relational bound failed")
}

#[allow(clippy::too_many_arguments)]
fn search_minimum_subsets(
    start: usize,
    remaining: usize,
    intersection: u128,
    target: u128,
    pool: &[OrderedSigilClue],
    masks: &[u128],
    chosen: &mut Vec<usize>,
    best: &mut Option<(usize, usize, Vec<usize>)>,
    evaluated: &mut usize,
) -> Result<(), &'static str> {
    if remaining == 0 {
        *evaluated += 1;
        if *evaluated > MAX_MINIMIZATION_SUBSETS {
            return Err("ordered-sigil minimization exceeded its subset bound");
        }
        if intersection == target {
            let exact = chosen
                .iter()
                .filter(|index| matches!(pool[**index], OrderedSigilClue::Exact { .. }))
                .count();
            let not_at = chosen
                .iter()
                .filter(|index| matches!(pool[**index], OrderedSigilClue::NotAt { .. }))
                .count();
            let candidate = (exact, not_at, chosen.clone());
            if best.as_ref().is_none_or(|current| candidate < *current) {
                *best = Some(candidate);
            }
        }
        return Ok(());
    }
    let last_start = pool.len().saturating_sub(remaining);
    for index in start..=last_start {
        chosen.push(index);
        search_minimum_subsets(
            index + 1,
            remaining - 1,
            intersection & masks[index],
            target,
            pool,
            masks,
            chosen,
            best,
            evaluated,
        )?;
        chosen.pop();
    }
    Ok(())
}

pub fn solutions(clues: &[OrderedSigilClue], limit: usize) -> Vec<[Sigil; ORDERED_SIGIL_COUNT]> {
    fn visit(
        at: usize,
        current: &mut [Sigil; ORDERED_SIGIL_COUNT],
        clues: &[OrderedSigilClue],
        limit: usize,
        found: &mut Vec<[Sigil; ORDERED_SIGIL_COUNT]>,
    ) {
        if found.len() >= limit {
            return;
        }
        if at == current.len() {
            if clues.iter().all(|clue| clue_holds(clue, current)) {
                found.push(*current);
            }
            return;
        }
        for next in at..current.len() {
            current.swap(at, next);
            visit(at + 1, current, clues, limit, found);
            current.swap(at, next);
        }
    }
    let mut found = Vec::new();
    let mut current = Sigil::ALL;
    visit(0, &mut current, clues, limit, &mut found);
    found
}

fn clue_holds(clue: &OrderedSigilClue, candidate: &[Sigil; ORDERED_SIGIL_COUNT]) -> bool {
    let position = |wanted| candidate.iter().position(|sigil| *sigil == wanted).unwrap();
    match *clue {
        OrderedSigilClue::Exact { sigil, position: p } => position(sigil) == usize::from(p),
        OrderedSigilClue::Before { first, second } => position(first) < position(second),
        OrderedSigilClue::Adjacent { first, second } => {
            position(first).abs_diff(position(second)) == 1
        }
        OrderedSigilClue::NotAt { sigil, position: p } => position(sigil) != usize::from(p),
    }
}

fn is_permutation(ordering: &[Sigil; ORDERED_SIGIL_COUNT]) -> bool {
    Sigil::ALL.iter().all(|sigil| {
        ordering
            .iter()
            .filter(|candidate| *candidate == sigil)
            .count()
            == 1
    })
}

struct SplitMix64(u64);
impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeyPresenterCatalogId {
    LadyBeneathThornV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeySpeechPart {
    Introduction,
    Instruction,
    Wrong,
    Correct,
}

pub const FEY_PRESENTER_NAME: &str = "The Lady Beneath the Thorn";
pub const FEY_PRESENTER_TITLE: &str = "Now Hear the Lady Crowned with Briar Thorn";

impl FeyPresenterCatalogId {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LadyBeneathThornV1 => FEY_PRESENTER_NAME,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::LadyBeneathThornV1 => "lady-beneath-thorn-v1",
        }
    }
}

/// Closed server-owned speech selected by typed catalog identity. No persisted
/// prose can become a supernatural utterance.
pub fn fey_speech(catalog: FeyPresenterCatalogId, part: FeySpeechPart) -> &'static [&'static str] {
    match (catalog, part) {
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Introduction) => &[
            "Good knight, five signs attend my moonlit gate.",
            "Set each in place, and thereby prove thy wit.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Instruction) => &[
            "Mark well my words, then set the signs in rank.",
            "Submit thy choice when all the five are set.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Wrong) => &[
            "Thy chosen rank hath missed the hidden truth.",
            "Take heart, good knight, and read my words anew.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Correct) => &[
            "Thy wit hath won the passage through my wood.",
            "Go forth with honor; none shall bar thy road.",
        ],
    }
}

/// Puzzle-specific closed speech for the same supernatural presenter. Formal
/// observations remain in the puzzle projection; this catalog can change the
/// asking entity without changing puzzle truth.
pub fn fey_puzzle_speech(
    catalog: FeyPresenterCatalogId,
    kind: PuzzleKind,
    part: FeySpeechPart,
) -> &'static [&'static str] {
    match kind {
        PuzzleKind::OrderedSigils => fey_speech(catalog, part),
        PuzzleKind::TruthfulWitnesses => match (catalog, part) {
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Introduction) => &[
                "Three travelers wait beneath my briar.",
                "One tongue speaks false; the other two speak true.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Instruction) => &[
                "Judge not the tongue, but choose the path made safe.",
                "Mark each sworn word, then name the road thou'lt take.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Wrong) => &[
                "Thy chosen road would lead thee far astray.",
                "Weigh every oath, and choose thy path anew.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Correct) => &[
                "Thy judgment parts the falsehood from the true.",
                "The guarded road lies open to thy tread.",
            ],
        },
        PuzzleKind::RuneTransformation => match (catalog, part) {
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Introduction) => &[
                "The runes pass changed beneath my silver hand.",
                "Learn thou the law their altered faces keep.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Instruction) => &[
                "Mark what went in, and what returned anew.",
                "Then name the sign my final gate shall yield.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Wrong) => &[
                "The hidden rule denies thy chosen sign.",
                "Review each change, and try the gate anew.",
            ],
            (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Correct) => &[
                "Thou hast discerned the law beneath each change.",
                "My woodland path now opens at thy word.",
            ],
        },
    }
}

pub fn fey_clue_text(_catalog: FeyPresenterCatalogId, clue: &OrderedSigilClue) -> &'static str {
    let sigil = |value: Sigil| match value {
        Sigil::Crown => 0,
        Sigil::Hart => 1,
        Sigil::Moon => 2,
        Sigil::Rose => 3,
        Sigil::Sword => 4,
    };
    match *clue {
        OrderedSigilClue::Exact {
            sigil: value,
            position,
        } => FEY_EXACT[sigil(value)][usize::from(position)],
        OrderedSigilClue::NotAt {
            sigil: value,
            position,
        } => FEY_NOT_AT[sigil(value)][usize::from(position)],
        OrderedSigilClue::Before { first, second } => FEY_BEFORE[sigil(first)][sigil(second)],
        OrderedSigilClue::Adjacent { first, second } => FEY_ADJACENT[sigil(first)][sigil(second)],
    }
}

// Closed, reviewed supernatural clue speech. Every reachable entry is an
// authored modern-spelling Shakespearean line intended as iambic pentameter.
const FEY_EXACT: [[&str; 5]; 5] = [
    [
        "The Crown shall now in first position stand.",
        "The Crown shall in the second station stand.",
        "The Crown shall now in third position stand.",
        "The Crown shall now in fourth position stand.",
        "The Crown shall now in fifth position stand.",
    ],
    [
        "The Hart shall now in first position stand.",
        "The Hart shall in the second station stand.",
        "The Hart shall now in third position stand.",
        "The Hart shall now in fourth position stand.",
        "The Hart shall now in fifth position stand.",
    ],
    [
        "The Moon shall now in first position stand.",
        "The Moon shall in the second station stand.",
        "The Moon shall now in third position stand.",
        "The Moon shall now in fourth position stand.",
        "The Moon shall now in fifth position stand.",
    ],
    [
        "The Rose shall now in first position stand.",
        "The Rose shall in the second station stand.",
        "The Rose shall now in third position stand.",
        "The Rose shall now in fourth position stand.",
        "The Rose shall now in fifth position stand.",
    ],
    [
        "The Sword shall now in first position stand.",
        "The Sword shall in the second station stand.",
        "The Sword shall now in third position stand.",
        "The Sword shall now in fourth position stand.",
        "The Sword shall now in fifth position stand.",
    ],
];

const FEY_NOT_AT: [[&str; 5]; 5] = [
    [
        "The Crown shall never hold the first estate.",
        "The Crown shall not hold the second estate.",
        "The Crown shall never hold the third estate.",
        "The Crown shall never hold the fourth estate.",
        "The Crown shall never hold the fifth estate.",
    ],
    [
        "The Hart shall never hold the first estate.",
        "The Hart shall not hold the second estate.",
        "The Hart shall never hold the third estate.",
        "The Hart shall never hold the fourth estate.",
        "The Hart shall never hold the fifth estate.",
    ],
    [
        "The Moon shall never hold the first estate.",
        "The Moon shall not hold the second estate.",
        "The Moon shall never hold the third estate.",
        "The Moon shall never hold the fourth estate.",
        "The Moon shall never hold the fifth estate.",
    ],
    [
        "The Rose shall never hold the first estate.",
        "The Rose shall not hold the second estate.",
        "The Rose shall never hold the third estate.",
        "The Rose shall never hold the fourth estate.",
        "The Rose shall never hold the fifth estate.",
    ],
    [
        "The Sword shall never hold the first estate.",
        "The Sword shall not hold the second estate.",
        "The Sword shall never hold the third estate.",
        "The Sword shall never hold the fourth estate.",
        "The Sword shall never hold the fifth estate.",
    ],
];

const FEY_BEFORE: [[&str; 5]; 5] = [
    [
        "",
        "The Crown must take its place before the Hart.",
        "The Crown must take its place before the Moon.",
        "The Crown must take its place before the Rose.",
        "The Crown must take its place before the Sword.",
    ],
    [
        "The Hart must take its place before the Crown.",
        "",
        "The Hart must take its place before the Moon.",
        "The Hart must take its place before the Rose.",
        "The Hart must take its place before the Sword.",
    ],
    [
        "The Moon must take its place before the Crown.",
        "The Moon must take its place before the Hart.",
        "",
        "The Moon must take its place before the Rose.",
        "The Moon must take its place before the Sword.",
    ],
    [
        "The Rose must take its place before the Crown.",
        "The Rose must take its place before the Hart.",
        "The Rose must take its place before the Moon.",
        "",
        "The Rose must take its place before the Sword.",
    ],
    [
        "The Sword must take its place before the Crown.",
        "The Sword must take its place before the Hart.",
        "The Sword must take its place before the Moon.",
        "The Sword must take its place before the Rose.",
        "",
    ],
];

const FEY_ADJACENT: [[&str; 5]; 5] = [
    [
        "",
        "Let Crown and Hart stand ever side by side.",
        "Let Crown and Moon stand ever side by side.",
        "Let Crown and Rose stand ever side by side.",
        "Let Crown and Sword stand ever side by side.",
    ],
    [
        "Let Hart and Crown stand ever side by side.",
        "",
        "Let Hart and Moon stand ever side by side.",
        "Let Hart and Rose stand ever side by side.",
        "Let Hart and Sword stand ever side by side.",
    ],
    [
        "Let Moon and Crown stand ever side by side.",
        "Let Moon and Hart stand ever side by side.",
        "",
        "Let Moon and Rose stand ever side by side.",
        "Let Moon and Sword stand ever side by side.",
    ],
    [
        "Let Rose and Crown stand ever side by side.",
        "Let Rose and Hart stand ever side by side.",
        "Let Rose and Moon stand ever side by side.",
        "",
        "Let Rose and Sword stand ever side by side.",
    ],
    [
        "Let Sword and Crown stand ever side by side.",
        "Let Sword and Hart stand ever side by side.",
        "Let Sword and Moon stand ever side by side.",
        "Let Sword and Rose stand ever side by side.",
        "",
    ],
];

// Historical physical-presenter text is compile-disabled while the initial
// errantry slice is chat-only.
#[cfg(any())]
const RUIN_EXACT: [[&str; 5]; 5] = [
    [
        "Crown: I.",
        "Crown: II.",
        "Crown: III.",
        "Crown: IV.",
        "Crown: V.",
    ],
    [
        "Hart: I.",
        "Hart: II.",
        "Hart: III.",
        "Hart: IV.",
        "Hart: V.",
    ],
    [
        "Moon: I.",
        "Moon: II.",
        "Moon: III.",
        "Moon: IV.",
        "Moon: V.",
    ],
    [
        "Rose: I.",
        "Rose: II.",
        "Rose: III.",
        "Rose: IV.",
        "Rose: V.",
    ],
    [
        "Sword: I.",
        "Sword: II.",
        "Sword: III.",
        "Sword: IV.",
        "Sword: V.",
    ],
];
#[cfg(any())]
const RUIN_NOT_AT: [[&str; 5]; 5] = [
    [
        "Crown ≠ I.",
        "Crown ≠ II.",
        "Crown ≠ III.",
        "Crown ≠ IV.",
        "Crown ≠ V.",
    ],
    [
        "Hart ≠ I.",
        "Hart ≠ II.",
        "Hart ≠ III.",
        "Hart ≠ IV.",
        "Hart ≠ V.",
    ],
    [
        "Moon ≠ I.",
        "Moon ≠ II.",
        "Moon ≠ III.",
        "Moon ≠ IV.",
        "Moon ≠ V.",
    ],
    [
        "Rose ≠ I.",
        "Rose ≠ II.",
        "Rose ≠ III.",
        "Rose ≠ IV.",
        "Rose ≠ V.",
    ],
    [
        "Sword ≠ I.",
        "Sword ≠ II.",
        "Sword ≠ III.",
        "Sword ≠ IV.",
        "Sword ≠ V.",
    ],
];
#[cfg(any())]
const RUIN_BEFORE: [[&str; 5]; 5] = [
    [
        "",
        "Crown < Hart.",
        "Crown < Moon.",
        "Crown < Rose.",
        "Crown < Sword.",
    ],
    [
        "Hart < Crown.",
        "",
        "Hart < Moon.",
        "Hart < Rose.",
        "Hart < Sword.",
    ],
    [
        "Moon < Crown.",
        "Moon < Hart.",
        "",
        "Moon < Rose.",
        "Moon < Sword.",
    ],
    [
        "Rose < Crown.",
        "Rose < Hart.",
        "Rose < Moon.",
        "",
        "Rose < Sword.",
    ],
    [
        "Sword < Crown.",
        "Sword < Hart.",
        "Sword < Moon.",
        "Sword < Rose.",
        "",
    ],
];
#[cfg(any())]
const RUIN_ADJACENT: [[&str; 5]; 5] = [
    [
        "",
        "Crown—Hart.",
        "Crown—Moon.",
        "Crown—Rose.",
        "Crown—Sword.",
    ],
    ["Hart—Crown.", "", "Hart—Moon.", "Hart—Rose.", "Hart—Sword."],
    ["Moon—Crown.", "Moon—Hart.", "", "Moon—Rose.", "Moon—Sword."],
    ["Rose—Crown.", "Rose—Hart.", "Rose—Moon.", "", "Rose—Sword."],
    [
        "Sword—Crown.",
        "Sword—Hart.",
        "Sword—Moon.",
        "Sword—Rose.",
        "",
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn countermeasure(
        kind: CountermeasureKind,
        source: &str,
        item: &str,
        counters: FinaleDefenseKind,
        scale_reduction: u32,
        capability_multiplier: u32,
    ) -> MaterialCountermeasure {
        MaterialCountermeasure {
            kind,
            source_challenge_id: source.into(),
            item_id: item.into(),
            counters,
            enemy_scale_reduction_bps: scale_reduction,
            enemy_capability_multiplier_bps: capability_multiplier,
        }
    }

    #[test]
    fn material_countermeasures_resolve_only_authored_defenses_once() {
        let defenses = [
            FinaleDefenseKind::Reinforcements,
            FinaleDefenseKind::UnnaturalProwess,
            FinaleDefenseKind::Reinforcements,
        ];
        let resolution = resolve_finale_approach(
            &defenses,
            &[
                countermeasure(
                    CountermeasureKind::CapturedDispatch,
                    "challenge:courier",
                    "captured_dispatch",
                    FinaleDefenseKind::Reinforcements,
                    1_500,
                    8_500,
                ),
                countermeasure(
                    CountermeasureKind::RescuedAlly,
                    "challenge:weaker-duplicate",
                    "ally",
                    FinaleDefenseKind::Reinforcements,
                    500,
                    9_500,
                ),
                countermeasure(
                    CountermeasureKind::ColdIronCharm,
                    "challenge:fey",
                    "favor",
                    FinaleDefenseKind::UnnaturalProwess,
                    2_500,
                    7_500,
                ),
                countermeasure(
                    CountermeasureKind::Antidote,
                    "challenge:irrelevant",
                    "antidote",
                    FinaleDefenseKind::PoisonedArms,
                    4_000,
                    5_000,
                ),
            ],
        );
        assert_eq!(
            resolution.authored_defenses,
            vec![
                FinaleDefenseKind::UnnaturalProwess,
                FinaleDefenseKind::Reinforcements,
            ]
        );
        assert_eq!(resolution.applied.len(), 2);
        assert!(
            resolution
                .applied
                .iter()
                .any(|item| item.kind == CountermeasureKind::CapturedDispatch)
        );
        assert!(resolution.unresolved_defenses.is_empty());
        assert_eq!(resolution.enemy_scale_reduction_bps, 4_000);
        assert_eq!(resolution.enemy_capability_multiplier_bps, 6_375);
    }

    #[test]
    fn tactical_insight_reports_consumed_physical_mechanics_without_modifying_them() {
        let threat = crate::bestiary::ThreatId::ArmedRetainer;
        let before = threat.profile().combat;
        let insight = tactical_insight_for(threat).expect("retainers must close to melee");
        let after = threat.profile().combat;

        assert_eq!(insight.kind, TacticalInsightKind::MustCloseToMelee);
        assert!(insight.finding.contains("no missile weapons"));
        assert!(insight.preparation.contains("bows and arrows"));
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }

    #[test]
    fn unresolved_defenses_remain_visible_and_effects_are_bounded() {
        let resolution = resolve_finale_approach(
            &[
                FinaleDefenseKind::Reinforcements,
                FinaleDefenseKind::PoisonedArms,
            ],
            &[countermeasure(
                CountermeasureKind::CapturedDispatch,
                "challenge:courier",
                "captured_dispatch",
                FinaleDefenseKind::Reinforcements,
                9_999,
                1,
            )],
        );
        assert_eq!(resolution.enemy_scale_reduction_bps, 5_000);
        assert_eq!(resolution.enemy_capability_multiplier_bps, 5_000);
        assert_eq!(
            resolution.unresolved_defenses,
            vec![FinaleDefenseKind::PoisonedArms]
        );
    }

    #[test]
    fn camp_identity_survives_rest_but_not_movement_or_a_new_journey() {
        let stops = [60, 120];
        assert!(journey_camp_identity_matches(40, 60, &stops, 40, 60));
        // Rest is not an input: only stable journey and movement coordinates
        // identify the persisted camp.
        assert!(!journey_camp_identity_matches(40, 120, &stops, 40, 60));
        assert!(!journey_camp_identity_matches(41, 60, &stops, 40, 60));
        assert!(!journey_camp_identity_matches(40, 60, &[120], 40, 60));
    }

    #[test]
    fn road_trial_interrupts_rest_only_at_its_bound_camp() {
        let stops = [60, 120];
        assert!(!rested_road_trial_camp_matches(
            40, 60, 119, &stops, 40, 60, 120
        ));
        assert!(rested_road_trial_camp_matches(
            40, 60, 120, &stops, 40, 60, 120
        ));
        assert!(!rested_road_trial_camp_matches(
            40, 120, 180, &stops, 40, 60, 120
        ));
        assert!(!rested_road_trial_camp_matches(
            41, 60, 180, &stops, 40, 60, 120
        ));
    }

    #[test]
    fn every_solution_gets_a_globally_minimum_bounded_necessary_clue_set() {
        for solution in all_orderings() {
            let (clues, evaluated) = minimum_clues(&solution).unwrap();
            assert!(clues.len() <= 4);
            assert!(evaluated <= MAX_MINIMIZATION_SUBSETS);
            assert_eq!(solutions(&clues, 2), vec![solution]);

            for removed in 0..clues.len() {
                let mut reduced = clues.clone();
                reduced.remove(removed);
                assert_ne!(solutions(&reduced, 2), vec![solution]);
            }

            let pool = true_clue_pool(&solution);
            for cardinality in 1..clues.len() {
                assert!(
                    best_unique_subset(&pool, &solution, cardinality).is_none(),
                    "found a smaller clue set for {solution:?}"
                );
            }
        }
    }

    #[test]
    fn replay_is_versioned_and_deterministic() {
        let first =
            OrderedSigilPuzzle::generate_versioned(ORDERED_SIGIL_RULES_VERSION, 41).unwrap();
        assert_eq!(
            first.solution,
            [
                Sigil::Hart,
                Sigil::Crown,
                Sigil::Sword,
                Sigil::Moon,
                Sigil::Rose
            ]
        );
        assert_eq!(
            first.clues,
            vec![
                OrderedSigilClue::Before {
                    first: Sigil::Hart,
                    second: Sigil::Crown,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Crown,
                    second: Sigil::Sword,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Sword,
                    second: Sigil::Moon,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Moon,
                    second: Sigil::Rose,
                },
            ]
        );
        assert_eq!(
            first,
            OrderedSigilPuzzle::generate_versioned(ORDERED_SIGIL_RULES_VERSION, 41).unwrap()
        );
        assert!(OrderedSigilPuzzle::generate_versioned(1, 41).is_err());
    }

    #[test]
    fn tie_break_prefers_relations_then_not_at_then_exact() {
        for solution in all_orderings() {
            let (chosen, _) = minimum_clues(&solution).unwrap();
            let pool = true_clue_pool(&solution);
            let chosen_indices = chosen
                .iter()
                .map(|clue| pool.iter().position(|candidate| candidate == clue).unwrap())
                .collect::<Vec<_>>();
            let chosen_score = subset_style_score(&pool, &chosen_indices);
            assert_eq!(
                Some(chosen_score),
                best_unique_subset(&pool, &solution, chosen.len())
            );
        }
    }

    #[test]
    fn projection_is_safe() {
        let puzzle = OrderedSigilPuzzle::generate(9);
        let projection = puzzle.projection();
        let json = serde_json::to_string(&projection).unwrap();
        assert!(!json.contains("solution"));
        assert!(!json.contains("seed"));
        assert_eq!(projection.clues, puzzle.clues);
    }

    #[test]
    fn only_the_solution_is_accepted() {
        let puzzle = OrderedSigilPuzzle::generate(77);
        for candidate in solutions(&[], usize::MAX) {
            let submission = OrderedSigilSubmission {
                expected_revision: 0,
                ordering: candidate,
            };
            assert_eq!(
                puzzle.check(&submission).unwrap(),
                candidate == puzzle.solution
            );
        }
        let malformed = OrderedSigilSubmission {
            expected_revision: 0,
            ordering: [Sigil::Crown; ORDERED_SIGIL_COUNT],
        };
        assert_eq!(
            puzzle.check(&malformed),
            Err(SubmissionError::DuplicateSigil)
        );
    }

    #[test]
    fn supernatural_speech_comes_only_from_the_closed_verse_catalog() {
        let catalog = FeyPresenterCatalogId::LadyBeneathThornV1;
        let all = [
            FeySpeechPart::Introduction,
            FeySpeechPart::Instruction,
            FeySpeechPart::Wrong,
            FeySpeechPart::Correct,
        ]
        .into_iter()
        .flat_map(|part| fey_speech(catalog, part))
        .collect::<Vec<_>>();
        assert_eq!(all.len(), 8);
        assert!(
            all.iter()
                .all(|line| line.ends_with('.') && !line.contains('{'))
        );
        for kind in PuzzleKind::ALL {
            let lines = [
                FeySpeechPart::Introduction,
                FeySpeechPart::Instruction,
                FeySpeechPart::Wrong,
                FeySpeechPart::Correct,
            ]
            .into_iter()
            .flat_map(|part| fey_puzzle_speech(catalog, kind, part))
            .collect::<Vec<_>>();
            assert_eq!(lines.len(), 8);
            assert!(lines.iter().all(|line| {
                line.ends_with('.') && !line.contains('{') && !line.contains("{}")
            }));
        }

        let mut clue_lines = Vec::new();
        for sigil in Sigil::ALL {
            for position in 0..ORDERED_SIGIL_COUNT as u8 {
                clue_lines.push(fey_clue_text(
                    catalog,
                    &OrderedSigilClue::Exact { sigil, position },
                ));
                clue_lines.push(fey_clue_text(
                    catalog,
                    &OrderedSigilClue::NotAt { sigil, position },
                ));
            }
            for other in Sigil::ALL {
                if sigil != other {
                    clue_lines.push(fey_clue_text(
                        catalog,
                        &OrderedSigilClue::Before {
                            first: sigil,
                            second: other,
                        },
                    ));
                    clue_lines.push(fey_clue_text(
                        catalog,
                        &OrderedSigilClue::Adjacent {
                            first: sigil,
                            second: other,
                        },
                    ));
                }
            }
        }
        assert_eq!(clue_lines.len(), 90);
        assert!(clue_lines.iter().all(|line| {
            !line.is_empty()
                && line.ends_with('.')
                && !line.contains('{')
                && !line.contains("position {}")
        }));
    }

    fn subset_style_score(
        pool: &[OrderedSigilClue],
        indices: &[usize],
    ) -> (usize, usize, Vec<usize>) {
        let exact = indices
            .iter()
            .filter(|index| matches!(pool[**index], OrderedSigilClue::Exact { .. }))
            .count();
        let not_at = indices
            .iter()
            .filter(|index| matches!(pool[**index], OrderedSigilClue::NotAt { .. }))
            .count();
        (exact, not_at, indices.to_vec())
    }

    fn best_unique_subset(
        pool: &[OrderedSigilClue],
        solution: &[Sigil; ORDERED_SIGIL_COUNT],
        cardinality: usize,
    ) -> Option<(usize, usize, Vec<usize>)> {
        let orderings = all_orderings();
        let target = 1_u128
            << orderings
                .iter()
                .position(|candidate| candidate == solution)
                .unwrap();
        let masks = pool
            .iter()
            .map(|clue| {
                orderings
                    .iter()
                    .enumerate()
                    .fold(0_u128, |mask, (index, ordering)| {
                        mask | (u128::from(clue_holds(clue, ordering)) << index)
                    })
            })
            .collect::<Vec<_>>();
        let mut best = None;
        visit_mask_subsets(
            pool,
            &masks,
            cardinality,
            0,
            (1_u128 << orderings.len()) - 1,
            target,
            &mut Vec::new(),
            &mut best,
        );
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_mask_subsets(
        pool: &[OrderedSigilClue],
        masks: &[u128],
        remaining: usize,
        start: usize,
        intersection: u128,
        target: u128,
        chosen: &mut Vec<usize>,
        best: &mut Option<(usize, usize, Vec<usize>)>,
    ) {
        if remaining == 0 {
            if intersection == target {
                let candidate = subset_style_score(pool, chosen);
                if best.as_ref().is_none_or(|current| candidate < *current) {
                    *best = Some(candidate);
                }
            }
            return;
        }
        for index in start..=pool.len() - remaining {
            chosen.push(index);
            visit_mask_subsets(
                pool,
                masks,
                remaining - 1,
                index + 1,
                intersection & masks[index],
                target,
                chosen,
                best,
            );
            chosen.pop();
        }
    }
}
