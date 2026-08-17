//! Export a deterministic weapon silhouette as a grayscale PNG.

use std::{env, fs, path::PathBuf};

use adventuresim_weapon_model::{
    WeaponIconSpec, default_design, default_holder_design, generate_holder_icon, generate_icon,
    preset_design,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let id = args
        .next()
        .ok_or("usage: export_icon <preset-or-catalog-id|holder:catalog-id> <output.png> [size]")?;
    let output = PathBuf::from(args.next().ok_or(
        "usage: export_icon <preset-or-catalog-id|holder:catalog-id> <output.png> [size]",
    )?);
    let size = args.next().map_or(Ok(128_u16), |value| value.parse())?;
    if args.next().is_some() {
        return Err(
            "usage: export_icon <preset-or-catalog-id|holder:catalog-id> <output.png> [size]"
                .into(),
        );
    }
    let spec = WeaponIconSpec {
        size,
        ..WeaponIconSpec::default()
    };
    let icon = if let Some(weapon_id) = id.strip_prefix("holder:") {
        let weapon = default_design(weapon_id)
            .ok_or_else(|| format!("unknown weapon catalog ID `{weapon_id}`"))?;
        let holder = default_holder_design(&weapon)
            .ok_or_else(|| format!("weapon `{weapon_id}` has no fitted holder"))?;
        generate_holder_icon(&holder, spec)?
    } else {
        let design = preset_design(&id)
            .or_else(|| default_design(&id))
            .ok_or_else(|| format!("unknown weapon preset or catalog ID `{id}`"))?;
        generate_icon(&design, spec)?
    };
    fs::write(&output, icon.encode_png()?)?;
    println!(
        "exported {:?} {}x{} icon to {}",
        icon.layout,
        size,
        size,
        output.display()
    );
    Ok(())
}
