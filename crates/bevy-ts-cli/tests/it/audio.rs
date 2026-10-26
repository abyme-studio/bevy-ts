//! Change `script-audio`: the engine's audio by display, Ogg and WAV decoding headless,
//! `AudioPlayer` and `PlaybackSettings` from TSX and from a script's commands, `GlobalVolume` and
//! `DefaultSpatialScale` from TS, and `bevy-ts/audio`'s `AudioSinks` on sinks that have no device.
//! Nothing here opens an audio device: no test builds `AudioPlugin`.

use crate::common;

use bevy::prelude::*;
use bevy_audio::{
    AudioPlayer, AudioPlugin, AudioSink, AudioSinkPlayback, AudioSource, Decodable, DefaultSpatialScale, GlobalVolume, PlaybackMode,
    PlaybackSettings, SpatialAudioSink, SpatialScale, Volume,
};
use bevy_ts::generation::ReloadStatus;
use bevy_ts_cli::audio::{asset_half, audio_output, decodable, plays};
use bevy_ts_cli::cli::{Display, Engine, base_app};
use common::{messages, spawn_ts};
use rodio::Source;
use rodio::queue::SourcesQueueOutput;
use serde_json::json;
use std::num::NonZero;
use std::path::Path;
use std::time::{Duration, Instant};

fn engine(display: Display, assets: Option<&Path>) -> App {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    base_app(&engine, bevy_ts_cli::Prebuilt, display, Default::default(), assets, false, Default::default(), &[], false)
}

const TONE_OGG: &[u8] = include_bytes!("fixtures/tone.ogg");

/// A mono 16-bit PCM WAV at 44.1 kHz of `frames` samples, written by hand.
fn wav(frames: u32) -> Vec<u8> {
    let data = frames * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&44_100u32.to_le_bytes());
    b.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        b.extend_from_slice(&(((i as f32 / 100.0).sin() * 8000.0) as i16).to_le_bytes());
    }
    b
}

#[test]
fn a_headless_engine_loads_audio_and_names_audio_player() {
    let app = engine(Display::Headless, None);
    assert!(!app.is_plugin_added::<AudioPlugin>());
    let server = app.world().resource::<AssetServer>().clone();
    for ext in ["ogg", "oga", "spx", "wav"] {
        assert!(bevy::tasks::block_on(server.get_asset_loader_with_extension(ext)).is_ok(), "no loader for {ext}");
    }
    for ext in ["mp3", "flac"] {
        assert!(bevy::tasks::block_on(server.get_asset_loader_with_extension(ext)).is_err(), "{ext} is compiled in");
    }
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(registry.get_with_type_path("bevy_audio::audio::AudioPlayer<bevy_audio::audio_source::AudioSource>").is_some(), "AudioPlayer is not registered");
    drop(registry);
    assert!(app.world().contains_resource::<bevy_audio::GlobalVolume>());
    assert!(app.world().contains_resource::<bevy_audio::DefaultSpatialScale>());
}

/// The rule: a window plays, a viewport only with `--audio`, headless never. Checked on the rule,
/// since an app that plays would open the device.
#[test]
fn only_a_window_or_a_viewport_given_audio_plays() {
    assert!(plays(Display::Window, false));
    assert!(plays(Display::Viewport, true));
    assert!(!plays(Display::Viewport, false));
    assert!(!plays(Display::Headless, true));
    for (display, audio) in [(Display::Headless, false), (Display::Headless, true), (Display::Viewport, false)] {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        audio_output(&mut app, display, audio);
        assert!(!app.is_plugin_added::<AudioPlugin>(), "{display:?} audio={audio}");
        assert!(app.world().contains_resource::<Assets<AudioSource>>());
    }
}

/// Updates until `handle` has loaded or failed, within a deadline.
fn loaded(app: &mut App, handle: &Handle<AudioSource>) -> bool {
    let deadline = Instant::now() + common::BOUND;
    loop {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if server.is_loaded(handle) {
            return true;
        }
        if server.load_state(handle).is_failed() {
            return false;
        }
        assert!(Instant::now() < deadline, "{:?} still loading", handle.path());
    }
}

#[test]
fn ogg_and_wav_decode_headless() {
    let dir = common::project("audio-decode", &[]);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets/tone.ogg"), TONE_OGG).unwrap();
    std::fs::write(dir.join("assets/tone.wav"), wav(4410)).unwrap();
    let mut app = engine(Display::Headless, Some(&dir.join("assets")));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    // The fixture: 0.5 s of stereo at 22.05 kHz; the WAV: 0.1 s of mono at 44.1 kHz.
    for (file, rate, channels, frames) in [("tone.ogg", 22_050, 2, 11_025), ("tone.wav", 44_100, 1, 4410)] {
        let handle = server.load::<AudioSource>(file);
        assert!(loaded(&mut app, &handle), "{file} failed to load");
        let source = app.world().resource::<Assets<AudioSource>>().get(&handle).expect(file).clone();
        let decoder = source.decoder();
        assert_eq!((decoder.sample_rate().get(), decoder.channels().get()), (rate, channels), "{file}");
        let samples = decoder.count();
        // Vorbis frames pad the end a little; WAV is exact.
        assert!(samples.abs_diff(frames * channels as usize) <= 2048, "{file}: {samples} samples");
    }
}

/// An `AudioPlayer`'s asset loads by type, so an mp3 is read as bytes, and Bevy's decoder would
/// panic on it when it plays. The engine removes it and names it instead.
#[test]
fn an_mp3_is_refused_by_name() {
    assert!(decodable(Some(Path::new("a.ogg")), TONE_OGG) && decodable(Some(Path::new("a.WAV")), &wav(4)));
    assert!(!decodable(Some(Path::new("a.ogg")), &wav(4)) && !decodable(Some(Path::new("a.flac")), b"fLaC"));
    assert!(decodable(None, b"made by code"));
    let dir = common::project("audio-mp3", &[]);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    // An MPEG frame header, as an mp3 starts.
    std::fs::write(dir.join("assets/music.mp3"), [0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0]).unwrap();
    let mut app = engine(Display::Headless, Some(&dir.join("assets")));
    app.finish();
    app.cleanup();
    let handle = app.world().resource::<AssetServer>().load::<AudioSource>("music.mp3");
    assert!(loaded(&mut app, &handle), "the bytes load: Bevy picks AudioLoader by type");
    app.update();
    assert!(app.world().resource::<Assets<AudioSource>>().get(&handle).is_none(), "an undecodable source is removed");
    let refused: Vec<String> = messages(&app).into_iter().filter(|m| m.contains("music.mp3")).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].contains("Ogg Vorbis or WAV") && refused[0].contains("docs/audio.md"), "{}", refused[0]);
}

#[test]
fn a_reloaded_source_is_checked_again() {
    let dir = common::project("audio-reload", &[]);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets/tone.ogg"), TONE_OGG).unwrap();
    let mut app = engine(Display::Headless, Some(&dir.join("assets")));
    app.finish();
    app.cleanup();
    let handle = app.world().resource::<AssetServer>().load::<AudioSource>("tone.ogg");
    assert!(loaded(&mut app, &handle));
    app.update();
    assert!(app.world().resource::<Assets<AudioSource>>().get(&handle).is_some(), "an Ogg source stays");
    // A hot reload keeps the id: an mp3 saved over tone.ogg is a `Modified` source.
    let mp3: std::sync::Arc<[u8]> = std::sync::Arc::from(&[0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0][..]);
    app.world_mut().resource_mut::<Assets<AudioSource>>().insert(handle.id(), AudioSource { bytes: mp3 }).unwrap();
    app.update();
    assert!(app.world().resource::<Assets<AudioSource>>().get(&handle).is_none(), "the reloaded source is removed");
    let refused: Vec<String> = messages(&app).into_iter().filter(|m| m.contains("tone.ogg")).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
}

const SCENE: &str = r#"import { app } from "bevy-ts";
import { AudioPlayer, PlaybackSettings } from "bevy-ts/bevy";

app.scene("music", () => (
  <entity name="music">
    <AudioPlayer value="tone.ogg" />
    <PlaybackSettings mode="Loop" volume={{ Decibels: -6 }} start_position={0.5} />
  </entity>
));
"#;

#[test]
fn tsx_writes_audio_player_and_playback_settings() {
    let dir = common::project("audio-tsx", &[("game.tsx", SCENE)]);
    let mut app = common::app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default());
    asset_half(&mut app);
    app.add_plugins(bevy_ts_cli::audio::ScriptAudioPlugin);
    let r = common::reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let log = common::messages(&app);
    let world = app.world_mut();
    let found: Vec<_> = world.query::<(&Name, Has<AudioPlayer>, Has<PlaybackSettings>)>().iter(world).map(|(n, p, s)| (n.to_string(), p, s)).collect();
    let (player, settings) =
        world.query::<(&AudioPlayer, &PlaybackSettings)>().single(world).unwrap_or_else(|e| panic!("one sound: {e}; {found:?}; {log:?}"));
    assert_eq!(player.0.path().map(|p| p.to_string()).as_deref(), Some("tone.ogg"));
    assert!(matches!(settings.mode, PlaybackMode::Loop), "{:?}", settings.mode);
    assert!(matches!(settings.volume, Volume::Decibels(db) if db == -6.0), "{:?}", settings.volume);
    assert_eq!(settings.start_position, Some(Duration::from_millis(500)));
    assert_eq!(settings.duration, None);
}

const SPAWN: &str = r#"import { app, Startup } from "bevy-ts";
import type { Commands } from "bevy-ts";
import { AudioPlayer, PlaybackSettings } from "bevy-ts/bevy";

export function hit(commands: Commands) {
  commands.spawn(AudioPlayer.of("tone.ogg"), PlaybackSettings.of({ mode: "Despawn", volume: { Linear: 0.5 }, spatial_scale: [2, 2, 2], start_position: 0.25 }));
}
app.addSystems(Startup, hit);
"#;

#[test]
fn a_script_spawns_audio_player_and_playback_settings() {
    let dir = common::project("audio-spawn", &[("game.ts", SPAWN)]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(AssetPlugin::default());
    asset_half(&mut app);
    app.add_plugins(bevy_ts_cli::audio::ScriptAudioPlugin);
    let r = common::reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    common::updates(&mut app, 2);
    let log = common::messages(&app);
    let world = app.world_mut();
    let (player, settings) = world.query::<(&AudioPlayer, &PlaybackSettings)>().single(world).unwrap_or_else(|e| panic!("one sound: {e}; {log:?}"));
    assert_eq!(player.0.path().map(|p| p.to_string()).as_deref(), Some("tone.ogg"));
    assert!(matches!(settings.mode, PlaybackMode::Despawn), "{:?}", settings.mode);
    assert_eq!(settings.volume, Volume::Linear(0.5));
    assert_eq!(settings.spatial_scale.map(|s| s.0), Some(Vec3::splat(2.0)));
    assert_eq!(settings.start_position, Some(Duration::from_millis(250)));
    assert_eq!(settings.duration, None);
}

const RESOURCES: &str = r#"import { app, Update } from "bevy-ts";
import type { Res, ResMut } from "bevy-ts";
import { DefaultSpatialScale, GlobalVolume } from "bevy-ts/bevy";

export function volume(global: ResMut<GlobalVolume>, scale: Res<DefaultSpatialScale>) {
  console.log(`scale ${JSON.stringify(scale.value)} volume ${JSON.stringify(global.volume)}`);
  if (!("Linear" in global.volume && global.volume.Linear === 0.25)) global.volume = { Linear: 0.25 };
}
app.addSystems(Update, volume);
"#;

#[test]
fn global_volume_and_default_spatial_scale_from_ts() {
    let dir = common::project("audio-resources", &[("game.ts", RESOURCES)]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(AssetPlugin::default());
    asset_half(&mut app);
    app.add_plugins(bevy_ts_cli::audio::ScriptAudioPlugin);
    app.insert_resource(DefaultSpatialScale(SpatialScale::new(0.5)));
    let r = common::reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    frame(&mut app);
    assert_eq!(app.world().resource::<GlobalVolume>().volume, Volume::Linear(0.25));
    frame(&mut app);
    assert_eq!(line(&app, "scale "), "scale [0.5,0.5,0.5] volume {\"Linear\":0.25}");
    assert!(common::messages(&app).iter().all(|m| !m.contains("threw")), "{:?}", common::messages(&app));
}

const SINKS: &str = r#"import { app, Component, Update } from "bevy-ts";
import type { Entity, Query, With } from "bevy-ts";
import type { AudioSinks } from "bevy-ts/audio";

export class Plain extends Component { static path = "game::Plain" }
export class Spatial extends Component { static path = "game::Spatial" }
export class Seeker extends Component { static path = "game::Seeker" }
export class Loading extends Component { static path = "game::Loading" }
export class Silent extends Component { static path = "game::Silent" }

const first = (q: Query<[Entity], With<any>>): Entity => {
  let found: Entity | undefined;
  q.each((e) => { found = e; });
  return found!;
};
const attempt = (f: () => unknown): string => {
  try { return `ok ${f()}`; } catch (e) { return `threw ${(e as Error).message}`; }
};

let step = 0;
export function drive(sinks: AudioSinks, plains: Query<[Entity], With<Plain>>, spatials: Query<[Entity], With<Spatial>>,
  seekers: Query<[Entity], With<Seeker>>, loadings: Query<[Entity], With<Loading>>, silents: Query<[Entity], With<Silent>>) {
  const p = first(plains);
  switch (step++) {
    case 0:
      sinks.setVolume(p, 0.5);
      sinks.mute(p);
      sinks.setSpeed(p, 2);
      sinks.pause(p);
      console.log(`s0 ${sinks.isMuted(p)} ${sinks.volume(p)} ${sinks.speed(p)} ${sinks.isPaused(p)}`);
      break;
    case 1:
      sinks.togglePlayback(p);
      sinks.toggleMute(p);
      console.log(`s1 ${sinks.isPaused(p)} ${sinks.isMuted(p)} ${sinks.volume(p)}`);
      break;
    case 2:
      sinks.pause(p);
      sinks.play(p);
      sinks.mute(p);
      sinks.unmute(p);
      sinks.setSpeed(p, 1);
      console.log(`s2 ${sinks.isPaused(p)} ${sinks.isMuted(p)} ${sinks.volume(p)} ${sinks.speed(p)} ${sinks.empty(p)}`);
      break;
    case 3: {
      const l = first(loadings), s = first(silents), sp = first(spatials), k = first(seekers);
      console.log(`s3 position ${sinks.position(p)}`);
      console.log(`s3 loading ${sinks.isPaused(l)} ${sinks.volume(l)} ${attempt(() => sinks.pause(l))} ${attempt(() => sinks.setEmitterPosition(l, [1, 0, 0]))}`);
      console.log(`s3 silent ${s} ${attempt(() => sinks.pause(s))}`);
      console.log(`s3 spatial-set ${attempt(() => sinks.setEmitterPosition(sp, [1, 0, 0]))} ${attempt(() => sinks.setEarsPosition(sp, [-0.1, 0, 0], [0.1, 0, 0]))} ${attempt(() => sinks.setListenerPosition(sp, [0, 0, 0], [0, 0, 0, 1], 0.3))}`);
      sinks.setVolume(sp, 0.25);
      console.log(`s3 spatial volume ${sinks.volume(sp)}`);
      console.log(`s3 plain-ears ${attempt(() => sinks.setEmitterPosition(p, [1, 0, 0]))}`);
      console.log(`s3 seek ${attempt(() => sinks.trySeek(k, 0.1))}`);
      console.log(`s3 seek-negative ${attempt(() => sinks.trySeek(p, -1))}`);
      console.log(`s3 bad-volume ${attempt(() => sinks.setVolume(p, NaN))} ${attempt(() => sinks.setVolume(p, -0.5))}`);
      console.log(`s3 bad-speed ${attempt(() => sinks.setSpeed(p, 0))} ${sinks.volume(p)} ${sinks.speed(p)}`);
      sinks.stop(p);
      break;
    }
    case 4:
      console.log(`s4 ${sinks.empty(p)}`);
      break;
  }
}
app.addSystems(Update, drive);
"#;

/// A source that cannot seek: rodio's default `try_seek`.
struct Unseekable(u32);

impl Iterator for Unseekable {
    type Item = rodio::Sample;
    fn next(&mut self) -> Option<Self::Item> {
        self.0 = self.0.checked_sub(1)?;
        Some(0.0)
    }
}

impl Source for Unseekable {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(44_100).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// A sound whose asset has no path: the sinks below are the test's.
fn sound() -> AudioPlayer {
    AudioPlayer(Handle::default())
}

/// One frame, console output pumped into the log.
fn frame(app: &mut App) {
    app.update();
    bevy_ts::runtime::pump(app.world_mut());
}

fn line(app: &App, prefix: &str) -> String {
    messages(app).into_iter().rfind(|m| m.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix:?} in {:?}", messages(app)))
}

fn pull(out: &mut SourcesQueueOutput, n: usize) {
    for _ in 0..n {
        out.next();
    }
}

#[test]
fn audio_sinks_control_sinks_with_no_device() {
    let dir = common::project("audio-sinks", &[("game.ts", SINKS)]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(AssetPlugin::default());
    asset_half(&mut app);
    app.add_plugins(bevy_ts_cli::audio::ScriptAudioPlugin);
    let r = common::reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);

    // A plain sink playing one second of 44.1 kHz mono, pulled by the test as a device would.
    let (player, mut out) = rodio::Player::new();
    player.append(rodio::buffer::SamplesBuffer::new(NonZero::new(1).unwrap(), NonZero::new(44_100).unwrap(), vec![0.1; 44_100]));
    let plain = spawn_ts(&mut app, "game::Plain", json!({}), (sound(), AudioSink::new(player)));
    // A spatial sink, through a mixer with no device.
    let (mixer, _mixed) = rodio::mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(44_100).unwrap());
    let spatial = rodio::SpatialPlayer::connect_new(&mixer, [0.0; 3], [-0.1, 0.0, 0.0], [0.1, 0.0, 0.0]);
    let spatial = spawn_ts(&mut app, "game::Spatial", json!({}), (sound(), SpatialAudioSink::new(spatial)));
    // A sink whose source cannot seek; a seek waits for the source to be pulled, so a thread pulls it.
    let (seeker, mut seeker_out) = rodio::Player::new();
    seeker.append(Unseekable(u32::MAX));
    spawn_ts(&mut app, "game::Seeker", json!({}), (sound(), AudioSink::new(seeker)));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let puller = {
        let done = done.clone();
        std::thread::spawn(move || {
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                pull(&mut seeker_out, 64);
            }
        })
    };
    // Still loading: an AudioPlayer and no sink. And an entity that is not a sound.
    spawn_ts(&mut app, "game::Loading", json!({}), sound());
    let silent = spawn_ts(&mut app, "game::Silent", json!({}), ());

    frame(&mut app);
    assert_eq!(line(&app, "s0"), "s0 true 0.5 2 true");
    let sink = app.world().get::<AudioSink>(plain).unwrap();
    assert!(sink.is_muted() && sink.is_paused() && sink.speed() == 2.0);
    assert_eq!(sink.volume(), Volume::Linear(0.5), "mute keeps the volume");

    frame(&mut app);
    assert_eq!(line(&app, "s1"), "s1 false false 0.5");

    frame(&mut app);
    assert_eq!(line(&app, "s2"), "s2 false false 0.5 1 false");
    let sink = app.world().get::<AudioSink>(plain).unwrap();
    assert!(!sink.is_muted() && !sink.is_paused() && sink.speed() == 1.0);

    pull(&mut out, 22_050);
    frame(&mut app);
    let position: f64 = line(&app, "s3 position ").trim_start_matches("s3 position ").parse().unwrap();
    // rodio updates the position every 5 ms of samples.
    assert!((position - 0.5).abs() <= 0.006, "position {position}");
    assert_eq!(line(&app, "s3 loading"), "s3 loading undefined undefined ok undefined ok undefined");
    let s = line(&app, "s3 silent");
    let bits = silent.to_bits().to_string();
    assert!(s.starts_with(&format!("s3 silent {bits} threw ")), "{s}");
    for needle in ["AudioSinks", "pause", &bits, "no AudioPlayer"] {
        assert!(s.contains(needle), "{needle} in {s}");
    }
    assert_eq!(line(&app, "s3 spatial-set"), "s3 spatial-set ok undefined ok undefined ok undefined");
    assert_eq!(line(&app, "s3 spatial volume"), "s3 spatial volume 0.25");
    assert_eq!(app.world().get::<SpatialAudioSink>(spatial).unwrap().volume(), Volume::Linear(0.25));
    let ears = line(&app, "s3 plain-ears");
    for needle in ["threw", "AudioSinks", "setEmitterPosition", "not spatial"] {
        assert!(ears.contains(needle), "{needle} in {ears}");
    }
    let seek = line(&app, "s3 seek ");
    assert!(seek.contains("threw") && seek.contains("trySeek") && seek.to_lowercase().contains("not supported"), "{seek}");
    let negative = line(&app, "s3 seek-negative");
    assert!(negative.contains("threw") && negative.contains("-1 is not a number of seconds"), "{negative}");
    let volume = line(&app, "s3 bad-volume");
    assert!(volume.contains("threw") && volume.contains("NaN is not a volume") && volume.contains("-0.5 is not a volume"), "{volume}");
    let speed = line(&app, "s3 bad-speed");
    assert!(speed.contains("0 is not a speed"), "{speed}");
    assert!(speed.ends_with(" 0.5 1"), "a refused value leaves the sink as it was: {speed}");

    // `stop` ends the sound once the device next pulls.
    pull(&mut out, 1000);
    frame(&mut app);
    assert_eq!(line(&app, "s4"), "s4 true");
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    puller.join().unwrap();
}
