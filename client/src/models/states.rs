use super::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<Session>()
        .init_resource::<GameMode>()
        .register_type::<Mood>();
}

/// Entities that survive gameplay exit. Applied to all Startup entities
/// automatically; gameplay-spawned entities lack this and get cleaned up.
#[derive(Component)]
pub struct Persistent;

/// System set for the nuclear gameplay cleanup. All `OnExit(Screen::Gameplay)`
/// systems that need their target entities alive must run `.before(GameplayCleanup)`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameplayCleanup;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameMode {
    #[default]
    Singleplayer,
    Multiplayer,
}

pub fn is_multiplayer_mode(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Multiplayer
}

/// Describes where the SpacetimeDB instance lives.
/// Inserted when the player picks a mode on the title screen;
/// removed when returning to title.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum ServerTarget {
    /// Native singleplayer — launch a local SpacetimeDB subprocess.
    Local { port: u16 },
    /// Multiplayer (all platforms) or web solo — connect to a remote server.
    Remote { uri: String },
}

/// Runtime session flags — debug toggles, preferences, and transient state.
/// Reset on return to title. Not persisted (see [`Settings`] for that).
#[derive(Resource, Reflect, Debug, Clone)]
#[reflect(Resource)]
pub struct Session {
    pub last_screen: Screen,
    pub current_mood: Mood,

    pub diagnostics: bool,
    pub debug_ui: bool,
    pub screen_shake: bool,
    pub paused: bool,
    pub muted: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            last_screen: Screen::Title,
            current_mood: Mood::Exploration,
            diagnostics: false,
            debug_ui: false,    // Off by default
            screen_shake: true, // On by default
            paused: false,
            muted: false,
        }
    }
}

impl Session {
    pub fn reset(&mut self) {
        self.paused = false;
        self.muted = false;
    }
}

pub fn is_paused(session: Res<Session>) -> bool {
    session.paused
}

/// The game's main screen states.
/// See <https://bevy-cheatbook.github.io/programming/states.html>
/// Or <https://github.com/bevyengine/bevy/blob/main/examples/ecs/state.rs>
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Reflect)]
pub enum Screen {
    #[cfg_attr(not(feature = "dev_native"), default)]
    Splash,
    #[cfg_attr(feature = "dev_native", default)]
    Loading,
    Tutorial,
    Settings,
    // Here the menu is drawn and waiting for player interaction
    Title,
    // MP connection handshake — between Title and Gameplay
    Connecting,
    // During this State the actual game logic is executed
    Gameplay,
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[reflect(Component)]
pub enum Mood {
    #[default]
    Exploration,
    Combat,
}
