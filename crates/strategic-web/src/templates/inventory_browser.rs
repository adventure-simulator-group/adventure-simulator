//! Shared, progressively enhanced inventory browser presentation.
//!
//! Reducer-specific row controls remain supplied by each workflow, while this
//! component owns the browser contract: panel identity, searchable/sortable
//! columns, optional-column availability, and the quantity/target split.

use maud::{Markup, html};

use super::game_icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryColumnSet {
    Basic,
    Weapons,
    Armor,
    All,
}

impl InventoryColumnSet {
    fn names(self) -> &'static str {
        match self {
            Self::Basic => "",
            Self::Weapons => "accuracy,reach,penetration,damage,block",
            Self::Armor => "coverage,resistance,padding,flexibility,range-of-motion",
            Self::All => {
                "accuracy,reach,penetration,damage,block,coverage,resistance,padding,flexibility,range-of-motion"
            }
        }
    }
}

pub struct InventoryBrowser<'a> {
    /// Stable URL-state namespace. Left and right panels must never share it.
    pub namespace: &'a str,
    pub show_equipped: bool,
    pub condition_header: Option<Markup>,
    pub optional_columns: InventoryColumnSet,
    pub rows: Markup,
}

impl InventoryBrowser<'_> {
    pub fn render(self) -> Markup {
        let show_condition = self.condition_header.is_some();
        let table_class = if show_condition {
            "trade-inventory-table smith-player-inventory-table"
        } else {
            "trade-inventory-table"
        };
        html! {
            div class="inventory-browser" data-inventory-browser=(self.namespace)
                data-optional-columns=(self.optional_columns.names()) {
                div class="inventory-browser-toolbar" {
                    label class="inventory-browser-search" {
                        span class="sr-only" { "Search items by name" }
                        input type="search" data-inventory-search placeholder="Search items" autocomplete="off"
                            aria-label="Search items by name";
                    }
                    details class="inventory-column-picker" {
                        summary data-inventory-columns aria-label="Choose visible columns" title="Choose visible columns" {
                            span aria-hidden="true" { "⚙" }
                        }
                        fieldset {
                            legend { "Columns" }
                            div data-inventory-column-options {}
                        }
                    }
                }
                table class=(table_class) {
                    colgroup {
                        col class="inventory-column-type";
                        col class="inventory-column-item";
                        col class="inventory-column-count";
                        col class="inventory-column-target";
                        @if self.show_equipped { col class="inventory-column-equipped"; }
                        @if show_condition { col class="inventory-column-durability"; }
                        col class="inventory-column-weight";
                        col class="inventory-column-gold";
                    }
                    thead { tr {
                        (sortable_icon_header("type", "inventory-column-type", "Item type", game_icon("Item type", "knapsack")))
                        (sortable_text_header("name", "Item", "inventory-column-item"))
                        (sortable_text_header("quantity", "#", "inventory-column-count"))
                        (sortable_text_header("target", "#?", "inventory-column-target"))
                        @if self.show_equipped {
                            (sortable_icon_header("equipped", "inventory-column-equipped", "Equipped", game_icon("Equipped", "check-mark")))
                        }
                        @if let Some(condition_header) = self.condition_header {
                            th scope="col" class="inventory-column-durability" {
                                button type="button" data-inventory-sort="durability" aria-label="Sort by durability" {
                                    span class="sr-only" { "Durability" }
                                    span class="inventory-sort-indicator" aria-hidden="true" {}
                                }
                                (condition_header)
                            }
                        }
                        (sortable_icon_header("weight", "inventory-column-weight", "Weight", game_icon("Weight", "weight")))
                        (sortable_icon_header("value", "inventory-column-gold", "Currency", game_icon("Currency", "coins")))
                    } }
                    tbody { (self.rows) }
                }
            }
        }
    }
}

fn sortable_text_header(key: &str, label: &str, class: &str) -> Markup {
    html! { th scope="col" class=(class) { button type="button" data-inventory-sort=(key) aria-label=(format!("Sort by {label}")) { (label) span class="inventory-sort-indicator" aria-hidden="true" {} } } }
}

fn sortable_icon_header(key: &str, class: &str, label: &str, icon: Markup) -> Markup {
    html! { th scope="col" class=(class) title=(label) { button type="button" data-inventory-sort=(key) aria-label=(format!("Sort by {label}")) { (icon) span class="inventory-sort-indicator" aria-hidden="true" {} } } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_independent_state_namespace_and_quantity_target_headers() {
        let rendered = InventoryBrowser {
            namespace: "trade-left",
            show_equipped: false,
            condition_header: None,
            optional_columns: InventoryColumnSet::Weapons,
            rows: html! {},
        }
        .render()
        .into_string();
        assert!(rendered.contains("data-inventory-browser=\"trade-left\""));
        assert!(rendered.contains("data-inventory-sort=\"quantity\""));
        assert!(rendered.contains("data-inventory-sort=\"target\""));
        assert!(rendered.contains("accuracy,reach,penetration,damage,block"));
    }

    #[test]
    fn condition_header_controls_are_siblings_of_the_sort_button() {
        let rendered = InventoryBrowser {
            namespace: "smith",
            show_equipped: true,
            condition_header: Some(html! { form class="repair-all" { button { "Repair" } } }),
            optional_columns: InventoryColumnSet::Weapons,
            rows: html! {},
        }
        .render()
        .into_string();
        assert!(rendered.contains("</button><form class=\"repair-all\">"));
        assert!(!rendered.contains("<button type=\"button\" data-inventory-sort=\"durability\" aria-label=\"Sort by durability\"><span class=\"sr-only\">Durability</span><span class=\"inventory-sort-indicator\" aria-hidden=\"true\"></span><form"));
    }
}
