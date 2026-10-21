//! The asset viewer's side: list the project's assets from disk and Bevy's asset server
//! (loading nothing), watch them change, and render thumbnails offscreen on layer 30.

use crate::edit::{EditorOnly, THUMBNAIL_LAYER};
use crate::methods::{internal, invalid, required, to_value, CRATE};
use bevy::asset::io::file::FileAssetReader;
use bevy::asset::{AssetPath, LoadState, RecursiveDependencyLoadState, RenderAssetUsages};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::remote::{BrpResult};
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy_ts_levels::engine::BsnLevel;
use bevy_ts_levels::picture::spawn_level_picture;
use bevy_ts_remote::protocol;
use std::any::TypeId;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Where assets are read from: Bevy's file reader's root unless the app says otherwise.
#[derive(Resource, Clone, Debug)]
pub struct AssetRoot(pub PathBuf);

impl Default for AssetRoot {
    fn default() -> Self {
        AssetRoot(FileAssetReader::get_base_path().join("assets"))
    }
}

pub(crate) fn build(app: &mut App) {
    app.init_resource::<AssetRoot>().init_resource::<Listing>().init_resource::<Thumbnails>().add_systems(Update, (poll, thumbnails));
}

pub(crate) fn add_methods(app: &mut App) {
    use bevy_ts_remote::{add_method, add_watching_method};
    add_method(app, CRATE, "bevy-ts/assets", list);
    add_watching_method(app, CRATE, "bevy-ts/assets+watch", watch);
    add_method(app, CRATE, "bevy-ts/asset_thumbnail", thumbnail);
}

/// An asset kind by extension.
pub fn kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".scn.ron") || lower.ends_with(".bsn") {
        return "scene";
    }
    match lower.rsplit('.').next().unwrap_or("") {
        "png" | "jpg" | "jpeg" | "ktx2" | "dds" | "hdr" | "exr" | "bmp" | "tga" | "webp" | "basis" | "qoi" => "image",
        "gltf" | "glb" | "obj" => "mesh",
        "ogg" | "oga" | "wav" | "mp3" | "flac" => "audio",
        "ttf" | "otf" => "font",
        "wgsl" | "wesl" | "glsl" | "vert" | "frag" => "shader",
        _ => "other",
    }
}

/// Whether `asset_thumbnail` draws this file: images, meshes and `.bsn` levels. The listing's
/// `thumbnail` and the method both ask here, so they never disagree.
pub fn has_thumbnail(path: &str) -> bool {
    match kind(path) {
        "image" | "mesh" => true,
        "scene" => path.to_ascii_lowercase().ends_with(".bsn"),
        _ => false,
    }
}

/// One file under the root: its asset path, size and modification time (ms since the epoch).
#[derive(Clone, Debug, PartialEq)]
struct File {
    path: String,
    size: u64,
    modified: Option<u64>,
}

fn walk(root: &Path) -> Vec<File> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name.ends_with(".meta") {
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let path = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let modified = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
            out.push(File { path, size: meta.len(), modified });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn loader_name(server: &AssetServer, path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    // The longest matching extension first (`scn.ron` before `ron`), as Bevy picks.
    let parts: Vec<&str> = lower.split('.').skip(1).collect();
    (0..parts.len()).find_map(|i| {
        let ext = parts[i..].join(".");
        bevy::tasks::block_on(server.get_asset_loader_with_extension(&ext)).ok().map(|l| l.type_path().rsplit("::").next().unwrap_or("").to_string())
    })
}

fn load_state(server: &AssetServer, path: &str) -> (&'static str, Option<String>) {
    let Some(id) = server.get_path_id(AssetPath::from(path.to_string())) else { return ("not_loaded", None) };
    match server.get_load_state(id) {
        None | Some(LoadState::NotLoaded) => ("not_loaded", None),
        Some(LoadState::Loading) => ("loading", None),
        Some(LoadState::Loaded) => ("loaded", None),
        Some(LoadState::Failed(e)) => ("failed", Some(e.to_string())),
    }
}

/// The labelled sub-assets of a loaded glTF.
fn gltf_labels(world: &World, path: &str) -> Vec<String> {
    use bevy::gltf::{Gltf, GltfAssetLabel, GltfMesh};
    let server = world.resource::<AssetServer>();
    let Some(id) = server.get_path_id(AssetPath::from(path.to_string())) else { return vec![] };
    let Ok(id) = id.try_typed::<Gltf>() else { return vec![] };
    let Some(g) = world.get_resource::<Assets<Gltf>>().and_then(|a| a.get(id)) else { return vec![] };
    let meshes = world.get_resource::<Assets<GltfMesh>>();
    let mut out: Vec<String> = (0..g.scenes.len()).map(|i| GltfAssetLabel::Scene(i).to_string()).collect();
    for (i, m) in g.meshes.iter().enumerate() {
        out.push(GltfAssetLabel::Mesh(i).to_string());
        let n = meshes.and_then(|a| a.get(m)).map_or(0, |m| m.primitives.len());
        out.extend((0..n).map(|p| GltfAssetLabel::Primitive { mesh: i, primitive: p }.to_string()));
    }
    out.extend((0..g.materials.len()).map(|i| GltfAssetLabel::Material { index: i, is_scale_inverted: false }.to_string()));
    out.extend((0..g.animations.len()).map(|i| GltfAssetLabel::Animation(i).to_string()));
    out
}

fn info(world: &World, f: &File) -> Value {
    let server = world.resource::<AssetServer>();
    let (state, error) = load_state(server, &f.path);
    let k = kind(&f.path);
    let mut v = json!({
        "path": f.path,
        "kind": k,
        "loader": loader_name(server, &f.path),
        "size": f.size,
        "modified": f.modified,
        "loadState": state,
        "error": error,
        "thumbnail": has_thumbnail(&f.path),
    });
    if k == "mesh" && state == "loaded" {
        v["labels"] = json!(gltf_labels(world, &f.path));
    }
    v
}

fn assets(world: &World, prefix: Option<&str>, want: Option<&str>) -> Vec<Value> {
    let root = world.resource::<AssetRoot>().0.clone();
    walk(&root)
        .iter()
        .filter(|f| prefix.is_none_or(|p| f.path.starts_with(p)))
        .filter(|f| want.is_none_or(|k| kind(&f.path) == k))
        .map(|f| info(world, f))
        .collect()
}

fn list(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::AssetsParams = match p {
        None | Some(Value::Null) => protocol::AssetsParams::default(),
        Some(v) => serde_json::from_value(v).map_err(|e| invalid(format!("invalid params: {e}")))?,
    };
    let want = p.kind.map(|k| k.to_string());
    let root = world.resource::<AssetRoot>().0.display().to_string();
    Ok(json!({ "root": root, "assets": assets(world, p.prefix.as_deref(), want.as_deref()) }))
}

/// The listing as last polled, and changes since, numbered for the watch.
#[derive(Resource, Default)]
struct Listing {
    files: BTreeMap<String, (File, &'static str)>,
    last_poll: Option<Instant>,
    seq: u64,
    changes: Vec<(u64, String, &'static str)>,
    watchers: HashMap<String, (u64, u64)>,
}

const POLL: Duration = Duration::from_millis(250);

/// Polls the root at most every 250 ms: files added, removed or modified, and load states that
/// changed, coalesced into one change per path per poll.
fn poll(world: &mut World) {
    let due = world.resource::<Listing>().last_poll.is_none_or(|t| t.elapsed() >= POLL);
    if !due || world.resource::<Listing>().watchers.is_empty() {
        return;
    }
    let root = world.resource::<AssetRoot>().0.clone();
    let now: Vec<(File, &'static str)> = {
        let server = world.resource::<AssetServer>();
        walk(&root).into_iter().map(|f| {
            let s = load_state(server, &f.path).0;
            (f, s)
        }).collect()
    };
    let mut listing = world.resource_mut::<Listing>();
    let first = listing.last_poll.is_none();
    listing.last_poll = Some(Instant::now());
    let mut changed: Vec<(String, &'static str)> = vec![];
    let mut next = BTreeMap::new();
    for (f, s) in now {
        match listing.files.get(&f.path) {
            None => changed.push((f.path.clone(), "added")),
            Some((old, _)) if old.size != f.size || old.modified != f.modified => changed.push((f.path.clone(), "modified")),
            Some((_, os)) if *os != s => changed.push((f.path.clone(), "load_state")),
            _ => {}
        }
        next.insert(f.path.clone(), (f, s));
    }
    for path in listing.files.keys() {
        if !next.contains_key(path) {
            changed.push((path.clone(), "removed"));
        }
    }
    listing.files = next;
    if first {
        return;
    }
    for (path, change) in changed {
        listing.seq += 1;
        let seq = listing.seq;
        listing.changes.push((seq, path, change));
    }
    if listing.changes.len() > 1000 {
        let drop = listing.changes.len() - 1000;
        listing.changes.drain(..drop);
    }
}

fn watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = world.resource::<crate::UpdateCount>().0;
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let (seq, known) = {
        let l = world.resource::<Listing>();
        (l.seq, l.watchers.get(&key).copied())
    };
    world.resource_mut::<Listing>().watchers.insert(key.clone(), (frame, seq));
    match known {
        Some((last, sent)) if last + 1 >= frame => {
            if sent == seq {
                return Ok(None);
            }
            let changes: Vec<(String, &'static str)> = world.resource::<Listing>().changes.iter().filter(|(s, _, _)| *s > sent).map(|(_, p, c)| (p.clone(), *c)).collect();
            let root = world.resource::<AssetRoot>().0.clone();
            let out: Vec<Value> = changes
                .into_iter()
                .map(|(path, change)| {
                    let asset = (change != "removed").then(|| {
                        let full = root.join(&path);
                        let meta = std::fs::metadata(&full).ok();
                        let f = File {
                            path: path.clone(),
                            size: meta.as_ref().map_or(0, |m| m.len()),
                            modified: meta.and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64),
                        };
                        info(world, &f)
                    });
                    json!({ "path": path, "change": change, "asset": asset })
                })
                .collect();
            Ok(Some(json!({ "changes": out })))
        }
        _ => {
            // A new subscriber: the whole list; polling starts from it.
            world.resource_mut::<Listing>().last_poll = None;
            poll(world);
            Ok(Some(json!({ "assets": assets(world, None, None) })))
        }
    }
}

// ---- thumbnails ----

/// A thumbnail in progress, or done.
enum Job {
    Loading { handle: UntypedHandle, size: u32, since: Instant },
    Rendering { entities: Vec<Entity>, image: Handle<Image>, frames: u32, framed: bool },
    Reading { entities: Vec<Entity> },
}

#[derive(Resource, Default)]
struct Thumbnails {
    /// (path, modified, size) to base64 PNG, or None for kinds with no thumbnail.
    cache: HashMap<(String, Option<u64>, u32), Option<String>>,
    jobs: HashMap<(String, Option<u64>, u32), Job>,
    done: HashMap<Entity, (String, Option<u64>, u32)>,
}

fn modified(root: &Path, path: &str) -> Option<u64> {
    std::fs::metadata(root.join(path)).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

fn thumbnail(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::AssetThumbnailParams = required(p)?;
    let size = p.size.get().min(512);
    let root = world.resource::<AssetRoot>().0.clone();
    if !root.join(&p.path).is_file() {
        return Err(invalid(format!("no asset {}", p.path)));
    }
    let key = (p.path.clone(), modified(&root, &p.path), size);
    let answer = |png: Option<String>, pending: bool| to_value(json!({ "path": p.path, "size": size, "png": png, "pending": pending }));
    if let Some(png) = world.resource::<Thumbnails>().cache.get(&key) {
        return answer(png.clone(), false);
    }
    if world.resource::<Thumbnails>().jobs.contains_key(&key) {
        return answer(None, true);
    }
    if !has_thumbnail(&p.path) {
        world.resource_mut::<Thumbnails>().cache.insert(key, None);
        return answer(None, false);
    }
    match kind(&p.path) {
        "image" => {
            let png = image_thumbnail(&root.join(&p.path), size).map_err(internal)?;
            let png = Some(base64(&png));
            world.resource_mut::<Thumbnails>().cache.insert(key, png.clone());
            answer(png, false)
        }
        "mesh" => {
            let handle: UntypedHandle = if p.path.to_ascii_lowercase().ends_with(".obj") {
                world.resource::<AssetServer>().load::<Mesh>(p.path.clone()).untyped()
            } else {
                world.resource::<AssetServer>().load::<bevy::world_serialization::WorldAsset>(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(p.path.clone())).untyped()
            };
            world.resource_mut::<Thumbnails>().jobs.insert(key, Job::Loading { handle, size, since: Instant::now() });
            answer(None, true)
        }
        // A `.bsn` level, in a game that reads levels.
        "scene" if world.contains_resource::<Assets<BsnLevel>>() => {
            let handle = world.resource::<AssetServer>().load::<BsnLevel>(p.path.clone()).untyped();
            world.resource_mut::<Thumbnails>().jobs.insert(key, Job::Loading { handle, size, since: Instant::now() });
            answer(None, true)
        }
        _ => {
            world.resource_mut::<Thumbnails>().cache.insert(key, None);
            answer(None, false)
        }
    }
}

/// Decodes an image file with Bevy's decoders and scales it to fit `size`, centred on clear.
fn image_thumbnail(path: &Path, size: u32) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let image = Image::from_buffer(&bytes, ImageType::Extension(&ext), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::all())
        .map_err(|e| e.to_string())?;
    let image = image.convert(TextureFormat::Rgba8UnormSrgb).ok_or("cannot convert the image to RGBA8")?;
    let (w, h) = (image.width().max(1), image.height().max(1));
    let data = image.data.ok_or("the image has no data")?;
    let scale = (size as f32 / w as f32).min(size as f32 / h as f32);
    let (tw, th) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let (ox, oy) = ((size - tw.min(size)) / 2, (size - th.min(size)) / 2);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..th.min(size) {
        for x in 0..tw.min(size) {
            let sx = ((x as f32 + 0.5) / scale) as u32;
            let sy = ((y as f32 + 0.5) / scale) as u32;
            let s = ((sy.min(h - 1) * w + sx.min(w - 1)) * 4) as usize;
            let d = (((y + oy) * size + x + ox) * 4) as usize;
            out[d..d + 4].copy_from_slice(&data[s..s + 4]);
        }
    }
    Ok(png(size, size, &out))
}

const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// Frames to let a scene spawn and its bounds settle before framing it.
const SETTLE_FRAMES: u32 = 3;

fn thumbnails(world: &mut World) {
    let keys: Vec<_> = world.resource::<Thumbnails>().jobs.keys().cloned().collect();
    for key in keys {
        let job = world.resource_mut::<Thumbnails>().jobs.remove(&key).unwrap();
        let next = match job {
            Job::Loading { handle, size, since } if handle.type_id() == TypeId::of::<BsnLevel>() => {
                let server = world.resource::<AssetServer>();
                let failed = matches!(server.get_load_state(handle.id()), Some(LoadState::Failed(_)))
                    || matches!(server.get_recursive_dependency_load_state(handle.id()), Some(RecursiveDependencyLoadState::Failed(_)));
                if server.is_loaded_with_dependencies(handle.id()) {
                    start_level_render(world, &key.0, size)
                } else if failed || since.elapsed() > LOAD_TIMEOUT {
                    None
                } else {
                    Some(Job::Loading { handle, size, since })
                }
                .or_else(|| {
                    world.resource_mut::<Thumbnails>().cache.entry(key.clone()).or_insert(None);
                    None
                })
            }
            Job::Loading { handle, size, since } => {
                let state = world.resource::<AssetServer>().get_load_state(handle.id());
                match state {
                    Some(LoadState::Loaded) => Some(start_render(world, &handle, size)),
                    Some(LoadState::Failed(_)) => {
                        world.resource_mut::<Thumbnails>().cache.insert(key.clone(), None);
                        None
                    }
                    _ if since.elapsed() > LOAD_TIMEOUT => {
                        world.resource_mut::<Thumbnails>().cache.insert(key.clone(), None);
                        None
                    }
                    _ => Some(Job::Loading { handle, size, since }),
                }
            }
            Job::Rendering { entities, image, frames, framed } => {
                if frames == SETTLE_FRAMES && !framed {
                    frame_thumbnail(world, &entities);
                    Some(Job::Rendering { entities, image, frames: frames + 1, framed: true })
                } else if frames < SETTLE_FRAMES + 3 {
                    Some(Job::Rendering { entities, image, frames: frames + 1, framed })
                } else {
                    let reader = world.spawn((Readback::texture(image), EditorOnly)).observe(on_readback).id();
                    world.resource_mut::<Thumbnails>().done.insert(reader, key.clone());
                    let mut entities = entities;
                    entities.push(reader);
                    Some(Job::Reading { entities })
                }
            }
            Job::Reading { entities } => {
                if world.resource::<Thumbnails>().cache.contains_key(&key) {
                    for e in entities {
                        if let Ok(ec) = world.get_entity_mut(e) {
                            ec.despawn();
                        }
                    }
                    None
                } else {
                    Some(Job::Reading { entities })
                }
            }
        };
        if let Some(j) = next {
            world.resource_mut::<Thumbnails>().jobs.insert(key, j);
        }
    }
}

/// A level composed and spawned as a picture on layer 30, then staged as any subject; None when
/// it does not compose.
fn start_level_render(world: &mut World, path: &str, size: u32) -> Option<Job> {
    let subject = spawn_level_picture(world, path, (EditorOnly, RenderLayers::layer(THUMBNAIL_LAYER))).ok()?;
    world.entity_mut(subject).insert(ThumbnailSubject);
    Some(stage(world, subject, size))
}

/// The isolated scene: the asset, a light and a camera rendering layer 30 into an image.
fn start_render(world: &mut World, handle: &UntypedHandle, size: u32) -> Job {
    let layer = RenderLayers::layer(THUMBNAIL_LAYER);
    let subject = if let Ok(mesh) = handle.clone().try_typed::<Mesh>() {
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
        world.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::default(), layer.clone(), EditorOnly)).id()
    } else {
        let scene = handle.clone().typed::<bevy::world_serialization::WorldAsset>();
        world.spawn((bevy::world_serialization::WorldAssetRoot(scene), Transform::default(), Visibility::default(), EditorOnly, ThumbnailSubject)).id()
    };
    stage(world, subject, size)
}

/// A light and a camera on layer 30 rendering into an image, around `subject`.
fn stage(world: &mut World, subject: Entity, size: u32) -> Job {
    let layer = RenderLayers::layer(THUMBNAIL_LAYER);
    let mut image = Image::new_target_texture(size, size, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let image = world.resource_mut::<Assets<Image>>().add(image);
    let light = world
        .spawn((DirectionalLight { illuminance: 8000.0, ..default() }, Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y), layer.clone(), EditorOnly))
        .id();
    let camera = world
        .spawn((
            Camera3d::default(),
            Camera { order: -100, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(image.clone().into()),
            Transform::from_xyz(2.0, 1.6, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
            layer,
            EditorOnly,
            ThumbnailCamera,
        ))
        .id();
    Job::Rendering { entities: vec![subject, light, camera], image, frames: 0, framed: false }
}

#[derive(Component)]
struct ThumbnailSubject;

#[derive(Component)]
struct ThumbnailCamera;

/// Puts every spawned piece of the subject on layer 30 and frames its bounds from three-quarters.
fn frame_thumbnail(world: &mut World, entities: &[Entity]) {
    let subject = entities[0];
    let camera = entities[2];
    let mut all = vec![];
    let mut stack = vec![subject];
    while let Some(e) = stack.pop() {
        all.push(e);
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    for &e in &all {
        world.entity_mut(e).insert((RenderLayers::layer(THUMBNAIL_LAYER), EditorOnly));
    }
    let (lo, hi) = crate::edit::bounds(world, subject).unwrap_or((Vec3::splat(-0.5), Vec3::splat(0.5)));
    let center = (lo + hi) / 2.0;
    let radius = ((hi - lo).length() / 2.0).max(0.05);
    let dir = Vec3::new(1.0, 0.8, 1.0).normalize();
    let distance = radius / (std::f32::consts::FRAC_PI_4 / 2.0).sin() * 1.05;
    if let Some(mut t) = world.get_mut::<Transform>(camera) {
        *t = Transform::from_translation(center + dir * distance).looking_at(center, Vec3::Y);
    }
}

fn on_readback(done: On<ReadbackComplete>, mut thumbs: ResMut<Thumbnails>) {
    let Some(key) = thumbs.done.remove(&done.entity) else { return };
    let size = key.2;
    // Rows may be padded to 256 bytes.
    let row = (size * 4) as usize;
    let padded = row.div_ceil(256) * 256;
    let mut rgba = Vec::with_capacity(row * size as usize);
    for y in 0..size as usize {
        let start = y * if done.data.len() >= padded * size as usize { padded } else { row };
        rgba.extend_from_slice(&done.data[start..start + row]);
    }
    thumbs.cache.insert(key, Some(base64(&png(size, size, &rgba))));
}

// ---- PNG and base64 with no dependencies ----

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// An RGBA8 PNG, its image data in stored (uncompressed) deflate blocks.
pub fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w * 4 + 1) as usize * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push(if i + 1 == chunks.len() { 1 } else { 0 });
        let len = c.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = vec![];
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind("models/enemy.glb"), "mesh");
        assert_eq!(kind("textures/floor.PNG"), "image");
        assert_eq!(kind("levels/arena.bsn"), "scene");
        assert_eq!(kind("scenes/a.scn.ron"), "scene");
        assert_eq!(kind("notes.txt"), "other");
    }

    #[test]
    fn thumbnails_by_kind() {
        for (path, want) in [
            ("textures/floor.png", true),
            ("models/enemy.glb", true),
            ("models/crate.gltf", true),
            ("models/rock.obj", true),
            ("levels/arena.BSN", true),
            ("scenes/a.scn.ron", false),
            ("sounds/hit.wav", false),
            ("fonts/ui.ttf", false),
            ("shaders/glow.wgsl", false),
            ("notes.txt", false),
        ] {
            assert_eq!(has_thumbnail(path), want, "{path}");
        }
    }

    #[test]
    fn base64_matches_the_rfc() {
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn png_round_trips_through_bevys_decoder() {
        let rgba: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 10) as u8).collect();
        let bytes = png(3, 2, &rgba);
        let image = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, false, ImageSampler::Default, RenderAssetUsages::all()).unwrap();
        assert_eq!((image.width(), image.height()), (3, 2));
        assert_eq!(image.data.unwrap(), rgba);
    }
}
