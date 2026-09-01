use std::{fs, path::PathBuf};

use adventuresim_procedural_textures::{
    PROCEDURAL_TEXTURE_CATALOGUE, ProceduralTextureAssets, SurfaceTextureSet, TextureRecipeId,
    TextureRecipeStatus, generate_procedural_textures,
};
use bevy::{asset::Assets, image::Image, prelude::Handle, render::render_resource::TextureFormat};
use clap::{Parser, Subcommand};
use image::{ColorType, ImageFormat};

#[derive(Parser)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List stable recipe names and their implementation status.
    List,
    /// Export the current outputs for one implemented recipe as PNG files.
    Export {
        recipe: String,
        #[arg(long, default_value = "target/procedural-texture-lab")]
        output: PathBuf,
    },
}

fn main() -> Result<(), String> {
    match Arguments::parse().command {
        Command::List => {
            for recipe in PROCEDURAL_TEXTURE_CATALOGUE {
                println!(
                    "{}\t{:?}\t{:?}",
                    recipe.id.slug(),
                    recipe.family,
                    recipe.status
                );
            }
            Ok(())
        }
        Command::Export { recipe, output } => export(&recipe, &output),
    }
}

fn export(slug: &str, output: &PathBuf) -> Result<(), String> {
    let descriptor = PROCEDURAL_TEXTURE_CATALOGUE
        .iter()
        .find(|recipe| recipe.id.slug() == slug)
        .ok_or_else(|| format!("unknown recipe {slug:?}; run `procedural-texture-lab list`"))?;
    if descriptor.status == TextureRecipeStatus::Planned {
        return Err(format!(
            "recipe {slug:?} is planned and deliberately has no placeholder generator"
        ));
    }
    fs::create_dir_all(output).map_err(|error| error.to_string())?;
    let mut images = Assets::<Image>::default();
    let textures = generate_procedural_textures(&mut images);
    for (channel, handle) in outputs(descriptor.id, &textures)? {
        save_png(
            &images,
            &handle,
            output.join(format!("{slug}-{channel}.png")),
        )?;
    }
    Ok(())
}

fn outputs(
    recipe: TextureRecipeId,
    textures: &ProceduralTextureAssets,
) -> Result<Vec<(&'static str, Handle<Image>)>, String> {
    let leaf = match recipe {
        TextureRecipeId::WhiteOakLeaf => Some(&textures.oak_leaf),
        TextureRecipeId::DryWhiteOakLeaf => Some(&textures.dry_oak_leaf),
        TextureRecipeId::HazelLeaf => Some(&textures.hazel_leaf),
        TextureRecipeId::BlackthornLeaf => Some(&textures.blackthorn_leaf),
        TextureRecipeId::HawthornLeaf => Some(&textures.hawthorn_leaf),
        TextureRecipeId::BeechLeaf => Some(&textures.beech_leaf),
        _ => None,
    };
    if let Some(leaf) = leaf {
        return Ok(vec![
            ("opacity", leaf.opacity.clone()),
            ("front-albedo", leaf.front_albedo.clone()),
            ("back-albedo", leaf.back_albedo.clone()),
            ("front-normal", leaf.front_normal.clone()),
            ("back-normal", leaf.back_normal.clone()),
            ("height", leaf.height.clone()),
            ("arm", leaf.arm.clone()),
        ]);
    }
    let surface = match recipe {
        TextureRecipeId::Rock => Some(&textures.rock),
        TextureRecipeId::LimePlaster => Some(&textures.lime_plaster),
        TextureRecipeId::HewnOak => Some(&textures.hewn_oak),
        TextureRecipeId::WattleAndDaub => Some(&textures.wattle_and_daub),
        TextureRecipeId::HandmadeBrick => Some(&textures.handmade_brick),
        TextureRecipeId::RubbleMasonry => Some(&textures.rubble_masonry),
        TextureRecipeId::DressedStone => Some(&textures.dressed_stone),
        TextureRecipeId::ClayRoofTile => Some(&textures.clay_roof_tile),
        TextureRecipeId::SlateRoof => Some(&textures.slate_roof),
        TextureRecipeId::TimberShingle => Some(&textures.timber_shingle),
        TextureRecipeId::PlankFloor => Some(&textures.plank_floor),
        TextureRecipeId::Ironwork => Some(&textures.ironwork),
        TextureRecipeId::LeadSheet => Some(&textures.lead_sheet),
        _ => None,
    };
    if let Some(surface) = surface {
        return Ok(surface_outputs(surface));
    }
    match recipe {
        TextureRecipeId::OakBark => Ok(vec![("height-ao", textures.oak_bark.height_ao.clone())]),
        TextureRecipeId::ForestSoil => {
            Ok(vec![("height-ao", textures.forest_soil.height_ao.clone())])
        }
        TextureRecipeId::ForestLitter => Ok(vec![
            ("surface", textures.forest_soil.litter_surface.clone()),
            ("normal", textures.forest_soil.litter_normal.clone()),
        ]),
        TextureRecipeId::WindowGlass => Ok(vec![
            ("transmittance", textures.window_glass.transmittance.clone()),
            (
                "optical-normal",
                textures.window_glass.optical_normal_gl.clone(),
            ),
            (
                "thickness-roughness",
                textures.window_glass.thickness_roughness.clone(),
            ),
        ]),
        TextureRecipeId::CrenellationMask => {
            Ok(vec![("opacity", textures.crenellation_mask.clone())])
        }
        _ => Err(format!(
            "recipe {:?} is a baseline exposed to the runtime but not yet wired to lab export",
            recipe
        )),
    }
}

fn surface_outputs(surface: &SurfaceTextureSet) -> Vec<(&'static str, Handle<Image>)> {
    vec![
        ("albedo", surface.albedo.clone()),
        ("normal", surface.normal_gl.clone()),
        ("height", surface.height.clone()),
        ("arm", surface.arm.clone()),
    ]
}

fn save_png(images: &Assets<Image>, handle: &Handle<Image>, path: PathBuf) -> Result<(), String> {
    let image = images
        .get(handle)
        .ok_or_else(|| format!("missing generated image for {}", path.display()))?;
    let data = image
        .data
        .as_deref()
        .ok_or_else(|| format!("generated image {} has no CPU data", path.display()))?;
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    let (color, bytes_per_pixel) = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => (ColorType::Rgba8, 4),
        TextureFormat::Rg8Unorm => (ColorType::La8, 2),
        format => return Err(format!("unsupported export format {format:?}")),
    };
    let base_mip_length = width as usize * height as usize * bytes_per_pixel;
    image::save_buffer_with_format(
        &path,
        &data[..base_mip_length],
        width,
        height,
        color,
        ImageFormat::Png,
    )
    .map_err(|error| error.to_string())
}
