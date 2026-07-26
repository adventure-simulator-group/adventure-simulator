# Health

Adventure Simulator uses durable injuries, blood loss, disease, pain, and
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

Prepared interventions act on the patient's condition rather than naming a
disease they magically cure. Treatment may improve or worsen the evidence
available to an observer, but never reveals hidden truth directly.

The detailed privacy, meter, Humour, and notebook contract lives in
[Physiology](../reference/physiology.md).

## Treatment

Treatment is performed on one patient and one body region at a time:

- bandaging uses Anatomy and a bandage;
- splinting uses Anatomy and a splint;
- projectile extraction combines Anatomy and Knife;
- stitching combines Anatomy and Tailoring;
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

## Fantasy relief

Real recovery can be slow. The game uses two ways to keep realistic injuries
from becoming dead time:

- strategic time can skip uneventful recovery;
- fantasy ancestry, rare preparations, or other setting-specific content may
  provide costly exceptions without changing the physical baseline.

The intent is for wounds to shape an expedition and its aftermath without
forcing the player to watch every hour pass.
