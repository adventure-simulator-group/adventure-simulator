//! Shared authoritative settlement storefront predicates.

use adventuresim_world_schema::{
    SettlementEconomyProfile, SettlementService as Service, StockCategory as Stock,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Storefront {
    General,
    Weapons,
    Armor,
    Clothing,
    Herbalist,
    Inn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementNpcTab {
    pub location_id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementActionService {
    Inn,
    Temple,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementDowntimeAccess {
    PublicService { at_inn: bool },
    PrivateSystem,
}

pub const fn action_service_location_id(service: SettlementActionService) -> &'static str {
    match service {
        SettlementActionService::Inn => "inn",
        SettlementActionService::Temple => "church",
    }
}

pub fn action_service_available(
    profile: &SettlementEconomyProfile,
    service: SettlementActionService,
) -> bool {
    match service {
        SettlementActionService::Inn => storefront_available(profile, Storefront::Inn),
        SettlementActionService::Temple => profile.has_service(Service::Temple),
    }
}

/// Maps the compatibility rest flag at the public reducer boundary.
pub const fn required_settlement_rest_service(
    access: SettlementDowntimeAccess,
) -> Option<SettlementActionService> {
    match access {
        SettlementDowntimeAccess::PublicService { at_inn: true } => {
            Some(SettlementActionService::Inn)
        }
        SettlementDowntimeAccess::PublicService { at_inn: false } => {
            Some(SettlementActionService::Temple)
        }
        SettlementDowntimeAccess::PrivateSystem => None,
    }
}

/// Deterministic venue selection for strategic automation. Prefer the
/// no-charge Church and otherwise use an available Inn; no service means no
/// valid call to a public settlement-rest reducer.
pub fn available_settlement_rest_service(
    profile: &SettlementEconomyProfile,
) -> Option<SettlementActionService> {
    select_available_settlement_rest_service(
        action_service_available(profile, SettlementActionService::Inn),
        action_service_available(profile, SettlementActionService::Temple),
    )
}

pub const fn select_available_settlement_rest_service(
    inn_available: bool,
    temple_available: bool,
) -> Option<SettlementActionService> {
    if temple_available {
        Some(SettlementActionService::Temple)
    } else if inn_available {
        Some(SettlementActionService::Inn)
    } else {
        None
    }
}

pub const fn action_service_at_inn(service: SettlementActionService) -> bool {
    matches!(service, SettlementActionService::Inn)
}

pub fn player_visible_npc_tabs(
    profile: &SettlementEconomyProfile,
    has_keep: bool,
) -> Vec<SettlementNpcTab> {
    let mut tabs = vec![
        SettlementNpcTab {
            location_id: "overview",
            label: "Public square",
        },
        SettlementNpcTab {
            location_id: "residences",
            label: "Residences",
        },
    ];
    if has_keep {
        tabs.push(SettlementNpcTab {
            location_id: "keep",
            label: "Keep",
        });
    }
    for (available, location_id, label) in [
        (
            storefront_available(profile, Storefront::General),
            "market",
            "General Market",
        ),
        (
            storefront_available(profile, Storefront::Weapons),
            "forge",
            "Weapons",
        ),
        (
            storefront_available(profile, Storefront::Armor),
            "armoury",
            "Armour",
        ),
        (
            storefront_available(profile, Storefront::Clothing),
            "tailor",
            "Clothing",
        ),
        (
            storefront_available(profile, Storefront::Herbalist),
            "herbalist",
            "Herbalist",
        ),
        (
            action_service_available(profile, SettlementActionService::Inn),
            action_service_location_id(SettlementActionService::Inn),
            "Inn",
        ),
        (
            action_service_available(profile, SettlementActionService::Temple),
            action_service_location_id(SettlementActionService::Temple),
            "Church",
        ),
    ] {
        if available {
            tabs.push(SettlementNpcTab { location_id, label });
        }
    }
    tabs
}

pub fn visible_npc_tab<'a>(
    tabs: &'a [SettlementNpcTab],
    location_id: &str,
) -> Option<&'a SettlementNpcTab> {
    tabs.iter().find(|tab| tab.location_id == location_id)
}

pub fn npc_location_is_navigable(
    profile: &SettlementEconomyProfile,
    has_keep: bool,
    location_id: &str,
) -> bool {
    visible_npc_tab(&player_visible_npc_tabs(profile, has_keep), location_id).is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogKind {
    Simple,
    Weapon,
    Armor,
    Shield,
    Clothing,
    Ingredient,
    Medication,
    Food,
    Currency,
}

pub fn item_stock_category(id: &str, kind: CatalogKind) -> Option<Stock> {
    Some(match kind {
        CatalogKind::Currency => return None,
        CatalogKind::Weapon | CatalogKind::Shield => Stock::Weapons,
        CatalogKind::Armor => Stock::Armor,
        CatalogKind::Clothing => Stock::Cloth,
        CatalogKind::Ingredient | CatalogKind::Medication => Stock::Herbs,
        CatalogKind::Simple => match id {
            "cooking_pan" | "cooking_pot" | "portable_oven" => Stock::Metalwares,
            _ => Stock::GeneralGoods,
        },
        CatalogKind::Food => match id {
            "oat_grain" | "rye_bread" | "travel_ration" => Stock::Grain,
            "raw_venison" | "raw_fowl" | "raw_fish" | "raw_beast_meat" => Stock::Meat,
            "garlic" | "sage" | "wild_mushrooms" | "salt" | "mustard" | "horseradish" | "honey"
            | "vinegar" => Stock::Herbs,
            "butter" | "lard" => Stock::Meat,
            "sour_cherries" => Stock::GeneralGoods,
            _ => Stock::GeneralGoods,
        },
    })
}

pub fn storefront_available(profile: &SettlementEconomyProfile, storefront: Storefront) -> bool {
    match storefront {
        Storefront::General => {
            profile.has_service(Service::Market) || profile.has_service(Service::GeneralStore)
        }
        Storefront::Weapons => {
            profile.has_service(Service::Weaponsmith)
                || profile.has_service(Service::GeneralBlacksmith)
        }
        Storefront::Armor => {
            profile.has_service(Service::Armorer) || profile.has_service(Service::GeneralBlacksmith)
        }
        Storefront::Clothing => profile.has_service(Service::Tailor),
        Storefront::Herbalist => profile.has_service(Service::Herbalist),
        Storefront::Inn => profile.has_service(Service::Inn),
    }
}

pub fn storefront_stocks(
    profile: &SettlementEconomyProfile,
    storefront: Storefront,
    id: &str,
    kind: CatalogKind,
) -> bool {
    if !storefront_available(profile, storefront) {
        return false;
    }
    // A market or inn is always a viable place to outfit a basic journey.
    // These two staples deliberately do not depend on a settlement's broader
    // commodity profile: the travel planner must never direct a player to an
    // exposed storefront that cannot sell the provisions it just recommended.
    if matches!(storefront, Storefront::General | Storefront::Inn)
        && matches!(id, "travel_ration" | "waterskin")
    {
        return true;
    }
    let Some(category) = item_stock_category(id, kind) else {
        return false;
    };
    let category_available = profile.stock.iter().any(|entry| entry.category == category);
    category_available
        && match storefront {
            Storefront::General => !matches!(
                kind,
                CatalogKind::Ingredient
                    | CatalogKind::Medication
                    | CatalogKind::Weapon
                    | CatalogKind::Shield
                    | CatalogKind::Armor
                    | CatalogKind::Clothing
                    | CatalogKind::Food
            ),
            Storefront::Weapons => matches!(kind, CatalogKind::Weapon | CatalogKind::Shield),
            Storefront::Armor => kind == CatalogKind::Armor,
            Storefront::Clothing => kind == CatalogKind::Clothing,
            Storefront::Herbalist => matches!(
                kind,
                CatalogKind::Ingredient | CatalogKind::Medication | CatalogKind::Food
            ),
            Storefront::Inn => {
                (matches!(kind, CatalogKind::Food) || crate::food::definition(id).is_some())
                    || matches!(id, "cooking_pan" | "cooking_pot" | "portable_oven")
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::{ProfileFactProvenance, SettlementStock};
    fn profile(services: Vec<Service>, stock: Vec<Stock>) -> SettlementEconomyProfile {
        let mut value = SettlementEconomyProfile::stage_placeholder();
        value.services = services;
        value.services.sort();
        value.stock = stock
            .into_iter()
            .map(|category| SettlementStock {
                category,
                abundance: 1,
                provenance: ProfileFactProvenance::DeterministicGapFill,
            })
            .collect();
        value.stock.sort_by_key(|v| v.category);
        value
    }
    #[test]
    fn storefront_needs_both_service_and_stock() {
        let p = profile(vec![Service::Weaponsmith], vec![Stock::GeneralGoods]);
        assert!(!storefront_stocks(
            &p,
            Storefront::Weapons,
            "club",
            CatalogKind::Weapon
        ));
    }

    #[test]
    fn markets_and_inns_always_stock_basic_travel_provisions() {
        let market = profile(vec![Service::Market], Vec::new());
        let inn = profile(vec![Service::Inn], Vec::new());

        for profile in [&market, &inn] {
            let storefront = if profile.has_service(Service::Inn) {
                Storefront::Inn
            } else {
                Storefront::General
            };
            assert!(storefront_stocks(
                profile,
                storefront,
                "travel_ration",
                CatalogKind::Food,
            ));
            assert!(storefront_stocks(
                profile,
                storefront,
                "waterskin",
                CatalogKind::Simple,
            ));
        }
    }

    #[test]
    fn inns_stock_dual_role_cooking_ingredients_without_reclassifying_them() {
        let inn = profile(vec![Service::Inn], vec![Stock::Herbs]);
        for id in ["garlic", "sage", "honey", "vinegar"] {
            assert!(storefront_stocks(
                &inn,
                Storefront::Inn,
                id,
                CatalogKind::Ingredient
            ));
        }
    }

    #[test]
    fn generalist_blacksmith_can_stock_bounded_weapons() {
        let p = profile(vec![Service::GeneralBlacksmith], vec![Stock::Weapons]);
        assert!(storefront_stocks(
            &p,
            Storefront::Weapons,
            "club",
            CatalogKind::Weapon
        ));
    }

    #[test]
    fn simulator_selects_only_an_available_rest_venue() {
        assert_eq!(
            select_available_settlement_rest_service(true, true),
            Some(SettlementActionService::Temple)
        );
        assert_eq!(
            select_available_settlement_rest_service(false, true),
            Some(SettlementActionService::Temple)
        );
        assert_eq!(select_available_settlement_rest_service(false, false), None);
        assert!(action_service_at_inn(SettlementActionService::Inn));
        assert!(!action_service_at_inn(SettlementActionService::Temple));
        assert_eq!(
            required_settlement_rest_service(SettlementDowntimeAccess::PrivateSystem),
            None,
            "party synchronization and companion convalescence are venue-neutral"
        );
        assert_eq!(
            required_settlement_rest_service(SettlementDowntimeAccess::PublicService {
                at_inn: false,
            }),
            Some(SettlementActionService::Temple)
        );
    }

    #[test]
    fn npc_navigation_rejects_hidden_services_and_impossible_keeps() {
        let p = profile(vec![Service::Inn], vec![Stock::GeneralGoods]);
        assert!(npc_location_is_navigable(&p, false, "inn"));
        assert!(npc_location_is_navigable(&p, false, "residences"));
        assert!(!npc_location_is_navigable(&p, false, "church"));
        assert!(!npc_location_is_navigable(&p, false, "armoury"));
        assert!(!npc_location_is_navigable(&p, false, "keep"));
        assert!(npc_location_is_navigable(&p, true, "keep"));
    }

    #[test]
    fn inferred_general_blacksmith_stocks_limited_weapon_and_armor_categories() {
        let industries = adventuresim_world_schema::InferredIndustryProfile::new(vec![
            adventuresim_world_schema::IndustryEvidence::Fallback(
                adventuresim_world_schema::FallbackIndustry::CommonAggregate,
            ),
        ])
        .unwrap();
        let general =
            adventuresim_world_schema::infer_settlement_economy(2, 500, 1, false, &industries)
                .unwrap();
        assert!(storefront_stocks(
            &general,
            Storefront::Weapons,
            "club",
            CatalogKind::Weapon
        ));
        assert!(storefront_stocks(
            &general,
            Storefront::Armor,
            "leather vest",
            CatalogKind::Armor
        ));
        assert_eq!(
            general
                .stock
                .iter()
                .find(|s| s.category == Stock::Weapons)
                .unwrap()
                .abundance,
            1
        );
        assert_eq!(
            general
                .stock
                .iter()
                .find(|s| s.category == Stock::Armor)
                .unwrap()
                .abundance,
            1
        );
        let specialist =
            adventuresim_world_schema::infer_settlement_economy(4, 5_000, 4, true, &industries)
                .unwrap();
        assert!(
            specialist
                .stock
                .iter()
                .find(|s| s.category == Stock::Weapons)
                .unwrap()
                .abundance
                >= 2
        );
        assert!(
            specialist
                .stock
                .iter()
                .find(|s| s.category == Stock::Armor)
                .unwrap()
                .abundance
                >= 2
        );
    }
}
