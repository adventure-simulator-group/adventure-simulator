# Organizations

Organizations replace the former universal profession record. A character may
hold any number of memberships, while `organization_presentation` records the
single organization (or none) whose identity and privileges the character is
currently asserting.

## Content

`content/organizations/*.yaml` is compiled into `adventuresim-core`. Definitions
declare stable IDs, names, chapters, recognition, admission requirements and
fees, arbitrary ordered ranks, recurring dues, activity training and rewards,
and privileges such as bearing arms, wearing armor, or licensed foraging.
Organization-level privileges are inherited at every rank; rank-level
privileges are additive.

Definitions also declare a typed organization `kind` and an additive `roles`
catalog. A role has exactly one purpose: `estate`, `profession`, or `office`.
Estate roles intrinsically map to one of `serf`, `freeman`, `burgher`, or
`noble`; the validator permits those roles only on compatible organization
kinds. For example, `house_habsburg/prince` always derives Noble and cannot be
paired with a separately writable Serf value, because no such actor scalar
exists. Professional roles are independent, so a House member may
simultaneously hold `roman_catholic_church/priest`.

This role layer is additive. Existing `organization_membership` ranks, dues,
training, presentation, privileges, UI labels, and starting professions keep
their current semantics. The first-pass social roles do not yet grant gameplay
rights or impose movement, faction, or eligibility restrictions.

Chapters are explicit authored records, not settlement-ID flags. Every record
names its settlement, a bounded stable `organization-*` location ID, building
name and kind, and the title and profession of its representative. Each
chapter therefore becomes a distinct navigable building even when several
organizations share a settlement or an organization is also linked to an
ordinary settlement service.

An organization may also declare explicit `starting_role` metadata: one of the
ten authored start profession families plus distinct adult and old rank IDs.
Presence of this block makes an organization eligible for deterministic
first-character sampling. The catalog validator rejects unknown families,
missing ranks, identical adult/old mappings, chapterless eligibility, and a
full catalog that leaves any profession family uncovered. This metadata is
never inferred from an organization's name, service, requirements, or skills;
the catalog-only test organization is therefore not eligible.
For settlement-scoped recognition, at least one playable chapter must also be
recognized. Admission and selected starting-rank professions of faith must
agree; conflicting authored faith requirements are rejected.

Requirements and training targets are tagged data. No code path decides that an
organization is a guild because it teaches Smithing, or a church because it
requires a religion. Mixed requirements are intentional and supported.

`content/settlement-policies.yaml` declares settlement arms and armor
restrictions. Only the currently presented, recognized, active, dues-current
membership supplies an exemption. Ownership is unaffected: when a character
loses an exemption, prohibited equipment is unequipped.

Foraging licenses use a separate global presented-privilege evaluation. They
still require persisted presentation plus a matching active, dues-current
membership and its current rank, but do not require the current settlement to
recognize the organization. A valid persisted presentation therefore survives
travel and entry into an unrecognizing settlement. This does not weaken the
locally recognized equipment privilege rules. The Lodge of the Hart King
grants Low Game, Fish, and Plants throughout its ranks; its Forest-4.0 Master
rank adds High Game.

The first catalog includes migrated trade and religious bodies plus three
universal, denomination-neutral adventurer organizations: The Hunt of the Pale
Lantern for witch hunters, The Order of St. George for knights, and The Lodge
of the Hart King for foresters. Their chapters combine the playable footprints
of the former denominational and regional variants. The catalog also retains a
deliberately eccentric Catholic cooks test organization. These are historically
informed fictional institutions rather than claims that each exact organization
existed in every listed settlement.

The character sheet exposes the presented organization as a compact profession
picker; it is only a self-presentation control. Joining, dues, reactivation,
and promotion are conducted by speaking to the representative in the
organization's chapter building. Its large label combines the member's rank with the service profession
where one exists (for example, `Apprentice Weaponsmith`), while the smaller
label names the organization. Crests are stable heraldic marks derived from the
organization ID and service using the locally vendored Game Icons charges, so
every catalog organization has a stable heraldic identity without
presentation-only persistence fields.

## Persistence and authority

SpacetimeDB owns membership, rank, dues, presentation, payment, promotion, and
equipment-law enforcement. Startup seeds exactly one deterministic persistent
representative per authored chapter. The NPC carries an explicit organization
binding, has an all-day authoritative presence at the exact chapter location,
and uses the compiled `organization-representative` conversation. Dialogue
effects carry no organization ID: authority resolves it from that live NPC and
revalidates the actor, session settlement, and exact chapter location before
reusing membership reducers. Joining is idempotent; the joining fee is charged once.
Crossing a paid-through boundary suspends membership and clears its
presentation. Paying at a chapter reactivates it without retroactive arrears.

Forester (ranger), witch-hunter, and knightly organizations explicitly author
`public_threat_referrals`; the capability is never inferred from names, skills,
or services. At any authored chapter, its representative may disclose public
hostile cases within that settlement's bounded rumor reach to an active,
dues-current member. Organization presentation is not required. The dialogue
uses the same canonical, observer-scoped disclosure path as an eligible
innkeeper.

Organization training and activity require a current membership and local
chapter. Their skill mix is read from the catalog, including fixed skills,
Bestiary and Terrain leaves, equipped weapon skills, and Religion only where
an organization explicitly teaches a particular tradition. The Hunt of the
Pale Lantern instead divides its training evenly between Spirit Bestiary and
equipped weapon skills.

### MVP privacy and uniqueness limitation

`organization_membership` rows remain public in the MVP because strategic-web
subscribes to them to render membership and schedule management. This is not an
authorization boundary: only the effective presented organization governs
privileges or public dialogue identity. An owner-scoped projection can replace
the raw subscription later. The join reducer enforces one procedural membership
per character and organization by checking the pair before insertion; the
pre-launch schema does not yet add a composite unique index.

## Validation

Build validation rejects unknown fields and invalid or duplicate IDs,
requirements, ranks, weights, organization- and rank-level privileges,
religions, skill leaves, malformed chapter locations, duplicate chapter
settlements, and
settlement policies. A canonical check against a compiled Viabundus world is
also required:

```powershell
python scripts/validate_organization_world.py --world path\to\compiled-world.json
```

The cross-world check is separate because the catalog can be compiled without
the large Viabundus dataset.

### Social estate basis

Private SpacetimeDB tables materialize organization instances and actor-role
assignments. Every durable `Character` and persistent `SettlementNpc` receives
exactly one estate-bearing assignment selected as its exclusive estate basis;
transient tactical enemies do not. The basis tables use actor IDs as primary
keys, while authoritative insertion verifies that the assignment belongs to
the actor, the instance references a known definition, and the role belongs to
that definition. Actor deletion removes the basis before its assignments.

House of Habsburg and the aggregate Habsburg Crown Lordships are chapterless,
so they create no buildings, services, or representatives. Civic communities
are instances of one catalog template, with a deterministic
`civic:<settlement-id>` identity created as settlement actors are materialized.
Citizenship derives Burgher only in urban settlements; `free_resident` derives
Freeman explicitly, never from missing data. The initial assignment uses the
persistence-contract stable hash with a versioned actor-domain key, is
order-independent, and does not consume reducer RNG.
