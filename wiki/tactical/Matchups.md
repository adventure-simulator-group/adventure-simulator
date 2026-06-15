# Matchups

This is a coarse tuning reference for combat parameters. The goal is not to predict every attack exactly, but to provide stereotyped baselines that humans and LLMs can use consistently while adjusting weapon terms, defense, reach, incapacitation, armor mobility, and shield behavior.

Each attack row reports the outcome of a single connecting strike as **three numbers**, following the [Combat](Combat.md) model and evaluated at the average-player baseline of precision `0.75` and reflex `0.75` (see [Controls](../client/Controls.md#direct-controls)). Each is a fraction from `0.00` to `1.00`:

- **Unbalance** — the [imbalance](Combat.md#imbalance-white) (incapacitation) the blow adds, where `1.00` is enough to put the target on the ground. *Every* connecting blow imparts force and adds unbalance, even one that armor stops cold — it is the main effect of heavy blunt blows against plate. It scales roughly as `directness · weapon_joules / (10 · defender_mass_kg)`, so heavy weapons against light targets stagger most.
- **Cut** — flesh damage from energy that [penetrates](Combat.md#penetrating) armor, as the fraction of the struck body part disabled. Only blows that defeat the armor's resistance (or bypass it through a gap) cut: against rigid plate a sword does none, but a thrust into a gap against a knocked-down or unaware target does a great deal.
- **Blunt** — internal trauma (bruising, broken bones, internal bleeding) from absorbed energy that exceeds the armor's padding. A non-penetrating blow still does blunt damage *if it is severe enough*; lighter blows are soaked up entirely.

In short: a blow that does not penetrate shows **unbalance, plus blunt only if it is hard enough**; a blow that penetrates **adds cut on top**. Read the three together — cut and blunt both `0.00` with nonzero unbalance means a solid hit that armor turned aside, which is very different from `0.00` across the board (a miss, or out of reach). Damage lands on the chest, the only body part the current resolver targets.

These are **realistic targets**, not predictions of the current code. They are estimated from the [Combat](Combat.md) mechanics — connection, weapon energy and type, penetration coefficient, and the defender's armor — but corrected toward realism where the present formulas fall short (e.g. a flanged mace *should* concuss a knight through plate as blunt; a blade *should* skitter off a skeleton's bones while a mace shatters them). They form a specification the model is calibrated against. The estimates lean on the old 1–5 connection intuition as a prior for *how solidly* the blow lands at precision `0.75`, then fold in penetration — so a clean connection that armor defeats still reads low on cut, while a helpless, knocked-down target reads high even against plate because the attacker can work the gaps.

`Attack` is the attacker's relevant weapon skill for the listed weapon. A character with a bow uses ranged training, a character with a sword uses melee training, and so on, but this page keeps that distinction out of the table because each character has only one primary weapon.

## Characters

Armor stats use the format `resistance/padding/flexibility`. `Coverage` is the 0-5 armor coverage term used when checking whether a precise attack bypasses armor. Weapon energy is an approximate direct-hit calibration in joules for a trained attacker. `Shield` is the shield bonus from the combat page.

| Character          | Str | Agi | Attack | Dodge | Block | Mass kg | Weapon         | Reach m | Wpn term |   J | Pen | Armor material | Coverage | Armor dodge | Armor r/p/f | Shield |
| ------------------ | --: | --: | -----: | ----: | ----: | ------: | -------------- | ------: | -------: | --: | --: | -------------- | -------: | ----------: | ----------- | -----: |
| Peasant levy       |   3 |   2 |      2 |     2 |     1 |      70 | Spear          |     2.0 |      0.8 |  45 | 2.0 | Padded cloth   |        2 |         0.9 | 60/40/0.3   |      0 |
| Human soldier      |   3 |   3 |      3 |     2 |     2 |      84 | Halberd        |     2.2 |      0.6 | 100 | 2.0 | Steel plate    |        3 |         0.7 | 100/40/0.2  |      0 |
| Human knight       |   4 |   3 |      4 |     3 |     4 |      95 | Greatsword     |     1.6 |      0.7 |  90 | 1.0 | Steel plate    |        5 |         0.6 | 120/60/0.0  |      0 |
| Human adventurer   |   3 |   3 |      3 |     3 |     3 |      78 | Longsword      |     1.3 |      0.9 |  70 | 1.0 | Steel plate    |        2 |        0.85 | 120/35/0.0  |      0 |
| Human swordsman    |   3 |   3 |      3 |     3 |     3 |      80 | Arming sword   |     1.1 |      1.0 |  45 | 1.0 | Steel plate    |        3 |        0.85 | 120/35/0.0  |      3 |
| Human maceman      |   3 |   3 |      3 |     3 |     3 |      84 | Flanged mace   |     0.9 |      0.8 |  60 | 0.5 | Steel plate    |        3 |        0.75 | 100/35/0.2  |      3 |
| Human assassin     |   3 |   4 |      4 |     4 |     1 |      68 | Rondel dagger  |     0.4 |      2.0 |  20 | 4.0 | Cloth          |        0 |         1.0 | 0/0/0.0     |      0 |
| Human duelist      |   3 |   4 |      4 |     4 |     3 |      72 | Rapier         |     1.1 |      1.4 |  25 | 4.0 | Cloth          |        0 |         1.0 | 0/0/0.0     |      1 |
| Human hunter       |   3 |   3 |      3 |     3 |     1 |      68 | Shortbow       |    20.0 |      1.1 |  45 | 2.0 | Buff leather   |        1 |        0.95 | 20/15/0.7   |      0 |
| Human sharpshooter |   3 |   2 |      4 |     2 |     1 |      82 | Heavy crossbow |    40.0 |      0.8 | 110 | 4.0 | Steel plate    |        2 |         0.8 | 120/35/0.0  |      0 |
| Human wildman      |   4 |   4 |      2 |     4 |     1 |      85 | Club           |     1.0 |      0.9 |  70 | 0.1 | Fur hide       |        1 |        0.95 | 30/20/0.7   |      0 |
| Civilian woman     |   2 |   2 |      1 |     1 |     0 |      58 | Knife          |     0.3 |      2.0 |  15 | 2.0 | Cloth          |        0 |         1.0 | 0/0/0.0     |      0 |
| Shieldmaiden       |   3 |   4 |      4 |     4 |     3 |      62 | Glaive         |     2.0 |      0.7 |  70 | 2.0 | Steel plate    |        1 |         0.9 | 100/35/0.2  |      0 |
| Elf hero           |   4 |   5 |      5 |     5 |     4 |      66 | Elven sword    |     1.1 |      1.2 |  55 | 1.0 | Elven lamellar |        3 |         0.9 | 70/40/0.8   |      2 |
| Goblin raider      |   2 |   2 |      2 |     2 |     1 |      40 | Short spear    |     1.5 |      1.0 |  25 | 2.0 | Leather        |        1 |        0.95 | 20/10/0.6   |      1 |
| Orc brute          |   4 |   1 |      2 |     1 |     1 |     105 | Club           |     1.0 |      0.9 |  70 | 0.1 | Hide           |        1 |         0.9 | 30/20/0.7   |      0 |
| Armored orc        |   4 |   2 |      3 |     2 |     2 |     115 | Heavy axe      |     1.2 |      0.7 |  90 | 1.0 | Crude steel    |        4 |        0.65 | 100/35/0.2  |      0 |
| Zombie             |   3 |   1 |      1 |     0 |     0 |      75 | Claws          |     0.5 |      1.8 |  20 | 0.1 | Cloth          |        0 |         1.0 | 0/0/0.0     |      0 |
| Skeleton           |   2 |   2 |      2 |     1 |     1 |      25 | Rusted sword   |     1.0 |      0.9 |  25 | 1.0 | Rusted mail    |        2 |         1.0 | 20/0/0.1    |      2 |
| Vampire fledgling  |   4 |   4 |      1 |     2 |     0 |      72 | Claws          |     0.5 |      1.8 |  25 | 0.1 | Cloth          |        0 |         1.0 | 0/0/0.0     |      0 |
| Vampire knight     |   5 |   5 |      5 |     5 |     5 |      95 | Longsword      |     1.3 |      1.0 |  90 | 1.0 | Steel plate    |        5 |         0.6 | 120/60/0.0  |      0 |
| Wolf               |   2 |   4 |      3 |     2 |     0 |      45 | Bite           |     0.4 |      1.6 |  25 | 0.5 | Fur            |        0 |         1.0 | 5/0/0.8     |      0 |
| Bear               |   6 |   2 |      3 |     1 |     0 |     250 | Claws          |     0.8 |      1.0 | 120 | 0.5 | Fur and fat    |        0 |         1.0 | 10/20/0.8   |      0 |

## Attacks

`Distance m` is the separation at attack release. Blank modifier cells mean the baseline case: the attacker is using their listed weapon at normal distance, the defender is aware and defending correctly, and neither combatant has a special positional or incapacitation state. The attack itself is inferred from the attacker's weapon in the character table.

The matchups are deliberately **not** exhaustive — pairing every character against every other would be a combinatorial explosion to maintain. The player characters are *builds* a player might run — the humans together with the elf hero — and they are tested almost entirely against the fantasy enemies (orc, goblin, undead, beasts), which is what players actually fight; the wildman is grouped with the enemies for this purpose. Player-versus-player matchups are restricted to the three PvP builds — **knight**, **duelist**, and **assassin** — matched against one another; the elf hero, though a player build, stays out of PvP. Every build still appears in enough rows to exercise its weapon and armor against representative threats.

Where two builds attack identically, only one carries the attack rows. The **swordsman** is the stand-in for the one-handed sword; the **adventurer**, whose longsword is swung two-handed (it carries no shield), attacks much like the **knight**'s greatsword, so it needs no attack rows of its own. The two sword builds diverge on **defense** — the swordsman's shield versus the adventurer's lack of one — so both still appear as defenders.

| Attacker | Defender | Distance m | Modifier | Unbalance | Cut | Blunt |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| Human soldier | Orc brute | 2.2 |  | 0.07 | 0.85 | 0.00 |
| Human soldier | Orc brute | 0.8 | Too close for weapon, fit 0.2 | 0.00 | 0.00 | 0.00 |
| Human soldier | Orc brute | 2.2 | Defender flanked 90 degrees | 0.09 | 1.00 | 0.00 |
| Human soldier | Orc brute | 2.2 | Attacker incapacitation 60% | 0.05 | 0.55 | 0.00 |
| Orc brute | Human soldier | 1.0 |  | 0.03 | 0.00 | 0.00 |
| Orc brute | Human soldier | 1.0 | Defender flanked 90 degrees | 0.06 | 0.00 | 0.15 |
| Orc brute | Human soldier | 1.0 | Defender incapacitation 60% | 0.07 | 0.00 | 0.25 |
| Human soldier | Armored orc | 2.2 |  | 0.04 | 0.00 | 0.20 |
| Human soldier | Armored orc | 0.8 | Too close for weapon, fit 0.2 | 0.00 | 0.00 | 0.00 |
| Human soldier | Armored orc | 2.2 | Defender flanked 90 degrees | 0.07 | 0.15 | 0.35 |
| Armored orc | Human soldier | 1.2 |  | 0.03 | 0.00 | 0.00 |
| Armored orc | Human soldier | 1.2 | Defender flanked 90 degrees | 0.08 | 0.25 | 0.20 |
| Armored orc | Human soldier | 1.2 | Defender incapacitation 60% | 0.09 | 0.30 | 0.25 |
| Human duelist | Human knight | 1.1 |  | 0.01 | 0.00 | 0.00 |
| Human duelist | Human knight | 1.1 | Defender flanked 90 degrees | 0.02 | 0.00 | 0.00 |
| Human duelist | Human knight | 1.1 | Defender incapacitation 60% | 0.02 | 0.00 | 0.00 |
| Human assassin | Human knight | 0.4 |  | 0.01 | 0.00 | 0.00 |
| Human assassin | Human knight | 1.1 | Too far for weapon, fit 0.0 | 0.00 | 0.00 | 0.00 |
| Human assassin | Human knight | 0.4 | Defender flanked 180 degrees | 0.02 | 0.40 | 0.00 |
| Human assassin | Human knight | 0.4 | Defender knocked down, incapacitation 110% | 0.02 | 0.85 | 0.00 |
| Human duelist | Human assassin | 1.1 |  | 0.02 | 0.30 | 0.00 |
| Human duelist | Human assassin | 1.1 | Defender flanked 90 degrees | 0.03 | 0.50 | 0.00 |
| Human duelist | Human assassin | 1.1 | Attacker incapacitation 60% | 0.01 | 0.20 | 0.00 |
| Human assassin | Human duelist | 0.4 |  | 0.01 | 0.25 | 0.00 |
| Human assassin | Human duelist | 0.4 | Defender shield unavailable | 0.02 | 0.40 | 0.00 |
| Human assassin | Human duelist | 0.4 | Defender flanked 180 degrees | 0.03 | 0.55 | 0.00 |
| Human wildman | Human adventurer | 1.0 |  | 0.04 | 0.00 | 0.00 |
| Human wildman | Human adventurer | 1.0 | Defender flanked 90 degrees | 0.06 | 0.05 | 0.20 |
| Human swordsman | Human wildman | 1.1 |  | 0.02 | 0.20 | 0.00 |
| Human swordsman | Human wildman | 1.1 | Defender flanked 90 degrees | 0.04 | 0.50 | 0.00 |
| Human swordsman | Human wildman | 1.1 | Defender incapacitation 60% | 0.04 | 0.60 | 0.00 |
| Shieldmaiden | Armored orc | 2.0 |  | 0.03 | 0.00 | 0.00 |
| Shieldmaiden | Armored orc | 0.7 | Too close for weapon, fit 0.2 | 0.00 | 0.00 | 0.00 |
| Shieldmaiden | Armored orc | 2.0 | Defender flanked 90 degrees | 0.05 | 0.10 | 0.20 |
| Armored orc | Shieldmaiden | 1.2 |  | 0.04 | 0.00 | 0.00 |
| Armored orc | Shieldmaiden | 1.2 | Defender flanked 90 degrees | 0.10 | 0.50 | 0.15 |
| Armored orc | Shieldmaiden | 1.2 | Defender incapacitation 60% | 0.12 | 0.55 | 0.15 |
| Human hunter | Orc brute | 15.0 |  | 0.02 | 0.25 | 0.00 |
| Human hunter | Orc brute | 25.0 | Long range 25 m | 0.01 | 0.15 | 0.00 |
| Human hunter | Orc brute | 15.0 | Defender flanked 90 degrees | 0.03 | 0.40 | 0.00 |
| Human sharpshooter | Vampire knight | 35.0 |  | 0.01 | 0.00 | 0.00 |
| Human sharpshooter | Vampire knight | 35.0 | Defender flanked 180 degrees | 0.11 | 0.15 | 0.00 |
| Human sharpshooter | Vampire knight | 35.0 | Defender knocked down, incapacitation 110% | 0.12 | 0.85 | 0.00 |
| Elf hero | Orc brute | 1.1 |  | 0.05 | 0.90 | 0.00 |
| Elf hero | Orc brute | 1.1 | Defender flanked 90 degrees | 0.05 | 1.00 | 0.00 |
| Elf hero | Orc brute | 1.1 | Attacker incapacitation 60% | 0.03 | 0.60 | 0.00 |
| Orc brute | Elf hero | 1.0 |  | 0.01 | 0.00 | 0.00 |
| Orc brute | Elf hero | 1.0 | Defender flanked 90 degrees | 0.07 | 0.05 | 0.15 |
| Orc brute | Elf hero | 1.0 | Defender incapacitation 60% | 0.08 | 0.00 | 0.25 |
| Elf hero | Vampire knight | 1.1 |  | 0.03 | 0.00 | 0.00 |
| Elf hero | Vampire knight | 1.1 | Defender flanked 180 degrees | 0.05 | 0.15 | 0.00 |
| Elf hero | Vampire knight | 1.1 | Defender knocked down, incapacitation 110% | 0.06 | 0.60 | 0.00 |
| Goblin raider | Human swordsman | 1.5 |  | 0.01 | 0.00 | 0.00 |
| Goblin raider | Human swordsman | 1.5 | Defender shield unavailable | 0.02 | 0.10 | 0.00 |
| Goblin raider | Human swordsman | 1.5 | Defender flanked 90 degrees | 0.02 | 0.15 | 0.00 |
| Goblin raider | Human hunter | 1.5 |  | 0.03 | 0.20 | 0.00 |
| Goblin raider | Human hunter | 0.4 | Too close for weapon, fit 0.2 | 0.00 | 0.00 | 0.00 |
| Goblin raider | Human hunter | 1.5 | Attacker incapacitation 60% | 0.02 | 0.15 | 0.00 |
| Human swordsman | Goblin raider | 1.1 |  | 0.08 | 0.55 | 0.00 |
| Human swordsman | Goblin raider | 1.1 | Attacker incapacitation 60% | 0.06 | 0.30 | 0.00 |
| Human swordsman | Goblin raider | 1.1 | Defender flanked 90 degrees | 0.10 | 0.85 | 0.00 |
| Peasant levy | Orc brute | 2.0 |  | 0.03 | 0.35 | 0.00 |
| Peasant levy | Orc brute | 0.6 | Too close for weapon, fit 0.2 | 0.00 | 0.00 | 0.00 |
| Orc brute | Peasant levy | 1.0 |  | 0.05 | 0.00 | 0.00 |
| Orc brute | Peasant levy | 1.0 | Defender flanked 90 degrees | 0.08 | 0.00 | 0.20 |
| Zombie | Human swordsman | 0.5 |  | 0.00 | 0.00 | 0.00 |
| Zombie | Human swordsman | 0.5 | Defender flanked 180 degrees | 0.02 | 0.10 | 0.00 |
| Zombie | Human swordsman | 0.5 | Defender knocked down, incapacitation 110% | 0.03 | 0.25 | 0.00 |
| Human swordsman | Zombie | 1.1 |  | 0.06 | 0.90 | 0.00 |
| Human swordsman | Zombie | 1.1 | Attacker incapacitation 60% | 0.05 | 0.75 | 0.00 |
| Human swordsman | Zombie | 1.8 | Out of reach, fit 0.0 | 0.00 | 0.00 | 0.00 |
| Skeleton | Human swordsman | 1.0 |  | 0.00 | 0.00 | 0.00 |
| Skeleton | Human swordsman | 1.0 | Defender flanked 180 degrees | 0.03 | 0.15 | 0.00 |
| Skeleton | Human swordsman | 1.0 | Defender knocked down, incapacitation 110% | 0.03 | 0.35 | 0.00 |
| Human swordsman | Skeleton | 1.1 |  | 0.14 | 0.15 | 0.25 |
| Human swordsman | Skeleton | 1.1 | Attacker incapacitation 60% | 0.09 | 0.10 | 0.20 |
| Human maceman | Skeleton | 0.9 |  | 0.22 | 0.10 | 0.80 |
| Human maceman | Skeleton | 0.9 | Attacker incapacitation 60% | 0.18 | 0.10 | 0.65 |
| Vampire fledgling | Human swordsman | 0.5 |  | 0.00 | 0.00 | 0.00 |
| Vampire fledgling | Human swordsman | 0.5 | Defender flanked 180 degrees | 0.03 | 0.15 | 0.00 |
| Vampire fledgling | Human swordsman | 0.5 | Defender knocked down, incapacitation 110% | 0.03 | 0.35 | 0.05 |
| Human swordsman | Vampire fledgling | 1.1 |  | 0.05 | 0.75 | 0.00 |
| Human swordsman | Vampire fledgling | 1.1 | Defender flanked 90 degrees | 0.06 | 0.90 | 0.00 |
| Wolf | Human hunter | 0.4 |  | 0.03 | 0.25 | 0.00 |
| Wolf | Human hunter | 0.4 | Attacker incapacitation 60% | 0.02 | 0.00 | 0.00 |
| Wolf | Human swordsman | 0.4 |  | 0.01 | 0.00 | 0.00 |
| Wolf | Human swordsman | 0.4 | Defender shield unavailable | 0.02 | 0.05 | 0.00 |
| Human swordsman | Wolf | 1.1 |  | 0.08 | 0.75 | 0.00 |
| Human swordsman | Wolf | 1.8 | Out of reach, fit 0.0 | 0.00 | 0.00 | 0.00 |
| Bear | Human knight | 0.8 |  | 0.06 | 0.00 | 0.00 |
| Bear | Human knight | 0.8 | Defender knocked down, incapacitation 110% | 0.13 | 0.50 | 0.55 |
| Human knight | Bear | 1.6 |  | 0.03 | 0.65 | 0.00 |
| Human knight | Bear | 1.6 | Attacker incapacitation 60% | 0.02 | 0.45 | 0.00 |
| Vampire knight | Human knight | 1.3 |  | 0.09 | 0.00 | 0.35 |
| Vampire knight | Human knight | 1.3 | Attacker incapacitation 60% | 0.07 | 0.00 | 0.10 |
| Vampire knight | Human knight | 2.0 | Out of reach, fit 0.0 | 0.00 | 0.00 | 0.00 |
| Human knight | Vampire knight | 1.6 |  | 0.01 | 0.00 | 0.00 |
| Human knight | Vampire knight | 1.6 | Defender flanked 180 degrees | 0.09 | 0.15 | 0.30 |
| Human knight | Vampire knight | 1.6 | Defender knocked down, incapacitation 110% | 0.09 | 0.45 | 0.45 |
| Human soldier | Vampire knight | 2.2 |  | 0.01 | 0.00 | 0.00 |
| Human soldier | Vampire knight | 2.2 | Defender flanked 180 degrees | 0.10 | 0.20 | 0.35 |
| Human soldier | Vampire knight | 2.2 | Defender knocked down, incapacitation 110% | 0.11 | 0.45 | 0.55 |
| Human assassin | Vampire knight | 0.4 |  | 0.00 | 0.00 | 0.00 |
| Human assassin | Vampire knight | 0.4 | Defender attacked from behind 150 degrees | 0.02 | 0.40 | 0.00 |
| Human assassin | Vampire knight | 0.4 | Defender knocked down, incapacitation 110% | 0.02 | 0.85 | 0.00 |
