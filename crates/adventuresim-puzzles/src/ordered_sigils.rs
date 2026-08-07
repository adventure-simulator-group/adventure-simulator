use serde::{Deserialize, Serialize};

use super::{MAX_MINIMIZATION_SUBSETS, ORDERED_SIGIL_COUNT, ORDERED_SIGIL_RULES_VERSION};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilSpec {
    pub allow_exact: bool,
    pub allow_before: bool,
    pub allow_adjacent: bool,
    pub allow_not_at: bool,
    pub max_clues: u8,
}

impl Default for OrderedSigilSpec {
    fn default() -> Self {
        Self {
            allow_exact: true,
            allow_before: true,
            allow_adjacent: true,
            allow_not_at: true,
            max_clues: 4,
        }
    }
}

impl OrderedSigilSpec {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !(self.allow_exact || self.allow_before || self.allow_adjacent || self.allow_not_at) {
            return Err("ordered-sigil spec must enable at least one clue family");
        }
        if !(1..=8).contains(&self.max_clues) {
            return Err("ordered-sigil max clues must be between one and eight");
        }
        Ok(self)
    }

    fn allows(self, clue: &OrderedSigilClue) -> bool {
        match clue {
            OrderedSigilClue::Exact { .. } => self.allow_exact,
            OrderedSigilClue::Before { .. } => self.allow_before,
            OrderedSigilClue::Adjacent { .. } => self.allow_adjacent,
            OrderedSigilClue::NotAt { .. } => self.allow_not_at,
        }
    }
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
    pub spec: OrderedSigilSpec,
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
        Self::generate_with_spec(seed, OrderedSigilSpec::default())
            .expect("current ordered-sigil rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != ORDERED_SIGIL_RULES_VERSION {
            return Err("unsupported ordered-sigil rules version");
        }
        Self::generate_with_spec(seed, OrderedSigilSpec::default())
    }

    pub fn generate_with_spec(seed: u64, spec: OrderedSigilSpec) -> Result<Self, &'static str> {
        let spec = spec.validate()?;
        let mut rng = SplitMix64(seed ^ 0x6572_7261_6e74_7279);
        let mut solution = Sigil::ALL;
        for end in (1..solution.len()).rev() {
            let selected = (rng.next() as usize) % (end + 1);
            solution.swap(end, selected);
        }

        let (clues, _) = minimum_clues(&solution, spec)?;
        let puzzle = Self {
            rules_version: ORDERED_SIGIL_RULES_VERSION,
            seed,
            spec,
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
        self.spec.validate()?;
        if self.clues.len() > usize::from(self.spec.max_clues)
            || self.clues.iter().any(|clue| !self.spec.allows(clue))
        {
            return Err("ordered-sigil clues violate their generation spec");
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

pub fn true_clue_pool(solution: &[Sigil; ORDERED_SIGIL_COUNT]) -> Vec<OrderedSigilClue> {
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

pub fn all_orderings() -> Vec<[Sigil; ORDERED_SIGIL_COUNT]> {
    solutions(&[], usize::MAX)
}

pub fn minimum_clues(
    solution: &[Sigil; ORDERED_SIGIL_COUNT],
    spec: OrderedSigilSpec,
) -> Result<(Vec<OrderedSigilClue>, usize), &'static str> {
    let pool = true_clue_pool(solution)
        .into_iter()
        .filter(|clue| spec.allows(clue))
        .collect::<Vec<_>>();
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
    for cardinality in 1..=usize::from(spec.max_clues) {
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
    Err("ordered-sigil clue bound failed for this specification")
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

pub fn clue_holds(clue: &OrderedSigilClue, candidate: &[Sigil; ORDERED_SIGIL_COUNT]) -> bool {
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
