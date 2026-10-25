//! Audio: Bevy's `AudioPlugin` where a game is heard, its asset half everywhere else, and
//! `bevy-ts/audio`, a built-in module whose param `AudioSinks` controls a playing sound under
//! Bevy's `AudioSinkPlayback` names: `sinks.pause(e)`, `sinks.setVolume(e, 0.5)`.
//!
//! `AudioPlugin` opens the default output device when it is built, so it is added only in a
//! window, or in a viewport given `--audio` (docs/audio.md). Headless, `bevy-ts types` and every
//! test get the asset half: audio assets load and audio types resolve, and no sink is ever made.

use crate::cli::Display;
use bevy::ecs::reflect::{ReflectFromWorld, ReflectFromWorldFns};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_audio::{
    AudioLoader, AudioPlayer, AudioPlugin, AudioSink, AudioSinkPlayback, AudioSource, DefaultSpatialScale, GlobalVolume,
    SpatialAudioSink, Volume,
};
use bevy_ts_native::prelude::*;
use std::time::Duration;

/// Registers `AudioPlayer` and `bevy-ts/audio`'s param, in every mode.
pub struct ScriptAudioPlugin;

impl Plugin for ScriptAudioPlugin {
    fn build(&self, app: &mut App) {
        // Generic, so reflection's auto-registration skips it. It has no `Default`: TSX and
        // levels build it from this `FromWorld`, a default handle the `value` prop replaces.
        app.register_type::<AudioPlayer>()
            .ts_param::<AudioSinksParam<'static, 'static>>()
            .add_systems(Update, refuse_undecodable);
        let from_world = ReflectFromWorld::new(ReflectFromWorldFns { from_world: |_| Box::new(AudioPlayer::new(Handle::default())) });
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        registry.write().get_mut(std::any::TypeId::of::<AudioPlayer>()).expect("registered above").insert(from_world);
    }
}

/// Whether Bevy's decoder takes a source at `path` (its extension) holding `bytes`: an Ogg one
/// starts `OggS`, a WAV one `RIFF`..`WAVE`, and another extension decodes when Bevy's
/// `AudioLoader` lists it (a project that turns on Bevy's `mp3` or `flac`). A source with no path
/// was made by code, and is its own.
pub fn decodable(path: Option<&std::path::Path>, bytes: &[u8]) -> bool {
    use bevy::asset::AssetLoader;
    let Some(path) = path else { return true };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "ogg" | "oga" | "spx" => bytes.starts_with(b"OggS"),
        "wav" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
        e => AudioLoader.extensions().contains(&e),
    }
}

/// An `AudioPlayer`'s asset loads by type, whatever its extension, so `music.mp3` loads as bytes,
/// and Bevy's `AudioSource::decoder` panics on it when it plays (`bevy_audio/src/audio_source.rs:96-103`).
/// Before Bevy's playback (`PostUpdate`), a new source it cannot decode is removed and named in
/// the script log: Bevy's player then waits for it, as for an asset still loading.
fn refuse_undecodable(
    sources: Option<ResMut<Assets<AudioSource>>>,
    server: Res<AssetServer>,
    log: Option<ResMut<bevy_ts::log::ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut seen: Local<bevy::platform::collections::HashSet<AssetId<AudioSource>>>,
) {
    let Some(mut sources) = sources.filter(|s| s.is_changed()) else { return };
    let refused: Vec<(AssetId<AudioSource>, String)> = sources
        .iter()
        .filter(|(id, _)| !seen.contains(id))
        .filter_map(|(id, s)| {
            let path = server.get_path(id)?;
            (!decodable(Some(path.path()), &s.bytes)).then(|| (id, path.to_string()))
        })
        .collect();
    seen.extend(sources.ids());
    let mut log = log;
    for (id, path) in refused {
        sources.remove(id);
        let message = format!(
            "{path} is not Ogg Vorbis or WAV, the audio this engine decodes; it is not played (docs/audio.md says how to get mp3 and flac)"
        );
        match log.as_mut() {
            Some(log) => log.push(bevy_ts::log::LogLevel::Error, message, None, generations.as_ref().map_or(0, |g| g.live)),
            None => error!("{message}"),
        }
    }
}

/// Whether an engine of `display` plays sound: in a window, and in a viewport given `--audio`.
pub fn plays(display: Display, audio: bool) -> bool {
    match display {
        Display::Window => true,
        Display::Viewport => audio,
        Display::Headless => false,
    }
}

/// Adds Bevy's `AudioPlugin` when the engine [`plays`], else its asset half.
pub fn audio_output(app: &mut App, display: Display, audio: bool) {
    if plays(display, audio) {
        app.add_plugins(AudioPlugin::default());
    } else {
        asset_half(app);
    }
}

/// What `AudioPlugin` adds besides its output and playback systems, Bevy's calls in Bevy's order
/// (`bevy_audio/src/lib.rs:81-105`): no device is opened and no sink is made.
pub fn asset_half(app: &mut App) {
    app.init_asset::<AudioSource>()
        .init_asset_loader::<AudioLoader>()
        .insert_resource(GlobalVolume::default())
        .insert_resource(DefaultSpatialScale::default());
}

/// `AudioSinks`: every entity's `AudioSink` or `SpatialAudioSink`, by the entity. Bevy inserts a
/// sink once the `AudioPlayer`'s asset has loaded and an output device exists; until then the
/// entity reads as not playing.
#[derive(SystemParam)]
pub struct AudioSinksParam<'w, 's> {
    sinks: Query<'w, 's, (Option<&'static mut AudioSink>, Option<&'static mut SpatialAudioSink>, Has<AudioPlayer>)>,
}

fn not_a_sound(e: Entity) -> String {
    format!("entity {} has no AudioPlayer", e.to_bits())
}

/// A non-negative, finite number of seconds.
fn seconds(secs: f64) -> Result<Duration, String> {
    Duration::try_from_secs_f64(secs).map_err(|_| format!("{secs} is not a number of seconds"))
}

impl AudioSinksParam<'_, '_> {
    /// `f` on the entity's sink; `None` with an `AudioPlayer` and no sink yet.
    fn read<T>(&self, e: Entity, f: impl FnOnce(&dyn AudioSinkPlayback) -> T) -> Result<Option<T>, String> {
        let (plain, spatial, player) = self.sinks.get(e).map_err(|_| not_a_sound(e))?;
        match (plain, spatial) {
            (Some(s), _) => Ok(Some(f(s))),
            (None, Some(s)) => Ok(Some(f(s))),
            (None, None) if player => Ok(None),
            (None, None) => Err(not_a_sound(e)),
        }
    }

    /// As [`read`](Self::read), for Bevy's `&mut self` methods.
    fn write<T>(&mut self, e: Entity, f: impl FnOnce(&mut dyn AudioSinkPlayback) -> T) -> Result<Option<T>, String> {
        let (plain, spatial, player) = self.sinks.get_mut(e).map_err(|_| not_a_sound(e))?;
        match (plain, spatial) {
            (Some(mut s), _) => Ok(Some(f(&mut *s))),
            (None, Some(mut s)) => Ok(Some(f(&mut *s))),
            (None, None) if player => Ok(None),
            (None, None) => Err(not_a_sound(e)),
        }
    }

    /// The entity's spatial sink; a plain sink throws.
    fn spatial(&self, e: Entity, f: impl FnOnce(&SpatialAudioSink)) -> Result<(), String> {
        let (plain, spatial, player) = self.sinks.get(e).map_err(|_| not_a_sound(e))?;
        match (plain, spatial) {
            (_, Some(s)) => {
                f(s);
                Ok(())
            }
            (Some(_), None) => Err(format!(
                "entity {} has an AudioSink, which is not spatial: set PlaybackSettings.spatial to play it as a SpatialAudioSink",
                e.to_bits()
            )),
            (None, None) if player => Ok(()),
            (None, None) => Err(not_a_sound(e)),
        }
    }
}

#[export(ts = "AudioSinks", module = "bevy-ts/audio")]
impl AudioSinksParam<'_, '_> {
    /// The volume, linear: while muted, the volume `unmute` restores.
    pub fn volume(&self, e: Entity) -> Result<Option<f32>, String> {
        self.read(e, |s| s.volume().to_linear())
    }

    /// Sets the volume, linear (`1` is the source's own). While muted, it is kept for `unmute`.
    pub fn set_volume(&mut self, e: Entity, volume: f32) -> Result<(), String> {
        self.write(e, |s| s.set_volume(Volume::Linear(volume))).map(drop)
    }

    /// The speed: `1` is normal.
    pub fn speed(&self, e: Entity) -> Result<Option<f32>, String> {
        self.read(e, |s| s.speed())
    }

    /// Sets the speed, which changes the pitch too.
    pub fn set_speed(&mut self, e: Entity, speed: f32) -> Result<(), String> {
        self.write(e, |s| s.set_speed(speed)).map(drop)
    }

    /// Resumes a paused sound.
    pub fn play(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.play()).map(drop)
    }

    /// Pauses the sound; `play` resumes it.
    pub fn pause(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.pause()).map(drop)
    }

    /// Pauses a playing sound, or resumes a paused one.
    pub fn toggle_playback(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.toggle_playback()).map(drop)
    }

    /// Whether the sound is paused.
    pub fn is_paused(&self, e: Entity) -> Result<Option<bool>, String> {
        self.read(e, |s| s.is_paused())
    }

    /// Seconds played, after any speed change.
    pub fn position(&self, e: Entity) -> Result<Option<f64>, String> {
        self.read(e, |s| s.position().as_secs_f64())
    }

    /// Seeks to `seconds` into the source; throws Bevy's `SeekError` when it cannot.
    pub fn try_seek(&mut self, e: Entity, seconds: f64) -> Result<(), String> {
        let at = self::seconds(seconds)?;
        self.write(e, |s| s.try_seek(at).map_err(|err| err.to_string()))?.unwrap_or(Ok(()))
    }

    /// Stops the sound for good: it cannot be played again.
    pub fn stop(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.stop()).map(drop)
    }

    /// Whether the sink has nothing left to play.
    pub fn empty(&self, e: Entity) -> Result<Option<bool>, String> {
        self.read(e, |s| s.empty())
    }

    /// Whether the sound is muted.
    pub fn is_muted(&self, e: Entity) -> Result<Option<bool>, String> {
        self.read(e, |s| s.is_muted())
    }

    /// Mutes the sound, keeping its volume for `unmute`.
    pub fn mute(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.mute()).map(drop)
    }

    /// Restores the volume the sound had when muted.
    pub fn unmute(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.unmute()).map(drop)
    }

    /// Mutes a sound, or unmutes a muted one.
    pub fn toggle_mute(&mut self, e: Entity) -> Result<(), String> {
        self.write(e, |s| s.toggle_mute()).map(drop)
    }

    /// A spatial sound's two ears, in world space. A plain sink throws.
    pub fn set_ears_position(&self, e: Entity, left: Vec3, right: Vec3) -> Result<(), String> {
        self.spatial(e, |s| s.set_ears_position(left, right))
    }

    /// A spatial sound's listener: an ear on each side of `translation`, `gap` apart, facing by
    /// `rotation`. A plain sink throws.
    pub fn set_listener_position(&self, e: Entity, translation: Vec3, rotation: Quat, gap: f32) -> Result<(), String> {
        self.spatial(e, |s| s.set_listener_position(Transform { translation, rotation, ..default() }, gap))
    }

    /// A spatial sound's emitter, in world space. A plain sink throws.
    pub fn set_emitter_position(&self, e: Entity, position: Vec3) -> Result<(), String> {
        self.spatial(e, |s| s.set_emitter_position(position))
    }
}
