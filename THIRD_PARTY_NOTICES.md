# Third-party notices

## Fantastic-disease folklore research

The fantastic-disease content paraphrases historical dictionary, scholarly,
library-catalogue, and public-domain sources. Source text is not redistributed:

- *Frühneuhochdeutsches Wörterbuch*, “Mar” and “Bilwis” entries:
  <https://www.fwb-online.de/go/mar.s.0m_1709314623> and
  <https://fwb-online.de/lemma/bilwis.s.1f>
- Thomas Schürmann, “Der Nachzehrerglauben in Mitteleuropa,”
  *Ethnographisch-Archäologische Zeitschrift* 50 (2009):
  <https://www.eaz-journal.org/index.php/eaz/article/download/851/907/2938>
- Georgius Agricola, *De re metallica* (1556), public-domain English
  translation: <https://www.gutenberg.org/files/38015/38015-h/38015-h>
- Folger Shakespeare Library catalogue record for Agricola,
  *De animantibus subterraneis* (1549):
  <https://catalog.folger.edu/record/367251>
- D. L. Ashliman's research collection, “The Mare in Scandinavian Belief,”
  used only for explicitly later comparative variants:
  <https://sites.pitt.edu/~dash/nightmare.html#mart>

## Strategic map data

The generated Paper map and terrain-routing packs adapt Viabundus Pre-modern
Street Map 2, Copernicus DEM GLO-30, and Copernicus Land Monitoring Service
forest data offline. Their canonical redistribution notice is
[MAP_DATA_LICENSE.md](MAP_DATA_LICENSE.md). It applies CC BY-SA 4.0 to the
project-owned contributions, excludes the generated data artifacts from the
software AGPL, identifies modifications, and retains the source-specific
attribution, liability, and no-endorsement terms. The compiler copies that
notice beside every generated map or terrain pack.

## Game Icons

The strategic web interface vendors monochrome SVG artwork from
[Game-Icons.net](https://game-icons.net/), obtained from Iconify's
`@iconify-json/game-icons` package version 1.2.4.

- License: [Creative Commons Attribution 3.0](https://creativecommons.org/licenses/by/3.0/)
- Upstream repository and license: <https://github.com/game-icons/icons/blob/master/license.txt>
- Iconify collection: <https://icon-sets.iconify.design/game-icons/>

Upstream contributors (the complete roster in the upstream license): Lorc,
Delapouite, John Colburn, Felbrigg, John Redman, Carl Olsen, Sbed, PriorBlue,
Willdabeast, Viscious Speed, Lord Berandas, Irongamer, HeavenlyDog, Lucas,
Faithtoken, Skoll, Andy Meneely, Cathelineau, Kier Heyl, Aussiesim, Sparker,
Zeromancer, Rihlsul, Quoting, Guard13007, DarkZaitzev, SpencerDub,
GeneralAce135, Zajkonur, Catsu, Starseeker, Pepijn Poolman, Pierre Leducq,
Caro Asercion, and SeregaCthtuf. Some upstream folders are expressly CC0;
the vendored compilation is conservatively treated as CC BY 3.0 because its
Iconify metadata does not preserve per-glyph authorship or CC0 status.

Vendored icon names:

acrobatic, ancient-sword, anvil, arm, arm-bandage, armor-cuisses, armor-vest,
awareness, bandage-roll, barbute, bed, beer-stein, belt-armor, biceps, bleeding-eye,
bleeding-wound, bo, bordered-shield, bow-arrow, bowie-knife, bracer, brain,
bread, breastplate, broad-dagger, broadsword, brodie-helmet, broken-heart,
bullseye, byzantin-temple, caduceus, calendar, campfire, camping-tent, castle, chain-mail, check-mark,
chest-armor, church, clothes, coins, coma, conversation, crested-helmet,
cross-mark, crossbow, crossed-swords, crown, daggers, death-skull, dodge, duration,
eye-target, flanged-mace, gothic-cross, greaves, halberd, hammer-nails, hammer-sickle,
heart-beats, heart-minus, heavy-helm, helmet, help, holy-symbol, hood, house,
human-ear, inner-self, juggler, knapsack, layered-armor, leg, light-helm,
lockpicks, mailed-fist, mail-shirt, meal, medical-pack, metal-skirt,
mounted-knight, musket, night-sleep, open-book, open-chest, person, piercing-sword,
plain-arrow, plain-dagger, pocket-bow, prayer, pteruges, relic-blade, rifle,
roman-shield, rose, round-shield, running-ninja, saber-slash, samara-mosque,
scales, scalpel, shield,
shield-echoes, shirt, shop, skirt, sleeveless-jacket, spear-hook, spears,
spiked-halo, split-cross, stiletto, stomach, stopwatch, sun, sword-brandish, sword-clash,
sword-hilt, templar-shield,
terror, tightrope, torch, treasure-map, trousers, two-handed-sword, visored-helm,
warhammer, water-bottle, water-drop, waterskin, weight, wingfoot, wood-axe,
wood-club.

The files in `crates/strategic-web/static/icons/game/` were converted from
Iconify JSON bodies into standalone SVGs without altering the artwork. CSS
masks supply colour at runtime.

## Font Awesome Free

The Religion skill icons `fontawesome-cross.svg`,
`fontawesome-star-and-crescent.svg`, and `fontawesome-star-of-david.svg` are
from Font Awesome Free 7.3.1 by Fonticons, Inc. They are licensed under
[Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/).

- Sources: <https://fontawesome.com/icons/cross>, <https://fontawesome.com/icons/star-and-crescent>, and <https://fontawesome.com/icons/star-of-david>
- License: <https://fontawesome.com/license/free>
### HYDE 3.5

Generated strategic map tiles and terrain packs include a cultivated-land
classification derived from HYDE 3.5 c9 historical cropland-area data. See
`wiki/reference/historical-land-use.md` and the source release metadata for provenance
and applicable terms.
