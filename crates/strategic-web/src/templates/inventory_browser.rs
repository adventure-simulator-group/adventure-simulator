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
    pub show_quantities: bool,
    pub show_equipped: bool,
    pub show_condition: bool,
    pub optional_columns: InventoryColumnSet,
    pub rows: Markup,
}

impl InventoryBrowser<'_> {
    pub fn render(self) -> Markup {
        let optional_columns = self.optional_columns.names();
        let table_class = if self.show_condition {
            "trade-inventory-table smith-player-inventory-table"
        } else {
            "trade-inventory-table"
        };
        html! {
            div class="inventory-browser" data-inventory-browser=(self.namespace)
                data-optional-columns=(optional_columns) {
                div class="inventory-browser-toolbar" {
                    label class="inventory-browser-search" {
                        span class="sr-only" { "Search items by name" }
                        input type="search" data-inventory-search placeholder="Search items" autocomplete="off"
                            aria-label="Search items by name";
                    }
                    @if !optional_columns.is_empty() {
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
                }
                div class="inventory-browser-table-frame" {
                table class=(table_class) {
                    colgroup {
                        col class="inventory-column-type";
                        col class="inventory-column-item";
                        @if self.show_quantities {
                            col class="inventory-column-count";
                            col class="inventory-column-target";
                        }
                        @if self.show_equipped { col class="inventory-column-equipped"; }
                        @if self.show_condition { col class="inventory-column-durability"; }
                        col class="inventory-column-weight";
                        col class="inventory-column-gold";
                        col class="inventory-column-actions";
                    }
                    thead { tr {
                        (sortable_icon_header("type", "inventory-column-type", "Item type", game_icon("Item type", "knapsack")))
                        (sortable_text_header("name", "Item", "inventory-column-item"))
                        @if self.show_quantities {
                            (sortable_icon_header("quantity", "inventory-column-count", "Quantity", game_icon("Quantity", "open-chest")))
                            (sortable_icon_header("target", "inventory-column-target", "Target quantity", game_icon("Target quantity", "eye-target")))
                        }
                        @if self.show_equipped {
                            (sortable_icon_header("equipped", "inventory-column-equipped", "Equipped", game_icon("Equipped", "check-mark")))
                        }
                        @if self.show_condition {
                            th scope="col" class="inventory-column-durability" {
                                button type="button" data-inventory-sort="durability" aria-label="Sort by durability" {
                                    (game_icon("Durability", "hammer-nails"))
                                    span class="inventory-sort-indicator" aria-hidden="true" {}
                                }
                            }
                        }
                        (sortable_icon_header("weight", "inventory-column-weight", "Weight", game_icon("Weight", "weight")))
                        (sortable_icon_header("value", "inventory-column-gold", "Currency", game_icon("Currency", "coins")))
                        th class="inventory-actions-header" aria-label="Inventory actions" {}
                    } }
                    tbody { (self.rows) }
                }
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
            show_quantities: true,
            show_equipped: false,
            show_condition: false,
            optional_columns: InventoryColumnSet::Weapons,
            rows: html! {},
        }
        .render()
        .into_string();
        assert!(rendered.contains("data-inventory-browser=\"trade-left\""));
        assert!(rendered.contains("data-inventory-sort=\"quantity\""));
        assert!(rendered.contains("data-inventory-sort=\"target\""));
        assert!(rendered.contains("aria-label=\"Sort by Quantity\""));
        assert!(rendered.contains("open-chest.svg"));
        assert!(rendered.contains("aria-label=\"Sort by Target quantity\""));
        assert!(!rendered.contains(">#<"));
        assert!(!rendered.contains(">#?<"));
        assert!(rendered.contains("accuracy,reach,penetration,damage,block"));
        assert!(rendered.contains("inventory-browser-table-frame"));
        assert!(rendered.contains("inventory-actions-header"));
    }

    #[test]
    fn condition_header_is_a_dedicated_sort_control() {
        let rendered = InventoryBrowser {
            namespace: "smith",
            show_quantities: true,
            show_equipped: true,
            show_condition: true,
            optional_columns: InventoryColumnSet::Weapons,
            rows: html! {},
        }
        .render()
        .into_string();
        assert!(rendered.contains("data-inventory-sort=\"durability\""));
        assert!(rendered.contains("hammer-nails.svg"));
        assert!(!rendered.contains("repair-all"));
    }

    #[test]
    fn merchant_inventory_can_hide_quantity_and_target_columns() {
        let rendered = InventoryBrowser {
            namespace: "merchant-left",
            show_quantities: false,
            show_equipped: false,
            show_condition: false,
            optional_columns: InventoryColumnSet::Weapons,
            rows: html! {},
        }
        .render()
        .into_string();
        assert!(!rendered.contains("inventory-column-count"));
        assert!(!rendered.contains("inventory-column-target"));
        assert!(!rendered.contains("data-inventory-sort=\"quantity\""));
        assert!(!rendered.contains("data-inventory-sort=\"target\""));
    }
}
