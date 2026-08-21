# Sky presentation

The tactical sky derives from the strategic scene's authoritative latitude,
longitude, absolute minute, elevation, and weather snapshot. It is transient
presentation state: tactical celestial positions and lighting are not written
back to SpacetimeDB.

## Rendering layers

Bevy's atmosphere is the base layer. It supplies physically plausible horizon
scattering, daylight colour, sunrise and sunset transitions, and a small
atmosphere-generated environment light for terrain and objects. The tactical
camera uses an explicit exposure curve keyed to solar altitude so the scene can
move from daylight through twilight to moonlight without auto-exposure pumping.
Once Bevy has allocated the filtered atmosphere environment map and a bounded
four-frame handoff grace has elapsed, it becomes
the sole daylight/twilight diffuse and specular sky source. The legacy
isotropic daylight approximation is retained only while that map is unavailable
or disabled. With IBL active, a bounded 35%-strength unresolved multi-bounce
term preserves shaded outdoor material identity, while the moon-conditioned
visibility floor remains at night. This avoids counting full hemispherical sky
energy twice. Bevy does not expose render-world filter completion in the main
world, so interactive presentation retains full fallback light during the
grace; deterministic evidence additionally requires stable consecutive image
readbacks before it reports lighting readiness.

The Sun is Bevy's physical 0.533-degree angular `SunDisk` paired with the
scene's directional light; it is not enlarged to manufacture a glare effect.
A shared analytical ephemeris resolves its direction from time, season,
latitude, and longitude. It intentionally favours stable, inexpensive outdoor
lighting over high-precision astronomical coordinates. With the atmosphere
enabled, this light carries top-of-atmosphere solar energy throughout twilight;
Bevy's atmosphere-enabled PBR evaluation applies planetary transmittance and
visible-disc occlusion so below-horizon energy scatters into the sky without
lighting the ground directly. The no-atmosphere fallback has no such planetary
occlusion, so its direct illuminance fades in over the first eight degrees
above the horizon and remains exactly zero below it. The deterministic exposure
curve transitions continuously from nautical twilight to the moon-conditioned
night target between -12 and -18 degrees solar altitude.

The Moon uses the strategic layer's canonical lunar phase and illumination.
Its shader draws the phase terminator across a constant-angular-size sphere,
including a restrained earthshine floor, while a separate cool directional
light reaches 0.25 lux at full moon. Sun and Moon shadows are mutually selected
from their altitude and lunar illumination rather than paying for both shadow
maps at once. The active light uses one 1024-pixel cascade out to 28 m: enough
for close tactical contact shadows, without spending shadow work on distant
scenery.

Stars come from the checked-in naked-eye Hipparcos subset in
`assets/data/hipparcos-bright-stars.csv`. The renderer submits the catalog as
one mesh and expands each entry into a Gaussian point-spread function in the
vertex shader. Star radii are measured in physical viewport pixels, so a 4K
display gains more resolved stars instead of scaling up a 1080p environment
texture. Apparent magnitude controls energy and B-V colour index controls
colour. Twilight and existing precipitation dim the field on the GPU.

Regenerate the catalog from the official CDS VizieR endpoint with:

```powershell
python scripts/import_hipparcos_stars.py
```

## Procedural clouds

The default tactical presentation renders up to three bounded atmospheric
decks. Camera-centred hemispheres supply rasterization geometry, but the
fragment shader intersects each view ray with scene-anchored concentric
spherical shells. Their deliberately exaggerated local curvature is negligible
across the playable area but bends distant clouds into the tactical horizon.
The shells remain stationary; wind and vertical shear advect each procedural
density field through its layer instead of moving a finite volume past the
camera.

The ray marcher searches empty air in coarse steps, backtracks and switches to
quarter-sized steps after finding density, and returns to coarse search after
leaving a cloud. A deterministic per-pixel starting offset prevents coherent
sampling bands along the curved shell. Short sun-facing shadow probes provide
internal self-shadowing, while multi-scale edge erosion, bounded trace distance,
and stronger grazing-angle aerial extinction keep distant layers from forming
a hard horizon cutoff. Solar chroma follows Sun altitude rather than applying a
permanent gold cast; dense storm cores converge toward neutral gray-blue
multiple scattering while low-Sun cloud edges can remain warm. Distinct
profiles cover cirrus, cirrocumulus, cirrostratus, altocumulus, altostratus,
nimbostratus, stratocumulus, stratus, cumulus, cumulus congestus, and
cumulonimbus. The decks can coexist, so high ice cloud need not disappear when
lower cloud develops.

The strategic weather snapshot authoritatively supplies each deck's form,
coverage, optical density, base, and top. Those layers are diagnosed from
spatially and temporally correlated humidity, dew point, pressure, wind,
vertical wind shear, instability, and broad lift fields. Precipitation follows
only from a sufficiently moist and ascending nimbostratus or cumulonimbus
state. Wind advects each density field in the authoritative direction; higher
decks move faster and turn with shear. This is a bounded procedural weather
model rather than numerical fluid dynamics: the strategic authority evaluates
the fields from time and location without storing continental atmospheric rows,
and the tactical client does not mutate them.

Cloud lighting reuses the production Sun direction, exposure, and weather
transmission. Cloud amount and optical density attenuate direct sunlight and
star visibility even when no precipitation reaches the ground. The shader uses
bounded optical depth, an inexpensive forward-scattering approximation, and
denser undersides instead of secondary shadow rays. Clouds remain separate from
the atmosphere-generated environment map; they neither trigger environment-map
regeneration nor bake their moving shape into image-based lighting.

## Verification

Use `just tactical-sky-capture` with `sun`, `sun-detail`, `twilight`, `moon`,
`stars`, `cloud-cumulus`, `cloud-stratocumulus`, `cloud-cirrus`,
`cloud-overcast`, or `cloud-storm`. Cloud captures use a diagnostics-only
profile override and a longer warm-up so shader and atmosphere pipelines have
settled before validation. `sun-detail` remains a clearly labeled 20-degree-FOV
diagnostic of the unchanged production `SunDisk::EARTH` and natural bloom at
low solar altitude;
it must not be interpreted as gameplay-scale disc size.
These deterministic native views run the production presentation plugin. The
Moon view uses a narrow verification field of view so the first-quarter
terminator remains inspectable; gameplay retains the physically scaled disc.
Together the views cover horizon colour, exposure, lunar phase, and
resolution-independent star rendering. Native scene captures disable the
atmosphere environment map only when the selected graphics preset requests
that fallback. The production-parity environment review records the observed
environment map, exposure, and post-processing state, and its twilight gate
requires a non-black, chromatically warm sky gradient rather than accepting a
dark sky over a brighter verification plane.
