//! Settlement trade coordination and stable facade exports.

use adventuresim_core::{
    equipment::{EncumbranceSummary, INPUT_ADDRESS_MAPPINGS, InputAddressMapping},
    item_catalog_schema::{
        EquipmentChannel as CoreEquipmentChannel, EquipmentLocation as CoreEquipmentLocation,
    },
};
use adventuresim_stdb_client::{
    EquipmentChannel as SatsEquipmentChannel, IngredientPreparationAction,
};
use maud::{Markup, html};

use super::{
    character_health::stat_icon,
    character_skills::{SkillRankBarOptions, skill_rank_bar},
    chrome::{VisualStageKind, party_portrait_overlay, visual_stage},
    context::LocationView,
    rest::{RestServiceKind, SoapRestPreview, rest_service_menu},
    social::{
        forge_description_stage, npc_description_stage, npc_location_id, npc_portrait_strip,
        player_chat_area, settlement_chat_area, settlement_resident_chat_area,
    },
};
use crate::spacetimedb::{
    BackendIngredientPreparationPlan, CharacterEquipmentGraph, CharacterView, EquipmentBodyPart,
    EquipmentLocation, FoodLot, InventoryItem, InventoryItemAmount, InventoryQuantityTarget,
    ItemConditionExt, PartyInventoryItem, SettlementView,
};
use crate::templates::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use crate::templates::{
    decorative_game_icon, empty_state, game_icon, item_display_name, item_icon_name,
    item_source_edit_url, item_type_header, item_type_icon, settlement_layout_with_session,
    sidebar_section,
};

mod discard;
mod equipment;
mod inventory;
mod merchant;
mod party_pool;
mod party_transfer;
mod repairs;

pub use discard::party_discard_page;
pub use merchant::{MerchantShop, live_merchant_shop_page, merchants_page};
pub use party_pool::party_pool_page;
pub use party_transfer::party_inventory_page;

pub(in crate::templates) use inventory::{
    inventory_footer_controls, item_name_with_food_lot, item_name_with_quality, transfer_glyph,
};
pub(super) use inventory::{item_name_with_display, trade_inventory_table_header};
