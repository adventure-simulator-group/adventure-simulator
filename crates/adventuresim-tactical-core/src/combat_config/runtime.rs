use std::{
    cell::Cell,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use super::*;

impl TacticalCombatConfig {
    /// Makes a validated process-lifetime snapshot available to animation
    /// code that runs below Bevy's system-parameter boundary.
    pub fn install_runtime_snapshot(&self) -> Result<(), TacticalCombatConfigError> {
        self.validate()?;
        *runtime_config_lock()
            .write()
            .expect("tactical combat config lock should not be poisoned") = self.clone();
        RUNTIME_CONFIG_VERSION.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

static RUNTIME_CONFIG: OnceLock<RwLock<TacticalCombatConfig>> = OnceLock::new();
static RUNTIME_CONFIG_VERSION: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static CACHED_ANIMATION_CONFIG: Cell<Option<(u64, TacticalAnimationConfig)>> = const {
        Cell::new(None)
    };
}

fn runtime_config_lock() -> &'static RwLock<TacticalCombatConfig> {
    RUNTIME_CONFIG.get_or_init(|| RwLock::new(TacticalCombatConfig::default()))
}

/// Returns the active animation tuning selected from runtime YAML. The value
/// is copied so animation evaluation never holds a synchronization guard.
pub fn runtime_animation_config() -> TacticalAnimationConfig {
    let version = RUNTIME_CONFIG_VERSION.load(Ordering::Acquire);
    CACHED_ANIMATION_CONFIG.with(|cache| {
        if let Some((cached_version, config)) = cache.get()
            && cached_version == version
        {
            return config;
        }
        let config = runtime_config_lock()
            .read()
            .expect("tactical combat config lock should not be poisoned")
            .animation;
        cache.set(Some((version, config)));
        config
    })
}

pub fn runtime_combat_presentation_config() -> CombatPresentationConfig {
    runtime_config_lock()
        .read()
        .expect("tactical combat config lock should not be poisoned")
        .presentation
        .clone()
}

pub fn runtime_melee_authority_config() -> MeleeAuthorityConfig {
    runtime_config_lock()
        .read()
        .expect("tactical combat config lock should not be poisoned")
        .realtime_authority
        .melee
        .clone()
}
