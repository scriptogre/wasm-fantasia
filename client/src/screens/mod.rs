//! The game's main screen states and transitions between them.
use crate::*;
use bevy::ui::Val::*;

#[cfg(feature = "multiplayer")]
mod connecting;
mod gameplay;
mod loading;
mod settings;
mod splash;
mod title;

pub fn plugin(app: &mut App) {
    app.init_state::<Screen>();

    app.add_plugins((
        camera::plugin,
        splash::plugin,
        loading::plugin,
        title::plugin,
        settings::plugin,
        gameplay::plugin,
    ));

    #[cfg(feature = "multiplayer")]
    app.add_plugins(connecting::plugin);

    app.add_systems(Update, track_last_screen.run_if(state_changed::<Screen>))
        .add_observer(on_back)
        .add_observer(on_go_to);
}

// TODO: figure out how to make it a cool observer
// mut transitions: On<StateTransitionEvent<Screen>>,
fn track_last_screen(
    mut transitions: MessageReader<StateTransitionEvent<Screen>>,
    mut state: ResMut<Session>,
) {
    let Some(transition) = transitions.read().last() else {
        return;
    };
    state.last_screen = transition.clone().exited.unwrap_or(Screen::Title);
}

fn on_back(
    trigger: On<Back>,
    mut next_screen: ResMut<NextState<Screen>>,
    screen: Res<State<Screen>>,
) {
    // Do not go to the title on back, we'd rather handle it in gameplay observers
    if *screen.get() == Screen::Gameplay {
        return;
    }

    let back = trigger.event();
    next_screen.set(back.screen.clone());
}

pub fn on_go_to(goto: On<GoTo>, mut next_screen: ResMut<NextState<Screen>>) {
    next_screen.set(goto.event().0.clone());
}

// TODO: figure out nice click_go_to(Screen::Title) HOF
// fn click_go_to<E, B, M>(s: Screen) -> impl IntoObserverSystem<OnPress, B, M> {
//     |_: On<OnPress>, mut cmds: Commands| cmds.trigger(OnGoTo(s.clone()))
// }
pub mod to {
    use super::*;

    pub fn title(
        _: On<Pointer<Click>>,
        mut commands: Commands,
        mut state: ResMut<Session>,
        mut modals: ResMut<Modals>,
    ) {
        state.reset();
        modals.clear();
        commands.remove_resource::<ServerTarget>();
        // Reset config so stale local URIs don't leak into multiplayer
        commands.insert_resource(crate::networking::SpacetimeDbConfig::default());
        commands.trigger(GoTo(Screen::Title));
    }
    pub fn settings(_: On<Pointer<Click>>, mut commands: Commands) {
        commands.trigger(GoTo(Screen::Settings));
    }

    /// Native singleplayer: start a local SpacetimeDB subprocess, then connect.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn singleplayer(
        _: On<Pointer<Click>>,
        mut mode: ResMut<GameMode>,
        mut commands: Commands,
        resource_handles: Res<ResourceHandles>,
        mut next_screen: ResMut<NextState<Screen>>,
    ) {
        *mode = GameMode::Singleplayer;

        #[cfg(feature = "multiplayer")]
        {
            // Start the local SpacetimeDB subprocess
            let (server, state) = crate::networking::local_server::start();
            let port = server.port;
            commands.insert_resource(server);
            commands.insert_resource(state);
            commands.insert_resource(ServerTarget::Local { port });
        }

        if resource_handles.is_all_done() {
            #[cfg(feature = "multiplayer")]
            next_screen.set(Screen::Connecting);
            #[cfg(not(feature = "multiplayer"))]
            next_screen.set(Screen::Gameplay);
        } else {
            next_screen.set(Screen::Loading);
        }
    }

    /// Web solo: private session on the remote server.
    #[cfg(target_arch = "wasm32")]
    #[cfg(feature = "multiplayer")]
    pub fn solo(
        _: On<Pointer<Click>>,
        mut mode: ResMut<GameMode>,
        mut commands: Commands,
        config: Res<crate::networking::SpacetimeDbConfig>,
        resource_handles: Res<ResourceHandles>,
        mut next_screen: ResMut<NextState<Screen>>,
    ) {
        *mode = GameMode::Singleplayer;
        commands.insert_resource(ServerTarget::Remote {
            uri: config.uri.clone(),
        });

        if resource_handles.is_all_done() {
            next_screen.set(Screen::Connecting);
        } else {
            next_screen.set(Screen::Loading);
        }
    }

    #[cfg(feature = "multiplayer")]
    pub fn multiplayer(
        _: On<Pointer<Click>>,
        mut mode: ResMut<GameMode>,
        mut commands: Commands,
        config: Res<crate::networking::SpacetimeDbConfig>,
        resource_handles: Res<ResourceHandles>,
        mut next_screen: ResMut<NextState<Screen>>,
    ) {
        *mode = GameMode::Multiplayer;
        commands.insert_resource(ServerTarget::Remote {
            uri: config.uri.clone(),
        });

        if resource_handles.is_all_done() {
            next_screen.set(Screen::Connecting);
        } else {
            next_screen.set(Screen::Loading);
        }
    }
}
