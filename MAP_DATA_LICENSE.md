# Strategic map and terrain data licence

This notice accompanies these generated Adventure Simulator artifacts:

- `strategic-map-v1.json`
- `strategic-map-tiles-v1.pack`
- `terrain-routing-v1.json`
- `terrain-routing-v1.pack`

## Licence boundary

Except for the third-party material identified below, Adventure Simulator's
copyright and similar rights in its original selection, classification,
styling, arrangement, and other contributions to these generated artifacts are
licensed under the [Creative Commons Attribution-ShareAlike 4.0 International
licence](https://creativecommons.org/licenses/by-sa/4.0/).

These generated data and image artifacts are not software and are not licensed
under the repository's GNU Affero General Public License. CC BY-SA 4.0 does not
replace or relicense the underlying datasets. Recipients must also comply with
the applicable source terms below. To retain the required attribution in a
reasonable form, redistribute this notice with the artifacts or provide a
reasonably prominent link to an equivalent permanent copy.

## Viabundus

[Viabundus map of premodern European transport and mobility, version
2](https://doi.org/10.5281/zenodo.16611998), by Bart Holterman, Maria Carina
Dengg, Maartje A.B., and Kasper H. Andersen.

The Zenodo description identifies the downloadable dataset as CC BY-SA, while
its structured rights field identifies CC BY 4.0. Adventure Simulator
conservatively treats the source and its adapted map material as [CC BY-SA
4.0](https://creativecommons.org/licenses/by-sa/4.0/).

Adventure Simulator clips, simplifies, classifies, and rasterizes Viabundus
roads, ferries, settlements, and water geometry; it also projects active roads
into the native-detail routing surface. These modifications are not endorsed
by the Viabundus authors or institutions.

## Copernicus DEM GLO-30

[Copernicus DEM GLO-30](https://doi.org/10.5270/ESA-c5d3d65) is used under the
Copernicus DEM licence, which permits reproduction, distribution, public
communication, adaptation, modification, and combination subject to its
conditions.

Produced using Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus Defence
and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and
ESA; all rights reserved.

The organisations in charge of the Copernicus programme by law or by
delegation do not incur any liability for any use of the Copernicus
WorldDEM-30.

Adventure Simulator resamples and classifies the elevation source into visual
relief, hill and mountain presentation, and a native-detail routing surface.
Neither the provider, the licensor, nor any organisation responsible for the
Copernicus programme endorses Adventure Simulator or these modifications.

## Copernicus Land Monitoring Service forest data

Generated using European Union's Copernicus Land Monitoring Service
information: [High Resolution Layer Forest 2018](https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a).

Adventure Simulator aggregates the available Tree Cover Density and leaf-type
products, classifies canopy coverage, and naturalizes forest boundaries for map
presentation and terrain routing. The source coverage and these modifications
are identified in the generated manifests. No endorsement by the European
Union or the Copernicus programme is implied.

## No raw source redistribution

The generated packs contain rendered tiles and compiled terrain values. They do
not redistribute the original Viabundus CSV files, Copernicus DEM GeoTIFFs, or
Copernicus forest source rasters. Source access remains subject to each
provider's terms and distribution service.
Cultivated-land classification and its rendered map layer are derived from
HYDE 3.5 c9 cropland areas, combined with the project inputs already identified
for roads, hydrology, elevation, wetlands, forest cover, and settlements. HYDE
terms and attribution remain applicable to distributed final terrain and map
artifacts.
