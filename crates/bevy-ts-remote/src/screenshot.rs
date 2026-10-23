//! Screenshots: the one owner of what a shot captures, how it is taken and what it answers.
//! `bevy-ts/screenshot` and `BEVY_TS_SCREENSHOT` both take theirs with [`capture`], which spawns
//! Bevy's `Screenshot` observed by Bevy's `save_to_disk`.

use crate::{frame, internal, invalid, protocol, required};
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult};
use bevy::ecs::observer::ObservedBy;
use bevy::render::view::screenshot::{Captured, Capturing, Screenshot, ScreenshotCaptured, save_to_disk};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How long a shot waits for a frame to render it. Wall time: a viewport with no viewer renders none.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a shot Bevy has started capturing waits for its image. A first frame under heavy load
/// takes longer than [`TIMEOUT`] to read back; a viewport shot with no viewer waits this long too,
/// since Bevy marks it captured on its first frame.
pub const READBACK_TIMEOUT: Duration = Duration::from_secs(60);

/// What a shot captures. Absent: the primary window. Headless names its offscreen image.
#[derive(Resource, Clone)]
pub struct ScreenshotTarget(pub RenderTarget);

/// A shot asked for: where it goes, and what is known of it so far.
#[derive(Component)]
pub struct Shot {
    pub path: PathBuf,
    /// The file's modified time before the shot, if it existed.
    pub asked: Option<SystemTime>,
    pub since: Instant,
    /// The bevy-ts frame the shot shows. A target captured later than asked sets it.
    pub frame: u64,
    /// The captured image's size, once captured.
    pub size: Option<UVec2>,
}

/// Spawns a shot of the run's [`ScreenshotTarget`] (or the primary window), saved to `path` on
/// the next rendered frame.
pub fn capture(world: &mut World, path: impl Into<PathBuf>) -> Entity {
    let path = path.into();
    let target = world.get_resource::<ScreenshotTarget>().map_or(RenderTarget::Window(Default::default()), |t| t.0.clone());
    let asked = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let shot = Shot { path: path.clone(), asked, since: Instant::now(), frame: frame(world), size: None };
    world
        .spawn((Screenshot(target), shot))
        .observe(save_to_disk(path))
        .observe(|c: On<ScreenshotCaptured>, mut shots: Query<&mut Shot>| {
            if let Ok(mut s) = shots.get_mut(c.entity) {
                s.size = Some(c.image.size());
            }
        })
        .id()
}

/// Shots `bevy-ts/screenshot` is serving, keyed by params, and the updates counted so far.
#[derive(Resource, Default)]
pub(crate) struct Shots(HashMap<String, Served>, u64);

struct Served {
    state: Progress,
    /// Called this update; a request not called for an update has closed.
    touched: bool,
}

enum Progress {
    Pending(Entity),
    /// Answered in update `at`, to `owed` requests. Bevy calls each once more in the next update
    /// (its send then fails, and Bevy drops it): those calls get the answer again, as does an
    /// identical request in update `at`. Any other call is a new request and takes a new shot.
    Done { answer: BrpResult, at: u64, owed: u32 },
}

/// Checks a path before anything is captured.
fn check(path: &str) -> Result<PathBuf, BrpError> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(invalid(format!("{path}: the path must be absolute")));
    }
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Err(invalid(format!("{path}: the path must end in .png")));
    }
    match p.parent() {
        Some(dir) if dir.is_dir() => Ok(p.to_path_buf()),
        Some(dir) => Err(invalid(format!("{path}: the folder {} does not exist", dir.display()))),
        None => Err(invalid(format!("{path}: the path names no folder"))),
    }
}

/// `bevy-ts/screenshot`, a deferred method: `None` until the shot is written or fails.
pub(crate) fn screenshot(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let update = world.resource::<Shots>().1;
    let state = world.resource_mut::<Shots>().0.remove(&key).map(|s| s.state);
    let state = match state {
        Some(Progress::Done { answer, at, owed }) if at == update => Progress::Done { answer, at, owed: owed + 1 },
        Some(Progress::Done { answer, at, owed }) if at + 1 == update && owed > 0 => Progress::Done { answer, at, owed: owed - 1 },
        Some(Progress::Pending(e)) => Progress::Pending(e),
        _ => {
            let params: protocol::ScreenshotParams = required(p)?;
            let path = check(&params.path)?;
            Progress::Pending(capture(world, path))
        }
    };
    let state = match state {
        Progress::Pending(e) => match result(world, e) {
            Some(r) => {
                if r.is_err() {
                    abandon(world, e);
                }
                Progress::Done { answer: r, at: update, owed: 1 }
            }
            None => Progress::Pending(e),
        },
        done => done,
    };
    let answer = match &state {
        Progress::Done { answer, .. } => Some(answer.clone()),
        Progress::Pending(_) => None,
    };
    world.resource_mut::<Shots>().0.insert(key, Served { state, touched: true });
    answer.transpose()
}

/// A shot's answer, or `None` while it waits.
fn result(world: &World, e: Entity) -> Option<BrpResult> {
    let Ok(entity) = world.get_entity(e) else {
        return Some(Err(internal("Bevy dropped the shot: another shot of the same view was taken that frame")));
    };
    let shot = entity.get::<Shot>().expect("a shot entity has its Shot");
    let path = shot.path.display();
    if !entity.contains::<Captured>() {
        let (limit, what) = if entity.contains::<Capturing>() { (READBACK_TIMEOUT, "no image read back") } else { (TIMEOUT, "no frame rendered") };
        return (shot.since.elapsed() > limit).then(|| Err(internal(format!("{path}: {what} within {} s", limit.as_secs()))));
    }
    let modified = std::fs::metadata(&shot.path).and_then(|m| m.modified()).ok();
    let written = modified.is_some_and(|m| shot.asked.is_none_or(|a| m > a));
    let size = shot.size.unwrap_or_default();
    Some(if written {
        serde_json::to_value(protocol::ScreenshotResult { path: shot.path.display().to_string(), width: size.x, height: size.y, frame: shot.frame })
            .map_err(|e| internal(e.to_string()))
    } else {
        Err(internal(format!("Bevy captured but did not write {path}; the engine log has its error")))
    })
}

/// Gives up on a shot so nothing writes it. One Bevy is reading back keeps its entity, without
/// its observers, for Bevy to finish and despawn: Bevy inserts `Captured` on it when it does. A
/// viewport shot with no viewer resolves when one attaches; a shot of a window Bevy never captures
/// (one with no wgpu surface and no ring) keeps its entity.
fn abandon(world: &mut World, e: Entity) {
    let Ok(mut entity) = world.get_entity_mut(e) else { return };
    if !entity.contains::<Capturing>() || entity.contains::<Captured>() {
        entity.despawn();
        return;
    }
    let observers = entity.take::<ObservedBy>().map(|o| o.get().to_vec()).unwrap_or_default();
    entity.remove::<Shot>();
    for o in observers {
        world.entity_mut(o).despawn();
    }
}

/// Forgets the shots of requests that closed, and the shot entities still waiting for them.
pub(crate) fn forget_closed(mut shots: ResMut<Shots>, mut commands: Commands) {
    shots.1 += 1;
    shots.0.retain(|_, s| {
        let keep = std::mem::take(&mut s.touched);
        if !keep && let Progress::Pending(e) = s.state {
            commands.queue(move |world: &mut World| abandon(world, e));
        }
        keep
    });
}
