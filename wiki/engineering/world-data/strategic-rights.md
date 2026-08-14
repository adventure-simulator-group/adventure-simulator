# Strategic rights vocabulary

`adventuresim_core::rights` provides pure, typed questions and private evidence
for asking whether a subject may perform one exact operation on an object,
place, fixture, or domain-owned resource in one jurisdiction. It is not a
universal ACL and does not grant authority. Strategic reducers still gather
authoritative rows, decide domain consequences, and mutate transactionally.

Ownership, operational custody, permission grants, organization privileges,
jurisdiction, and obligations are deliberately separate values. In particular,
`OperationalCustody` describes where an object is held and proves neither title
nor legality. A permission for `Use` cannot answer `Own`, `Alter`, or
`TransferCustody`; a grant scoped to a place cannot answer a global question.
Checked question construction rejects a place or fixture paired with a
different place jurisdiction and rejects transfer of an object into itself.
Global questions remain valid for place-bound resources.

Domain extensions implement marker traits with closed Rust enums or structs.
They do not use action-kind strings, JSON payloads, or client-authored evidence.
`OrganizationPrivilegeEvidence` carries typed organization, role,
presentation, recognition, jurisdiction, and revision hooks so adapters can
retain their own rules. Local equipment law and global foraging privileges are
therefore distinct exact questions rather than one inferred membership rule.

Permission validity is inclusive at both authored endpoints. Revoked, expired,
not-yet-valid, and consumed grants fail closed, and a revocation revision cannot
precede its grant revision. A single-use active grant yields a pure consumption
proposal bound to grant ID/revision, attempt ID, server-authored request/action
identity, and authoritative input digest. The reducer must atomically
revalidate, apply, and persist that proposal with a closed typed domain outcome.
The exact persisted receipt and provenance produce a top-level idempotent replay
that returns the prior outcome without reapplying effects, even after the
original validity window. Reusing the attempt with different action provenance
is a typed collision; a different attempt sees an already-consumed grant.
Reusable grants never carry consumption state.

Private decisions can retain ownership, custody, hidden grants, organization
recognition, obligations, and domain evidence. Their public projection exposes
only an allowed preview supplied by the adapter or the single sanitized
`Unavailable` rejection. Separate checked constructors make it impossible for
a denied decision to carry a reusable, consume, or replay proposal. The
vocabulary has no serialization derives, schema,
SQL, generated bindings, mutation, prices, fines, reputation, objectives,
offenses, or crime mechanics.

## Planned adapters

- Inventory and equipment will ask object ownership/custody/use/transfer and
  local equipment-jurisdiction questions while retaining existing mutations.
- Foraging will ask its existing global organization-privilege question without
  inheriting local presentation rules.
- Corpse actions will use typed corpse resources and transactional family or
  authority permission evidence.
- Repair will keep custody/escrow and retry-safe return obligations separate
  from ownership.
- Residence access and existing obligations will use domain-owned resource and
  obligation enums without introducing general trespass or lock mechanics.

After the pure strategic action planner is stacked with this foundation, domain
adapters may supply private rights decisions as planner requirements. Plans
remain evidence rather than authority, and commit-time reducers must re-ask the
same question against a fresh authoritative revision.
