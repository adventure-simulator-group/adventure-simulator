//! Reusable Maud components

use maud::{Markup, html};

const GAME_ICON_ROOT: &str = "/static/icons/game";

/// A locally-vendored, theme-recolourable Game Icons mask.
pub fn game_icon(label: &str, icon: &str) -> Markup {
    html! {
        span class="game-icon" style=(format!("--game-icon: url('{GAME_ICON_ROOT}/{icon}.svg')"))
            role="img" aria-label=(label) title=(label) {}
    }
}

/// A Game Icons mask that decorates adjacent visible text without creating a
/// duplicate screen-reader announcement.
pub fn decorative_game_icon(icon: &str) -> Markup {
    html! {
        span class="game-icon" style=(format!("--game-icon: url('{GAME_ICON_ROOT}/{icon}.svg')"))
            aria-hidden="true" {}
    }
}

/// Exact icon name for a seeded item. Unknown/modded items get a real fallback
/// asset rather than a URL derived from untrusted data.
pub fn item_icon_name(item_id: &str) -> &'static str {
    match item_id {
        "torch" => "torch",
        "arrow" => "plain-arrow",
        "gold_coin" => "coins",
        "bandage" => "bandage-roll",
        "travel_ration" => "bread",
        "waterskin" => "waterskin",
        "linen_tunic" => "shirt",
        "club" => "wood-club",
        "walking_staff" => "bo",
        "hand_axe" => "wood-axe",
        "flanged_mace" => "flanged-mace",
        "war_hammer" => "warhammer",
        "utility_knife" => "plain-dagger",
        "baselard" => "broad-dagger",
        "rondel_dagger" => "daggers",
        "misericorde" => "stiletto",
        "bauernwehr" => "bowie-knife",
        "katzbalger" => "sword-hilt",
        "arming_sword" => "broadsword",
        "longsword" => "ancient-sword",
        "messer" => "saber-slash",
        "kriegsmesser" => "relic-blade",
        "rapier" => "piercing-sword",
        "zweihander" => "two-handed-sword",
        "hunting_spear" => "spear-hook",
        "military_pike" => "spears",
        "halberd" => "halberd",
        "self_bow" => "pocket-bow",
        "longbow" => "bow-arrow",
        "light_crossbow" | "heavy_crossbow" => "crossbow",
        "matchlock_arquebus" => "musket",
        "hooked_arquebus" => "rifle",
        "buckler" => "bordered-shield",
        "targe" => "round-shield",
        "heater_shield" => "templar-shield",
        "round_shield" => "shield",
        "pavise" => "roman-shield",
        "arming_cap" => "helmet",
        "mail_coif" => "chain-mail",
        "kettle_hat" => "brodie-helmet",
        "barbute" => "barbute",
        "sallet" => "light-helm",
        "visored_sallet" => "visored-helm",
        "burgonet" => "crested-helmet",
        "close_helmet" => "heavy-helm",
        "quilted_sleeve" => "arm-bandage",
        "mail_sleeve" => "mailed-fist",
        "vambrace" => "bracer",
        "padded_chausses" => "trousers",
        "mail_chausses" => "armor-cuisses",
        "greave" => "greaves",
        "arming_doublet" => "sleeveless-jacket",
        "jack_of_plates" => "armor-vest",
        "brigandine" => "layered-armor",
        "mail_shirt" => "mail-shirt",
        "breastplate" => "breastplate",
        "cuirass" => "chest-armor",
        "padded_skirt" => "skirt",
        "mail_skirt" => "metal-skirt",
        "fauld" => "belt-armor",
        "tassets" => "pteruges",
        _ => "help",
    }
}

pub fn item_type_icon(item_id: &str) -> Markup {
    let readable = item_id.replace('_', " ");
    game_icon(&format!("Item type: {readable}"), item_icon_name(item_id))
}

pub fn item_type_header() -> Markup {
    html! { th scope="col" class="inventory-column-type" title="Item type" aria-label="Item type" { "T" } }
}

pub fn stat_game_icon_name(icon: &str) -> &'static str {
    match icon {
        "will" => "inner-self",
        "charisma" => "conversation",
        "medicine" => "medical-pack",
        "faith" => "holy-symbol",
        "melee" => "crossed-swords",
        "ranged" => "bullseye",
        "dodge" => "dodge",
        "block" => "shield",
        "stealth" => "hood",
        "balance" => "tightrope",
        "surgeon" => "scalpel",
        "smithing" => "anvil",
        "intelligence" => "brain",
        "instinct" => "awareness",
        "eyesight" => "eye-target",
        "hearing" => "human-ear",
        "endurance" => "heart-beats",
        "immunity" => "shield-echoes",
        "gut" => "stomach",
        "strength-arm" => "arm",
        "strength-leg" => "leg",
        "agility-arm" => "juggler",
        "agility-leg" => "wingfoot",
        _ => "help",
    }
}

/// A panel component with header and body
pub fn panel(title: &str, content: Markup) -> Markup {
    html! {
        div class="panel" {
            @if !title.is_empty() {
                div class="panel-header" { (title) }
            }
            div class="panel-body" {
                (content)
            }
        }
    }
}

/// A form input field
pub fn input_field(
    name: &str,
    label: &str,
    input_type: &str,
    required: bool,
    value: Option<&str>,
) -> Markup {
    html! {
        div class="form-group" {
            label for=(name) class="form-label" { (label) }
            input
                type=(input_type)
                id=(name)
                name=(name)
                required[required]
                value=[value];
        }
    }
}

/// Gold amount display
pub fn gold_display(amount: impl std::fmt::Display) -> Markup {
    html! {
        span class="gold-amount" {
            (amount)
            (game_icon("Gold", "coins"))
        }
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn limb_stats_use_four_distinct_icons() {
        let icons = [
            stat_game_icon_name("strength-arm"),
            stat_game_icon_name("strength-leg"),
            stat_game_icon_name("agility-arm"),
            stat_game_icon_name("agility-leg"),
        ];

        assert_eq!(icons, ["arm", "leg", "juggler", "wingfoot"]);
        for (index, icon) in icons.iter().enumerate() {
            assert!(
                !icons[..index].contains(icon),
                "duplicate limb icon: {icon}"
            );
        }
    }

    #[test]
    fn all_seeded_items_have_exact_icons_and_unknowns_fallback() {
        let mappings = [
            ("torch", "torch"),
            ("arrow", "plain-arrow"),
            ("gold_coin", "coins"),
            ("bandage", "bandage-roll"),
            ("travel_ration", "bread"),
            ("waterskin", "waterskin"),
            ("linen_tunic", "shirt"),
            ("club", "wood-club"),
            ("walking_staff", "bo"),
            ("hand_axe", "wood-axe"),
            ("flanged_mace", "flanged-mace"),
            ("war_hammer", "warhammer"),
            ("utility_knife", "plain-dagger"),
            ("baselard", "broad-dagger"),
            ("rondel_dagger", "daggers"),
            ("misericorde", "stiletto"),
            ("bauernwehr", "bowie-knife"),
            ("katzbalger", "sword-hilt"),
            ("arming_sword", "broadsword"),
            ("longsword", "ancient-sword"),
            ("messer", "saber-slash"),
            ("kriegsmesser", "relic-blade"),
            ("rapier", "piercing-sword"),
            ("zweihander", "two-handed-sword"),
            ("hunting_spear", "spear-hook"),
            ("military_pike", "spears"),
            ("halberd", "halberd"),
            ("self_bow", "pocket-bow"),
            ("longbow", "bow-arrow"),
            ("light_crossbow", "crossbow"),
            ("heavy_crossbow", "crossbow"),
            ("matchlock_arquebus", "musket"),
            ("hooked_arquebus", "rifle"),
            ("buckler", "bordered-shield"),
            ("targe", "round-shield"),
            ("heater_shield", "templar-shield"),
            ("round_shield", "shield"),
            ("pavise", "roman-shield"),
            ("arming_cap", "helmet"),
            ("mail_coif", "chain-mail"),
            ("kettle_hat", "brodie-helmet"),
            ("barbute", "barbute"),
            ("sallet", "light-helm"),
            ("visored_sallet", "visored-helm"),
            ("burgonet", "crested-helmet"),
            ("close_helmet", "heavy-helm"),
            ("quilted_sleeve", "arm-bandage"),
            ("mail_sleeve", "mailed-fist"),
            ("vambrace", "bracer"),
            ("padded_chausses", "trousers"),
            ("mail_chausses", "armor-cuisses"),
            ("greave", "greaves"),
            ("arming_doublet", "sleeveless-jacket"),
            ("jack_of_plates", "armor-vest"),
            ("brigandine", "layered-armor"),
            ("mail_shirt", "mail-shirt"),
            ("breastplate", "breastplate"),
            ("cuirass", "chest-armor"),
            ("padded_skirt", "skirt"),
            ("mail_skirt", "metal-skirt"),
            ("fauld", "belt-armor"),
            ("tassets", "pteruges"),
        ];
        assert_eq!(mappings.len(), 62);
        for (item, icon) in mappings {
            assert_eq!(item_icon_name(item), icon, "{item}");
        }
        assert_eq!(item_icon_name("modded_item"), "help");
    }

    #[test]
    fn icon_markup_is_local_accessible_and_header_is_compact() {
        let icon = item_type_icon("arming_sword").into_string();
        assert!(icon.contains("/static/icons/game/broadsword.svg"));
        assert!(icon.contains("aria-label=\"Item type: arming sword\""));
        let header = item_type_header().into_string();
        assert!(header.contains("inventory-column-type"));
        assert!(header.contains("aria-label=\"Item type\""));
        let decorative = decorative_game_icon("sun").into_string();
        assert!(decorative.contains("aria-hidden=\"true\""));
        assert!(!decorative.contains("role=\"img\""));
    }
}

/// Status badge
pub fn status_badge(status: &str) -> Markup {
    let class = match status.to_lowercase().as_str() {
        "available" => "badge badge-success",
        "accepted" => "badge badge-warning",
        "completed" => "badge badge-info",
        "ready" => "badge badge-success",
        "pending" | "searching" | "deploying" => "badge badge-warning",
        "failed" => "badge badge-danger",
        "ended" => "badge badge-info",
        _ => "badge",
    };
    html! {
        span class=(class) { (status) }
    }
}

/// Empty state placeholder
pub fn empty_state(message: &str, action_href: Option<&str>, action_label: Option<&str>) -> Markup {
    html! {
        div class="empty-state" {
            p { (message) }
            @if let (Some(href), Some(label)) = (action_href, action_label) {
                a href=(href) class="btn btn-primary" {
                    (label)
                }
            }
        }
    }
}

/// Population level description
pub fn population_description(level: i32) -> &'static str {
    match level {
        1 => "Hamlet",
        2 => "Village",
        3 => "Town",
        4 => "City",
        5 => "Capital",
        _ => "Unknown",
    }
}
