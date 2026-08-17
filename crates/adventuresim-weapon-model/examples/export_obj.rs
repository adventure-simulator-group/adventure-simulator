//! Export a canonical weapon recipe as OBJ for manual geometry review.

use std::{env, fmt::Write as _, fs, path::PathBuf};

use adventuresim_weapon_model::{
    MaterialClass, default_design, default_holder_design, generate, generate_holder, preset_design,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let id = args
        .next()
        .ok_or("usage: export_obj <preset-or-catalog-id> <output.obj> [--holder]")?;
    let output = PathBuf::from(
        args.next()
            .ok_or("usage: export_obj <preset-or-catalog-id> <output.obj> [--holder]")?,
    );
    let holder = match args.next().as_deref() {
        None => false,
        Some("--holder") => true,
        _ => return Err("usage: export_obj <preset-or-catalog-id> <output.obj> [--holder]".into()),
    };
    if args.next().is_some() {
        return Err("usage: export_obj <preset-or-catalog-id> <output.obj> [--holder]".into());
    }
    let design = preset_design(&id)
        .or_else(|| default_design(&id))
        .ok_or_else(|| format!("unknown weapon preset or catalog ID `{id}`"))?;
    let parts = if holder {
        let holder = default_holder_design(&design)
            .ok_or("this weapon is hand-only and has no body-mounted holder")?;
        generate_holder(&holder)?.parts
    } else {
        generate(&design)?.parts
    };
    let material_path = output.with_extension("mtl");
    let material_name = material_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("output material path is not valid UTF-8")?;

    let mut obj = format!("mtllib {material_name}\no {id}\n");
    let mut vertex_base = 1_u32;
    for part in &parts {
        writeln!(obj, "g {}", part.component_id)?;
        writeln!(obj, "usemtl {}", material_label(part.material))?;
        for position in &part.positions {
            writeln!(obj, "v {} {} {}", position[0], position[1], position[2])?;
        }
        for normal in &part.normals {
            writeln!(obj, "vn {} {} {}", normal[0], normal[1], normal[2])?;
        }
        for triangle in part.indices.as_chunks::<3>().0 {
            let a = vertex_base + triangle[0];
            let b = vertex_base + triangle[1];
            let c = vertex_base + triangle[2];
            writeln!(obj, "f {a}//{a} {b}//{b} {c}//{c}")?;
        }
        vertex_base += u32::try_from(part.positions.len())?;
    }
    fs::write(&output, obj)?;
    fs::write(material_path, material_library())?;
    println!("exported {} parts to {}", parts.len(), output.display());
    Ok(())
}

fn material_label(material: MaterialClass) -> &'static str {
    match material {
        MaterialClass::Wood => "wood",
        MaterialClass::Leather => "leather",
        MaterialClass::DarkLeather => "dark_leather",
        MaterialClass::Brass => "brass",
        MaterialClass::Steel => "steel",
        MaterialClass::DarkSteel => "dark_steel",
    }
}

fn material_library() -> &'static str {
    "newmtl wood\nKd 0.30 0.18 0.09\n\
newmtl leather\nKd 0.16 0.09 0.05\n\
newmtl dark_leather\nKd 0.055 0.045 0.038\n\
newmtl brass\nKd 0.58 0.42 0.13\n\
newmtl steel\nKd 0.58 0.61 0.64\n\
newmtl dark_steel\nKd 0.22 0.24 0.26\n"
}
