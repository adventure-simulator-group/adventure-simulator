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
maps at once.

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

## Deferred clouds

Clouds remain deliberately separate from this module while terrain foliage is
under parallel development. The intended quality path is volumetric clouds,
with a later browser fallback using a lower-cost procedural sky layer. Both
paths should consume the same normalized coverage, density, wind, and lighting
inputs so graphics presets can switch implementations without changing the
amount or broad silhouette of cloud cover. Clouds should not be baked into an
environment map.

## Verification

Use `just tactical-sky-capture` with `sun`, `sun-detail`, `twilight`, `moon`, or
`stars`. `sun-detail` is a clearly labeled 20-degree-FOV diagnostic of the same
unchanged production `SunDisk::EARTH` and natural bloom at low solar altitude;
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
