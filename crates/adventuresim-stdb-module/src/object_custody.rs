//! Schema adapters for stable physical-object identity and operational custody.
//!
//! This module validates persisted inventory locations, containment edges, and
//! exact strategic fixtures against the dependency-light core custody
//! vocabulary. Custody is not legal ownership.

use std::{collections::BTreeSet, str::FromStr};

use adventuresim_core::{
    physical_object::{ObjectCustody, OperationalCustody, PhysicalObjectId},
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table};

use crate::{
    Character, InventoryObject, character::character as _,
    inventory_container::inventory_containment, inventory_container::inventory_object,
    inventory_item, repair::repair_order, strategic::party_authority,
    strategic::party_inventory_item,
};

/// Closed SpacetimeDB transport for the shared core custody vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub enum PersistedOperationalCustody {
    Character { character_id: u64 },
    Party { party_id: String },
    Container { object_id: u64 },
    Place { place_id: String },
    Fixture { fixture_id: String },
}

pub(crate) fn decode_custody(
    persisted: &PersistedOperationalCustody,
) -> Result<OperationalCustody, String> {
    match persisted {
        PersistedOperationalCustody::Character { character_id } => {
            OperationalCustody::character(*character_id).map_err(|error| error.to_string())
        }
        PersistedOperationalCustody::Party { party_id } => {
            OperationalCustody::party(party_id.clone()).map_err(|error| error.to_string())
        }
        PersistedOperationalCustody::Container { object_id } => Ok(OperationalCustody::Container(
            object_id_from_u64(*object_id)?,
        )),
        PersistedOperationalCustody::Place { place_id } => StrategicPlaceId::from_str(place_id)
            .map(OperationalCustody::Place)
            .map_err(|_| "Custody place identity is not canonical".into()),
        PersistedOperationalCustody::Fixture { fixture_id } => {
            StrategicFixtureId::from_str(fixture_id)
                .map(OperationalCustody::Fixture)
                .map_err(|_| "Custody fixture identity is not canonical".into())
        }
    }
}

pub(crate) fn encode_custody(custody: &OperationalCustody) -> PersistedOperationalCustody {
    match custody {
        OperationalCustody::Character(character_id) => PersistedOperationalCustody::Character {
            character_id: character_id.get(),
        },
        OperationalCustody::Party(party_id) => PersistedOperationalCustody::Party {
            party_id: party_id.as_str().into(),
        },
        OperationalCustody::Container(object_id) => PersistedOperationalCustody::Container {
            object_id: object_id.get(),
        },
        OperationalCustody::Place(place_id) => PersistedOperationalCustody::Place {
            place_id: place_id.to_string(),
        },
        OperationalCustody::Fixture(fixture_id) => PersistedOperationalCustody::Fixture {
            fixture_id: fixture_id.to_string(),
        },
    }
}

/// Stable public binding for the immediate operational custodian.
pub(crate) fn canonical_custody_binding(custody: &OperationalCustody) -> String {
    match custody {
        OperationalCustody::Character(character_id) => {
            format!("character:{}", character_id.get())
        }
        OperationalCustody::Party(party_id) => format!("party:{}", party_id.as_str()),
        OperationalCustody::Container(object_id) => format!("container:{}", object_id.get()),
        OperationalCustody::Place(place_id) => format!("place:{place_id}"),
        OperationalCustody::Fixture(fixture_id) => format!("fixture:{fixture_id}"),
    }
}

pub(crate) fn carried_scope_custody(
    ctx: &ReducerContext,
    actor: &Character,
    inventory_scope: adventuresim_core::physical_object::CarriedInventoryScope,
) -> Result<OperationalCustody, String> {
    match inventory_scope {
        adventuresim_core::physical_object::CarriedInventoryScope::Personal => {
            OperationalCustody::character(actor.id).map_err(|error| error.to_string())
        }
        adventuresim_core::physical_object::CarriedInventoryScope::Party => {
            let party_id = actor
                .party_id
                .as_ref()
                .ok_or("Character has no party inventory")?;
            if ctx
                .db
                .party_authority()
                .id()
                .find(party_id.clone())
                .is_none()
            {
                return Err("Party inventory custody is unavailable".into());
            }
            OperationalCustody::party(party_id.clone()).map_err(|error| error.to_string())
        }
    }
}

pub(crate) fn carried_location_custody(
    location: &adventuresim_core::physical_object::InventoryLocation,
) -> Result<OperationalCustody, String> {
    match location {
        adventuresim_core::physical_object::InventoryLocation::Personal(location) => {
            OperationalCustody::character(location.character_id).map_err(|error| error.to_string())
        }
        adventuresim_core::physical_object::InventoryLocation::Party(location) => {
            OperationalCustody::party(location.party_id.clone()).map_err(|error| error.to_string())
        }
        adventuresim_core::physical_object::InventoryLocation::Fireplace(_)
        | adventuresim_core::physical_object::InventoryLocation::Repair(_) => {
            Err("Custody is not a carried inventory".into())
        }
    }
}

pub(crate) fn carried_destination(
    custody: &PersistedOperationalCustody,
    expected_character_id: u64,
) -> Result<OperationalCustody, String> {
    match decode_custody(custody)? {
        OperationalCustody::Character(character_id)
            if character_id.get() == expected_character_id =>
        {
            Ok(OperationalCustody::Character(character_id))
        }
        OperationalCustody::Character(_) => {
            Err("Personal custody conflicts with the acting character".into())
        }
        OperationalCustody::Party(party_id) => Ok(OperationalCustody::Party(party_id)),
        OperationalCustody::Container(_)
        | OperationalCustody::Place(_)
        | OperationalCustody::Fixture(_) => {
            Err("Custody is not a carried inventory destination".into())
        }
    }
}

fn object_id_from_u64(value: u64) -> Result<PhysicalObjectId, String> {
    PhysicalObjectId::try_new(value).map_err(|error| error.to_string())
}

fn persisted_location_custody(
    ctx: &ReducerContext,
    object: &InventoryObject,
) -> Result<OperationalCustody, String> {
    match &object.location {
        adventuresim_core::physical_object::InventoryLocation::Personal(location) => {
            let custody = OperationalCustody::character(location.character_id)
                .map_err(|error| error.to_string())?;
            if ctx
                .db
                .character()
                .id()
                .find(location.character_id)
                .is_none()
            {
                return Err("Inventory object character custody is unavailable".into());
            }
            let row = ctx
                .db
                .inventory_item()
                .id()
                .find(location.row_id)
                .ok_or("Inventory object personal row is missing")?;
            require_unique_backing_object(ctx, object)?;
            if row.character_id != location.character_id
                || row.item_id != object.item_id
                || row.quantity != 1
            {
                return Err("Inventory object conflicts with its personal row custody".into());
            }
            Ok(custody)
        }
        adventuresim_core::physical_object::InventoryLocation::Party(location) => {
            let custody = OperationalCustody::party(location.party_id.clone())
                .map_err(|error| error.to_string())?;
            if ctx
                .db
                .party_authority()
                .id()
                .find(location.party_id.clone())
                .is_none()
            {
                return Err("Inventory object party custody is unavailable".into());
            }
            let row = ctx
                .db
                .party_inventory_item()
                .id()
                .find(location.row_id)
                .ok_or("Inventory object party row is missing")?;
            require_unique_backing_object(ctx, object)?;
            if row.party_id != location.party_id
                || row.item_id != object.item_id
                || row.quantity != 1
            {
                return Err("Inventory object conflicts with its party row custody".into());
            }
            Ok(custody)
        }
        adventuresim_core::physical_object::InventoryLocation::Fireplace(location) => {
            let fixture = StrategicFixtureId::from_str(&location.fixture_id)
                .map_err(|_| "Inventory object fireplace fixture is not canonical")?;
            if !matches!(fixture, StrategicFixtureId::Fireplace { .. }) {
                return Err("Inventory object fireplace custody names another fixture kind".into());
            }
            Ok(OperationalCustody::Fixture(fixture))
        }
        adventuresim_core::physical_object::InventoryLocation::Repair(location) => {
            let place = StrategicPlaceId::settlement(&location.settlement_id)
                .map_err(|_| "Inventory object repair place is not canonical")?;
            let row = ctx
                .db
                .inventory_item()
                .id()
                .find(location.row_id)
                .ok_or("Inventory object repair row is missing")?;
            require_unique_backing_object(ctx, object)?;
            let order = ctx
                .db
                .repair_order()
                .inventory_item_id()
                .find(location.row_id)
                .ok_or("Inventory object repair order is missing")?;
            if row.character_id != 0
                || row.item_id != object.item_id
                || row.quantity != 1
                || order.item_id != object.item_id
                || order.settlement_id != location.settlement_id
            {
                return Err("Inventory object conflicts with its repair escrow custody".into());
            }
            Ok(OperationalCustody::Place(place))
        }
    }
}

fn require_unique_backing_object(
    ctx: &ReducerContext,
    object: &InventoryObject,
) -> Result<(), String> {
    let aliases = ctx
        .db
        .inventory_object()
        .iter()
        .filter(|candidate| same_backing_row(&candidate.location, &object.location))
        .take(2)
        .count();
    require_exactly_one_backing_alias(aliases)
}

fn same_backing_row(
    left: &adventuresim_core::physical_object::InventoryLocation,
    right: &adventuresim_core::physical_object::InventoryLocation,
) -> bool {
    use adventuresim_core::physical_object::InventoryLocation;

    match (left, right) {
        (InventoryLocation::Personal(left), InventoryLocation::Personal(right)) => {
            left.row_id == right.row_id
        }
        (InventoryLocation::Party(left), InventoryLocation::Party(right)) => {
            left.row_id == right.row_id
        }
        (InventoryLocation::Repair(left), InventoryLocation::Repair(right)) => {
            left.row_id == right.row_id
        }
        (InventoryLocation::Fireplace(_), InventoryLocation::Fireplace(_)) => false,
        _ => false,
    }
}

fn require_exactly_one_backing_alias(aliases: usize) -> Result<(), String> {
    (aliases == 1)
        .then_some(())
        .ok_or_else(|| "Inventory row must have exactly one physical object identity".into())
}

fn next_containment_depth(depth: usize) -> Result<usize, String> {
    let next = depth
        .checked_add(1)
        .ok_or("Inventory containment custody depth overflow")?;
    if next > adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        Err("Inventory containment custody exceeds the maximum depth".into())
    } else {
        Ok(next)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedObjectCustody {
    pub object: ObjectCustody,
    pub root: OperationalCustody,
}

/// Resolves direct containment and ultimate custody while validating every
/// persisted object and edge in the chain. Containment is the direct location;
/// the root character, party, or fixture remains the operational authority.
pub(crate) fn resolve_object_custody(
    ctx: &ReducerContext,
    object: &InventoryObject,
) -> Result<ResolvedObjectCustody, String> {
    let object_id = object_id_from_u64(object.id)?;
    let initial_storage = persisted_location_custody(ctx, object)?;
    let direct_parent = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(object.id)
        .map(|edge| edge.parent_object_id);
    let direct = match direct_parent {
        Some(parent_id) => OperationalCustody::Container(object_id_from_u64(parent_id)?),
        None => initial_storage.clone(),
    };
    let object_custody =
        ObjectCustody::try_new(object_id, direct).map_err(|error| error.to_string())?;

    let mut cursor = direct_parent;
    let mut visited = BTreeSet::from([object.id]);
    let mut carried_backing: Option<OperationalCustody> = match &initial_storage {
        custody @ (OperationalCustody::Character(_) | OperationalCustody::Party(_)) => {
            Some(custody.clone())
        }
        _ => None,
    };
    let mut depth = 0usize;
    loop {
        let Some(parent_id) = cursor else {
            return Ok(ResolvedObjectCustody {
                object: object_custody,
                root: initial_storage,
            });
        };
        depth = next_containment_depth(depth)?;
        if !visited.insert(parent_id) {
            return Err("Inventory containment custody contains a cycle".into());
        }
        let parent = ctx
            .db
            .inventory_object()
            .id()
            .find(parent_id)
            .ok_or("Inventory containment parent object is missing")?;
        let parent_storage = persisted_location_custody(ctx, &parent)?;
        if matches!(
            &parent_storage,
            OperationalCustody::Character(_) | OperationalCustody::Party(_)
        ) {
            if carried_backing
                .as_ref()
                .is_some_and(|expected| expected != &parent_storage)
            {
                return Err("Contained objects have conflicting carried custody".into());
            }
            carried_backing = Some(parent_storage.clone());
        }
        cursor = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(parent_id)
            .map(|edge| edge.parent_object_id);
        if cursor.is_none() {
            return Ok(ResolvedObjectCustody {
                object: object_custody,
                root: parent_storage,
            });
        }
    }
}

pub(crate) fn require_actor_carried_object(
    ctx: &ReducerContext,
    actor: &Character,
    object: &InventoryObject,
) -> Result<ResolvedObjectCustody, String> {
    let resolved = resolve_object_custody(ctx, object)?;
    if !resolved
        .root
        .matches_carried_inventory(actor.id, actor.party_id.as_deref())
    {
        return Err("Physical object is outside the actor's exact carried custody".into());
    }
    Ok(resolved)
}

pub(crate) fn require_object_at_fixture(
    ctx: &ReducerContext,
    object: &InventoryObject,
    fixture: &StrategicFixtureId,
) -> Result<(), String> {
    let resolved = resolve_object_custody(ctx, object)?;
    match resolved.root {
        OperationalCustody::Fixture(actual) if actual == *fixture => Ok(()),
        OperationalCustody::Fixture(actual) if actual.place() != fixture.place() => {
            Err("Object fixture custody conflicts with the expected place".into())
        }
        OperationalCustody::Fixture(_) => {
            Err("Object custody names another fixture at this place".into())
        }
        _ => Err("Object is not in exact fixture custody".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::strategic_place::SettlementVenueKind;

    #[test]
    fn persisted_custody_round_trips_all_closed_variants() {
        let place = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let fixture = StrategicFixtureId::fireplace(place.clone()).unwrap();
        for custody in [
            OperationalCustody::character(7).unwrap(),
            OperationalCustody::party("party-red").unwrap(),
            OperationalCustody::Container(PhysicalObjectId::try_new(4).unwrap()),
            OperationalCustody::Place(place),
            OperationalCustody::Fixture(fixture),
        ] {
            assert_eq!(decode_custody(&encode_custody(&custody)), Ok(custody));
        }
    }

    #[test]
    fn carried_destination_is_exact_and_never_forges_authority() {
        let party = encode_custody(&OperationalCustody::party("party-before").unwrap());
        assert_eq!(
            carried_destination(&party, 7),
            OperationalCustody::party("party-before").map_err(|error| error.to_string())
        );
        let personal = encode_custody(&OperationalCustody::character(7).unwrap());
        assert!(carried_destination(&personal, 8).is_err());
        let container = PersistedOperationalCustody::Container { object_id: 7 };
        assert!(carried_destination(&container, 7).is_err());
    }

    #[test]
    fn malformed_and_noncanonical_custody_fails_closed() {
        assert!(
            decode_custody(&PersistedOperationalCustody::Character { character_id: 0 }).is_err()
        );
        assert!(
            decode_custody(&PersistedOperationalCustody::Party {
                party_id: " party".into()
            })
            .is_err()
        );
        assert!(decode_custody(&PersistedOperationalCustody::Container { object_id: 0 }).is_err());
        assert!(
            decode_custody(&PersistedOperationalCustody::Place {
                place_id: "settlement|lubeck".into()
            })
            .is_err()
        );
        assert!(
            decode_custody(&PersistedOperationalCustody::Fixture {
                fixture_id: "fireplace|lubeck".into()
            })
            .is_err()
        );
    }

    #[test]
    fn containment_depth_boundary_is_exact() {
        let mut depth = 0;
        for _ in 0..adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
            depth = next_containment_depth(depth).unwrap();
        }
        assert_eq!(
            depth,
            adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH
        );
        assert!(next_containment_depth(depth).is_err());
    }

    #[test]
    fn backing_alias_multiplicity_fails_closed() {
        assert!(require_exactly_one_backing_alias(0).is_err());
        assert_eq!(require_exactly_one_backing_alias(1), Ok(()));
        assert!(require_exactly_one_backing_alias(2).is_err());
    }
}
