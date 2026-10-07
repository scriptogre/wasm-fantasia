use super::*;
use crate::scripting::{EntityBehaviors, ScriptRegistryRes};
use game_core::runtime::registry::{
    BUILTIN_SCRIPTS, DEFAULT_ABILITIES, DEFAULT_BEHAVIORS, ScriptDefinition,
};

#[derive(Component)]
pub struct RunesView;
#[derive(Component)]
struct RunesContent;
#[derive(Component)]
struct RuneTooltip;
#[derive(Resource)]
struct SelectedRune(&'static str);
#[derive(Component)]
pub(super) struct InlineTerm;

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(SelectedRune("melee_attack"))
        .add_systems(OnEnter(Screen::Runes), |mut commands: Commands| {
            commands.spawn((DespawnOnExit(Screen::Runes), runes_ui()));
        })
        .add_systems(
            Update,
            populate_runes.run_if(
                resource_changed::<SelectedRune>
                    .or(any_match_filter::<Added<RunesContent>>)
                    .or(resource_changed::<ScriptRegistryRes>),
            ),
        );
}

pub fn runes_ui() -> impl Bundle {
    (
        RunesView,
        GlobalZIndex(200),
        sheet(
            menu_bar("Runes"),
            (
                RunesContent,
                Node {
                    width: Percent(100.0),
                    max_width: Px(1200.0),
                    align_self: AlignSelf::Center,
                    flex_direction: FlexDirection::Column,
                    row_gap: Px(24.0),
                    ..default()
                },
            ),
            "",
        ),
    )
}

fn find_definition(id: &str) -> &'static ScriptDefinition {
    BUILTIN_SCRIPTS
        .iter()
        .find(|script| script.id == id)
        .expect("known mechanic")
}

fn build_term(id: &'static str, selected: bool, ability: bool) -> impl Bundle {
    let script = find_definition(id);
    let palette = if selected && ability {
        PaletteSet::selected()
    } else if ability {
        PaletteSet::default()
    } else {
        PaletteSet::ghost()
    };
    let mut props = Props::new(script.title)
        .icon(script.icon)
        .font(TextFont {
            font: fonts::SEMIBOLD,
            weight: bevy::text::FontWeight::SEMIBOLD,
            font_size: if ability { 17.0 } else { 16.0 },
            ..default()
        })
        .palette_set(palette);
    props.node.padding = UiRect::axes(Px(if ability { 16.0 } else { 8.0 }), Px(10.0));
    if ability {
        props.node.width = Percent(100.0);
    }
    btn(
        props,
        move |_: On<Activate>, mut selected: ResMut<SelectedRune>| {
            selected.0 = id;
        },
    )
}

fn build_trigger(hook: &'static str) -> impl Bundle {
    let (text, help) = match hook {
        "on_pre_hit" => (
            "Before hit",
            "Runs before damage and knockback are applied.",
        ),
        _ => (
            "On hit",
            "Runs once for each enemy struck, after damage and knockback.",
        ),
    };
    let line = || {
        (
            Node {
                height: Px(1.0),
                flex_grow: 1.0,
                ..default()
            },
            BackgroundColor(colors::LINE),
            Pickable::IGNORE,
        )
    };
    (
        Node {
            align_items: AlignItems::Center,
            min_width: Px(88.0),
            flex_grow: 1.0,
            max_width: Px(260.0),
            ..default()
        },
        children![
            line(),
            (
                Button,
                bevy::input_focus::tab_navigation::TabIndex(0),
                Outline::default(),
                Node {
                    min_height: Px(44.0),
                    padding: UiRect::axes(Px(6.0), Px(6.0)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![label(
                    Props::new(text).font_size(13.0).color(colors::INK_SOFT)
                )],
                observe(move |on: On<Pointer<Over>>, mut commands: Commands| {
                    if !matches!(on.pointer_id, bevy::picking::pointer::PointerId::Touch(_)) {
                        show_tooltip(on.entity, help, &mut commands);
                    }
                }),
                observe(
                    move |on: On<Activate>,
                          tips: Query<Entity, With<RuneTooltip>>,
                          mut commands: Commands| {
                        for entity in &tips {
                            commands.entity(entity).despawn();
                        }
                        if tips.is_empty() {
                            show_tooltip(on.entity, help, &mut commands);
                        }
                    }
                ),
                observe(
                    |_: On<Pointer<Out>>,
                     tips: Query<Entity, With<RuneTooltip>>,
                     mut commands: Commands| {
                        for entity in &tips {
                            commands.entity(entity).despawn();
                        }
                    }
                ),
            ),
            line(),
            label(Props::new("›").font_size(16.0).color(colors::INK_SOFT)),
        ],
    )
}

fn show_tooltip(parent: Entity, help: &'static str, commands: &mut Commands) {
    commands.entity(parent).with_children(|parent| {
        parent.spawn((
            RuneTooltip,
            GlobalZIndex(220),
            Node {
                position_type: PositionType::Absolute,
                top: Percent(100.0),
                left: Px(-44.0),
                width: Px(200.0),
                padding: UiRect::all(Px(12.0)),
                border_radius: BorderRadius::all(Px(3.0)),
                ..default()
            },
            BackgroundColor(colors::HOVER),
            Pickable::IGNORE,
            children![label(Props::new(help).font_size(13.0))],
        ));
    });
}

fn populate_runes(
    selected: Res<SelectedRune>,
    registry: Res<ScriptRegistryRes>,
    equipped: Query<&EntityBehaviors, With<Player>>,
    roots: Query<Entity, With<RunesContent>>,
    mut commands: Commands,
) {
    let behaviors: Vec<&str> = equipped
        .single()
        .map(|equipped| equipped.0.iter().map(String::as_str).collect())
        .unwrap_or_else(|_| DEFAULT_BEHAVIORS.to_vec());
    let selected_script = find_definition(selected.0);
    for root in &roots {
        commands.entity(root).despawn_children().with_children(|parent| {
            parent.spawn(label(Props::new(if equipped.is_empty() { "Starting loadout" } else { "Equipped runes" })
                .font_size(13.0).color(colors::INK_SOFT)));
            for &ability in DEFAULT_ABILITIES {
                if registry.0.get(ability).is_none() { continue; }
                parent.spawn(Node { align_items: AlignItems::Center, column_gap: Px(0.0), ..default() })
                    .with_children(|row| {
                        row.spawn(Node { width: Percent(35.0), max_width: Px(360.0), min_width: Px(90.0), ..default() })
                            .with_child(build_term(ability, ability == selected.0, true));
                        row.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: Px(4.0), ..default() })
                            .with_children(|branches| {
                                if behaviors.len() > 1 {
                                    branches.spawn((Node { position_type: PositionType::Absolute, left: Px(0.0), top: Percent(25.0), height: Percent(50.0), width: Px(1.0), ..default() }, BackgroundColor(colors::LINE), Pickable::IGNORE));
                                }
                                for &behavior in &behaviors {
                                    let Some(definition) = BUILTIN_SCRIPTS.iter().find(|script| script.id == behavior) else { continue };
                                    let Some(engine) = registry.0.get(behavior) else { continue };
                                    for hook in ["on_pre_hit", "on_hit"] {
                                        if engine.has_function(hook) {
                                            branches.spawn(Node { align_items: AlignItems::Center, ..default() })
                                                .with_child(build_trigger(hook))
                                                .with_children(|branch| {
                                                    branch.spawn(Node { width: Percent(55.0), min_width: Px(126.0), max_width: Px(420.0), ..default() })
                                                        .with_child((InlineTerm, build_term(definition.id, definition.id == selected.0, false)));
                                                });
                                        }
                                    }
                                }
                            });
                    });
            }
            parent.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Px(48.0), row_gap: Px(24.0), margin: UiRect::top(Px(24.0)), ..default() })
                .with_children(|details| {
                    details.spawn((column(280.0, 550.0), children![
                        rule(),
                        label(Props::new(if DEFAULT_ABILITIES.contains(&selected.0) { "ABILITY" } else { "BEHAVIOR" })
                            .font_size(12.0).color(colors::INK_SOFT).margin(UiRect::top(Px(16.0)))),
                        (Node { align_items: AlignItems::Center, column_gap: Px(12.0), ..default() }, children![
                            spawn_icon(selected_script.icon, 28.0), header(selected_script.title, 28.0),
                        ]),
                        label(Props::new(selected_script.description).font_size(18.0).margin(UiRect::top(Px(12.0)))),
                    ]));
                    details.spawn((column(280.0, 550.0), children![rule()])).with_children(|lines| {
                        for &behavior in &behaviors {
                            if behavior == "crit" && registry.0.get(behavior).is_some_and(|engine| engine.has_function("on_pre_hit")) {
                                lines.spawn(build_interaction("crit", "can multiply damage and knockback before a hit."));
                            }
                            if behavior == "stacking" && registry.0.get(behavior).is_some_and(|engine| engine.has_function("on_hit")) {
                                let normal = game_core::fury::on_hit(0, false);
                                let critical = game_core::fury::on_hit(0, true);
                                lines.spawn(build_interaction("stacking", &format!("gains {} stack on hit, or {} on a critical hit.", normal.stacks, critical.stacks)));
                                lines.spawn(label(Props::new(format!("+{}% attack speed per stack. Up to {} stacks. Hits refresh the {:.1}s duration.",
                                    game_core::fury::bonus_percent(1), game_core::fury::bounded(i64::MAX), normal.remaining_micros as f32 / 1_000_000.0))
                                    .font_size(15.0).color(colors::INK_SOFT)));
                            }
                        }
                    });
                });
        });
    }
}

fn build_interaction(id: &'static str, text: &str) -> impl Bundle {
    let text = text.to_owned();
    (
        Node {
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: Px(4.0),
            margin: UiRect::top(Px(12.0)),
            ..default()
        },
        children![
            (InlineTerm, build_term(id, false, false)),
            label(Props::new(text).font_size(16.0))
        ],
    )
}
