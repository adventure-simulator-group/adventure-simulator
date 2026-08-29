use std::path::Path;

use serde::Deserialize;

const MAX_GRAPHICS_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalGraphicsConfig {
    pub rendering: RenderingConfig,
    pub grass: GrassConfig,
    pub desktop: DesktopConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderingConfig {
    pub anti_aliasing: AntiAliasingConfig,
    pub shadows: ShadowConfig,
    pub bloom: ToggleConfig,
    pub atmosphere: AtmosphereConfig,
    pub clouds: CloudConfig,
    pub vista: VistaConfig,
    pub tonemapping: TonemappingConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AntiAliasingConfig {
    Off,
    Msaa { samples: u8 },
    Fxaa,
    Smaa { quality: SmaaQuality },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SmaaQuality {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowFiltering {
    Hardware2x2,
    Gaussian,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub map_size: usize,
    pub filtering: ShadowFiltering,
    pub cascades: usize,
    pub maximum_distance_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToggleConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereConfig {
    pub enabled: bool,
    pub celestial: bool,
    pub environment_light: bool,
    pub environment_map_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudConfig {
    pub enabled: bool,
    pub quality_scale: f32,
    pub resolution_scale: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VistaConfig {
    pub maximum_lods: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TonemappingConfig {
    None,
    AcesFitted,
    AgX,
    TonyMcMapface,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassConfig {
    pub enabled: bool,
    pub density_scale: f32,
    pub placement: GrassPlacementConfig,
    pub blade: GrassBladeConfig,
    pub lod: GrassLodConfig,
    pub transition: GrassTransitionConfig,
    pub lighting: GrassLightingConfig,
    pub interaction: GrassInteractionConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassPlacementConfig {
    pub playable_patch_spacing_m: f32,
    pub vista_patch_spacing_m: f32,
    pub jitter_fraction: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassBladeConfig {
    pub width_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassLodConfig {
    pub near: GrassTierConfig,
    pub near_edge: GrassTierConfig,
    pub far: GrassTierConfig,
    pub vista: GrassTierConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassTierConfig {
    pub fade_in_m: [f32; 2],
    pub fade_out_m: [f32; 2],
    pub ribbon_rows: Vec<f32>,
    pub native_tufts_per_cell_side: usize,
    pub native_blades_per_tuft_side: usize,
    pub root_stratum_side: Option<usize>,
    pub width_compensation: Option<f32>,
    pub width_compensation_limit: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassTransitionConfig {
    pub terrain_gap_fill_fraction: f32,
    pub cover_mask_feather_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassLightingConfig {
    pub reduced_lighting_scale: f32,
    pub root_occlusion: f32,
    pub ambient_scale: f32,
    pub casts_shadows: GrassShadowTiers,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassShadowTiers {
    pub near: bool,
    pub near_edge: bool,
    pub far: bool,
    pub vista: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassInteractionConfig {
    pub radius_m: f32,
    pub minimum_push: f32,
    pub maximum_push: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopConfig {
    pub window: DesktopWindowConfig,
    pub present_mode: PresentModeConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopWindowConfig {
    pub mode: WindowModeConfig,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub decorations: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowModeConfig {
    Windowed,
    BorderlessFullscreen,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresentModeConfig {
    AutoVsync,
    AutoNoVsync,
    Fifo,
    FifoRelaxed,
    Mailbox,
    Immediate,
}

impl TacticalGraphicsConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = serde_saphyr::from_str(text)
            .map_err(|error| format!("graphics configuration is not valid YAML: {error}"))?;
        config.validate()?;
        Ok(config)
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn load(path: &Path) -> Result<Self, String> {
        let length = std::fs::metadata(path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?
            .len();
        if length == 0 || length > MAX_GRAPHICS_CONFIG_BYTES {
            return Err("graphics config must contain between 1 byte and 64 KiB".into());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    fn validate(&self) -> Result<(), String> {
        if let AntiAliasingConfig::Msaa { samples } = self.rendering.anti_aliasing
            && !matches!(samples, 2 | 4)
        {
            return Err("rendering.anti_aliasing.samples must be 2 or 4".into());
        }
        let shadows = &self.rendering.shadows;
        if !shadows.map_size.is_power_of_two() || !(256..=8192).contains(&shadows.map_size) {
            return Err(
                "rendering.shadows.map_size must be a power of two from 256 through 8192".into(),
            );
        }
        if !(1..=4).contains(&shadows.cascades) || !positive(shadows.maximum_distance_m) {
            return Err(
                "rendering.shadows requires 1..=4 cascades and a positive maximum_distance_m"
                    .into(),
            );
        }
        if !self
            .rendering
            .atmosphere
            .environment_map_size
            .is_power_of_two()
        {
            return Err("rendering.atmosphere.environment_map_size must be a power of two".into());
        }
        bounded(
            "rendering.clouds.quality_scale",
            self.rendering.clouds.quality_scale,
            0.35,
            1.0,
        )?;
        bounded(
            "rendering.clouds.resolution_scale",
            self.rendering.clouds.resolution_scale,
            0.25,
            1.0,
        )?;
        bounded("grass.density_scale", self.grass.density_scale, 0.0, 1.0)?;
        if !positive(self.grass.placement.playable_patch_spacing_m)
            || !positive(self.grass.placement.vista_patch_spacing_m)
        {
            return Err("grass patch spacings must be positive".into());
        }
        bounded(
            "grass.placement.jitter_fraction",
            self.grass.placement.jitter_fraction,
            0.0,
            0.25,
        )?;
        bounded("grass.blade.width_m", self.grass.blade.width_m, 0.001, 0.25)?;
        bounded(
            "grass.transition.terrain_gap_fill_fraction",
            self.grass.transition.terrain_gap_fill_fraction,
            0.0,
            1.0,
        )?;
        if !positive(self.grass.transition.cover_mask_feather_m) {
            return Err("grass.transition.cover_mask_feather_m must be positive".into());
        }
        for (name, tier) in [
            ("near", &self.grass.lod.near),
            ("near_edge", &self.grass.lod.near_edge),
            ("far", &self.grass.lod.far),
            ("vista", &self.grass.lod.vista),
        ] {
            validate_tier(name, tier)?;
        }
        if self.grass.lighting.root_occlusion <= 0.0 || self.grass.lighting.root_occlusion > 1.0 {
            return Err("grass.lighting.root_occlusion must be in (0, 1]".into());
        }
        if !positive(self.grass.lighting.reduced_lighting_scale)
            || !positive(self.grass.lighting.ambient_scale)
        {
            return Err("grass lighting scales must be positive".into());
        }
        let interaction = &self.grass.interaction;
        if !positive(interaction.radius_m)
            || !positive(interaction.minimum_push)
            || interaction.maximum_push < interaction.minimum_push
        {
            return Err("grass interaction requires positive radius/push values and maximum_push >= minimum_push".into());
        }
        if self.desktop.window.width < 320 || self.desktop.window.height < 240 {
            return Err("desktop window dimensions must be at least 320 by 240".into());
        }
        Ok(())
    }
}

fn validate_tier(name: &str, tier: &GrassTierConfig) -> Result<(), String> {
    let ordered = tier.fade_in_m[0] <= tier.fade_in_m[1]
        && tier.fade_in_m[1] <= tier.fade_out_m[0]
        && tier.fade_out_m[0] <= tier.fade_out_m[1];
    if !ordered
        || tier
            .fade_in_m
            .iter()
            .chain(tier.fade_out_m.iter())
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(format!(
            "grass.lod.{name} fade distances must be finite, nonnegative, and ordered"
        ));
    }
    if tier.ribbon_rows.len() < 2
        || tier.ribbon_rows.first() != Some(&0.0)
        || tier
            .ribbon_rows
            .iter()
            .any(|value| !value.is_finite() || !(0.0..1.0).contains(value))
        || tier.ribbon_rows.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(format!(
            "grass.lod.{name}.ribbon_rows must be an increasing list from 0.0 to below 1.0"
        ));
    }
    if tier.native_tufts_per_cell_side == 0 || tier.native_blades_per_tuft_side == 0 {
        return Err(format!(
            "grass.lod.{name} native topology dimensions must be nonzero"
        ));
    }
    if tier
        .root_stratum_side
        .is_some_and(|side| side == 0 || side > 32 || 32 % side != 0)
    {
        return Err(format!(
            "grass.lod.{name}.root_stratum_side must divide the 32-root patch grid"
        ));
    }
    Ok(())
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn bounded(name: &str, value: f32, minimum: f32, maximum: f32) -> Result<(), String> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(format!("{name} must be between {minimum} and {maximum}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_configuration_is_valid() {
        TacticalGraphicsConfig::parse(include_str!(
            "../../../../assets/config/tactical-graphics.yaml"
        ))
        .unwrap();
    }

    #[test]
    fn unknown_fields_fail_closed() {
        let text = include_str!("../../../../assets/config/tactical-graphics.yaml")
            .replace("density_scale: 1.0", "density_scale: 1.0\n  densitty: 1.0");
        assert!(TacticalGraphicsConfig::parse(&text).is_err());
    }
}
