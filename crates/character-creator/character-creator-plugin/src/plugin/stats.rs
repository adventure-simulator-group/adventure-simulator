use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

pub struct RenderingStatsPlugin;

impl Plugin for RenderingStatsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_systems(EguiPrimaryContextPass, rendering_stats_ui);
    }
}

pub fn rendering_stats_ui(
    mut contexts: EguiContexts,
    diagnostics: Res<DiagnosticsStore>,
    meshes: Res<Assets<Mesh>>,
    mesh_query: Query<(&Mesh3d, &InheritedVisibility)>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diag| diag.smoothed())
        .unwrap_or(0.0);

    let mut total_vertices = 0;
    let mut total_triangles = 0;
    let mut mesh_count = 0;

    for (mesh_handle, visibility) in mesh_query.iter() {
        if visibility.get() {
            if let Some(mesh) = meshes.get(&mesh_handle.0) {
                mesh_count += 1;
                let vertex_count = mesh.count_vertices();
                total_vertices += vertex_count;

                if let Some(indices) = mesh.indices() {
                    total_triangles += indices.len() / 3;
                } else {
                    total_triangles += vertex_count / 3;
                }
            }
        }
    }

    if let Ok(ctx) = contexts.ctx_mut() {
        egui::Area::new(egui::Id::new("rendering_stats"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_black_alpha(150))
                    .show(ui, |ui| {
                        ui.heading("Rendering Stats");
                        ui.label(format!("FPS: {:.1}", fps));
                        ui.separator();
                        ui.label(format!("Meshes: {}", mesh_count));
                        ui.label(format!("Vertices: {}", total_vertices));
                        ui.label(format!("Triangles: {}", total_triangles));
                    });
            });
    }
}
