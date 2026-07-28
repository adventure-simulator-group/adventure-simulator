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
    // Legacy synthetic settlement-balance rows are not catalog items.
    if item_id == "coin" {
        return "coins";
    }
    adventuresim_core::item_catalog::definition(item_id)
        .map(|item| item.presentation.icon.as_str())
        .unwrap_or("help")
}

pub fn item_type_icon(item_id: &str) -> Markup {
    let readable = item_display_name(item_id);
    game_icon(&format!("Item type: {readable}"), item_icon_name(item_id))
}

/// Turn a stable snake-case item identifier into player-facing copy without
/// changing the identifier used by forms or client-side behavior.
pub fn item_display_name(item_id: &str) -> String {
    if let Some(item) = adventuresim_core::item_catalog::definition(item_id) {
        return item.display_name.clone();
    }
    let mut readable = item_id.replace('_', " ");
    if let Some(first) = readable.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    readable
}

/// Resolve a compiled item source location to the same centrally configured
/// GitHub editor used by dialogue developer links.
pub fn item_source_edit_url(item_id: &str) -> Option<String> {
    let source = adventuresim_core::item_catalog::source_for_item(item_id)?;
    adventuresim_dialogue::github_edit_url_for_location(
        "adventure-simulator-group/adventure-simulator",
        option_env!("ADVENTURESIM_SOURCE_REF").unwrap_or("main"),
        &source.file,
        source.line,
    )
}

pub fn item_type_header() -> Markup {
    html! {
        th scope="col" class="inventory-column-type" title="Item type" aria-label="Item type" {
            (decorative_game_icon("knapsack"))
        }
    }
}

pub fn stat_game_icon_name(icon: &str) -> &'static str {
    match icon {
        "will" => "inner-self",
        "social" => "conversation",
        "insight" => "awareness",
        "self-awareness" => "inner-self",
        "charm" => "rose",
        "command" => "crown",
        "deception" => "conversation",
        "physiology" => "caduceus",
        "cooking" => "meal",
        "faith" => "holy-symbol",
        "religion" => "holy-symbol",
        "melee" => "sword-clash",
        "combat" => "crossed-swords",
        "crossed-swords" => "crossed-swords",
        "archery-target" => "bullseye",
        "ranged" => "bullseye",
        "shield" => "shield",
        "spear-hook" => "spear-hook",
        "battle-axe" => "wood-axe",
        "flanged-mace" => "flanged-mace",
        "sword" => "sword-brandish",
        "bowie-knife" => "bowie-knife",
        "bow-arrow" => "bow-arrow",
        "crossbow" => "crossbow",
        "musket" => "musket",
        "throwing-ball" => "plain-arrow",
        "dodge" => "acrobatic",
        "block" => "shield",
        "stealth" => "hood",
        "balance" => "tightrope",
        "surgeon" => "scalpel",
        "sewing-needle" => "clothes",
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

/// Resolve the source used by a stat mask. The original limb and immunity
/// artwork remains clearer at the compact sizes used by the attribute rail.
pub fn stat_icon_path(category: &str, icon: &str) -> String {
    if category == "bestiary" {
        format!("/static/icons/stats/bestiary/{icon}.png")
    } else if category == "terrain" {
        format!("/static/icons/stats/terrain/{icon}.png")
    } else if category == "attributes"
        && matches!(
            icon,
            "strength-arm" | "strength-leg" | "agility-arm" | "agility-leg" | "immunity"
        )
    {
        format!("/static/icons/stats/attributes/{icon}.png")
    } else {
        format!("{GAME_ICON_ROOT}/{}.svg", stat_game_icon_name(icon))
    }
}

/// Resolve the shared denomination symbol used by skills and settlement temples.
pub fn religion_icon_path(religion_id: Option<&str>) -> &'static str {
    match religion_id {
        Some("roman_catholic") => "/static/icons/religion/catholic-cross-bottony.png",
        Some("lutheran") => "/static/icons/religion/luther-rose.svg",
        Some("reformed") => "/static/icons/religion/huguenot-cross.svg",
        Some("anglican") => "/static/icons/religion/canterbury-cross.svg",
        Some("eastern_orthodox") => "/static/icons/religion/orthodox-cross.svg",
        Some("islamic") => "/static/icons/religion/fontawesome-star-and-crescent.svg",
        Some("judaism") => "/static/icons/religion/fontawesome-star-of-david.svg",
        _ => "/static/icons/religion/fontawesome-cross.svg",
    }
}

pub fn religion_icon(label: &str, religion_id: Option<&str>, decorative: bool) -> Markup {
    html! {
        span class="game-icon" style=(format!("--game-icon: url('{}')", religion_icon_path(religion_id)))
            role=[(!decorative).then_some("img")]
            aria-label=[(!decorative).then_some(label)]
            title=[(!decorative).then_some(label)]
            aria-hidden=[decorative.then_some("true")] {}
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

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn limb_and_immunity_stats_use_the_original_artwork() {
        let paths = [
            stat_icon_path("attributes", "strength-arm"),
            stat_icon_path("attributes", "strength-leg"),
            stat_icon_path("attributes", "agility-arm"),
            stat_icon_path("attributes", "agility-leg"),
            stat_icon_path("attributes", "immunity"),
        ];

        for (path, icon) in paths.iter().zip([
            "strength-arm",
            "strength-leg",
            "agility-arm",
            "agility-leg",
            "immunity",
        ]) {
            assert_eq!(path, &format!("/static/icons/stats/attributes/{icon}.png"));
        }
    }

    #[test]
    fn terrain_skill_family_uses_generated_local_masks() {
        for icon in ["terrain", "plains", "forest", "hills", "wetlands", "urban"] {
            assert_eq!(
                stat_icon_path("terrain", icon),
                format!("/static/icons/stats/terrain/{icon}.png")
            );
        }
    }

    #[test]
    fn bestiary_skill_family_uses_generated_local_masks() {
        for icon in [
            "bestiary",
            "beast",
            "undead",
            "human",
            "werekin",
            "elf",
            "dwarf",
            "fey",
            "spirit",
            "greenskin",
            "insectoid",
            "draconid",
            "construct",
            "wildmen",
        ] {
            assert_eq!(
                stat_icon_path("bestiary", icon),
                format!("/static/icons/stats/bestiary/{icon}.png")
            );
        }
    }

    #[test]
    fn requested_game_icon_replacements_and_faith_icons_are_exact() {
        assert_eq!(stat_game_icon_name("dodge"), "acrobatic");
        assert_eq!(stat_game_icon_name("combat"), "crossed-swords");
        assert_eq!(stat_game_icon_name("melee"), "sword-clash");
        assert_ne!(
            stat_game_icon_name("combat"),
            stat_game_icon_name("melee"),
            "the Combat aggregate needs a distinct icon from its Melee detail row"
        );
        for (key, expected) in [
            ("social", "conversation"),
            ("insight", "awareness"),
            ("self-awareness", "inner-self"),
            ("charm", "rose"),
            ("command", "crown"),
            ("deception", "conversation"),
            ("physiology", "caduceus"),
        ] {
            assert_eq!(stat_game_icon_name(key), expected);
            assert_ne!(stat_game_icon_name(key), "help");
        }
        assert_eq!(
            religion_icon_path(Some("roman_catholic")),
            "/static/icons/religion/catholic-cross-bottony.png"
        );
        assert_eq!(
            religion_icon_path(Some("lutheran")),
            "/static/icons/religion/luther-rose.svg"
        );
        assert_eq!(
            religion_icon_path(Some("reformed")),
            "/static/icons/religion/huguenot-cross.svg"
        );
        assert_eq!(
            religion_icon_path(Some("anglican")),
            "/static/icons/religion/canterbury-cross.svg"
        );
        assert_eq!(
            religion_icon_path(Some("eastern_orthodox")),
            "/static/icons/religion/orthodox-cross.svg"
        );
        assert_eq!(
            religion_icon_path(Some("islamic")),
            "/static/icons/religion/fontawesome-star-and-crescent.svg"
        );
        assert_eq!(
            religion_icon_path(Some("judaism")),
            "/static/icons/religion/fontawesome-star-of-david.svg"
        );
        assert_eq!(
            religion_icon_path(None),
            "/static/icons/religion/fontawesome-cross.svg"
        );
    }

    #[test]
    fn all_rendered_skill_and_party_check_icons_avoid_the_help_placeholder() {
        for (key, expected) in [
            ("sewing-needle", "clothes"),
            ("crossed-swords", "crossed-swords"),
            ("archery-target", "bullseye"),
            ("shield", "shield"),
            ("spear-hook", "spear-hook"),
            ("battle-axe", "wood-axe"),
            ("flanged-mace", "flanged-mace"),
            ("sword", "sword-brandish"),
            ("bowie-knife", "bowie-knife"),
            ("bow-arrow", "bow-arrow"),
            ("crossbow", "crossbow"),
            ("musket", "musket"),
            ("throwing-ball", "plain-arrow"),
            ("religion", "holy-symbol"),
            ("physiology", "caduceus"),
        ] {
            assert_eq!(stat_game_icon_name(key), expected, "{key}");
            assert_ne!(stat_game_icon_name(key), "help", "{key}");
        }
    }

    #[test]
    fn all_seeded_items_have_exact_icons_and_unknowns_fallback() {
        let mappings = [
            ("torch", "torch"),
            ("arrow", "plain-arrow"),
            ("rhenish_gulden", "coins"),
            ("lubeck_mark", "coins"),
            ("hamburg_mark", "coins"),
            ("saxon_thaler", "coins"),
            ("brandenburg_groschen", "coins"),
            ("danish_mark", "coins"),
            ("bandage", "bandage-roll"),
            ("travel_ration", "bread"),
            ("waterskin", "waterskin"),
            ("small_beer", "beer-stein"),
            ("table_wine", "beer-stein"),
            ("aqua_vitae", "beer-stein"),
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
        assert_eq!(mappings.len(), 70);
        for (item, icon) in mappings {
            assert_eq!(item_icon_name(item), icon, "{item}");
        }
        assert_eq!(item_icon_name("modded_item"), "help");
        assert_eq!(item_icon_name("coin"), "coins");
    }

    #[test]
    fn icon_markup_is_local_accessible_and_header_is_compact() {
        let icon = item_type_icon("arming_sword").into_string();
        assert!(icon.contains("/static/icons/game/broadsword.svg"));
        assert!(icon.contains("aria-label=\"Item type: Arming sword\""));
        let header = item_type_header().into_string();
        assert!(header.contains("inventory-column-type"));
        assert!(header.contains("aria-label=\"Item type\""));
        let decorative = decorative_game_icon("sun").into_string();
        assert!(decorative.contains("aria-hidden=\"true\""));
        assert!(!decorative.contains("role=\"img\""));
    }

    #[test]
    fn item_ids_are_humanized_for_display() {
        assert_eq!(item_display_name("arming_sword"), "Arming sword");
        assert_eq!(item_display_name("torch"), "Torch");
        assert_eq!(item_display_name(""), "");
    }

    #[test]
    fn item_source_links_use_the_compiled_location_and_configured_ref() {
        let url = item_source_edit_url("arming_sword").unwrap();
        assert!(
            url.starts_with(
                "https://github.com/adventure-simulator-group/adventure-simulator/edit/"
            )
        );
        assert!(url.contains("/content/items/catalog.yaml#L"));
        assert_eq!(item_source_edit_url("modded_item"), None);
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
