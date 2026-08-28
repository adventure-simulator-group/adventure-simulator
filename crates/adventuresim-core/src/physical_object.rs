//! Stable physical-object identity and operational custody vocabulary.
//!
//! `PhysicalObjectId` is a typed view of the existing strategic
//! `InventoryObject.id`; it is not a second identifier. Custody says where an
//! object is operationally held now. It deliberately says nothing about legal
//! ownership, permissions, title, or material/lot identity.

use std::{fmt, num::NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::strategic_place::{StrategicFixtureId, StrategicPlaceId};

/// The two inventories that can operationally carry a physical object.
///
/// Raw scope strings belong at reducer, URL, and form boundaries. Domain code
/// uses this closed vocabulary instead.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum CarriedInventoryScope {
    Personal,
    Party,
}

impl CarriedInventoryScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Party => "party",
        }
    }
}

impl TryFrom<&str> for CarriedInventoryScope {
    type Error = CustodyIdentityError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "personal" => Ok(Self::Personal),
            "party" => Ok(Self::Party),
            _ => Err(CustodyIdentityError::InvalidInventoryScope),
        }
    }
}

/// A physical object's persisted backing location.
///
/// Each variant contains exactly the identity required by that state, so a
/// fireplace object cannot retain an inventory row and a carried object cannot
/// omit its owner. Containment remains a separate relation: descendants retain
/// the same carried backing while their direct custody names their parent.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PersonalInventoryLocation {
    pub character_id: u64,
    pub row_id: u64,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PartyInventoryLocation {
    pub party_id: String,
    pub row_id: u64,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct FireplaceInventoryLocation {
    pub fixture_id: String,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RepairInventoryLocation {
    pub settlement_id: String,
    pub row_id: u64,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum InventoryLocation {
    Personal(PersonalInventoryLocation),
    Party(PartyInventoryLocation),
    Fireplace(FireplaceInventoryLocation),
    Repair(RepairInventoryLocation),
}

impl InventoryLocation {
    pub const fn personal(character_id: u64, row_id: u64) -> Self {
        Self::Personal(PersonalInventoryLocation {
            character_id,
            row_id,
        })
    }

    pub fn party(party_id: impl Into<String>, row_id: u64) -> Self {
        Self::Party(PartyInventoryLocation {
            party_id: party_id.into(),
            row_id,
        })
    }

    pub fn fireplace(fixture_id: impl Into<String>) -> Self {
        Self::Fireplace(FireplaceInventoryLocation {
            fixture_id: fixture_id.into(),
        })
    }

    pub fn repair(settlement_id: impl Into<String>, row_id: u64) -> Self {
        Self::Repair(RepairInventoryLocation {
            settlement_id: settlement_id.into(),
            row_id,
        })
    }

    pub const fn scope(&self) -> Option<CarriedInventoryScope> {
        match self {
            Self::Personal(_) => Some(CarriedInventoryScope::Personal),
            Self::Party(_) => Some(CarriedInventoryScope::Party),
            Self::Fireplace(_) | Self::Repair(_) => None,
        }
    }

    pub const fn row_id(&self) -> Option<u64> {
        match self {
            Self::Personal(location) => Some(location.row_id),
            Self::Party(location) => Some(location.row_id),
            Self::Repair(location) => Some(location.row_id),
            Self::Fireplace(_) => None,
        }
    }

    pub fn has_same_carried_custody(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Personal(left), Self::Personal(right)) => {
                left.character_id == right.character_id
            }
            (Self::Party(left), Self::Party(right)) => left.party_id == right.party_id,
            _ => false,
        }
    }

    pub const fn is_fireplace(&self) -> bool {
        matches!(self, Self::Fireplace(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhysicalObjectId(NonZeroU64);

impl PhysicalObjectId {
    pub fn try_new(value: u64) -> Result<Self, CustodyIdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(CustodyIdentityError::ZeroObjectId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CustodyCharacterId(NonZeroU64);

impl CustodyCharacterId {
    pub fn try_new(value: u64) -> Result<Self, CustodyIdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(CustodyIdentityError::ZeroCharacterId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CustodyPartyId(String);

impl CustodyPartyId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CustodyIdentityError> {
        let value = value.into();
        if value.is_empty()
            || value.trim() != value
            || value.len() > 256
            || value.chars().any(char::is_control)
        {
            return Err(CustodyIdentityError::InvalidPartyId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The direct operational holder or exact physical location of an object.
///
/// Character and party variants name carried inventories. Container names a
/// direct physical parent. Place and fixture name exact strategic custody;
/// fixture custody is intentionally distinct from custody at its containing
/// place.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationalCustody {
    Character(CustodyCharacterId),
    Party(CustodyPartyId),
    Container(PhysicalObjectId),
    Place(StrategicPlaceId),
    Fixture(StrategicFixtureId),
}

impl OperationalCustody {
    pub fn character(character_id: u64) -> Result<Self, CustodyIdentityError> {
        Ok(Self::Character(CustodyCharacterId::try_new(character_id)?))
    }

    pub fn party(party_id: impl Into<String>) -> Result<Self, CustodyIdentityError> {
        Ok(Self::Party(CustodyPartyId::try_new(party_id)?))
    }

    /// Exact carried-inventory identity match. This is a projection law, not a
    /// legal permission or ownership decision.
    pub fn matches_carried_inventory(&self, character_id: u64, party_id: Option<&str>) -> bool {
        match self {
            Self::Character(expected) => expected.get() == character_id,
            Self::Party(expected) => party_id == Some(expected.as_str()),
            Self::Container(_) | Self::Place(_) | Self::Fixture(_) => false,
        }
    }
}

/// One stable physical object together with its current operational custody.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectCustody {
    object_id: PhysicalObjectId,
    custody: OperationalCustody,
}

impl ObjectCustody {
    pub fn try_new(
        object_id: PhysicalObjectId,
        custody: OperationalCustody,
    ) -> Result<Self, CustodyIdentityError> {
        if custody == OperationalCustody::Container(object_id) {
            return Err(CustodyIdentityError::SelfContainment);
        }
        Ok(Self { object_id, custody })
    }

    pub const fn object_id(&self) -> PhysicalObjectId {
        self.object_id
    }

    pub const fn custody(&self) -> &OperationalCustody {
        &self.custody
    }

    pub fn transfer(self, custody: OperationalCustody) -> Result<Self, CustodyIdentityError> {
        Self::try_new(self.object_id, custody)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustodyIdentityError {
    ZeroObjectId,
    ZeroCharacterId,
    InvalidPartyId,
    InvalidInventoryScope,
    SelfContainment,
}

impl fmt::Display for CustodyIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroObjectId => "Physical object identity must be nonzero",
            Self::ZeroCharacterId => "Custody character identity must be nonzero",
            Self::InvalidPartyId => "Custody party identity is not canonical",
            Self::InvalidInventoryScope => "Inventory scope must be personal or party",
            Self::SelfContainment => "A physical object cannot be in its own custody",
        })
    }
}

impl std::error::Error for CustodyIdentityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategic_place::{SettlementVenueKind, StrategicFixtureId, StrategicPlaceId};

    #[test]
    fn custody_transfer_preserves_the_only_physical_identity() {
        let id = PhysicalObjectId::try_new(41).unwrap();
        let carried =
            ObjectCustody::try_new(id, OperationalCustody::character(7).unwrap()).unwrap();
        let transferred = carried
            .transfer(OperationalCustody::party("party-red").unwrap())
            .unwrap();
        assert_eq!(transferred.object_id(), id);
        assert!(
            transferred
                .custody()
                .matches_carried_inventory(99, Some("party-red"))
        );
        assert!(
            !transferred
                .custody()
                .matches_carried_inventory(7, Some("party-blue"))
        );
    }

    #[test]
    fn custody_is_location_not_legal_ownership() {
        let id = PhysicalObjectId::try_new(9).unwrap();
        let place = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let fixture = StrategicFixtureId::fireplace(place.clone()).unwrap();
        let at_place = ObjectCustody::try_new(id, OperationalCustody::Place(place)).unwrap();
        let at_fixture = at_place
            .transfer(OperationalCustody::Fixture(fixture))
            .unwrap();
        assert_eq!(at_fixture.object_id(), id);
        assert!(!at_fixture.custody().matches_carried_inventory(7, None));
    }

    #[test]
    fn strict_identity_and_containment_fail_closed() {
        assert_eq!(
            PhysicalObjectId::try_new(0),
            Err(CustodyIdentityError::ZeroObjectId)
        );
        let id = PhysicalObjectId::try_new(12).unwrap();
        assert_eq!(
            ObjectCustody::try_new(id, OperationalCustody::Container(id)),
            Err(CustodyIdentityError::SelfContainment)
        );
        assert!(OperationalCustody::party(" party-red").is_err());
    }

    #[test]
    fn exact_character_and_party_authority_never_alias() {
        let personal = OperationalCustody::character(7).unwrap();
        let party = OperationalCustody::party("party-7").unwrap();
        assert!(personal.matches_carried_inventory(7, Some("party-7")));
        assert!(!personal.matches_carried_inventory(8, Some("party-7")));
        assert!(party.matches_carried_inventory(8, Some("party-7")));
        assert!(!party.matches_carried_inventory(7, Some("party-8")));
    }

    #[test]
    fn inventory_location_encodes_only_valid_state_shapes() {
        let personal = InventoryLocation::personal(7, 11);
        let party = InventoryLocation::party("party-red", 12);
        let fireplace = InventoryLocation::fireplace("fireplace|settlement|lubeck");
        let repair = InventoryLocation::repair("lubeck", 13);

        assert_eq!(personal.scope(), Some(CarriedInventoryScope::Personal));
        assert_eq!(personal.row_id(), Some(11));
        assert_eq!(party.scope(), Some(CarriedInventoryScope::Party));
        assert_eq!(party.row_id(), Some(12));
        assert_eq!(fireplace.scope(), None);
        assert_eq!(fireplace.row_id(), None);
        assert_eq!(repair.scope(), None);
        assert_eq!(repair.row_id(), Some(13));
        assert!(fireplace.is_fireplace());
    }

    #[test]
    fn raw_inventory_scope_parses_only_at_the_closed_boundary() {
        assert_eq!(
            CarriedInventoryScope::try_from("personal"),
            Ok(CarriedInventoryScope::Personal)
        );
        assert_eq!(
            CarriedInventoryScope::try_from("party"),
            Ok(CarriedInventoryScope::Party)
        );
        assert_eq!(
            CarriedInventoryScope::try_from("repair"),
            Err(CustodyIdentityError::InvalidInventoryScope)
        );
    }
}
