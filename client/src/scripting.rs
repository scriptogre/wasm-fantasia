//! Bevy plugin for Rune script loading and execution.
//!
//! Wraps the `game_core::scripting` engine as Bevy resources and components,
//! compiling `.rune` scripts at startup via `include_str!()`.
//!
//! In dev builds (`feature = "dev"`), scripts are loaded from the filesystem
//! and hot-reloaded when `.rune` files change — no restart required.

use std::sync::Arc;

use bevy::prelude::*;
use game_core::scripting::registry::ScriptRegistry;

/// Bevy resource wrapping an `Arc<ScriptRegistry>` that holds all compiled scripts.
#[derive(Resource)]
pub struct ScriptRegistryRes(pub Arc<ScriptRegistry>);

/// Behavior scripts attached to an entity (e.g. `["crit", "stacking"]`).
/// These are chained via `fire_hook` during ability execution.
#[derive(Component, Clone, Debug, Default)]
pub struct EntityBehaviors(pub Vec<String>);

/// Which ability script this entity uses (e.g. `"melee_attack"`).
#[derive(Component, Clone, Debug)]
pub struct ActiveAbility(pub String);

fn build_registry() -> ScriptRegistry {
    let mut registry = ScriptRegistry::new();

    // Behavior scripts
    registry
        .register(
            "crit".to_string(),
            include_str!("../../core/gameplay/behaviors/crit.rune"),
        )
        .expect("crit.rune should compile");

    registry
        .register(
            "stacking".to_string(),
            include_str!("../../core/gameplay/behaviors/stacking.rune"),
        )
        .expect("stacking.rune should compile");

    registry
        .register(
            "feedback".to_string(),
            include_str!("../../core/gameplay/behaviors/feedback.rune"),
        )
        .expect("feedback.rune should compile");

    // Ability scripts
    registry
        .register(
            "melee_attack".to_string(),
            include_str!("../../core/gameplay/abilities/melee_attack.rune"),
        )
        .expect("melee_attack.rune should compile");

    registry
        .register(
            "ground_pound".to_string(),
            include_str!("../../core/gameplay/abilities/ground_pound.rune"),
        )
        .expect("ground_pound.rune should compile");

    // Enemy AI scripts
    registry
        .register(
            "zombie_ai".to_string(),
            include_str!("../../core/gameplay/enemies/zombie_ai.rune"),
        )
        .expect("zombie_ai.rune should compile");

    registry
}

#[cfg(feature = "dev")]
mod hot_reload {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::SystemTime;

    /// Tracks script file modification times and polls for changes.
    #[derive(Resource)]
    pub struct ScriptWatcher {
        script_dir: PathBuf,
        last_modified: HashMap<PathBuf, SystemTime>,
        check_timer: Timer,
    }

    impl ScriptWatcher {
        pub fn new(script_dir: PathBuf) -> Self {
            Self {
                script_dir,
                last_modified: HashMap::new(),
                check_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            }
        }
    }

    /// Walk `script_dir` for all `.rune` files, compile each, and return a
    /// fresh [`ScriptRegistry`]. The registry key is the file stem (e.g.
    /// `"crit"` for `behaviors/crit.rune`).
    pub fn build_registry_from_files(script_dir: &PathBuf) -> Result<ScriptRegistry, String> {
        let mut registry = ScriptRegistry::new();

        for subdir in &["behaviors", "abilities", "enemies"] {
            let dir = script_dir.join(subdir);
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;

            for entry in entries {
                let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("rune") {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| format!("invalid filename: {}", path.display()))?
                    .to_string();

                let source = std::fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

                registry
                    .register(name.clone(), &source)
                    .map_err(|e| format!("compile error in {}: {e}", path.display()))?;
            }
        }
        Ok(registry)
    }

    /// Snapshot the current modification times for all `.rune` files.
    fn snapshot_times(script_dir: &PathBuf) -> HashMap<PathBuf, SystemTime> {
        let mut map = HashMap::new();
        for subdir in &["behaviors", "abilities", "enemies"] {
            let dir = script_dir.join(subdir);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("rune") {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(modified) = meta.modified() {
                        map.insert(path, modified);
                    }
                }
            }
        }
        map
    }

    /// Register the [`ScriptWatcher`] resource and polling system.
    pub fn setup(app: &mut App) {
        let script_dir = PathBuf::from("core/gameplay");
        let mut watcher = ScriptWatcher::new(script_dir.clone());
        // Seed initial timestamps so we don't reload on first tick.
        watcher.last_modified = snapshot_times(&script_dir);
        app.insert_resource(watcher);
        app.add_systems(Update, check_for_changes);
    }

    fn check_for_changes(
        time: Res<Time>,
        mut watcher: ResMut<ScriptWatcher>,
        mut registry: ResMut<ScriptRegistryRes>,
    ) {
        watcher.check_timer.tick(time.delta());
        if !watcher.check_timer.just_finished() {
            return;
        }

        let current = snapshot_times(&watcher.script_dir);
        let any_changed = current.iter().any(|(path, mtime)| {
            watcher
                .last_modified
                .get(path)
                .map_or(true, |prev| prev != mtime)
        }) || current.len() != watcher.last_modified.len();

        if any_changed {
            match build_registry_from_files(&watcher.script_dir) {
                Ok(new_registry) => {
                    registry.0 = Arc::new(new_registry);
                    info!("Rune scripts hot-reloaded");
                }
                Err(e) => {
                    warn!("Failed to hot-reload scripts: {e}");
                }
            }
            watcher.last_modified = current;
        }
    }
}

pub fn plugin(app: &mut App) {
    #[cfg(feature = "dev")]
    {
        let script_dir = std::path::PathBuf::from("core/gameplay");
        match hot_reload::build_registry_from_files(&script_dir) {
            Ok(reg) => {
                app.insert_resource(ScriptRegistryRes(Arc::new(reg)));
            }
            Err(e) => {
                warn!("Failed to load scripts from filesystem, falling back to embedded: {e}");
                app.insert_resource(ScriptRegistryRes(Arc::new(build_registry())));
            }
        }
        hot_reload::setup(app);
    }

    #[cfg(not(feature = "dev"))]
    {
        app.insert_resource(ScriptRegistryRes(Arc::new(build_registry())));
    }
}
