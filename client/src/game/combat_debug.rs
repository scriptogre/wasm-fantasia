use bevy::prelude::*;
use std::collections::VecDeque;
use std::fmt::Write;

use crate::asset_loading::Fonts;
use crate::combat::{DamageDealt, Died, Enemy, Health, PlayerCombatant};
use crate::models::{Player as LocalPlayer, Session};
use crate::rules::{Stat, Stats};
use crate::ui::{colors, size};

#[cfg(feature = "multiplayer")]
use crate::networking::SpacetimeDbConnection;
#[cfg(feature = "multiplayer")]
use crate::networking::generated::combat_event_table::CombatEventTableAccess;
#[cfg(feature = "multiplayer")]
use crate::networking::generated::enemy_table::EnemyTableAccess;
#[cfg(feature = "multiplayer")]
use crate::networking::generated::player_table::PlayerTableAccess;
#[cfg(feature = "multiplayer")]
use spacetimedb_sdk::{DbContext, Table};

const MAX_ENTRIES: usize = 10;

// ── Log entries ──────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Tag {
    Local,
    Remote,
    Kill,
    Spawn,
}

impl Tag {
    fn label(self) -> &'static str {
        match self {
            Tag::Local => "YOU",
            Tag::Remote => "IN ",
            Tag::Kill => "KILL",
            Tag::Spawn => "+/-",
        }
    }
}

struct DebugEntry {
    tag: Tag,
    msg: String,
}

struct PendingHit {
    is_local: bool,
    damage: f32,
    is_crit: bool,
    target_name: String,
}

// ── Resource ─────────────────────────────────────────────────────────

#[derive(Component)]
struct DebugPanel;

#[derive(Component)]
struct DebugText;

#[derive(Resource)]
pub struct DebugLog {
    entries: VecDeque<DebugEntry>,
    pending_hits: Vec<PendingHit>,
    dirty: bool,
    last_enemy_count: usize,
    frame: u32,
}

impl Default for DebugLog {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
            pending_hits: Vec::new(),
            dirty: true,
            last_enemy_count: 0,
            frame: 0,
        }
    }
}

impl DebugLog {
    fn push(&mut self, tag: Tag, msg: impl Into<String>) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(DebugEntry {
            tag,
            msg: msg.into(),
        });
        self.dirty = true;
    }
}

// ── Plugin ───────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.init_resource::<DebugLog>()
        .add_systems(Startup, spawn_panel)
        .add_observer(observe_damage)
        .add_observer(observe_death)
        .add_systems(
            Update,
            (
                toggle_overlay,
                flush_pending_hits,
                detect_enemy_changes,
                update_overlay,
            )
                .chain(),
        );
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        DebugPanel,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            top: Val::Px(10.0),
            max_height: Val::Vh(70.0),
            min_width: Val::Px(220.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            overflow: Overflow::clip_y(),
            ..default()
        },
        BackgroundColor(colors::NEUTRAL920.with_alpha(0.92)),
        BorderRadius::all(size::BORDER_RADIUS),
        Visibility::Hidden,
    ));
}

// ── Toggle ───────────────────────────────────────────────────────────

fn toggle_overlay(
    input: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<Session>,
    mut log: ResMut<DebugLog>,
    mut panel: Query<&mut Visibility, With<DebugPanel>>,
) {
    if input.just_pressed(KeyCode::F4) {
        session.diagnostics = !session.diagnostics;
        log.dirty = true;
    }

    let should_show = session.diagnostics && !session.paused;
    if let Ok(mut vis) = panel.single_mut() {
        *vis = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

// ── Observers ────────────────────────────────────────────────────────

fn observe_damage(
    on: On<DamageDealt>,
    mut log: ResMut<DebugLog>,
    local_players: Query<(), With<LocalPlayer>>,
    names: Query<&Name>,
) {
    let event = on.event();
    let target_name = names
        .get(event.target)
        .map(|n| n.as_str().to_string())
        .unwrap_or_else(|_| format!("#{}", event.target.index()));
    log.pending_hits.push(PendingHit {
        is_local: local_players.get(event.source).is_ok(),
        damage: event.damage,
        is_crit: event.is_crit,
        target_name,
    });
}

fn observe_death(on: On<Died>, mut log: ResMut<DebugLog>, names: Query<&Name>) {
    let event = on.event();
    let name = names
        .get(event.entity)
        .map(|n| n.as_str().to_string())
        .unwrap_or_else(|_| format!("#{}", event.entity.index()));
    log.push(Tag::Kill, name);
}

// ── Per-frame aggregation ────────────────────────────────────────────

fn flush_pending_hits(mut log: ResMut<DebugLog>) {
    if log.pending_hits.is_empty() {
        return;
    }
    let hits: Vec<PendingHit> = log.pending_hits.drain(..).collect();

    for is_local in [true, false] {
        let batch: Vec<&PendingHit> = hits.iter().filter(|h| h.is_local == is_local).collect();
        if batch.is_empty() {
            continue;
        }
        let total_hits = batch.len();
        let crits = batch.iter().filter(|h| h.is_crit).count();
        let total_dmg: f32 = batch.iter().map(|h| h.damage).sum();

        let mut targets: Vec<&str> = batch.iter().map(|h| h.target_name.as_str()).collect();
        targets.dedup();
        let target_str = if targets.len() == 1 {
            targets[0].to_string()
        } else {
            format!("{} targets", targets.len())
        };
        let crit_str = if crits > 0 {
            format!(" ({crits} crit)")
        } else {
            String::new()
        };
        let tag = if is_local { Tag::Local } else { Tag::Remote };
        log.push(
            tag,
            format!("{total_dmg:.0} dmg -> {target_str} x{total_hits}{crit_str}"),
        );
    }
}

fn detect_enemy_changes(mut log: ResMut<DebugLog>, enemies: Query<(), With<Enemy>>) {
    let count = enemies.iter().count();
    let prev = log.last_enemy_count;
    if prev != count && prev != 0 {
        let diff = count as i32 - prev as i32;
        if diff > 0 {
            log.push(Tag::Spawn, format!("+{diff} ({count} total)"));
        } else {
            log.push(Tag::Spawn, format!("{diff} ({count} total)"));
        }
    }
    log.last_enemy_count = count;
}

// ── Render helpers ──────────────────────────────────────────────────

fn spawn_title(commands: &mut Commands, panel: Entity, fonts: &Fonts, text: impl Into<String>) {
    commands.spawn((
        DebugText,
        ChildOf(panel),
        Text::new(text),
        TextFont {
            font: fonts.semibold.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(colors::NEUTRAL300),
    ));
}

fn spawn_body(commands: &mut Commands, panel: Entity, text: impl Into<String>) {
    commands.spawn((
        DebugText,
        ChildOf(panel),
        Text::new(text),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(colors::NEUTRAL500),
    ));
}

// ── Render ───────────────────────────────────────────────────────────

fn update_overlay(
    session: Res<Session>,
    mut log: ResMut<DebugLog>,
    fonts: Option<Res<Fonts>>,
    panel: Query<Entity, With<DebugPanel>>,
    existing: Query<Entity, With<DebugText>>,
    mut commands: Commands,
    #[cfg(feature = "multiplayer")] conn: Option<Res<SpacetimeDbConnection>>,
    player_query: Query<(&Health, Option<&Stats>), With<PlayerCombatant>>,
) {
    log.frame = log.frame.wrapping_add(1);

    let Some(fonts) = fonts else {
        return;
    };

    if !session.diagnostics {
        return;
    }

    if log.frame.is_multiple_of(10) {
        log.dirty = true;
    }

    if !log.dirty {
        return;
    }
    log.dirty = false;

    // Despawn previous text children
    for e in &existing {
        commands.entity(e).despawn();
    }

    let Ok(panel_entity) = panel.single() else {
        return;
    };

    // ── Player stats ────────────────────────────────────────
    if let Ok((health, stats)) = player_query.single() {
        let stacks = stats
            .map(|s| s.get(&Stat::Custom("Stacks".into())) as u32)
            .unwrap_or(0);
        let atk_spd = stats.map(|s| s.get(&Stat::AttackSpeed)).unwrap_or(1.0);
        spawn_title(
            &mut commands,
            panel_entity,
            &fonts,
            format!(
                "HP {:.0}/{:.0}   Stacks {}   Spd {atk_spd:.2}",
                health.current, health.max, stacks,
            ),
        );
    }

    // ── Server sections (MP only) ───────────────────────────
    #[cfg(feature = "multiplayer")]
    if let Some(ref conn) = conn {
        let our_id = conn.conn.try_identity();

        // Players
        let mut players: Vec<_> = conn.conn.db.player().iter().collect();
        players.sort_by_key(|p| {
            let is_you = if Some(p.identity) == our_id { 0 } else { 1 };
            let online = if p.online { 0 } else { 1 };
            (online, is_you)
        });
        let online = players.iter().filter(|p| p.online).count();
        spawn_title(
            &mut commands,
            panel_entity,
            &fonts,
            format!("Players  {online}/{}", players.len()),
        );

        const MAX_PLAYER_ROWS: usize = 5;
        let mut body = String::new();
        for p in players.iter().take(MAX_PLAYER_ROWS) {
            let name = p.name.as_deref().unwrap_or("?");
            let you = if Some(p.identity) == our_id {
                " (you)"
            } else {
                ""
            };
            let status = if p.online { "" } else { " [off]" };
            let _ = writeln!(
                body,
                "{name}{you}{status}  {:.0}/{:.0}",
                p.health, p.max_health
            );
        }
        if players.len() > MAX_PLAYER_ROWS {
            let _ = writeln!(body, "+{} more", players.len() - MAX_PLAYER_ROWS);
        }
        spawn_body(&mut commands, panel_entity, body.trim_end());

        // NPCs
        let enemies: Vec<_> = conn.conn.db.enemy().iter().collect();
        if !enemies.is_empty() {
            let alive = enemies.iter().filter(|n| n.health > 0.0).count();
            spawn_title(
                &mut commands,
                panel_entity,
                &fonts,
                format!("Enemies  {} alive / {} dead", alive, enemies.len() - alive),
            );
        } else {
            spawn_title(&mut commands, panel_entity, &fonts, "Enemies  none");
        }

        // Server events
        let events: Vec<_> = conn.conn.db.combat_event().iter().collect();
        if !events.is_empty() {
            spawn_title(
                &mut commands,
                panel_entity,
                &fonts,
                format!("Server Events  ({})", events.len()),
            );
            let mut body = String::new();
            for evt in events.iter().rev().take(3).rev() {
                let crit = if evt.is_crit { " CRIT" } else { "" };
                let _ = writeln!(
                    body,
                    "{:.0} dmg at ({:.1}, {:.1}){crit}",
                    evt.damage, evt.x, evt.z
                );
            }
            spawn_body(&mut commands, panel_entity, body.trim_end());
        }

        // Desync warning
        if let Ok((local_hp, _)) = player_query.single() {
            if let Some(id) = our_id {
                if let Some(sp) = conn.conn.db.player().identity().find(&id) {
                    let delta = (local_hp.current - sp.health).abs();
                    if delta > 0.1 {
                        spawn_title(
                            &mut commands,
                            panel_entity,
                            &fonts,
                            format!(
                                "DESYNC  local {:.0} / server {:.0}",
                                local_hp.current, sp.health
                            ),
                        );
                    }
                }
            }
        }
    }

    // ── Event log ───────────────────────────────────────────
    if !log.entries.is_empty() {
        spawn_title(&mut commands, panel_entity, &fonts, "Combat Log");
        let mut body = String::new();
        for entry in &log.entries {
            let _ = writeln!(body, "[{}] {}", entry.tag.label(), entry.msg);
        }
        spawn_body(&mut commands, panel_entity, body.trim_end());
    }
}
