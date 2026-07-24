use std::{fmt, str::FromStr};

use maud::Markup;

use crate::spacetimedb::SettlementCategory;
use crate::templates::{quest_location_layout_with_session, settlement_layout_with_session};

#[derive(Clone, Debug)]
pub struct LocationView {
    pub kind: LocationKind,
    pub id: String,
    pub name: String,
    pub religion_id: Option<String>,
    pub category: Option<SettlementCategory>,
    pub active_building: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationKind {
    Settlement,
    Quest,
}

impl LocationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Settlement => "settlement",
            Self::Quest => "quest",
        }
    }
}

impl fmt::Display for LocationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LocationKind {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "settlement" => Ok(Self::Settlement),
            "quest" => Ok(Self::Quest),
            _ => Err(()),
        }
    }
}

impl LocationView {
    pub fn base_path(&self) -> String {
        format!("/locations/{}/{}", self.kind, self.id)
    }

    pub fn preserve_building(&self, path: String) -> String {
        self.active_building
            .as_deref()
            .map_or(path.clone(), |building| {
                format!(
                    "{path}{}building={building}",
                    if path.contains('?') { "&" } else { "?" }
                )
            })
    }

    pub(super) fn render_layout(
        &self,
        title: &str,
        content: Markup,
        logged_in_as: Option<&str>,
    ) -> Markup {
        if self.kind == LocationKind::Settlement {
            settlement_layout_with_session(
                title,
                &self.name,
                &self.id,
                self.category
                    .as_ref()
                    .unwrap_or(&SettlementCategory::Unknown),
                self.active_building.as_deref().unwrap_or(""),
                self.religion_id.as_deref(),
                None,
                content,
                logged_in_as,
            )
        } else {
            quest_location_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                content,
                logged_in_as,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_kind_rejects_unknown_path_segments() {
        assert_eq!("quest".parse(), Ok(LocationKind::Quest));
        assert!("merchant".parse::<LocationKind>().is_err());
    }
}
