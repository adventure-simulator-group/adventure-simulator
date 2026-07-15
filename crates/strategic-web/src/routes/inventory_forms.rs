//! Parsing boundary for the browser's compact inventory form encoding.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuantityEntry<T> {
    pub id: T,
    pub quantity: u32,
}

fn parse_entries<T: std::str::FromStr>(
    ids: &str,
    quantities: &str,
) -> Result<Vec<QuantityEntry<T>>, &'static str> {
    let ids = ids
        .split(',')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let quantities = quantities
        .split(',')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if ids.len() != quantities.len() {
        return Err("inventory IDs and quantities have different lengths");
    }
    ids.into_iter()
        .zip(quantities)
        .map(|(id, quantity)| {
            let id = id.parse().map_err(|_| "invalid inventory ID")?;
            let quantity = quantity.parse().map_err(|_| "invalid inventory quantity")?;
            if quantity == 0 {
                return Err("inventory quantity must be positive");
            }
            Ok(QuantityEntry { id, quantity })
        })
        .collect()
}

#[derive(Deserialize)]
pub(crate) struct PartyPoolTransferForm {
    pub item_id: String,
    #[serde(default)]
    pub quantity: String,
}
impl PartyPoolTransferForm {
    pub fn entries(&self) -> Result<Vec<QuantityEntry<u64>>, &'static str> {
        parse_entries(&self.item_id, &self.quantity)
    }
}

#[derive(Deserialize)]
pub(crate) struct PartyOfferForm {
    pub from_character_ids: String,
    pub to_character_ids: String,
    pub inventory_item_ids: String,
    pub quantities: String,
}

pub(crate) struct PartyOfferEntry {
    pub from: u64,
    pub to: u64,
    pub inventory_id: u64,
    pub quantity: u32,
}
impl PartyOfferForm {
    pub fn entries(&self) -> Result<Vec<PartyOfferEntry>, &'static str> {
        let from = parse_ids(&self.from_character_ids)?;
        let to = parse_ids(&self.to_character_ids)?;
        let inventory = parse_entries::<u64>(&self.inventory_item_ids, &self.quantities)?;
        if from.len() != inventory.len() || to.len() != inventory.len() {
            return Err("party offer fields have different lengths");
        }
        Ok(from
            .into_iter()
            .zip(to)
            .zip(inventory)
            .map(|((from, to), item)| PartyOfferEntry {
                from,
                to,
                inventory_id: item.id,
                quantity: item.quantity,
            })
            .collect())
    }
}

#[derive(Deserialize)]
pub(crate) struct DiscardInventoryForm {
    pub inventory_item_ids: String,
    pub quantities: String,
}
impl DiscardInventoryForm {
    pub fn entries(&self) -> Result<Vec<QuantityEntry<u64>>, &'static str> {
        parse_entries(&self.inventory_item_ids, &self.quantities)
    }
}

#[derive(Deserialize)]
pub(crate) struct MerchantOfferForm {
    pub buy_item_ids: String,
    pub buy_quantities: String,
    #[serde(default)]
    pub sell_inventory_ids: String,
    #[serde(default)]
    pub sell_quantities: String,
    #[serde(default)]
    pub return_to: String,
    #[serde(default)]
    pub inventory_scope: String,
}
impl MerchantOfferForm {
    pub fn buys(&self) -> Result<Vec<QuantityEntry<String>>, &'static str> {
        parse_entries(&self.buy_item_ids, &self.buy_quantities)
    }
    pub fn sells(&self) -> Result<Vec<QuantityEntry<u64>>, &'static str> {
        parse_entries(&self.sell_inventory_ids, &self.sell_quantities)
    }
}

fn parse_ids<T: std::str::FromStr>(values: &str) -> Result<Vec<T>, &'static str> {
    values
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|value| value.parse().map_err(|_| "invalid ID"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mismatched_parallel_fields() {
        assert!(parse_entries::<u64>("1,2", "3").is_err());
    }
    #[test]
    fn rejects_malformed_or_zero_quantities() {
        assert!(parse_entries::<u64>("1", "nope").is_err());
        assert!(parse_entries::<u64>("1", "0").is_err());
    }
    #[test]
    fn parses_entries_once_at_the_boundary() {
        assert_eq!(
            parse_entries::<u64>("1,2", "3,4").unwrap(),
            vec![
                QuantityEntry { id: 1, quantity: 3 },
                QuantityEntry { id: 2, quantity: 4 }
            ]
        );
    }
}
