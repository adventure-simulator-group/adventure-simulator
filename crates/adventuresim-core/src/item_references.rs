//! Stable item IDs consumed by gameplay systems outside the item catalogue.

pub const CURRENCY_IDS: [&str; 6] = [
    "rhenish_gulden",
    "lubeck_mark",
    "hamburg_mark",
    "saxon_thaler",
    "brandenburg_groschen",
    "danish_mark",
];

pub const MEDICATION_IDS: [&str; 14] = [
    "oral_rehydration_draught",
    "weak_willow_decoction",
    "cooling_willow_draught",
    "strong_willow_decoction",
    "weak_comfrey_poultice",
    "comfrey_poultice",
    "fine_comfrey_poultice",
    "weak_poppy_tincture",
    "poppy_tincture",
    "strong_poppy_tincture",
    "weak_sage_infusion",
    "sage_infusion",
    "fine_sage_infusion",
    "honey_wound_dressing",
];

pub const STANDARD_TRAVEL_RATION_ID: &str = "travel_ration";
pub const STANDARD_WATERSKIN_ID: &str = "waterskin";
pub const ARROW_ID: &str = "arrow";
pub const SOFT_SOAP_ID: &str = "soft_soap";
pub const FIELD_TENT_ID: &str = "field_tent";
pub const SURGERY_KIT_ID: &str = "surgery_kit";
pub const TAVERN_DRINK_ITEM_ID: &str = "table_wine";

pub const REQUIRED_GAMEPLAY_ITEM_IDS: [&str; 16] = [
    "arrow",
    "bandage",
    "cooked_meal",
    "cooking_pan",
    "cooking_pot",
    "field_tent",
    "portable_oven",
    "small_beer",
    "table_wine",
    "aqua_vitae",
    "soft_soap",
    "splint",
    "surgery_kit",
    "torch",
    "travel_ration",
    "waterskin",
];
