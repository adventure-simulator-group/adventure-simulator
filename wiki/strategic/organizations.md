# Organizations

Organizations replace the former universal profession record. A character may
hold any number of memberships, while `organization_presentation` records the
single organization (or none) whose identity and privileges the character is
currently asserting.

## Content

Training entries may use `kind: written` with a typed Written language. These
entries use normal Intelligence-governed language training and apply both to
generated starting professionals and later scheduled organization training.

`content/organizations/*.yaml` is compiled into `adventuresim-core`. Definitions
declare stable IDs, names, chapters, recognition, admission requirements and
fees, arbitrary roles and direct role transitions, recurring dues, activity training and rewards,
and privileges such as bearing arms, wearing armor, or licensed foraging.
Organization-level privileges are inherited by every role; role-level
privileges are additive.

Definitions also declare a typed organization `kind` and a `roles` catalog.
Each role names a profession and may author an address title, address priority,
social precedence, public-recognition flag, and creation literacy entitlement.
A character has at most one role in a particular organization instance but may
belong to any number of organizations. Thus `noble`, `serf`, and `citizen` are
professions conferred by family, lordship, and civic organizations rather than
values on a character. A House member may simultaneously be a learned
religious practitioner; clergy's higher address priority makes its title win
without erasing birth-family membership. Clergy also carry precedence `400`,
above the highest noble-family role, so the same winning clerical identity
governs familiar address; noble family titles and precedence in turn outrank
civic roles.

Procedural membership continues to own dues, training, presentation,
privileges, UI labels, and starting professions. Social role assignments are
the canonical source for profession, address, precedence, family identity, and
role-authored literacy; they do not themselves grant equipment or foraging
privileges.

Chapters are explicit authored records, not settlement-ID flags. Every record
names its settlement, a bounded stable `organization-*` location ID, building
name and kind, and the title and profession of its representative. That
authored location remains the chapter's institutional identity. When a chapter
is linked to an ordinary service that is present in the settlement, its
representative is physically co-located with the service operator and visitors
instead of adding another building. Chapters with no mapped available service,
including physicians and surgeons, remain distinct navigable buildings.

An organization may also declare explicit `starting_role` metadata: one of the
ten authored start profession families plus distinct adult and old role IDs.
Presence of this block makes an organization eligible for deterministic
first-character sampling. The catalog validator rejects unknown families,
missing roles, identical adult/old mappings, chapterless eligibility, and a
full catalog that leaves any profession family uncovered. This metadata is
never inferred from an organization's name, service, requirements, or skills;
the catalog-only test organization is therefore not eligible.
For settlement-scoped recognition, at least one playable chapter must also be
recognized. Admission and selected starting-role professions of faith must
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
membership and its current role, but do not require the current settlement to
recognize the organization. A valid persisted presentation therefore survives
travel and entry into an unrecognizing settlement. This does not weaken the
locally recognized equipment privilege rules. The Lodge of the Hart King
grants Low Game, Fish, and Plants to its earlier roles; its Forest-4.0 Master
role adds High Game.

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
organization's local chapter venue. Its large label combines the member's role with the service profession
where one exists (for example, `Apprentice Weaponsmith`), while the smaller
label names the organization. Crests are stable heraldic marks derived from the
organization ID and service using the locally vendored Game Icons charges, so
every catalog organization has a stable heraldic identity without
presentation-only persistence fields.

## Persistence and authority

SpacetimeDB owns membership, canonical role assignment, dues, presentation, payment, promotion, and
equipment-law enforcement. Startup seeds exactly one deterministic persistent
representative per authored chapter. The NPC carries an explicit organization
binding and has an all-day authoritative presence at either the mapped ordinary
service building or, when no such service is available, the chapter building,
and uses the compiled `organization-representative` conversation. Dialogue
effects carry no organization ID: authority resolves it from that live NPC and
revalidates the actor, session settlement, authored local chapter, derived
physical location, and organization-bound representative ID before
reusing membership reducers. Joining is idempotent; the joining fee is charged once.
Crossing a paid-through boundary suspends membership and clears its
presentation. Service operators refer prospective apprentices to the named
representative when one is present; the representative remains the authority
for joining and membership. Paying at a chapter reactivates it without
retroactive arrears.

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

### Privacy and uniqueness limitation

`organization_membership` is operational storage without a role field. The
gateway-only `backend_organization_memberships` projection joins each row to
its canonical role for strategic-web rendering and schedule management. The
private operational row is not an authorization boundary: only the effective
presented organization governs privileges or public dialogue identity. The join
reducer enforces one procedural membership
per character and organization by checking the pair before insertion; the
pre-launch schema does not yet add a composite unique index.

## Validation

Build validation rejects unknown fields and invalid or duplicate IDs,
requirements, roles, entry roles, direct transitions, weights, organization- and role-level privileges,
religions, skill leaves, malformed chapter locations, duplicate chapter
settlements, and
settlement policies. A canonical check against a compiled Viabundus world is
also required:

```powershell
python scripts/validate_organization_world.py --world path\to\compiled-world.json
```

The cross-world check is separate because the catalog can be compiled without
the large Viabundus dataset.

### Social roles and family organizations

Private SpacetimeDB tables materialize organization instances and actor-role
assignments. Every durable `Character`, including persistent settlement
residents, may hold one role per instance and roles in many instances;
transient tactical enemies do not receive persistent social roles. Assignment
IDs are keyed by character and instance, and authoritative insertion rejects a
second, conflicting role for that pair. Actor deletion removes the assignments
and then removes only organization instances left wholly unreferenced.

The specifically authored House of Habsburg and Habsburg Crown Lordships remain
available for explicit historical content and tests. Generic assignment does
not claim that every settlement belongs to them: chapterless local-house and
local-lordship templates instead produce distinct `noble-house:<settlement-id>`
and `lordship:<settlement-id>` instances. Civic communities similarly use
`civic:<settlement-id>`. None creates buildings, services, or representatives.
Urban civic membership confers the `citizen` profession;
`free_resident` remains explicit, never inferred from missing data. Initial role selection uses the
persistence-contract stable hash with versioned settlement and actor-domain
keys, is order-independent, and does not consume reducer RNG.

Every seeded family cohort also receives one stable family organization
instance and one family role. Noble families use the authored local noble-house
definition; common families use the common-family definition. Forming a new
household or marrying does not replace either spouse's birth family. A newborn
copies the mother's durable birth-family role assignments, so family identity
and household residence remain separate concepts.

Settlement clerics receive their professional role from the settlement's
authoritative church religion and the matching existing denomination-specific
learned organization. Starting learned religious practitioners receive the
same role from their already-selected starting organization. Recruiting-party
leader Characters copy the exact social and professional assignments of their
source settlement NPC; transient tactical enemies remain excluded. When actors
are deleted, an organization instance is removed only after both Character and
settlement-NPC role tables show that it is unreferenced.
