//! Data model for the standalone MHR character creator.

pub mod export;

use serde::{Deserialize, Serialize};

pub const IDENTITY_COUNT: usize = 45;
pub const EXPRESSION_COUNT: usize = 72;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterRecipe {
    pub version: u8,
    pub name: String,
    pub identity: Vec<f32>,
    pub expression: Vec<f32>,
}

impl Default for CharacterRecipe {
    fn default() -> Self {
        Self {
            version: 1,
            name: "New adventurer".into(),
            identity: vec![0.0; IDENTITY_COUNT],
            expression: vec![0.0; EXPRESSION_COUNT],
        }
    }
}

impl CharacterRecipe {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported character recipe version {}",
                self.version
            ));
        }
        if self.identity.len() != IDENTITY_COUNT || self.expression.len() != EXPRESSION_COUNT {
            return Err("recipe has the wrong MHR coefficient counts".into());
        }
        if self.name.trim().is_empty() {
            return Err("character name cannot be empty".into());
        }
        if self
            .identity
            .iter()
            .chain(&self.expression)
            .any(|value| !value.is_finite())
        {
            return Err("recipe contains a non-finite coefficient".into());
        }
        Ok(())
    }

    pub fn reset_body(&mut self) {
        self.identity.fill(0.0);
    }
    pub fn reset_face(&mut self) {
        self.expression.fill(0.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityGroup {
    Body,
    Head,
    Hands,
}

impl IdentityGroup {
    pub const ALL: [Self; 3] = [Self::Body, Self::Head, Self::Hands];
    pub fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Head => "Head",
            Self::Hands => "Hands",
        }
    }
    pub fn range(self) -> std::ops::Range<usize> {
        match self {
            Self::Body => 0..20,
            Self::Head => 20..40,
            Self::Hands => 40..45,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_recipe_matches_mhr_layout() {
        let recipe = CharacterRecipe::default();
        assert!(recipe.validate().is_ok());
        assert_eq!(
            IdentityGroup::ALL
                .iter()
                .map(|g| g.range().len())
                .sum::<usize>(),
            45
        );
    }

    #[test]
    fn validation_rejects_corrupt_recipe() {
        let mut recipe = CharacterRecipe::default();
        recipe.identity.pop();
        assert!(recipe.validate().is_err());
    }
}
