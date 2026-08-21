pub mod parameter;
use indexmap::IndexMap;
pub use parameter::*;

#[derive(Clone, Debug)]
pub struct PassParameters {
    pub parameters: IndexMap<String, PassParameter>,
}

impl PassParameters {
    pub fn new() -> Self {
        Self {
            parameters: IndexMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<PassParameter>) {
        self.parameters.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&PassParameter> {
        self.parameters.get(key)
    }
}

impl From<IndexMap<String, PassParameter>> for PassParameters {
    fn from(value: IndexMap<String, PassParameter>) -> Self {
        Self { parameters: value }
    }
}

impl From<PassParameters> for IndexMap<String, PassParameter> {
    fn from(value: PassParameters) -> Self {
        value.parameters
    }
}
