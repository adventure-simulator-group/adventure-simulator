//! Canonical strategic place and fixture identities.
//!
//! These values name referents only. Constructing a settlement, venue, or
//! fixture identity does not establish that it exists, is visible, or is
//! reachable by an actor. Authoritative consumers must still validate those
//! facts at the actor's personal-time frontier.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

use crate::settlement_economy::{SettlementActionService, Storefront};

const FORMAT_VERSION: &str = "v1";
pub const MAX_STRATEGIC_ID_COMPONENT_BYTES: usize = 256;
pub const MAX_STRATEGIC_PLACE_ID_BYTES: usize = 4_096;
pub const MAX_STRATEGIC_FIXTURE_ID_BYTES: usize =
    MAX_STRATEGIC_PLACE_ID_BYTES * 2 + MAX_STRATEGIC_ID_COMPONENT_BYTES * 4 + 64;

/// A validated opaque identifier used inside a canonical strategic identity.
///
/// Domain IDs remain opaque: this type prevents delimiters, names, or prefix
/// conventions from being reinterpreted as additional place structure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StrategicIdentityComponent(String);

impl StrategicIdentityComponent {
    pub fn try_new(value: impl Into<String>) -> Result<Self, PlaceIdentityError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_STRATEGIC_ID_COMPONENT_BYTES
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
            })
        {
            return Err(PlaceIdentityError::InvalidComponent);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed physical vocabulary for existing settlement venues.
///
/// This is deliberately not another settlement-service taxonomy. Authority
/// adapters can use `storefront` and `action_service` to reach the existing
/// core service types where a venue actually represents one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementVenueKind {
    /// Canonical physical identity behind the `overview` presentation alias.
    PublicSquare,
    Residences,
    Keep,
    Market,
    Forge,
    Armoury,
    Tailor,
    Herbalist,
    Inn,
    Church,
    Bookstore,
}

impl SettlementVenueKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::PublicSquare => "public-square",
            Self::Residences => "residences",
            Self::Keep => "keep",
            Self::Market => "market",
            Self::Forge => "forge",
            Self::Armoury => "armoury",
            Self::Tailor => "tailor",
            Self::Herbalist => "herbalist",
            Self::Inn => "inn",
            Self::Church => "church",
            Self::Bookstore => "bookstore",
        }
    }

    pub const fn storefront(self) -> Option<Storefront> {
        match self {
            Self::Market => Some(Storefront::General),
            Self::Forge => Some(Storefront::Weapons),
            Self::Armoury => Some(Storefront::Armor),
            Self::Tailor => Some(Storefront::Clothing),
            Self::Herbalist => Some(Storefront::Herbalist),
            Self::Inn => Some(Storefront::Inn),
            Self::Bookstore => Some(Storefront::Books),
            Self::PublicSquare | Self::Residences | Self::Keep | Self::Church => None,
        }
    }

    pub const fn action_service(self) -> Option<SettlementActionService> {
        match self {
            Self::Inn => Some(SettlementActionService::Inn),
            Self::Church => Some(SettlementActionService::Temple),
            _ => None,
        }
    }

    pub const fn is_service(self) -> bool {
        self.storefront().is_some() || self.action_service().is_some()
    }

    pub const fn supports_fireplace(self) -> bool {
        !matches!(self, Self::PublicSquare)
    }

    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "public-square" => Some(Self::PublicSquare),
            "residences" => Some(Self::Residences),
            "keep" => Some(Self::Keep),
            "market" => Some(Self::Market),
            "forge" => Some(Self::Forge),
            "armoury" => Some(Self::Armoury),
            "tailor" => Some(Self::Tailor),
            "herbalist" => Some(Self::Herbalist),
            "inn" => Some(Self::Inn),
            "church" => Some(Self::Church),
            "bookstore" => Some(Self::Bookstore),
            _ => None,
        }
    }
}

/// One strategic place referent.
///
/// The coarse `Settlement` shell is deliberately unequal to all exact venues.
/// A chapter that shares an effective service venue uses `SettlementVenue`;
/// its
/// institutional identity remains on a separate chapter fixture.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StrategicPlaceId {
    Settlement {
        settlement_id: StrategicIdentityComponent,
    },
    SettlementVenue {
        settlement_id: StrategicIdentityComponent,
        kind: SettlementVenueKind,
    },
    ChapterVenue {
        settlement_id: StrategicIdentityComponent,
        organization_id: StrategicIdentityComponent,
        authored_location_id: StrategicIdentityComponent,
    },
    Residence {
        settlement_id: StrategicIdentityComponent,
        holding_id: StrategicIdentityComponent,
    },
    CaseSite {
        /// The pure-core counterpart of the current schema-owned `CaseSiteId`.
        /// The schema adapter stack must validate/replace that wrapper with
        /// this referent; their coexistence is not a final dual-ID design.
        site_id: StrategicIdentityComponent,
    },
    JourneyCamp {
        party_id: StrategicIdentityComponent,
        departure_minute: u64,
        movement_minute: u64,
    },
}

impl StrategicPlaceId {
    pub fn settlement(settlement_id: impl Into<String>) -> Result<Self, PlaceIdentityError> {
        Ok(Self::Settlement {
            settlement_id: StrategicIdentityComponent::try_new(settlement_id)?,
        })
    }

    pub fn settlement_venue(
        settlement_id: impl Into<String>,
        kind: SettlementVenueKind,
    ) -> Result<Self, PlaceIdentityError> {
        Ok(Self::SettlementVenue {
            settlement_id: StrategicIdentityComponent::try_new(settlement_id)?,
            kind,
        })
    }

    pub fn chapter_venue(
        settlement_id: impl Into<String>,
        organization_id: impl Into<String>,
        authored_location_id: impl Into<String>,
    ) -> Result<Self, PlaceIdentityError> {
        Ok(Self::ChapterVenue {
            settlement_id: StrategicIdentityComponent::try_new(settlement_id)?,
            organization_id: StrategicIdentityComponent::try_new(organization_id)?,
            authored_location_id: StrategicIdentityComponent::try_new(authored_location_id)?,
        })
    }

    pub fn residence(
        settlement_id: impl Into<String>,
        holding_id: impl Into<String>,
    ) -> Result<Self, PlaceIdentityError> {
        Ok(Self::Residence {
            settlement_id: StrategicIdentityComponent::try_new(settlement_id)?,
            holding_id: StrategicIdentityComponent::try_new(holding_id)?,
        })
    }

    pub fn case_site(site_id: impl Into<String>) -> Result<Self, PlaceIdentityError> {
        Ok(Self::CaseSite {
            site_id: StrategicIdentityComponent::try_new(site_id)?,
        })
    }

    pub fn journey_camp(
        party_id: impl Into<String>,
        departure_minute: u64,
        movement_minute: u64,
    ) -> Result<Self, PlaceIdentityError> {
        Ok(Self::JourneyCamp {
            party_id: StrategicIdentityComponent::try_new(party_id)?,
            departure_minute,
            movement_minute,
        })
    }

    /// Settlement scope where it is an intrinsic part of the identity.
    ///
    /// `None` does not mean that a case site or camp has no geography; those
    /// associations belong to authoritative state rather than this identity.
    pub fn settlement_id(&self) -> Option<&str> {
        match self {
            Self::Settlement { settlement_id }
            | Self::SettlementVenue { settlement_id, .. }
            | Self::ChapterVenue { settlement_id, .. }
            | Self::Residence { settlement_id, .. } => Some(settlement_id.as_str()),
            Self::CaseSite { .. } | Self::JourneyCamp { .. } => None,
        }
    }
}

/// One existing environmental or institutional fixture at an exact place.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StrategicFixtureId {
    Service {
        place: StrategicPlaceId,
    },
    Chapter {
        place: StrategicPlaceId,
        organization_id: StrategicIdentityComponent,
        authored_location_id: StrategicIdentityComponent,
    },
    OutbreakSource {
        place: StrategicPlaceId,
        outbreak_id: StrategicIdentityComponent,
    },
    Fireplace {
        place: StrategicPlaceId,
    },
}

impl StrategicFixtureId {
    pub fn service(place: StrategicPlaceId) -> Result<Self, PlaceIdentityError> {
        if !matches!(
            &place,
            StrategicPlaceId::SettlementVenue { kind, .. } if kind.is_service()
        ) {
            return Err(PlaceIdentityError::InvalidAssociation);
        }
        Ok(Self::Service { place })
    }

    /// Associates a chapter's institutional identity with either its authored
    /// standalone venue or an explicitly selected effective service venue.
    pub fn chapter(
        place: StrategicPlaceId,
        organization_id: impl Into<String>,
        authored_location_id: impl Into<String>,
    ) -> Result<Self, PlaceIdentityError> {
        let organization_id = StrategicIdentityComponent::try_new(organization_id)?;
        let authored_location_id = StrategicIdentityComponent::try_new(authored_location_id)?;
        let valid_place = match &place {
            StrategicPlaceId::ChapterVenue {
                organization_id: place_organization,
                authored_location_id: place_location,
                ..
            } => place_organization == &organization_id && place_location == &authored_location_id,
            StrategicPlaceId::SettlementVenue { kind, .. } => kind.is_service(),
            _ => false,
        };
        if !valid_place {
            return Err(PlaceIdentityError::InvalidAssociation);
        }
        Ok(Self::Chapter {
            place,
            organization_id,
            authored_location_id,
        })
    }

    pub fn outbreak_source(
        place: StrategicPlaceId,
        outbreak_id: impl Into<String>,
    ) -> Result<Self, PlaceIdentityError> {
        if !matches!(&place, StrategicPlaceId::CaseSite { .. }) {
            return Err(PlaceIdentityError::InvalidAssociation);
        }
        Ok(Self::OutbreakSource {
            place,
            outbreak_id: StrategicIdentityComponent::try_new(outbreak_id)?,
        })
    }

    pub fn fireplace(place: StrategicPlaceId) -> Result<Self, PlaceIdentityError> {
        let valid_place = match &place {
            StrategicPlaceId::SettlementVenue { kind, .. } => kind.supports_fireplace(),
            StrategicPlaceId::ChapterVenue { .. } | StrategicPlaceId::JourneyCamp { .. } => true,
            StrategicPlaceId::Settlement { .. }
            | StrategicPlaceId::Residence { .. }
            | StrategicPlaceId::CaseSite { .. } => false,
        };
        if !valid_place {
            return Err(PlaceIdentityError::InvalidAssociation);
        }
        Ok(Self::Fireplace { place })
    }

    pub fn place(&self) -> &StrategicPlaceId {
        match self {
            Self::Service { place, .. }
            | Self::Chapter { place, .. }
            | Self::OutbreakSource { place, .. }
            | Self::Fireplace { place } => place,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceIdentityError {
    MalformedEncoding,
    UnsupportedVersion,
    UnknownPlaceKind,
    UnknownFixtureKind,
    UnknownVenue,
    InvalidComponent,
    InvalidNumber,
    NonCanonicalNumber,
    InvalidAssociation,
}

impl fmt::Display for PlaceIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MalformedEncoding => "malformed strategic identity encoding",
            Self::UnsupportedVersion => "unsupported strategic identity version",
            Self::UnknownPlaceKind => "unknown strategic place kind",
            Self::UnknownFixtureKind => "unknown strategic fixture kind",
            Self::UnknownVenue => "unknown settlement venue",
            Self::InvalidComponent => "invalid strategic identity component",
            Self::InvalidNumber => "invalid strategic identity number",
            Self::NonCanonicalNumber => "non-canonical strategic identity number",
            Self::InvalidAssociation => "fixture is not valid for that strategic place",
        })
    }
}

impl std::error::Error for PlaceIdentityError {}

impl fmt::Display for StrategicPlaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("place:")?;
        formatter.write_str(FORMAT_VERSION)?;
        match self {
            Self::Settlement { settlement_id } => {
                write!(formatter, ":settlement:{}", encode(settlement_id.as_str()))
            }
            Self::SettlementVenue {
                settlement_id,
                kind,
            } => write!(
                formatter,
                ":venue:{}:{}",
                encode(settlement_id.as_str()),
                kind.id()
            ),
            Self::ChapterVenue {
                settlement_id,
                organization_id,
                authored_location_id,
            } => write!(
                formatter,
                ":chapter:{}:{}:{}",
                encode(settlement_id.as_str()),
                encode(organization_id.as_str()),
                encode(authored_location_id.as_str())
            ),
            Self::Residence {
                settlement_id,
                holding_id,
            } => write!(
                formatter,
                ":residence:{}:{}",
                encode(settlement_id.as_str()),
                encode(holding_id.as_str())
            ),
            Self::CaseSite { site_id } => {
                write!(formatter, ":case-site:{}", encode(site_id.as_str()))
            }
            Self::JourneyCamp {
                party_id,
                departure_minute,
                movement_minute,
            } => write!(
                formatter,
                ":journey-camp:{}:{departure_minute}:{movement_minute}",
                encode(party_id.as_str())
            ),
        }
    }
}

impl FromStr for StrategicPlaceId {
    type Err = PlaceIdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > MAX_STRATEGIC_PLACE_ID_BYTES {
            return Err(PlaceIdentityError::MalformedEncoding);
        }
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.first() != Some(&"place") {
            return Err(PlaceIdentityError::MalformedEncoding);
        }
        if parts.get(1) != Some(&FORMAT_VERSION) {
            return Err(PlaceIdentityError::UnsupportedVersion);
        }
        match parts.as_slice() {
            ["place", _, "settlement", settlement_id] => Self::settlement(decode(settlement_id)?),
            ["place", _, "venue", settlement_id, kind] => Self::settlement_venue(
                decode(settlement_id)?,
                SettlementVenueKind::from_id(kind).ok_or(PlaceIdentityError::UnknownVenue)?,
            ),
            [
                "place",
                _,
                "chapter",
                settlement_id,
                organization_id,
                authored_location_id,
            ] => Self::chapter_venue(
                decode(settlement_id)?,
                decode(organization_id)?,
                decode(authored_location_id)?,
            ),
            ["place", _, "residence", settlement_id, holding_id] => {
                Self::residence(decode(settlement_id)?, decode(holding_id)?)
            }
            ["place", _, "case-site", site_id] => Self::case_site(decode(site_id)?),
            ["place", _, "journey-camp", party_id, departure, movement] => Self::journey_camp(
                decode(party_id)?,
                parse_canonical_u64(departure)?,
                parse_canonical_u64(movement)?,
            ),
            ["place", _, _, ..] => Err(PlaceIdentityError::UnknownPlaceKind),
            _ => Err(PlaceIdentityError::MalformedEncoding),
        }
    }
}

impl fmt::Display for StrategicFixtureId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("fixture:")?;
        formatter.write_str(FORMAT_VERSION)?;
        match self {
            Self::Service { place } => {
                write!(formatter, ":service:{}", encode(&place.to_string()))
            }
            Self::Chapter {
                place,
                organization_id,
                authored_location_id,
            } => write!(
                formatter,
                ":chapter:{}:{}:{}",
                encode(organization_id.as_str()),
                encode(authored_location_id.as_str()),
                encode(&place.to_string())
            ),
            Self::OutbreakSource { place, outbreak_id } => write!(
                formatter,
                ":outbreak-source:{}:{}",
                encode(outbreak_id.as_str()),
                encode(&place.to_string())
            ),
            Self::Fireplace { place } => {
                write!(formatter, ":fireplace:{}", encode(&place.to_string()))
            }
        }
    }
}

impl FromStr for StrategicFixtureId {
    type Err = PlaceIdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > MAX_STRATEGIC_FIXTURE_ID_BYTES {
            return Err(PlaceIdentityError::MalformedEncoding);
        }
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.first() != Some(&"fixture") {
            return Err(PlaceIdentityError::MalformedEncoding);
        }
        if parts.get(1) != Some(&FORMAT_VERSION) {
            return Err(PlaceIdentityError::UnsupportedVersion);
        }
        match parts.as_slice() {
            ["fixture", _, "service", place] => Self::service(decode_place(place)?),
            [
                "fixture",
                _,
                "chapter",
                organization_id,
                authored_location_id,
                place,
            ] => Self::chapter(
                decode_place(place)?,
                decode(organization_id)?,
                decode(authored_location_id)?,
            ),
            ["fixture", _, "outbreak-source", outbreak_id, place] => {
                Self::outbreak_source(decode_place(place)?, decode(outbreak_id)?)
            }
            ["fixture", _, "fireplace", place] => Self::fireplace(decode_place(place)?),
            ["fixture", _, _, ..] => Err(PlaceIdentityError::UnknownFixtureKind),
            _ => Err(PlaceIdentityError::MalformedEncoding),
        }
    }
}

impl Serialize for StrategicPlaceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StrategicPlaceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl Serialize for StrategicFixtureId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StrategicFixtureId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode(value: &str) -> Result<String, PlaceIdentityError> {
    if value.is_empty()
        || value.len() > MAX_STRATEGIC_ID_COMPONENT_BYTES * 2
        || value.len() % 2 != 0
    {
        return Err(PlaceIdentityError::InvalidComponent);
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = decode_hex(pair[0]).ok_or(PlaceIdentityError::InvalidComponent)?;
        let low = decode_hex(pair[1]).ok_or(PlaceIdentityError::InvalidComponent)?;
        decoded.push((high << 4) | low);
    }
    let decoded = String::from_utf8(decoded).map_err(|_| PlaceIdentityError::InvalidComponent)?;
    StrategicIdentityComponent::try_new(decoded.clone())?;
    Ok(decoded)
}

fn decode_place(value: &str) -> Result<StrategicPlaceId, PlaceIdentityError> {
    if value.is_empty() || value.len() > MAX_STRATEGIC_PLACE_ID_BYTES * 2 || value.len() % 2 != 0 {
        return Err(PlaceIdentityError::MalformedEncoding);
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = decode_hex(pair[0]).ok_or(PlaceIdentityError::MalformedEncoding)?;
        let low = decode_hex(pair[1]).ok_or(PlaceIdentityError::MalformedEncoding)?;
        decoded.push((high << 4) | low);
    }
    String::from_utf8(decoded)
        .map_err(|_| PlaceIdentityError::MalformedEncoding)?
        .parse()
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn parse_canonical_u64(value: &str) -> Result<u64, PlaceIdentityError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PlaceIdentityError::InvalidNumber);
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| PlaceIdentityError::InvalidNumber)?;
    if parsed.to_string() != value {
        return Err(PlaceIdentityError::NonCanonicalNumber);
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_place(place: StrategicPlaceId) {
        let encoded = place.to_string();
        assert_eq!(encoded.parse::<StrategicPlaceId>().unwrap(), place);
        let json = serde_json::to_string(&place).unwrap();
        assert_eq!(
            serde_json::from_str::<StrategicPlaceId>(&json).unwrap(),
            place
        );
    }

    fn round_trip_fixture(fixture: StrategicFixtureId) {
        let encoded = fixture.to_string();
        assert_eq!(encoded.parse::<StrategicFixtureId>().unwrap(), fixture);
        let json = serde_json::to_string(&fixture).unwrap();
        assert_eq!(
            serde_json::from_str::<StrategicFixtureId>(&json).unwrap(),
            fixture
        );
    }

    #[test]
    fn place_families_have_stable_separate_identities() {
        let settlement = StrategicPlaceId::settlement("lubeck").unwrap();
        let public_square =
            StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::PublicSquare)
                .unwrap();
        let service =
            StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let chapter = StrategicPlaceId::chapter_venue(
            "lubeck",
            "physicians-guild",
            "organization-physicians",
        )
        .unwrap();
        let residence =
            StrategicPlaceId::residence("lubeck", "residence-holding:41:lubeck:cheap:0").unwrap();
        let site = StrategicPlaceId::case_site("case:outbreak:site:source").unwrap();
        let camp = StrategicPlaceId::journey_camp("party-7", 14_400, 480).unwrap();

        assert_ne!(settlement, public_square);
        assert_ne!(settlement, service);
        assert_ne!(public_square, service);
        assert_ne!(service, chapter);
        assert_ne!(chapter, residence);
        assert_ne!(site, camp);
        for place in [
            settlement,
            public_square,
            service,
            chapter,
            residence,
            site,
            camp,
        ] {
            round_trip_place(place);
        }
    }

    #[test]
    fn venue_kinds_map_to_existing_service_authority_without_a_second_taxonomy() {
        for (venue, storefront) in [
            (SettlementVenueKind::Market, Storefront::General),
            (SettlementVenueKind::Forge, Storefront::Weapons),
            (SettlementVenueKind::Armoury, Storefront::Armor),
            (SettlementVenueKind::Tailor, Storefront::Clothing),
            (SettlementVenueKind::Herbalist, Storefront::Herbalist),
            (SettlementVenueKind::Inn, Storefront::Inn),
            (SettlementVenueKind::Bookstore, Storefront::Books),
        ] {
            assert_eq!(venue.storefront(), Some(storefront));
            assert!(venue.is_service());
        }
        assert_eq!(
            SettlementVenueKind::Church.action_service(),
            Some(SettlementActionService::Temple)
        );
        assert_eq!(
            SettlementVenueKind::Inn.action_service(),
            Some(SettlementActionService::Inn)
        );
        assert!(SettlementVenueKind::Church.is_service());
        assert_eq!(SettlementVenueKind::Keep.storefront(), None);
        assert!(!SettlementVenueKind::Residences.is_service());
        assert!(!SettlementVenueKind::PublicSquare.is_service());
        assert_eq!(
            SettlementVenueKind::from_id("armoury"),
            Some(SettlementVenueKind::Armoury)
        );
        assert_eq!(SettlementVenueKind::from_id("armor"), None);
    }

    #[test]
    fn chapter_fixture_can_share_a_service_place_without_becoming_the_place() {
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let service = StrategicFixtureId::service(inn.clone()).unwrap();
        let chapter =
            StrategicFixtureId::chapter(inn.clone(), "innkeepers-guild", "organization-innkeepers")
                .unwrap();

        assert_eq!(service.place(), &inn);
        assert_eq!(chapter.place(), &inn);
        assert_ne!(service, chapter);
        round_trip_fixture(service);
        round_trip_fixture(chapter);
    }

    #[test]
    fn source_and_fireplace_keep_their_exact_place_association() {
        let site = StrategicPlaceId::case_site("case:outbreak:site:source").unwrap();
        let source = StrategicFixtureId::outbreak_source(site.clone(), "outbreak:42").unwrap();
        assert_eq!(source.place(), &site);
        round_trip_fixture(source);

        let camp = StrategicPlaceId::journey_camp("party-7", 14_400, 480).unwrap();
        let fireplace = StrategicFixtureId::fireplace(camp.clone()).unwrap();
        assert_eq!(fireplace.place(), &camp);
        round_trip_fixture(fireplace);

        for kind in [SettlementVenueKind::Residences, SettlementVenueKind::Keep] {
            let venue = StrategicPlaceId::settlement_venue("lubeck", kind).unwrap();
            let fireplace = StrategicFixtureId::fireplace(venue.clone()).unwrap();
            assert_eq!(fireplace.place(), &venue);
            round_trip_place(venue);
            round_trip_fixture(fireplace);
        }
    }

    #[test]
    fn invalid_fixture_associations_fail_closed() {
        let settlement = StrategicPlaceId::settlement("lubeck").unwrap();
        assert_eq!(
            StrategicFixtureId::fireplace(settlement.clone()),
            Err(PlaceIdentityError::InvalidAssociation)
        );
        assert_eq!(
            StrategicFixtureId::service(settlement),
            Err(PlaceIdentityError::InvalidAssociation)
        );

        let public_square =
            StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::PublicSquare)
                .unwrap();
        assert_eq!(
            StrategicFixtureId::fireplace(public_square.clone()),
            Err(PlaceIdentityError::InvalidAssociation)
        );
        assert_eq!(
            StrategicFixtureId::service(public_square),
            Err(PlaceIdentityError::InvalidAssociation)
        );

        let site = StrategicPlaceId::case_site("site:source").unwrap();
        assert_eq!(
            StrategicFixtureId::chapter(site, "guild", "organization-guild"),
            Err(PlaceIdentityError::InvalidAssociation)
        );
    }

    #[test]
    fn malformed_and_ambiguous_encodings_fail_closed() {
        for value in [
            "place:v2:settlement:6c756265636b",
            "place:v1:settlement:",
            "place:v1:settlement:6C756265636B",
            "place:v1:venue:6c756265636b:unknown",
            "place:v1:journey-camp:7061727479:01:480",
            "place:v1:journey-camp:7061727479:1:480:extra",
            "place:v1:case-site:zz",
        ] {
            assert!(
                value.parse::<StrategicPlaceId>().is_err(),
                "accepted {value}"
            );
        }

        let residences =
            StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Residences).unwrap();
        let wrong_service = format!("fixture:v1:service:{}", encode(&residences.to_string()));
        assert_eq!(
            wrong_service.parse::<StrategicFixtureId>(),
            Err(PlaceIdentityError::InvalidAssociation)
        );
        assert!(
            "fixture:v1:fireplace:00:extra"
                .parse::<StrategicFixtureId>()
                .is_err()
        );

        let oversized = format!(
            "fixture:v1:fireplace:{}",
            ":".repeat(MAX_STRATEGIC_FIXTURE_ID_BYTES)
        );
        assert_eq!(
            oversized.parse::<StrategicFixtureId>(),
            Err(PlaceIdentityError::MalformedEncoding)
        );
    }
}
