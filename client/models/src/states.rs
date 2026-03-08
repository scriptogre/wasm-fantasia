use bevy::prelude::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<Session>()
        .init_resource::<GameMode>()
        .init_state::<PauseState>()
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
            muted: false,
        }
    }
}

impl Session {
    pub fn reset(&mut self) {
        self.muted = false;
    }
}

/// Orthogonal state: gameplay pause, independent of [`Screen`].
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Reflect)]
pub enum PauseState {
    #[default]
    Running,
    Paused,
}

pub fn is_paused(state: Res<State<PauseState>>) -> bool {
    *state.get() == PauseState::Paused
}

/// Run condition: true when transitioning into [`Screen::GameOver`].
/// Used to skip cleanup when transitioning from Gameplay to GameOver.
/// Checks `State<Screen>` because Bevy consumes `NextState` before `OnExit` runs.
pub fn is_entering_game_over(state: Res<State<Screen>>) -> bool {
    *state.get() == Screen::GameOver
}

/// Run condition: true when transitioning into [`Screen::Connecting`] (restart).
/// Used to skip networking teardown when restarting from GameOver.
pub fn is_entering_connecting(state: Res<State<Screen>>) -> bool {
    *state.get() == Screen::Connecting
}

#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Reflect)]
pub enum Screen {
    #[default]
    Loading,
    Tutorial,
    Settings,
    // Here the menu is drawn and waiting for player interaction
    Title,
    // MP connection handshake — between Title and Gameplay
    Connecting,
    // During this State the actual game logic is executed
    Gameplay,
    // Player died — show death overlay with restart option
    GameOver,
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[reflect(Component)]
pub enum Mood {
    #[default]
    Exploration,
    Combat,
}
