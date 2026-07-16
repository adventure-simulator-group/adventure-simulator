# Elevation world data

Settlement elevation comes from the **Copernicus DEM GLO-30** digital surface
model. The current source is modern rather than a historical reconstruction;
terrain height is sufficiently stable for the game's plausibility-oriented
1544 world generation.

- Product DOI: <https://doi.org/10.5270/ESA-c5d3d65>
- Product information: <https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM>
- Terms: Copernicus DEM licence

The manually downloaded `*_DEM.tif` tiles belong in the Git-ignored
`target/world-data-sources/raw/elevation/` directory. This source is not yet
part of an initialization script while its suitability is being evaluated.

`sources::elevation` uses the pure-Rust `tiff` crate and does not require GDAL
or PROJ. It groups settlements by one-degree GLO-30 tile, decodes only the 159
tiles used by the current Viabundus settlement set, and releases each raster
before reading the next. A settlement receives a required `ElevationMeters`
value; `ElevationBand` is derived from it rather than redundantly stored, so the
two can never contradict each other. There is no unknown variant. Invalid or
void source pixels are replaced by the nearest plausible pixel within eight
raster cells, then by sea level if the local window is entirely void. The
build report counts these fallbacks. The verified 1544 build sampled all 6,041
settlements without using a fallback.

Elevation is stored on settlements because it describes the settlement's own
location and can directly influence scene selection, climate inference,
agriculture, travel preparation, and UI presentation. A future source may add
route elevation profiles separately; settlement elevation should not be used
as a proxy for terrain along an entire road edge.
