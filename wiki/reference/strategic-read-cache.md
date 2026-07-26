# Strategic read cache

This document records the bounded read-path change for issue #63. Browsers
still talk only to `strategic-web`; the generated SpacetimeDB SDK connection is
server-side and is not an authorization boundary.

## Route classification

| Route/read family | Classification | Boundary |
| --- | --- | --- |
| `/characters` and the shared character page model | Cache-backed when the explicit subscription is ready | Public `character` projection only; the authoritative case-site projection remains HTTP SQL. |
| `/` home routing and `/camp` active-character lookup | Cache-backed for character and party camp state | Inventory, journey, itinerary, encounter, and owner-scoped rows remain HTTP SQL. |
| Party, settlement, quest, dialogue, investigation, evidence, and chat page models | Consolidated/on-demand boundary | Keep HTTP SQL and reducer calls until each page has an authorization-scoped read model. |
| Settlement, item, travel-edge, world-clock, world-node, and world-import reads | Static/on-demand | Not globally subscribed. |
| Reducers, authoritative fallback reads, and tactical state | On-demand/authoritative | Never served from the strategic cache. |

The remaining page surface is intentionally not described as cache-backed. Its
many-row reads should be consolidated behind page-specific read models as those
pages are migrated; this change does not put SQL or cache logic in templates.

## Subscription policy

`crates/strategic-web/src/live.rs` contains the explicit
`STRATEGIC_CACHE_SUBSCRIPTIONS` inventory and the matching `add_query` chain.
The unit test in that module checks that every inventory entry appears in the
chain and that large static/world/import tables do not. Mutable character,
party, journey/itinerary, equipment/medication, inventory, and live invalidation
projections remain subscribed. The cache is incomplete until `on_applied`; a
connect, disconnect, or subscription error makes it unavailable again.

## Authorization rule

Only typed public reads are exposed by `LiveState`: public character rows and a
party's camp-present boolean. Private chat, dialogue, contract, investigation,
disease, and case projections may remain in the SDK subscription solely so
their callbacks can invalidate live UI, but no cache accessor returns those
rows. The selected
`character_id` cookie selects a presentation context; it does not prove
ownership. Owner-scoped/private projections continue through authenticated
HTTP SQL predicates and reducers. A future cache migration must add an
explicit selected-character/party authorization boundary before exposing such
rows.

## Measurement procedure and current results

`SpacetimeClient::query_metrics` is a monotonic, clone-safe SQL counter. Take a
snapshot immediately before a controlled page request and another immediately
after it; use `after.delta(before)` for request count and total elapsed time.
The elapsed value includes HTTP response parsing. Record the SSR boundary with
the request's `Instant`/`curl --write-out` timing. Run one request at a time
when attributing a page delta; the counters remain correct under concurrent
requests because they are not reset destructively.

For a representative run:

```powershell
$env:RUST_LOG = "strategic_web=info"
just web-isolated-strategic cache-measure 23100
# In a controlled harness, snapshot SpacetimeClient::query_metrics(), request
# /characters, /, and /camp, then snapshot again and emit after.delta(before).
```

The checked-in deterministic tests cover cache state transitions and an
injected-latency counter delta. Live before/after values for this checkout are
**unavailable**: the required disposable SpacetimeDB/web fixture was not
running during remediation, and no subscription payload-byte telemetry is
currently emitted by the SDK. Do not treat the following as measured values:

| Page | SQL requests/page before | SQL requests/page after | Initial subscription rows/bytes | SSR latency |
| --- | ---: | ---: | ---: | ---: |
| `/characters` | unavailable | unavailable | unavailable / unavailable | unavailable |
| `/` home routing | unavailable | unavailable | unavailable / unavailable | unavailable |
| `/camp` | unavailable | unavailable | unavailable / unavailable | unavailable |

The procedure above is the reproducible follow-up measurement; payload bytes
require SDK/client instrumentation or server WebSocket capture before they can
be reported honestly.
