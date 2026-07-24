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
            "raw_venison" | "raw_fowl" => Stock::Meat,
            "garlic" | "sage" | "wild_mushrooms" => Stock::Herbs,
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
                matches!(kind, CatalogKind::Food)
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
