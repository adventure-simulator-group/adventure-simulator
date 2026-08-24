use std::path::PathBuf;

use adventuresim_character_creator::{
    CharacterRecipe, ClothingSelection, IdentityGroup,
    clothing::{GarmentSpecification, generate_clothing_shells},
    export::{RiggedMesh, RiggedShell, export_rigged_glb},
    item_catalog_schema::{ItemCatalogDocument, ItemDefinition},
};
use anyhow::{Context, Result};
use bevy::{
    asset::RenderAssetUsages,
    input::mouse::{MouseMotion, MouseWheel},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use burn::tensor::{Device, Tensor, TensorData};
use clap::Parser;
use fabelgeist_mhr::{Mhr, MhrConfig, NUM_FACE_EXPRESSION_BLEND_SHAPES};
use rand::{Rng, SeedableRng, rngs::StdRng};

#[derive(Parser, Resource, Clone)]
#[command(about = "Fabelgeist's MHR character design studio")]
struct Args {
    #[arg(
        long,
        env = "MHR_ASSETS",
        default_value = "target/mhr-assets/v1.0.1/assets"
    )]
    assets: PathBuf,
    // LOD 1 retains enough facial, ear, and finger topology for close creator
    // views while remaining inexpensive with pose correctives disabled.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=6))]
    lod: u8,
    #[arg(long, default_value = "assets_src/characters/john_fabelgeist.json")]
    recipe: PathBuf,
    #[arg(long, default_value = "assets_src/biped/unarmed/base.glb")]
    glb: PathBuf,
    #[arg(long, default_value = "content/items")]
    catalog: PathBuf,
    #[arg(long, default_value = "assets/equipment/procedural")]
    equipment_output: PathBuf,
    /// Export the selected recipe without opening the studio window.
    #[arg(long)]
    export_only: bool,
    /// Generate one procedural MHR asset for every armor/clothing placement.
    #[arg(long)]
    generate_equipment: bool,
}

#[derive(Resource)]
struct BodyModel {
    mhr: Mhr,
    lod: u8,
    correctives: bool,
}

#[derive(Resource)]
struct EquipmentCatalog(Vec<ItemDefinition>);

#[derive(Resource)]
struct Studio {
    recipe: CharacterRecipe,
    selected: IdentityGroup,
    show_expressions: bool,
    dirty: bool,
    status: String,
    recipe_path: String,
    glb_path: String,
    seed: u64,
    selected_lod: u8,
    selected_correctives: bool,
}

#[derive(Component)]
struct CharacterMesh;

struct GeneratedCharacter {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    global_joint_states: Vec<[f32; 8]>,
}

#[derive(Component)]
struct OrbitCamera {
    yaw: f32,
    pitch: f32,
    radius: f32,
    focus: Vec3,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let device = Device::default();
    let model = load_body_model(&args.assets, args.lod, false, &device)
        .with_context(|| format!("loading MHR assets from {}", args.assets.display()))?;
    let catalog = EquipmentCatalog(load_item_catalog(&args.catalog)?);

    let recipe = if args.recipe.is_file() {
        let parsed: CharacterRecipe = serde_json::from_slice(&std::fs::read(&args.recipe)?)?;
        parsed.validate().map_err(anyhow::Error::msg)?;
        parsed
    } else {
        CharacterRecipe {
            name: "John Fabelgeist".into(),
            ..CharacterRecipe::default()
        }
    };

    if args.generate_equipment {
        generate_equipment_assets(&args.equipment_output, &model, &recipe, &catalog)?;
        println!(
            "Generated equipment under {}",
            args.equipment_output.display()
        );
        return Ok(());
    }
    if args.export_only {
        export_character(&args.glb, &model, &recipe, &catalog)?;
        println!("Exported {}", args.glb.display());
        return Ok(());
    }

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.035, 0.045, 0.055)))
        .insert_resource(args.clone())
        .insert_resource(model)
        .insert_resource(catalog)
        .insert_resource(Studio {
            recipe,
            selected: IdentityGroup::Body,
            show_expressions: false,
            dirty: true,
            status: format!("MHR LOD {} ready", args.lod),
            recipe_path: args.recipe.display().to_string(),
            glb_path: args.glb.display().to_string(),
            seed: 1544,
            selected_lod: args.lod,
            selected_correctives: false,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Fabelgeist · Character Studio".into(),
                resolution: (1440, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, studio_ui)
        .add_systems(
            Update,
            (
                reload_model.before(regenerate_mesh),
                regenerate_mesh,
                orbit_camera,
            ),
        )
        .run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        // A restrained ambient term stands in for indirect room bounce. It
        // prevents fully black occlusion without flattening the spotlight's
        // form and floor shadow.
        AmbientLight {
            color: Color::srgb(0.78, 0.84, 0.94),
            brightness: 155.0,
            ..default()
        },
        Transform::default(),
        OrbitCamera {
            yaw: 0.1,
            pitch: -0.05,
            radius: 2.7,
            focus: Vec3::new(0.0, 1.0, 0.0),
        },
    ));

    let light_position = Vec3::new(-2.4, 4.2, 3.0);
    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.90, 0.79),
            intensity: 1_050_000.0,
            range: 8.0,
            radius: 0.38,
            inner_angle: 0.30,
            outer_angle: 0.58,
            shadow_maps_enabled: true,
            shadow_depth_bias: 0.025,
            shadow_normal_bias: 1.9,
            shadow_map_near_z: 0.1,
            ..default()
        },
        Transform::from_translation(light_position).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.17, 0.18),
            perceptual_roughness: 0.86,
            reflectance: 0.12,
            ..default()
        })),
    ));
}

#[allow(deprecated)] // egui 0.34's replacement requires a parent Ui; this is a top-level panel.
fn studio_ui(
    mut contexts: EguiContexts,
    model: Res<BodyModel>,
    catalog: Res<EquipmentCatalog>,
    mut studio: ResMut<Studio>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::SidePanel::left("creator")
        .exact_width(360.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label("Name");
            ui.text_edit_singleline(&mut studio.recipe.name);
            let lod_changed = ui
                .add(
                    egui::Slider::new(&mut studio.selected_lod, 0..=6)
                        .text("Mesh LOD")
                        .custom_formatter(|value, _| {
                            let lod = value.round() as usize;
                            let vertices = [73_639, 18_439, 10_661, 4_899, 2_461, 971, 595][lod];
                            format!("{lod} · {vertices} vertices")
                        }),
                )
                .changed();
            if lod_changed {
                studio.status = format!("Loading MHR LOD {}…", studio.selected_lod);
            }
            ui.small("LOD 0 is highest fidelity; LOD 6 is lowest.");
            if ui
                .checkbox(&mut studio.selected_correctives, "Pose-corrective model")
                .changed()
            {
                studio.status = format!(
                    "Loading MHR LOD {} with correctives {}…",
                    studio.selected_lod,
                    if studio.selected_correctives {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
            }
            ui.small(
                "Correctives improve posed deformation but require substantially more memory.",
            );
            ui.separator();

            ui.collapsing("Catalog clothing and armor", |ui| {
                for item in procedural_items(&catalog) {
                    let equipment = item.equipment.as_ref().expect("filtered equipment");
                    for placement in &equipment.placements {
                        let selection = ClothingSelection {
                            item_id: item.id.clone(),
                            placement_id: placement.id.clone(),
                        };
                        let mut enabled = studio.recipe.clothing.contains(&selection);
                        let label = if equipment.placements.len() == 1 {
                            item.display_name.clone()
                        } else {
                            format!("{} · {}", item.display_name, placement.id)
                        };
                        if ui.checkbox(&mut enabled, label).changed() {
                            if enabled {
                                studio.recipe.clothing.push(selection.clone());
                            } else {
                                studio
                                    .recipe
                                    .clothing
                                    .retain(|selected| selected != &selection);
                            }
                            studio.dirty = true;
                        }
                    }
                }
            });
            ui.small("Bone-weight shells follow the generated body and share its MHR skin.");
            ui.separator();

            ui.horizontal(|ui| {
                for group in IdentityGroup::ALL {
                    ui.selectable_value(&mut studio.selected, group, group.label());
                }
            });
            let selected = studio.selected;
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .show(ui, |ui| {
                    for index in selected.range() {
                        let response = ui.add(
                            egui::Slider::new(&mut studio.recipe.identity[index], -3.0..=3.0)
                                .text(format!(
                                    "{} {:02}",
                                    selected.label(),
                                    index - selected.range().start + 1
                                ))
                                .fixed_decimals(2),
                        );
                        studio.dirty |= response.changed();
                    }
                });

            ui.collapsing("Expression laboratory", |ui| {
                ui.checkbox(
                    &mut studio.show_expressions,
                    "Show all 72 expression channels",
                );
                if studio.show_expressions {
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            let mut changed = false;
                            for (index, value) in studio.recipe.expression.iter_mut().enumerate() {
                                changed |= ui
                                    .add(
                                        egui::Slider::new(value, -1.0..=1.0)
                                            .text(format!("Expression {:02}", index + 1)),
                                    )
                                    .changed();
                            }
                            studio.dirty |= changed;
                        });
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Randomize body").clicked() {
                    studio.seed = studio.seed.wrapping_add(1);
                    let mut rng = StdRng::seed_from_u64(studio.seed);
                    for value in &mut studio.recipe.identity {
                        *value = rng.random_range(-1.35..=1.35);
                    }
                    studio.dirty = true;
                }
                if ui.button("Neutral").clicked() {
                    studio.recipe.reset_body();
                    studio.recipe.reset_face();
                    studio.dirty = true;
                }
            });
            ui.add(egui::TextEdit::singleline(&mut studio.recipe_path).hint_text("character.json"));
            ui.horizontal(|ui| {
                if ui.button("Save recipe").clicked() {
                    studio.status = save_recipe(&studio)
                        .unwrap_or_else(|error| format!("Save failed: {error:#}"));
                }
                if ui.button("Load recipe").clicked() {
                    match load_recipe(&studio.recipe_path) {
                        Ok(recipe) => {
                            studio.recipe = recipe;
                            studio.dirty = true;
                            studio.status = "Recipe loaded".into();
                        }
                        Err(error) => studio.status = format!("Load failed: {error:#}"),
                    }
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut studio.glb_path)
                    .hint_text("assets_src/biped/unarmed/base.glb"),
            );
            if ui.button("Export rigged GLB").clicked() {
                studio.status = export_character(
                    std::path::Path::new(&studio.glb_path),
                    &model,
                    &studio.recipe,
                    &catalog,
                )
                .map(|()| format!("Exported {}", studio.glb_path))
                .unwrap_or_else(|error| format!("Export failed: {error:#}"));
            }
            ui.add_space(6.0);
            ui.small(&studio.status);
            ui.small("Drag to orbit · wheel to zoom");
        });
}

fn save_recipe(studio: &Studio) -> Result<String> {
    studio.recipe.validate().map_err(anyhow::Error::msg)?;
    let path = std::path::Path::new(&studio.recipe_path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&studio.recipe)?)?;
    Ok(format!("Saved {}", studio.recipe_path))
}

fn load_recipe(path: &str) -> Result<CharacterRecipe> {
    let recipe: CharacterRecipe = serde_json::from_slice(&std::fs::read(path)?)?;
    recipe.validate().map_err(anyhow::Error::msg)?;
    Ok(recipe)
}

fn load_item_catalog(directory: &std::path::Path) -> Result<Vec<ItemDefinition>> {
    let mut files = std::fs::read_dir(directory)
        .with_context(|| format!("reading item catalog directory {}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "yaml")
    });
    files.sort();
    let mut items = Vec::new();
    for path in files {
        let document: ItemCatalogDocument = serde_json::from_slice(&std::fs::read(&path)?)
            .with_context(|| format!("parsing item catalog {}", path.display()))?;
        items.extend(document.items);
    }
    if items.is_empty() {
        anyhow::bail!("item catalog contains no definitions");
    }
    Ok(items)
}

fn procedural_items(catalog: &EquipmentCatalog) -> impl Iterator<Item = &ItemDefinition> {
    catalog.0.iter().filter(|item| {
        item.equipment.as_ref().is_some_and(|equipment| {
            equipment.material.is_some()
                && equipment
                    .placements
                    .iter()
                    .any(|placement| !placement.surface.is_empty())
        })
    })
}

fn selected_garments(
    recipe: &CharacterRecipe,
    catalog: &EquipmentCatalog,
) -> Result<Vec<GarmentSpecification>, String> {
    recipe
        .clothing
        .iter()
        .map(|selection| {
            let item = procedural_items(catalog)
                .find(|item| item.id == selection.item_id)
                .ok_or_else(|| format!("unknown procedural item {}", selection.item_id))?;
            let placement = item
                .equipment
                .as_ref()
                .and_then(|equipment| {
                    equipment
                        .placements
                        .iter()
                        .find(|placement| placement.id == selection.placement_id)
                })
                .ok_or_else(|| {
                    format!(
                        "item {} has no placement {}",
                        selection.item_id, selection.placement_id
                    )
                })?;
            Ok(GarmentSpecification::from_catalog(
                format!("{} · {}", item.display_name, placement.id),
                placement,
                item.equipment
                    .as_ref()
                    .and_then(|equipment| equipment.material)
                    .ok_or_else(|| format!("item {} has no procedural material", item.id))?,
            ))
        })
        .collect()
}

fn placement_coverage(
    placement: &adventuresim_character_creator::item_catalog_schema::EquipmentPlacement,
) -> f32 {
    let region_count = placement
        .surface
        .iter()
        .map(|span| span.regions.len())
        .sum::<usize>();
    placement
        .surface
        .iter()
        .map(|span| span.coverage * span.regions.len() as f32)
        .sum::<f32>()
        / region_count as f32
}

fn generate_equipment_assets(
    output: &std::path::Path,
    model: &BodyModel,
    recipe: &CharacterRecipe,
    catalog: &EquipmentCatalog,
) -> Result<()> {
    std::fs::create_dir_all(output)
        .with_context(|| format!("creating equipment output {}", output.display()))?;
    let generated = generate_character(model, recipe)?;
    let character = &model.mhr.character;
    let mut assets = Vec::new();
    let mut generated_files = std::collections::BTreeSet::new();
    for item in procedural_items(catalog) {
        let equipment = item.equipment.as_ref().expect("filtered equipment");
        for placement in &equipment.placements {
            if placement.surface.is_empty() {
                continue;
            }
            let specification = GarmentSpecification::from_catalog(
                format!("{} · {}", item.display_name, placement.id),
                placement,
                equipment.material.ok_or_else(|| {
                    anyhow::anyhow!("item {} has no procedural material", item.id)
                })?,
            );
            let clothed = generate_clothing_shells(
                &[specification],
                &generated.positions,
                &generated.normals,
                &character.mesh.faces,
                &character.skin_weights.index,
                &character.skin_weights.weight,
                &character.skeleton.names,
                &generated.global_joint_states,
            )
            .map_err(anyhow::Error::msg)?;
            let shell = &clothed.shells[0];
            let file_name = format!("{}--{}.glb", item.id, placement.id);
            let path = output.join(&file_name);
            let rigged_shell = RiggedShell {
                name: &shell.specification.name,
                positions: &shell.positions,
                normals: &shell.normals,
                faces: &shell.faces,
                base_color: shell.specification.base_color,
                metallic: shell.specification.metallic,
                roughness: shell.specification.roughness,
            };
            export_rigged_glb(
                &path,
                &item.id,
                recipe.version,
                model.lod,
                &RiggedMesh {
                    positions: &generated.positions,
                    normals: &generated.normals,
                    faces: &character.mesh.faces,
                    export_body: false,
                    joint_indices: &character.skin_weights.index,
                    joint_weights: &character.skin_weights.weight,
                    joint_names: &character.skeleton.names,
                    joint_parents: &character.skeleton.parents,
                    global_joint_states: &generated.global_joint_states,
                },
                &[rigged_shell],
            )?;
            generated_files.insert(file_name.clone());
            assets.push(serde_json::json!({
                "item_id": item.id,
                "placement_id": placement.id,
                "file": file_name,
                "coverage": placement_coverage(placement),
                "material": equipment.material,
                "triangles": shell.faces.len(),
            }));
        }
    }
    for entry in std::fs::read_dir(output)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "glb")
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !generated_files.contains(name))
        {
            std::fs::remove_file(&path)
                .with_context(|| format!("removing stale generated asset {}", path.display()))?;
        }
    }
    let manifest = serde_json::json!({
        "schema_version": 1,
        "mhr_release": "v1.0.1",
        "lod": model.lod,
        "assets": assets,
    });
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

fn load_body_model(
    assets: &std::path::Path,
    lod: u8,
    correctives: bool,
    device: &Device,
) -> Result<BodyModel> {
    let mhr = Mhr::from_files(
        assets,
        MhrConfig {
            lod,
            pose_correctives: correctives,
        },
        device,
    )?;
    Ok(BodyModel {
        mhr,
        lod,
        correctives,
    })
}

fn reload_model(args: Res<Args>, mut model: ResMut<BodyModel>, mut studio: ResMut<Studio>) {
    if studio.selected_lod == model.lod && studio.selected_correctives == model.correctives {
        return;
    }
    let requested = studio.selected_lod;
    let requested_correctives = studio.selected_correctives;
    let device = Device::default();
    match load_body_model(&args.assets, requested, requested_correctives, &device) {
        Ok(loaded) => {
            *model = loaded;
            studio.dirty = true;
            studio.status = format!(
                "MHR LOD {requested} ready · correctives {}",
                if requested_correctives { "on" } else { "off" }
            );
        }
        Err(error) => {
            studio.selected_lod = model.lod;
            studio.selected_correctives = model.correctives;
            studio.status = format!(
                "Could not load LOD {requested} with correctives {}: {error:#}",
                if requested_correctives { "on" } else { "off" }
            );
        }
    }
}

fn regenerate_mesh(
    mut commands: Commands,
    model: Res<BodyModel>,
    catalog: Res<EquipmentCatalog>,
    mut studio: ResMut<Studio>,
    old: Query<Entity, With<CharacterMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !studio.dirty {
        return;
    }
    studio.dirty = false;
    let generated = match generate_character(&model, &studio.recipe) {
        Ok(generated) => generated,
        Err(error) => {
            studio.status = format!("Generation failed: {error:#}");
            return;
        }
    };
    let GeneratedCharacter {
        positions,
        normals,
        global_joint_states,
    } = generated;
    let faces = &model.mhr.character.mesh.faces;
    let specifications = match selected_garments(&studio.recipe, &catalog) {
        Ok(specifications) => specifications,
        Err(error) => {
            studio.status = format!("Clothing selection failed: {error}");
            return;
        }
    };
    let clothed = match generate_clothing_shells(
        &specifications,
        &positions,
        &normals,
        faces,
        &model.mhr.character.skin_weights.index,
        &model.mhr.character.skin_weights.weight,
        &model.mhr.character.skeleton.names,
        &global_joint_states,
    ) {
        Ok(clothed) => clothed,
        Err(error) => {
            studio.status = format!("Clothing generation failed: {error}");
            return;
        }
    };
    let indices = clothed
        .visible_body_faces
        .iter()
        .flat_map(|face| face.iter().copied())
        .collect::<Vec<_>>();
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone())
    .with_inserted_indices(Indices::U32(indices));
    for entity in &old {
        commands.entity(entity).despawn();
    }
    commands.spawn((
        CharacterMesh,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            // Skin is a rough dielectric with a small amount of diffuse
            // transmission. This keeps thin features such as the nose, ears,
            // and fingers warm instead of crushing them to black.
            base_color: Color::srgb(0.64, 0.39, 0.30),
            metallic: 0.0,
            perceptual_roughness: 0.52,
            reflectance: 0.46,
            specular_tint: Color::srgb(1.0, 0.93, 0.89),
            // A small back-diffuse lobe is Bevy's inexpensive approximation
            // of the short scattering distance seen in skin. Kept subtle so
            // the body remains opaque and shadowed rather than wax-like.
            diffuse_transmission: 0.045,
            ..default()
        })),
    ));
    for shell in clothed.shells {
        let specification = shell.specification;
        let indices = shell
            .faces
            .iter()
            .flat_map(|face| face.iter().copied())
            .collect::<Vec<_>>();
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, shell.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, shell.normals)
        .with_inserted_indices(Indices::U32(indices));
        let [red, green, blue, alpha] = specification.base_color;
        commands.spawn((
            CharacterMesh,
            Name::new(specification.name.clone()),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(red, green, blue, alpha),
                metallic: specification.metallic,
                perceptual_roughness: specification.roughness,
                ..default()
            })),
        ));
    }
    studio.status = format!(
        "Generated {} vertices · {} clothing shells",
        model.mhr.num_vertices(),
        studio.recipe.clothing.len()
    );
}

fn generate_character(model: &BodyModel, recipe: &CharacterRecipe) -> Result<GeneratedCharacter> {
    recipe.validate().map_err(anyhow::Error::msg)?;
    let device = Device::default();
    let identity = Tensor::from_data(TensorData::new(recipe.identity.clone(), [1, 45]), &device);
    let expression = Tensor::from_data(
        TensorData::new(
            recipe.expression.clone(),
            [1, NUM_FACE_EXPRESSION_BLEND_SHAPES],
        ),
        &device,
    );
    let pose = model.mhr.zero_parameters(1);
    let output = model.mhr.forward(identity, pose, Some(expression))?;
    let vertex_values = output
        .vertices
        .into_data()
        .into_vec::<f32>()
        .map_err(|error| anyhow::anyhow!("GPU vertex readback failed: {error:?}"))?;
    let positions: Vec<[f32; 3]> = vertex_values
        .chunks_exact(3)
        .map(|v| [v[0] / 100.0, v[1] / 100.0, v[2] / 100.0])
        .collect();

    let normal_values = output
        .normals
        .into_data()
        .into_vec::<f32>()
        .map_err(|error| anyhow::anyhow!("GPU normal readback failed: {error:?}"))?;
    let normals: Vec<[f32; 3]> = normal_values
        .chunks_exact(3)
        .map(|n| [n[0], n[1], n[2]])
        .collect();

    let skeleton_values = output
        .skeleton_state
        .into_data()
        .into_vec::<f32>()
        .map_err(|error| anyhow::anyhow!("GPU skeleton readback failed: {error:?}"))?;
    let global_joint_states = skeleton_values
        .chunks_exact(8)
        .map(|joint| {
            let mut state: [f32; 8] = joint.try_into().expect("chunks_exact yields eight values");
            state[0] /= 100.0;
            state[1] /= 100.0;
            state[2] /= 100.0;
            state
        })
        .collect();
    Ok(GeneratedCharacter {
        positions,
        normals,
        global_joint_states,
    })
}

fn export_character(
    path: &std::path::Path,
    model: &BodyModel,
    recipe: &CharacterRecipe,
    catalog: &EquipmentCatalog,
) -> Result<()> {
    let generated = generate_character(model, recipe)?;
    let character = &model.mhr.character;
    let specifications = selected_garments(recipe, catalog).map_err(anyhow::Error::msg)?;
    let clothed = generate_clothing_shells(
        &specifications,
        &generated.positions,
        &generated.normals,
        &character.mesh.faces,
        &character.skin_weights.index,
        &character.skin_weights.weight,
        &character.skeleton.names,
        &generated.global_joint_states,
    )
    .map_err(anyhow::Error::msg)?;
    let shells = clothed
        .shells
        .iter()
        .map(|shell| {
            let specification = &shell.specification;
            RiggedShell {
                name: &specification.name,
                positions: &shell.positions,
                normals: &shell.normals,
                faces: &shell.faces,
                base_color: specification.base_color,
                metallic: specification.metallic,
                roughness: specification.roughness,
            }
        })
        .collect::<Vec<_>>();
    export_rigged_glb(
        path,
        &recipe.name,
        recipe.version,
        model.lod,
        &RiggedMesh {
            positions: &generated.positions,
            normals: &generated.normals,
            faces: &clothed.visible_body_faces,
            export_body: true,
            joint_indices: &character.skin_weights.index,
            joint_weights: &character.skin_weights.weight,
            joint_names: &character.skeleton.names,
            joint_parents: &character.skeleton.parents,
            global_joint_states: &generated.global_joint_states,
        },
        &shells,
    )
}

fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut camera: Query<(&mut Transform, &mut OrbitCamera)>,
) {
    let Ok((mut transform, mut orbit)) = camera.single_mut() else {
        return;
    };
    let pointer_owned_by_ui = contexts
        .ctx_mut()
        .is_ok_and(|context| context.egui_wants_pointer_input());
    if buttons.pressed(MouseButton::Left) && !pointer_owned_by_ui {
        for event in motion.read() {
            orbit.yaw -= event.delta.x * 0.007;
            orbit.pitch = (orbit.pitch - event.delta.y * 0.007).clamp(-1.2, 1.2);
        }
    } else {
        motion.clear();
    }
    if pointer_owned_by_ui {
        wheel.clear();
    } else {
        for event in wheel.read() {
            orbit.radius = (orbit.radius * (-event.y * 0.1).exp()).clamp(1.2, 6.0);
        }
    }
    let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, orbit.pitch, 0.0);
    transform.translation = orbit.focus + rotation * Vec3::new(0.0, 0.0, orbit.radius);
    transform.look_at(orbit.focus, Vec3::Y);
}
