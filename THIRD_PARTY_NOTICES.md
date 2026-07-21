# Third-party notices

## Strategic map data

The generated Paper map tiles adapt these datasets offline; the source raster
and vector files are not shipped to the browser:

- [Viabundus Pre-modern Street Map 2](https://doi.org/10.5281/zenodo.16611998),
  conservatively treated as [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
  Adventure Simulator clips, simplifies, classifies, and rasterizes the source
  roads, ferries, settlements, and water geometry.
- [Copernicus DEM GLO-30](https://doi.org/10.5270/ESA-c5d3d65). Credit: European
  Union, Copernicus DEM GLO-30. Produced using Copernicus WorldDEM-30 © DLR e.V.
  2010–2014 and © Airbus Defence and Space GmbH 2014–2018 provided under
  COPERNICUS by the European Union and ESA; all rights reserved. Adventure
  Simulator generalizes the elevation into map relief. Neither the European
  Commission nor ESA is liable for use of Copernicus data and information.
- [Copernicus HRL Forest 2018](https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a).
  © European Union, Copernicus Land Monitoring Service. Adventure Simulator
  aggregates and procedurally reshapes the partial source coverage into sparse
  and deep woodland bands. No endorsement by the European Union or Copernicus
  programme is implied.

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
bullseye, byzantin-temple, calendar, campfire, camping-tent, castle, chain-mail, check-mark,
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
