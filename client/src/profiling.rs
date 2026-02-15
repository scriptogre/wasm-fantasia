//! Per-system profiling via a tracing Layer that captures Bevy's system spans.
//!
//! Build with `--features profile` (enables `bevy/trace` without Tracy),
//! then press F9 to benchmark. The report includes a per-system timing breakdown.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use bevy::log::BoxedLayer;
use bevy::prelude::*;
use web_time::Instant;

#[cfg(feature = "profile")]
use bevy::log::tracing_subscriber::layer::Context;
#[cfg(feature = "profile")]
use bevy::log::tracing_subscriber::{Layer, Registry};

// ── Shared state (tracing layer ↔ Bevy systems) ─────────────────────────

static PROFILING_ACTIVE: AtomicBool = AtomicBool::new(false);
static STATE: LazyLock<Mutex<ProfileState>> =
    LazyLock::new(|| Mutex::new(ProfileState::default()));

#[derive(Default)]
struct ProfileState {
    span_names: HashMap<tracing::span::Id, String>,
    enter_times: HashMap<tracing::span::Id, Instant>,
    timings: HashMap<String, (Duration, u64)>,
}

// ── Public API (called from benchmark systems) ──────────────────────────

pub fn start() {
    let mut state = STATE.lock().unwrap();
    state.timings.clear();
    state.enter_times.clear();
    PROFILING_ACTIVE.store(true, Ordering::Relaxed);
}

pub fn stop() -> Vec<(String, Duration, u64)> {
    PROFILING_ACTIVE.store(false, Ordering::Relaxed);
    let mut state = STATE.lock().unwrap();
    let mut results: Vec<_> = state
        .timings
        .drain()
        .map(|(name, (total, count))| (name, total, count))
        .collect();
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}

pub fn format_report(timings: &[(String, Duration, u64)], benchmark_duration: Duration) -> String {
    if timings.is_empty() {
        return String::from(
            "System profiling: no data (build with --features profile to enable)",
        );
    }

    let total_wall: f64 = benchmark_duration.as_secs_f64();
    let mut lines = vec![String::from("=== SYSTEM TIMING ===")];

    for (name, total, count) in timings.iter().take(25) {
        let total_ms = total.as_secs_f64() * 1000.0;
        let avg_ms = total_ms / *count as f64;
        let pct = (total.as_secs_f64() / total_wall) * 100.0;
        // Trim the crate path prefix for readability
        let short_name = name
            .strip_prefix("wasm_fantasia::")
            .unwrap_or(name);
        lines.push(format!(
            "{short_name:<60} avg={avg_ms:>7.2}ms  total={total_ms:>8.1}ms  calls={count:<6} ({pct:.1}%)",
        ));
    }

    lines.join("\n")
}

// ── LogPlugin custom_layer callback ─────────────────────────────────────

pub fn system_profile_layer(_app: &mut App) -> Option<BoxedLayer> {
    #[cfg(feature = "profile")]
    {
        Some(Box::new(SystemProfileLayer))
    }
    #[cfg(not(feature = "profile"))]
    {
        None
    }
}

// ── Tracing Layer ───────────────────────────────────────────────────────

#[cfg(feature = "profile")]
struct SystemProfileLayer;

/// Extracts the `name` field from a tracing span's attributes.
#[cfg(feature = "profile")]
struct NameVisitor(Option<String>);

#[cfg(feature = "profile")]
impl tracing::field::Visit for NameVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.0 = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.0 = Some(value.to_string());
        }
    }
}

#[cfg(feature = "profile")]
impl Layer<Registry> for SystemProfileLayer {
    // Bevy creates system spans once at startup with info_span!("system", name = ...).
    // We must always record the span name → id mapping here, regardless of whether
    // profiling is currently active, because on_new_span won't fire again later.
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, Registry>,
    ) {
        let meta_name = attrs.metadata().name();
        if meta_name != "system" && meta_name != "system_commands" {
            return;
        }
        let mut visitor = NameVisitor(None);
        attrs.record(&mut visitor);
        if let Some(name) = visitor.0 {
            let key = if meta_name == "system_commands" {
                format!("{name} [commands]")
            } else {
                name
            };
            STATE.lock().unwrap().span_names.insert(id.clone(), key);
        }
    }

    fn on_enter(&self, id: &tracing::span::Id, _ctx: Context<'_, Registry>) {
        if !PROFILING_ACTIVE.load(Ordering::Relaxed) {
            return;
        }
        let mut state = STATE.lock().unwrap();
        if state.span_names.contains_key(id) {
            state.enter_times.insert(id.clone(), Instant::now());
        }
    }

    fn on_exit(&self, id: &tracing::span::Id, _ctx: Context<'_, Registry>) {
        if !PROFILING_ACTIVE.load(Ordering::Relaxed) {
            return;
        }
        let mut state = STATE.lock().unwrap();
        if let Some(start) = state.enter_times.remove(id) {
            if let Some(name) = state.span_names.get(id).cloned() {
                let elapsed = start.elapsed();
                let entry = state
                    .timings
                    .entry(name)
                    .or_insert((Duration::ZERO, 0));
                entry.0 += elapsed;
                entry.1 += 1;
            }
        }
    }

    fn on_close(&self, id: tracing::span::Id, _ctx: Context<'_, Registry>) {
        let mut state = STATE.lock().unwrap();
        state.span_names.remove(&id);
        state.enter_times.remove(&id);
    }
}
