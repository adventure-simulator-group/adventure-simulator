# Strategic place and fixture identities

`adventuresim-core::strategic_place` defines the canonical vocabulary used to
name strategic places and environmental or institutional fixtures. It is a
dependency-light identity contract, not persistence or action authority.

Places are a closed set:

- a settlement shell;
- an exact settlement venue: public square, residences facade, keep, market,
  forge, armoury, tailor, herbalist, inn, church, or bookstore;
- an authored standalone organization-chapter venue;
- a residence holding;
- a private case site, including an outbreak's physical source site; or
- an exact journey camp, identified by party, departure minute, and movement
  minute.

The settlement shell and every exact venue inside it are deliberately different
identities. Knowing that an actor is in a settlement therefore does not prove
that the actor is at its inn, church, market, chapter, residence, or any other
exact venue. A service-linked organization chapter may explicitly attach its
chapter fixture to the same service-place identity as the service fixture.
That shared place does not make the chapter itself a service or make either
fixture the owner of the venue. Physical venue kinds map back to the existing
`Storefront` and `SettlementActionService` authority enums where applicable;
they do not introduce a competing service taxonomy.

Fixtures are likewise closed to the existing seams that need a shared
referent: settlement services, organization chapters, outbreak sources, and
fireplaces. Every fixture contains its exact place identity. Constructors
reject associations that do not represent an existing family: for example, a
service must match a service venue, an outbreak source must be at a case site,
and a fireplace cannot be attached to a coarse settlement shell or the public
square. The existing residences facade and eligible keep are exact fireplace
venues; an individual residence holding remains a separate place and does not
inherit the facade's fireplace fixture.

Both place and fixture IDs have strict, versioned canonical text encodings.
Opaque source identifiers are validated and encoded without interpreting
their internal separators. Parsing rejects unknown variants, alternate
encodings, non-canonical numbers, extra fields, and invalid fixture/place
associations. Serde uses the same canonical string form, so persistence and
transport adapters cannot silently create a second identity format.

Creating or parsing an identity proves only which referent is named. It does
not prove existence, service availability, actor presence, co-presence,
schedule context, ownership, knowledge, visibility, permission, or action
rights. Later authoritative adapters must establish those facts against the
relevant private state and the actor's personal-time frontier. In particular,
settlement service availability remains an economy-profile fact, and private
case/outbreak sites remain undiscoverable until their existing knowledge and
gateway rules expose them.

## Presence and co-presence

`adventuresim-core::strategic_presence` is the shared fail-closed projection
contract. Each presence names a Character, a canonical `StrategicPlaceId`, the
authorized observer's personal-time frontier, and one closed evidence basis:
coarse settlement membership, validated instantaneous venue selection, scheduled
resident presence, or chronological residence occupancy. Co-presence requires
the same canonical place projected for the same observer frontier. It does not
require the observed Character's independent clock to equal the observer's;
pairwise-soft consumers continue to inspect only the actor's chronology.

A coarse settlement membership never equals an exact venue. The strategic
layer does not currently persist within-settlement travel or interior position.
A reducer request may therefore select any currently navigable venue inside the
actor's authoritative current settlement. A browser route or location parameter
is only that candidate: server-side navigability must resolve its canonical venue
before the selection becomes exact actor presence. This preserves instantaneous
venue selection without inventing durable navigation state. NPC presence then
applies historical alive, schedule, health, and context-suppression authority at
that actor-relative minute. Service-linked
chapter representatives therefore share the ordinary service place, while a
standalone chapter retains its authored chapter place.

Residence access projects an exact residence place only from an effective
occupancy transition and active holding at the occupant's personal frontier.
The typed basis distinguishes an owner who also occupies the home from a
household occupant; ownership alone is neither presence nor occupancy. Private
holding, household, and role facts remain inside the residence owner module.

The current SpacetimeDB investigation module still owns its serialized
`CaseSiteId` wrapper and generated client shape. The schema-adapter PR in this
stack must validate that value into `StrategicPlaceId::CaseSite`, convert the
cross-system joins, and then remove the feature-specific identity. Keeping the
schema wrapper unchanged in this pure-core PR avoids an otherwise unrelated
schema and generated-binding change; it is an ordered conversion boundary, not
approval for two permanent case-site identities.
