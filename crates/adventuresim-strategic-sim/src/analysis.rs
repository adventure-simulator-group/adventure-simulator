use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    Maximize,
    Minimize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParetoPoint {
    pub id: u32,
    pub values: Vec<f64>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParetoError {
    #[error("point {id} has {actual} values; expected {expected}")]
    Dimension {
        id: u32,
        actual: usize,
        expected: usize,
    },
    #[error("point {id} contains a nonfinite value")]
    Nonfinite { id: u32 },
}

/// Return all nondominated IDs in input order. Exact ties remain on the frontier.
pub fn nondominated(
    points: &[ParetoPoint],
    objectives: &[Objective],
) -> Result<Vec<u32>, ParetoError> {
    for point in points {
        if point.values.len() != objectives.len() {
            return Err(ParetoError::Dimension {
                id: point.id,
                actual: point.values.len(),
                expected: objectives.len(),
            });
        }
        if point.values.iter().any(|v| !v.is_finite()) {
            return Err(ParetoError::Nonfinite { id: point.id });
        }
    }
    Ok(points
        .iter()
        .enumerate()
        .filter(|(i, candidate)| {
            !points
                .iter()
                .enumerate()
                .any(|(j, other)| i != &j && dominates(other, candidate, objectives))
        })
        .map(|(_, p)| p.id)
        .collect())
}

fn dominates(a: &ParetoPoint, b: &ParetoPoint, objectives: &[Objective]) -> bool {
    let mut strict = false;
    a.values
        .iter()
        .zip(&b.values)
        .zip(objectives)
        .all(|((av, bv), objective)| match objective {
            Objective::Maximize => {
                strict |= av > bv;
                av >= bv
            }
            Objective::Minimize => {
                strict |= av < bv;
                av <= bv
            }
        })
        && strict
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ties_survive_and_dominated_points_do_not() {
        let points = vec![
            ParetoPoint {
                id: 1,
                values: vec![2.0, 1.0],
            },
            ParetoPoint {
                id: 2,
                values: vec![2.0, 1.0],
            },
            ParetoPoint {
                id: 3,
                values: vec![1.0, 2.0],
            },
        ];
        assert_eq!(
            nondominated(&points, &[Objective::Maximize, Objective::Minimize]).unwrap(),
            vec![1, 2]
        );
    }
    #[test]
    fn mixed_tradeoffs_survive() {
        let points = vec![
            ParetoPoint {
                id: 1,
                values: vec![3.0, 3.0],
            },
            ParetoPoint {
                id: 2,
                values: vec![2.0, 1.0],
            },
        ];
        assert_eq!(
            nondominated(&points, &[Objective::Maximize, Objective::Minimize]).unwrap(),
            vec![1, 2]
        );
    }
    #[test]
    fn nonfinite_is_rejected() {
        assert_eq!(
            nondominated(
                &[ParetoPoint {
                    id: 7,
                    values: vec![f64::NAN]
                }],
                &[Objective::Maximize]
            ),
            Err(ParetoError::Nonfinite { id: 7 })
        );
    }
}
