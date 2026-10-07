use super::*;
use bevy::{
    asset::{load_internal_asset, load_internal_binary_asset, uuid_handle},
    ecs::system::IntoObserverSystem,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    input_focus::tab_navigation::{TabGroup, TabIndex},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use std::borrow::Cow;

const GRID_SHADER: Handle<Shader> = uuid_handle!("31cff239-dc54-408c-830f-8d49ad267b20");
const GRID: Handle<GridMaterial> = uuid_handle!("31cff239-dc54-408c-830f-8d49ad267b21");
const ICONS: Handle<Image> = uuid_handle!("31cff239-dc54-408c-830f-8d49ad267b22");
include!(concat!(env!("OUT_DIR"), "/icons.rs"));

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct GridMaterial {
    #[uniform(0)]
    background: Vec4,
    #[uniform(1)]
    glow: Vec4,
    #[uniform(2)]
    ink: Vec4,
}

impl UiMaterial for GridMaterial {
    fn fragment_shader() -> ShaderRef {
        GRID_SHADER.into()
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(UiMaterialPlugin::<GridMaterial>::default());
    load_internal_asset!(
        app,
        GRID_SHADER,
        "../../assets/shaders/menu_grid.wgsl",
        Shader::from_wgsl
    );
    app.world_mut()
        .resource_mut::<Assets<GridMaterial>>()
        .insert(
            GRID.id(),
            GridMaterial {
                background: colors::PAPER.to_linear().to_vec4(),
                glow: Color::srgb(0.118, 0.11, 0.098).to_linear().to_vec4(),
                ink: colors::INK.to_linear().to_vec4(),
            },
        )
        .unwrap();
    load_internal_binary_asset!(
        app,
        ICONS,
        concat!(env!("OUT_DIR"), "/icons.png"),
        |bytes: &[u8], _: String| {
            Image::from_buffer(
                bytes,
                ImageType::Extension("png"),
                CompressedImageFormats::NONE,
                true,
                ImageSampler::linear(),
                default(),
            )
            .unwrap()
        }
    );
}

pub fn ui_root(name: impl Into<Cow<'static, str>>) -> impl Bundle {
    (
        Name::new(name),
        Node {
            width: Percent(100.0),
            height: Percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Vh(5.0),
            ..default()
        },
        Pickable::IGNORE,
    )
}

pub fn menu_background() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: Percent(100.0),
            height: Percent(100.0),
            ..default()
        },
        MaterialNode(GRID),
        Pickable::IGNORE,
    )
}

pub fn sheet(title: impl Bundle, body: impl Bundle, _hint: &'static str) -> impl Bundle {
    (
        Name::new("Menu"),
        TabGroup::default(),
        Node {
            width: Percent(100.0),
            height: Percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(colors::PAPER),
        children![
            menu_background(),
            (
                Node {
                    min_height: Px(68.0),
                    flex_shrink: 0.0,
                    padding: UiRect::axes(Vw(3.5), Px(12.0)),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(Px(1.0)),
                    ..default()
                },
                BorderColor::all(colors::LINE),
                children![title]
            ),
            (
                Node {
                    width: Percent(100.0),
                    flex_grow: 1.0,
                    min_height: Px(0.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Vw(5.0), Vh(4.0)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                MenuScroll,
                children![body]
            ),
        ],
    )
}

#[derive(Component)]
pub struct MenuScroll;

pub fn menu_bar(active: &'static str) -> impl Bundle {
    let tab = |text: &'static str| {
        Props::new(text)
            .font_size(15.0)
            .palette_set(if active == text {
                PaletteSet {
                    none: Palette::new(colors::INK, colors::TINT, BorderColor::all(Color::NONE)),
                    ..PaletteSet::ghost()
                }
            } else {
                PaletteSet::ghost()
            })
    };
    (
        Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: Px(12.0),
            row_gap: Px(4.0),
            ..default()
        },
        children![
            btn(
                Props::new("Back")
                    .icon("back")
                    .font_size(15.0)
                    .palette_set(PaletteSet::ghost()),
                navigate_back
            ),
            btn(tab("Runes").icon("runes"), open_runes),
            btn(tab("Settings").icon("settings"), open_settings),
        ],
    )
}

pub fn navigate_back(on: On<Activate>, screen: Res<State<Screen>>, mut commands: Commands) {
    if *screen.get() == Screen::Gameplay {
        commands.entity(on.entity).trigger(PopModal);
    } else {
        commands.trigger(GoTo(Screen::Title));
    }
}

fn open_runes(on: On<Activate>, screen: Res<State<Screen>>, mut commands: Commands) {
    if *screen.get() == Screen::Gameplay {
        commands.trigger(NewModal {
            entity: on.entity,
            modal: Modal::Runes,
        });
    } else {
        commands.trigger(GoTo(Screen::Runes));
    }
}

fn open_settings(on: On<Activate>, screen: Res<State<Screen>>, mut commands: Commands) {
    if *screen.get() == Screen::Gameplay {
        commands.trigger(NewModal {
            entity: on.entity,
            modal: Modal::Settings,
        });
    } else {
        commands.trigger(GoTo(Screen::Settings));
    }
}

pub fn rule() -> impl Bundle {
    (
        Node {
            width: Percent(100.0),
            height: Px(1.0),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(colors::LINE),
        Pickable::IGNORE,
    )
}

pub fn columns() -> Node {
    Node {
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::FlexStart,
        justify_content: JustifyContent::Center,
        column_gap: Px(64.0),
        row_gap: Px(32.0),
        ..default()
    }
}

pub fn column(basis: f32, max: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        flex_basis: Px(basis),
        max_width: Px(max),
        row_gap: Px(8.0),
        ..default()
    }
}

pub fn info_panel(
    title: &'static str,
    rows: &'static [(&'static str, &'static str)],
) -> impl Bundle {
    (
        column(260.0, 400.0),
        children![
            label(Props::new(title).font_size(15.0).color(colors::INK_SOFT)),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Px(12.0),
                    margin: UiRect::top(Px(12.0)),
                    ..default()
                },
                Children::spawn(SpawnIter(rows.iter().map(|&(key, value)| (
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Px(16.0),
                        ..default()
                    },
                    children![
                        label(Props::new(key).font_size(15.0).color(colors::INK_SOFT)),
                        label(Props::new(value).font_size(15.0))
                    ]
                ))))
            )
        ],
    )
}

pub fn spawn_icon(name: &'static str, size: f32) -> impl Bundle {
    let index = ICON_NAMES
        .iter()
        .position(|&icon| icon == name)
        .expect("known UI icon") as f32;
    (
        Node {
            width: Px(size),
            height: Px(size),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode {
            image: ICONS,
            rect: Some(Rect::from_corners(
                Vec2::new(index * 48.0, 0.0),
                Vec2::new((index + 1.0) * 48.0, 48.0),
            )),
            color: colors::INK,
            ..default()
        },
        Pickable::IGNORE,
    )
}

pub fn icon(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (
        opts.node.clone(),
        opts.into_image_bundle(),
        Pickable::IGNORE,
    )
}

pub fn label(opts: impl Into<Props>) -> impl Bundle {
    let opts = opts.into();
    (
        Node {
            border: UiRect::ZERO,
            padding: UiRect::ZERO,
            min_height: Auto,
            min_width: Px(0.0),
            ..opts.node.clone()
        },
        opts.into_text_bundle(),
        Pickable::IGNORE,
    )
}

pub fn header(opts: impl Into<Props>, font_size: f32) -> impl Bundle {
    label(opts.into().font(TextFont {
        font: fonts::LIGHT,
        weight: bevy::text::FontWeight::LIGHT,
        font_size,
        ..default()
    }))
}

pub fn btn<B, M, I>(opts: impl Into<Props>, action: I) -> impl Bundle
where
    B: Bundle,
    M: Send + Sync + 'static,
    I: IntoObserverSystem<Activate, B, M> + Send + Sync,
{
    let opts = opts.into();
    let idle = opts.palette_set.none.clone();
    let icon = opts.icon;
    (
        Button,
        bevy::picking::hover::Hovered::default(),
        TabIndex(0),
        Outline::default(),
        Name::new("Button"),
        opts.node.clone(),
        opts.palette_set.clone(),
        BackgroundColor(idle.bg),
        idle.border,
        observe(action),
        Children::spawn(SpawnWith(move |parent: &mut ChildSpawner| {
            if let Some(name) = icon {
                parent.spawn(spawn_icon(name, 20.0));
            }
            match &opts.content {
                WidgetContent::Image(_) => {
                    parent.spawn((opts.into_image_bundle(), Pickable::IGNORE));
                }
                WidgetContent::Text(_) => {
                    parent.spawn((opts.color(idle.text).into_text_bundle(), Pickable::IGNORE));
                }
            }
        })),
    )
}
