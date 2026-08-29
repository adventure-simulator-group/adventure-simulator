//! Typed outcomes shared by dialogue-start boundary adapters.

use std::{convert::Infallible, fmt};

/// Observer-safe result of attempting to start a public dialogue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicDialogueStartOutcome<Started> {
    Started(Started),
    ContactUnavailable,
}

/// Failure to complete or project a public dialogue start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicDialogueStartError<ReducerError, ProjectionError = Infallible> {
    SessionProjectionMissing,
    Reducer(ReducerError),
    Projection(ProjectionError),
}

impl<ReducerError, ProjectionError> fmt::Display
    for PublicDialogueStartError<ReducerError, ProjectionError>
where
    ReducerError: fmt::Display,
    ProjectionError: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionProjectionMissing => {
                formatter.write_str("dialogue reducer completed without an owner-scoped session")
            }
            Self::Reducer(error) => error.fmt(formatter),
            Self::Projection(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_and_projection_errors_remain_layer_specific() {
        let outcome: PublicDialogueStartOutcome<u64> = PublicDialogueStartOutcome::Started(17);
        assert_eq!(outcome, PublicDialogueStartOutcome::Started(17));

        let error: PublicDialogueStartError<&str, u16> = PublicDialogueStartError::Projection(503);
        assert_eq!(error, PublicDialogueStartError::Projection(503));
    }
}
