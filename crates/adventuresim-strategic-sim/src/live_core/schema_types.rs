//! Conversions from generated SpacetimeDB transport types into canonical
//! domain types, and back at reducer boundaries.

pub(super) const fn domain_incapacitation_status(
    status: adventuresim_stdb_client::IncapacitationStatus,
) -> adventuresim_core::morale::IncapacitationStatus {
    use adventuresim_core::morale::IncapacitationStatus as DomainStatus;
    use adventuresim_stdb_client::IncapacitationStatus as TransportStatus;

    match status {
        TransportStatus::Ready => DomainStatus::Ready,
        TransportStatus::Staggered => DomainStatus::Staggered,
        TransportStatus::Incapacitated => DomainStatus::Incapacitated,
    }
}

pub(super) const fn domain_body_region(
    region: adventuresim_stdb_client::BodyRegion,
) -> adventuresim_core::physiology::BodyRegion {
    use adventuresim_core::physiology::BodyRegion as DomainRegion;
    use adventuresim_stdb_client::BodyRegion as TransportRegion;

    match region {
        TransportRegion::LeftArm => DomainRegion::LeftArm,
        TransportRegion::RightArm => DomainRegion::RightArm,
        TransportRegion::LeftLeg => DomainRegion::LeftLeg,
        TransportRegion::RightLeg => DomainRegion::RightLeg,
        TransportRegion::Chest => DomainRegion::Chest,
        TransportRegion::Abdomen => DomainRegion::Abdomen,
        TransportRegion::Head => DomainRegion::Head,
    }
}

pub(super) const fn reducer_intervention_route(
    route: adventuresim_core::physiology::InterventionRoute,
) -> adventuresim_stdb_client::InterventionRoute {
    use adventuresim_core::physiology::InterventionRoute as DomainRoute;
    use adventuresim_stdb_client::InterventionRoute as TransportRoute;

    match route {
        DomainRoute::Oral => TransportRoute::Oral,
        DomainRoute::Topical => TransportRoute::Topical,
        DomainRoute::Inhaled => TransportRoute::Inhaled,
        DomainRoute::Injected => TransportRoute::Injected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_statuses_map_to_the_shared_domain_status() {
        assert_eq!(
            domain_incapacitation_status(adventuresim_stdb_client::IncapacitationStatus::Ready),
            adventuresim_core::morale::IncapacitationStatus::Ready,
        );
        assert_eq!(
            domain_incapacitation_status(
                adventuresim_stdb_client::IncapacitationStatus::Incapacitated,
            ),
            adventuresim_core::morale::IncapacitationStatus::Incapacitated,
        );
    }

    #[test]
    fn shared_intervention_routes_map_at_the_reducer_boundary() {
        assert_eq!(
            reducer_intervention_route(adventuresim_core::physiology::InterventionRoute::Topical),
            adventuresim_stdb_client::InterventionRoute::Topical,
        );
    }
}
