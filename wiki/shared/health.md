# Health

Durable post-mortem anatomy is a bounded strategic outcome rather than tactical
tick state. See [Autopsies](../strategic/autopsies.md) for custody,
decomposition, permission, and medical skill boundaries.

Fabelgeist uses durable injuries, blood loss, disease, pain, and
recovery rather than a single rapidly refilling combat-health pool. The
strategic/tactical split keeps that depth playable: immediate danger happens in
combat, while treatment and long recovery can advance through strategic time.

## Body regions and injuries

Damage is associated with seven body regions. Injury reduces the function of
the affected region and therefore its related attributes. Damage beyond
incapacitation still matters because it lengthens recovery and may destroy or
sever tissue.

Strategic injuries distinguish:

- open cuts, which bleed and deteriorate until bandaged;
- bruising, which heals without a procedure;
- fractures, which recover better after splinting;
- retained projectiles, which slow every healing component on that limb.

Autoresolve commits these durable results after the battle. Real-time tactical
combat will eventually produce the same result summary without persisting its
live hit-by-hit state.

## Blood loss and incapacitation

Open wounds drain blood in proportion to their severity. Bandaging stabilizes a
cut; stitching and projectile extraction require more specialized tools and
skills. Extremely low blood volume is fatal.

Pain, blood loss, fear, fatigue, and physical injury all contribute to whether
a character is ready for strategic activity. A character can survive a battle
yet remain unable to travel or fight safely.

## Disease

Characters do not automatically know which disease they have. They can observe
outward symptoms and seek help from someone trained in Physiology, but the
result is a fallible differential rather than an authoritative diagnosis.

Diseases use specific transmission routes such as close contact, food and
water, vermin, wounds, or infected blood. Filth raises some risks. Blood on a
character remains visibly dirty until washed, while its infectiousness fades
over strategic time.

Physiology knowledge in a co-located party passively reduces preventable
exposure: physicians can warn against suspect food or water, separate close
company, and reduce vermin contact. The reduction is dramatic for food, water,
and close contact, bounded for vermin, and modest for infected blood. It never
removes all risk. Wound acquisition remains Surgery's responsibility, and
Physiology does not invent missing space, clean water, soap, bandages, or
disinfectant. Those physical controls retain their stronger modeled effects.
Infections capable of close-contact spread can pass between party members,
including before symptoms make the source publicly identifiable.

The exposure calculation keeps the preventable behavior dose separate from
unavoidable environmental dose. Existing physical state also caps the
preventable share where relevant: soap-backed washing removes blood first,
while an unprotected wound and unavailable clean handling sharply limit what
knowledge alone can prevent.

Shared sleep is the overnight form of close-contact co-presence. Sleep, travel,
and treatment evaluate all co-advancing party members
from one pre-action snapshot. This makes prevention and close-contact
transmission independent of character ID or reducer iteration order. Solo
catch-up uses recorded co-presence only; it cannot borrow future protection
from a physician who is not advancing with the patient.

Community, blood, and close-contact acquisitions share one absolute-minute
timeline. Characters infected in the same minute do not infect one another
instantaneously; each becomes a possible contact source on the following
minute. Long shared rests and equivalent shorter chunks therefore preserve the
same secondary-spread chain.

Prepared interventions act on the patient's condition rather than naming a
disease they magically cure. Treatment may improve or worsen the evidence
available to an observer, but never reveals hidden truth directly.

The detailed privacy, meter, Humour, and notebook contract lives in
[Physiology](physiology.md).

## Treatment

Treatment is performed on one patient and one body region at a time:

- bandaging uses Surgery and a bandage;
- splinting uses Surgery and a splint;
- projectile extraction uses Surgery;
- stitching uses Surgery;
- cleaning consumes water, soap, and—where appropriate—disinfectant.

Procedures advance the participants' personal strategic time. They may be
interrupted by terminal events, and supplies are consumed only according to the
validated procedure boundary.

Treating another character can transfer their blood to the caregiver. Clean
tools, soap, bandages, and suitable alcohol therefore matter beyond simple UI
flavor.

## Recovery

Recovery occurs during unallocated leisure rather than every elapsed minute. A
fully scheduled day grants no passive convalescence.

The wound category determines its base recovery, modified by the party's
Physiology support. Retained projectiles impede recovery. Blood volume restores
over time once bleeding is controlled.

Settlements offer the safest place to convalesce because a party can advance
time, obtain food and shelter, and access services. Recovering in the field is
possible but competes with travel, supplies, exposure, and continuing danger.

A living party member at the same settlement may pay an inn directly for one
day of another member's publicly necessary convalescence when the patient
cannot pay. The patient contributes the coin they have, and the inn receives
only the remaining authoritative price directly from the payer; the patient
never receives transferable coin. This cooperative lodging does not
grant authority over treatment, diagnosis, inventory, or arbitrary stretches
of the patient's time.

## Fantasy relief

Real recovery can be slow. The game uses two ways to keep realistic injuries
from becoming dead time:

- strategic time can skip uneventful recovery;
- fantasy ancestry, rare preparations, or other setting-specific content may
  provide costly exceptions without changing the physical baseline.

The intent is for wounds to shape an expedition and its aftermath without
forcing the player to watch every hour pass.

Fantastic disease follows the same rule: invented organisms and toxins may
produce unusual but physical effects, while remaining inside the existing
meters, routes, deterministic curves, phenotype variation, and generic
interventions. Four starter diseases deliberately project to the four
element/Humour correspondences used by Paracelsian scholars, but the underlying
causes remain organisms, toxins, and environmental exposure rather than
Humours. See [Fantastic diseases](fantastic-diseases.md).

The ordinary limb-treatment reducer accepts any co-present full Character, not
only party members, when the patient explicitly consents or is incapacitated.
It revalidates presence and authorization after elapsed procedure time.
Wounded road actors carry real `LimbInjury` state and use ordinary bandages,
time, infection, and skill rules.
