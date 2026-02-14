use crate::models::{Session, Settings, VenomSpeak};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_seedling::prelude::*;

/// Path inside assets/ for the processed Venom voice.
const VENOM_VOICE_ASSET: &str = "audio/sfx/venom_voice.wav";
/// Absolute path where the pipeline writes the processed output.
const VENOM_VOICE_ABS: &str = "client/assets/audio/sfx/venom_voice.wav";

pub fn plugin(app: &mut App) {
    app.init_resource::<VenomVoiceState>()
        .add_observer(on_venom_speak);

    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Update, poll_tts_task);
}

#[derive(Resource, Default)]
struct VenomVoiceState {
    cached_sample: Option<Handle<AudioSample>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_task: Option<bevy::tasks::Task<bool>>,
}

fn on_venom_speak(
    _on: On<Start<VenomSpeak>>,
    state: Res<Session>,
    settings: Res<Settings>,
    mut voice_state: ResMut<VenomVoiceState>,
    mut commands: Commands,
    #[cfg(target_arch = "wasm32")] asset_server: Res<AssetServer>,
) {
    if state.muted {
        return;
    }

    if let Some(ref handle) = voice_state.cached_sample {
        play_venom_voice(&mut commands, handle, &settings);
        return;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if voice_state.pending_task.is_some() {
            return;
        }

        let task = bevy::tasks::IoTaskPool::get().spawn(async {
            generate_venom_voice()
        });
        voice_state.pending_task = Some(task);
        info!("Generating Venom voice (Kokoro → rubberband → ffmpeg)...");
    }

    #[cfg(target_arch = "wasm32")]
    {
        let handle = asset_server.load("audio/sfx/venom_voice.ogg");
        voice_state.cached_sample = Some(handle.clone());
        play_venom_voice(&mut commands, &handle, &settings);
    }
}

/// Native Venom voice pipeline:
/// 1. Kokoro TTS (expressive neural voice) → clean WAV
/// 2. rubberband (formant-preserving pitch shift, -4 semitones) → pitched WAV
/// 3. ffmpeg (chorus/slime, sub-bass, crystalizer, compression) → final WAV
#[cfg(not(target_arch = "wasm32"))]
fn generate_venom_voice() -> bool {
    let tts_out = "/tmp/venom_tts.wav";
    let pitched_out = "/tmp/venom_pitched.wav";

    // Step 1: Kokoro TTS — expressive neural voice with breathiness
    let tts = std::process::Command::new(expand_home("~/.venom_tts/bin/python"))
        .args(["tts.py", "We are Venom", tts_out])
        .output();

    match &tts {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!(
                "Kokoro TTS failed: {}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
            );
            return false;
        }
        Err(e) => {
            eprintln!("Failed to run Kokoro TTS: {e}");
            return false;
        }
    }

    // Step 2: rubberband — formant-preserving pitch shift
    // --pitch -7: drop 7 semitones (ratio ~0.67, in the movie's -7 to -9 range)
    // -F: preserve formants (the secret sauce — throat stays human-sized)
    let rb = std::process::Command::new("rubberband")
        .args(["--pitch", "-7", "-F", tts_out, pitched_out])
        .output();

    match &rb {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!("rubberband failed: {}", String::from_utf8_lossy(&o.stderr));
            return false;
        }
        Err(e) => {
            eprintln!("Failed to run rubberband: {e}");
            return false;
        }
    }

    // Step 3: ffmpeg — symbiote effect chain
    //
    // Three parallel tracks mixed together:
    // [bite]  — crystalizer restores high-freq clarity lost in pitch shift
    // [slime] — tight chorus (15/25ms) for wet, alien, biological texture
    // [sub]   — synthesized sub-harmonics + bass boost for chest rumble
    //
    // compand — extreme compression: quiet breaths forced up to -20dB,
    //           creates claustrophobic "voice inside your head" effect
    // alimiter — prevents clipping from the aggressive processing
    let filter = concat!(
        "[0:a]asplit=3[dry][wet1][wet2];",
        "[wet1]chorus=0.5:0.9:15|25:0.4|0.3:0.25|0.4:2|2.3[slime];",
        "[wet2]asubboost=dry=0.1:wet=0.8:feedback=0.5:cutoff=150,",
        "bass=g=5:f=100[sub];",
        "[dry]crystalizer=i=2.0[bite];",
        "[bite][slime][sub]amix=inputs=3:weights=1 0.8 0.6:dropout_transition=2[mixed];",
        "[mixed]compand=attacks=0:decays=0.2:",
        "points=-80/-80|-50/-20|-10/-3|0/-3:soft-knee=3,",
        "alimiter=limit=-1dB",
    );

    let ffmpeg = std::process::Command::new("ffmpeg")
        .args(["-y", "-i", pitched_out, "-filter_complex", filter, VENOM_VOICE_ABS])
        .output();

    match ffmpeg {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            eprintln!("ffmpeg failed: {}", String::from_utf8_lossy(&o.stderr));
            false
        }
        Err(e) => {
            eprintln!("Failed to run ffmpeg: {e}");
            false
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn poll_tts_task(
    mut voice_state: ResMut<VenomVoiceState>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    state: Res<Session>,
    settings: Res<Settings>,
) {
    let Some(ref mut task) = voice_state.pending_task else {
        return;
    };

    let Some(success) = bevy::tasks::block_on(bevy::tasks::poll_once(task)) else {
        return;
    };

    voice_state.pending_task = None;

    if !success {
        warn!("Venom voice generation failed");
        return;
    }

    let handle = asset_server.load(VENOM_VOICE_ASSET);
    voice_state.cached_sample = Some(handle.clone());

    info!("Venom voice ready");
    if !state.muted {
        play_venom_voice(&mut commands, &handle, &settings);
    }
}

fn play_venom_voice(commands: &mut Commands, handle: &Handle<AudioSample>, settings: &Settings) {
    let Volume::Linear(sfx_vol) = settings.sfx() else {
        return;
    };

    // All layering/effects baked in by the offline pipeline
    commands.spawn(
        SamplePlayer::new(handle.clone()).with_volume(Volume::Linear(sfx_vol)),
    );
}
