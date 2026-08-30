use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub(crate) struct GeologyArgs {
    #[arg(long, default_value_os_t = default_geology_geopackage())]
    pub(crate) geopackage: PathBuf,
    #[arg(long, default_value_os_t = default_fault_geopackage())]
    pub(crate) faults: PathBuf,
}

fn default_geology_geopackage() -> PathBuf {
    super::repository_root().join("target/world-data-sources/raw/geology/GeologicUnitView.gpkg")
}

fn default_fault_geopackage() -> PathBuf {
    super::repository_root().join("target/world-data-sources/raw/faults/hikefaultdbv17b.gpkg")
}
