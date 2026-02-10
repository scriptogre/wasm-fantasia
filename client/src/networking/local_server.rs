//! Local SpacetimeDB subprocess manager for native singleplayer.
//!
//! Starts a SpacetimeDB instance on localhost, deploys the game module,
//! and exposes the connection URI. The subprocess is shut down when the
//! [`LocalServer`] resource is removed or the app exits.

use bevy::prelude::*;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::models::Screen;

// =============================================================================
// Resources
// =============================================================================

/// Handle to the running SpacetimeDB subprocess.
#[derive(Resource)]
pub struct LocalServer {
    process: Option<Child>,
    pub port: u16,
    spacetime_binary: PathBuf,
    data_dir: Option<PathBuf>,
}

/// Progress of the local server lifecycle.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum LocalServerState {
    Starting,
    WaitingForReady,
    Deploying,
    Ready,
    Failed(String),
}

// =============================================================================
// Plugin
// =============================================================================

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Loading), prewarm_local_server)
        .add_systems(OnExit(Screen::Gameplay), shutdown_local_server);
}

/// Spawn the local SpacetimeDB process during loading so it has a head
/// start booting by the time the player clicks Singleplayer. The process
/// runs independently — the Connecting screen's `advance_local_server`
/// detects when it's ready and triggers the deploy step.
fn prewarm_local_server(mut commands: Commands) {
    let (server, state) = start();
    info!("Prewarming local SpacetimeDB on port {}", server.port);
    commands.insert_resource(server);
    commands.insert_resource(state);
}

// =============================================================================
// Binary discovery
// =============================================================================

/// Find the `spacetime` CLI binary.
///
/// Search order:
/// 1. Adjacent to the game executable (bundled distribution)
/// 2. `SPACETIMEDB_PATH` environment variable
/// 3. `~/.local/bin/spacetime` (default install location)
/// 4. System PATH via `which`
fn find_spacetime_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let adjacent = dir.join("spacetime");
            if adjacent.exists() {
                return Some(adjacent);
            }
        }
    }

    if let Ok(path) = std::env::var("SPACETIMEDB_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    if let Some(home) = home::home_dir() {
        let default_path = home.join(".local/bin/spacetime");
        if default_path.exists() {
            return Some(default_path);
        }
    }

    if let Ok(output) = Command::new("which").arg("spacetime").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    None
}

/// Pick a random available port by binding to :0 and reading the assigned port.
fn pick_available_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

// =============================================================================
// Lifecycle
// =============================================================================

/// Start a local SpacetimeDB subprocess.
///
/// Returns `(LocalServer, LocalServerState)` to be inserted as resources.
/// The caller should call [`advance`] each frame to drive the state machine.
pub fn start() -> (LocalServer, LocalServerState) {
    let Some(binary) = find_spacetime_binary() else {
        return (
            LocalServer {
                process: None,
                port: 0,
                spacetime_binary: PathBuf::new(),
                data_dir: None,
            },
            LocalServerState::Failed(
                "SpacetimeDB CLI not found. Install from https://install.spacetimedb.com \
                 or set SPACETIMEDB_PATH."
                    .to_string(),
            ),
        );
    };

    let Some(port) = pick_available_port() else {
        return (
            LocalServer {
                process: None,
                port: 0,
                spacetime_binary: binary,
                data_dir: None,
            },
            LocalServerState::Failed("Could not find an available port.".to_string()),
        );
    };

    // Use a unique temp data directory so the pid file doesn't conflict
    // with any other running SpacetimeDB instance.
    let data_dir = std::env::temp_dir().join(format!("spacetimedb-wf-{port}"));
    let _ = std::fs::create_dir_all(&data_dir);

    let listen_addr = format!("127.0.0.1:{port}");
    info!("Starting local SpacetimeDB on {listen_addr}");

    let data_dir_str = data_dir.to_string_lossy().to_string();
    let result = Command::new(&binary)
        .args([
            "start",
            "--listen-addr",
            &listen_addr,
            "--in-memory",
            "--data-dir",
            &data_dir_str,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match result {
        Ok(child) => (
            LocalServer {
                process: Some(child),
                port,
                spacetime_binary: binary,
                data_dir: Some(data_dir),
            },
            LocalServerState::Starting,
        ),
        Err(e) => (
            LocalServer {
                process: None,
                port,
                spacetime_binary: binary,
                data_dir: Some(data_dir),
            },
            LocalServerState::Failed(format!("Failed to start SpacetimeDB: {e}")),
        ),
    }
}

/// Drive the local server state machine forward.
///
/// Call each frame while state is not `Ready` or `Failed`.
/// Returns `true` when the state changed.
pub fn advance(server: &mut LocalServer, state: &mut LocalServerState) -> bool {
    match state {
        LocalServerState::Starting | LocalServerState::WaitingForReady => {
            // Port occupied = server is listening
            if TcpListener::bind(format!("127.0.0.1:{}", server.port)).is_err() {
                info!("Local SpacetimeDB listening on port {}", server.port);
                *state = LocalServerState::Deploying;
                return true;
            }

            // Check for premature exit
            if let Some(ref mut child) = server.process {
                if let Ok(Some(status)) = child.try_wait() {
                    let stderr = child
                        .stderr
                        .as_mut()
                        .and_then(|s| {
                            use std::io::Read;
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok().map(|_| buf)
                        })
                        .unwrap_or_default();
                    let detail = if stderr.is_empty() {
                        format!("exit status: {status}")
                    } else {
                        format!("exit status: {status}\n{stderr}")
                    };
                    *state = LocalServerState::Failed(format!(
                        "SpacetimeDB exited prematurely: {detail}"
                    ));
                    return true;
                }
            }

            if *state == LocalServerState::Starting {
                *state = LocalServerState::WaitingForReady;
                return true;
            }

            false
        }

        LocalServerState::Deploying => {
            let listen_addr = format!("127.0.0.1:{}", server.port);
            info!("Deploying game module to local server at {listen_addr}");

            // Use pre-compiled WASM if adjacent to the executable, otherwise
            // fall back to --project-path for dev workflow.
            let bin_path = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.join("wasm_fantasia_module.wasm")))
                .filter(|p| p.exists());

            let mut cmd = Command::new(&server.spacetime_binary);
            cmd.args([
                "publish",
                "wasm-fantasia",
                "--yes",
                "--delete-data",
                "-s",
                &format!("http://{listen_addr}"),
            ]);
            if let Some(ref wasm_path) = bin_path {
                cmd.args(["--bin-path", &wasm_path.to_string_lossy()]);
            } else {
                cmd.args(["--project-path", "server"]);
            }

            let result = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output();

            match result {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    info!("Module deployed successfully: {stdout}");
                    *state = LocalServerState::Ready;
                    true
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    *state = LocalServerState::Failed(format!(
                        "Module deploy failed:\nstdout: {stdout}\nstderr: {stderr}"
                    ));
                    true
                }
                Err(e) => {
                    *state =
                        LocalServerState::Failed(format!("Failed to run spacetime publish: {e}"));
                    true
                }
            }
        }

        LocalServerState::Ready | LocalServerState::Failed(_) => false,
    }
}

/// Returns the WebSocket URI for connecting to the local server.
pub fn connection_uri(server: &LocalServer) -> String {
    format!("ws://127.0.0.1:{}", server.port)
}

// =============================================================================
// Shutdown
// =============================================================================

fn shutdown(server: &mut LocalServer) {
    if let Some(ref mut child) = server.process {
        info!("Shutting down local SpacetimeDB (pid {})", child.id());
        let _ = child.kill();
        match child.wait() {
            Ok(status) => info!("Local SpacetimeDB exited with status: {status}"),
            Err(e) => warn!("Error waiting for SpacetimeDB to exit: {e}"),
        }
        server.process = None;
    }
    if let Some(ref dir) = server.data_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn shutdown_local_server(mut server: Option<ResMut<LocalServer>>, mut commands: Commands) {
    if let Some(ref mut server) = server {
        shutdown(server);
        commands.remove_resource::<LocalServer>();
        commands.remove_resource::<LocalServerState>();
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        if self.process.is_some() {
            shutdown(self);
        }
    }
}
