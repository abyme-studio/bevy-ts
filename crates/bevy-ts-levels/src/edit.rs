//! Writes in the engine (design decision 8): the disk wins. A batch names the content hash it was
//! made against; when the file on disk has moved on, the world first follows the disk, then the
//! batch is replayed by name path on the new text, or refused as a conflict. Never a text merge.

use crate::compose::Schema;
use crate::engine::{BsnLevel, Levels, WorldSchema, content_hash, open_sidecar, reconcile_level, root_name};
use bevy_ts_tsx::desc::node_id;
use bevy_ts_tsx::{TsxNode, TsxTree};
use crate::syntax::{self, Fields, ParsedFile, ValueKind};
use crate::writer::{self, Op};
use bevy::prelude::*;
use serde::Serialize;
use std::path::Path;

/// One batch for one file.
#[derive(Clone, Debug)]
pub struct WriteRequest {
    /// The client's id for the batch, echoed in its result.
    pub id: u64,
    /// The level file, as an asset path.
    pub file: String,
    /// The content hash the batch was made against.
    pub base: u64,
    pub ops: Vec<Op>,
}

/// What became of a batch.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WriteResult {
    pub id: u64,
    pub file: String,
    /// `written`, `rebased`, `conflict` or `refused`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The file's hash after the write.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Changed line ranges in the new file, 1-based and inclusive.
    pub lines: Vec<(u32, u32)>,
    /// The disk's value of the conflicting field, as written in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<String>,
    #[serde(skip)]
    pub inverse: Vec<Op>,
    /// What the batch added, by name path, with its entity once reconciled.
    #[serde(skip)]
    pub created: Vec<(Vec<String>, Option<Entity>)>,
}

impl WriteResult {
    fn refused(req: &WriteRequest, status: &'static str, reason: &str, message: impl Into<String>) -> Self {
        WriteResult { id: req.id, file: req.file.clone(), status, reason: Some(reason.into()), message: message.into(), hash: None, lines: Vec::new(), disk: None, inverse: Vec::new(), created: Vec::new() }
    }
}

/// Results for clients to collect (viewport drains them each frame).
#[derive(Resource, Default, Debug)]
pub struct WriteResults(pub Vec<WriteResult>);

impl WriteResults {
    pub fn drain(&mut self) -> Vec<WriteResult> {
        std::mem::take(&mut self.0)
    }
}

/// The 16-hex-digit form hashes take on the wire.
pub fn hash_text(h: u64) -> String {
    format!("{h:016x}")
}

/// Writes `text` through a temporary file and a rename.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp-{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("level"), std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Replaces the loaded asset with `text`, as a reload would: its levels reconcile.
fn load_text(world: &mut World, file: &str, text: &str) -> Result<(), String> {
    let parsed = syntax::parse(text).map_err(|d| d.render(file, text))?;
    let mut paths = Vec::new();
    crate::engine::includes_of(&parsed.entities, &mut paths);
    let server = world.resource::<AssetServer>().clone();
    let includes = paths.iter().map(|p| server.load::<BsnLevel>(crate::compose::resolve_path(file, p))).collect();
    let level = BsnLevel { includes, hash: content_hash(text.as_bytes()), parsed: std::sync::Arc::new(parsed) };
    world.resource_mut::<Levels>().remember(level.hash, level.parsed.clone());
    let handle = server.get_handle::<BsnLevel>(file.to_string()).unwrap_or_else(|| server.load::<BsnLevel>(file.to_string()));
    let mut assets = world.resource_mut::<Assets<BsnLevel>>();
    if assets.contains(&handle) {
        *assets.get_mut(&handle).expect("contained") = level;
    } else {
        let _ = assets.insert(&handle, level);
    }
    Ok(())
}

/// The text of a field's value in a file, for a conflict's `disk`.
fn field_text(file: &ParsedFile, path: &[String], component: &str, field: &[String]) -> Option<String> {
    let mut list = &file.entities;
    let mut entity = None;
    for seg in path {
        let e = list.items.iter().find(|e| e.name().is_some_and(|n| &n.name == seg))?;
        entity = Some(e);
        list = e.related("Children").map(|r| &r.list).unwrap_or(list);
    }
    let short = component.rsplit("::").next().unwrap_or(component);
    let p = entity?.patches().find(|p| p.path.joined() == component || p.path.last() == short)?;
    let mut fields: &Fields = &p.fields;
    let mut value = None;
    for (i, seg) in field.iter().enumerate() {
        let v = &fields.get(seg)?.value;
        value = Some(v);
        if i + 1 < field.len() {
            let ValueKind::Type(t) = &v.kind else { return None };
            fields = &t.fields;
        }
    }
    value.map(|v| file.text(v.span).to_string())
}

/// Applies a batch to a level file. The world follows the file before this returns.
pub fn write_level(world: &mut World, req: WriteRequest) -> WriteResult {
    let result = write_inner(world, &req);
    world.resource_mut::<WriteResults>().0.push(result.clone());
    if result.status != "refused" {
        let event = if result.status == "conflict" { "conflict" } else { result.status };
        world.resource_mut::<Levels>().emit(serde_json::json!({
            "event": event, "level": result.file, "hash": result.hash, "lines": result.lines, "id": result.id, "reason": result.reason,
        }));
    }
    result
}

fn write_inner(world: &mut World, req: &WriteRequest) -> WriteResult {
    let disk_path = world.resource::<Levels>().asset_root.join(&req.file);
    let disk = match std::fs::read_to_string(&disk_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && req.base == content_hash(b"") => String::new(),
        Err(e) => return WriteResult::refused(req, "refused", "not-found", format!("{}: {e}", disk_path.display())),
    };
    let disk_hash = content_hash(disk.as_bytes());
    let mut status = "written";
    if disk_hash != req.base {
        // Someone else wrote the file: the world follows the disk first.
        let base = world.resource::<Levels>().version(req.base);
        if let Err(e) = load_text(world, &req.file, &disk) {
            return WriteResult::refused(req, "conflict", "unparseable", e);
        }
        reconcile_affected(world, &req.file);
        let disk_parsed = syntax::parse(&disk).ok();
        for op in &req.ops {
            if let (Op::Set { path, component, field, .. } | Op::Revert { path, component, field }, Some(base), Some(now)) = (op, &base, &disk_parsed) {
                let before = field_text(base, path, component, field);
                let after = field_text(now, path, component, field);
                if before.is_some() && after.is_none() {
                    return WriteResult::refused(req, "conflict", "target removed", format!("{} or its {component}.{} is gone from the file", path.join("/"), field.join(".")));
                }
                if before != after {
                    let mut r = WriteResult::refused(req, "conflict", "changed on disk", format!("{}.{} changed on disk", component, field.join(".")));
                    r.disk = after;
                    return r;
                }
            }
        }
        status = "rebased";
    }
    // A sidecar level layers over its scene: open, or a scene's sidecar path written first now.
    let open = world.resource::<Levels>().roots.get(&req.file).map(|r| r.overlay.clone());
    let scene = match &open {
        Some(overlay) => overlay.clone(),
        None => sidecar_of(world, &req.file),
    };
    let mut ops = req.ops.clone();
    let planned = {
        let registry = world.resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let schema = WorldSchema::live(world, &registry);
        let resolve = |c: &str| schema.component(c).ok().map(|s| s.name);
        match check_world(world, &schema, &req.file, scene.as_deref(), &mut ops) {
            Ok(()) => writer::apply_resolved(&disk, &ops, scene.is_some(), &resolve),
            Err(e) => return WriteResult::refused(req, "refused", e.reason, e.message),
        }
    };
    let written = match planned {
        Ok(w) => w,
        Err(e) if status == "rebased" && e.reason == "not-found" => return WriteResult::refused(req, "conflict", "target removed", e.message),
        Err(e) => return WriteResult::refused(req, "refused", e.reason, e.message),
    };
    if let Err(e) = write_atomic(&disk_path, &written.text) {
        return WriteResult::refused(req, "refused", "io", format!("{}: {e}", disk_path.display()));
    }
    let root = scene.clone().unwrap_or_else(|| root_name(&req.file));
    // Renames and moves keep their entities: the retained tree moves before the reload sees the file.
    if open.is_some() {
        for (from, to) in &written.renamed {
            bevy_ts_tsx::rekey(world, &root, from, to);
        }
    }
    if let (None, Some(scene)) = (&open, &scene) {
        open_sidecar(world, scene, &req.file);
    }
    let hash = content_hash(written.text.as_bytes());
    if let Err(e) = load_text(world, &req.file, &written.text) {
        return WriteResult::refused(req, "refused", "internal", e);
    }
    reconcile_affected(world, &req.file);
    let tree = world.resource::<TsxTree>();
    let created = written.created.iter().map(|p| (p.clone(), tree.entity(node_id(&root, p)))).collect();
    WriteResult { id: req.id, file: req.file.clone(), status, reason: None, message: String::new(), hash: Some(hash_text(hash)), lines: written.lines, disk: None, inverse: written.inverse, created }
}

/// The `app.scene` root whose sidecar level is `file`, if any.
fn sidecar_of(world: &World, file: &str) -> Option<String> {
    let scenes: Vec<String> = world.resource::<TsxTree>().scenes.keys().cloned().collect();
    scenes.into_iter().find(|s| bevy_ts_tsx::scene_level(world, s).as_deref() == Some(file))
}

/// The shortest suffix of `component` that resolves to the type `full`.
fn shortest(schema: &WorldSchema, component: &str, full: &str) -> Option<String> {
    let segments: Vec<&str> = component.split("::").collect();
    (1..=segments.len()).map(|k| segments[segments.len() - k..].join("::")).find(|c| schema.component(c).is_ok_and(|s| s.name == full))
}

/// The tsx nodes the entity at `path` in `file` is in the world: through every open level that
/// is or includes the file (an included room is placed once per inclusion), or by key path in a
/// sidecar's scene. None when no open level shows the file.
fn nodes_of<'w>(world: &'w World, file: &str, scene: Option<&str>, path: &[String]) -> Option<Vec<&'w TsxNode>> {
    let tree = world.resource::<TsxTree>();
    let node = |root: &str, key: &[String]| tree.entity(node_id(root, key)).and_then(|e| world.get::<TsxNode>(e));
    let levels = world.resource::<Levels>();
    let mut shown = false;
    let mut out = Vec::new();
    if let Some(scene) = scene {
        shown = true;
        out.extend(node(scene, path));
    } else {
        for (level, r) in levels.roots.iter().filter(|(p, r)| p.as_str() == file || r.files.contains(file)) {
            shown = true;
            let root = r.overlay.clone().unwrap_or_else(|| root_name(level));
            for (key, frames) in &r.owners {
                if frames.iter().any(|f| f.file == file && f.path == path) {
                    out.extend(node(&root, key));
                }
            }
        }
    }
    shown.then_some(out)
}

/// Whether a node was made by the scene's TSX rather than a level: its outermost source, the
/// call site, is not a `.bsn` (a scene instance a sidecar placed has the prefab's TSX first).
fn made_in_tsx(n: &TsxNode) -> bool {
    n.source.last().is_none_or(|s| !s.file.ends_with(".bsn"))
}

/// Checks that need the world, before the writer (design decision 1): a component the entity has
/// cannot be added; one its prefab gives cannot be removed; a sidecar's TSX entities are the
/// scene's to rename, delete and move. A new patch is written in its shortest form.
///
/// The world is as it was before the batch: each op is checked through the batch's earlier
/// renames and moves, an entity the batch deleted is new, and a component it added or removed
/// counts as added or removed.
fn check_world(world: &World, schema: &WorldSchema, file: &str, scene: Option<&str>, ops: &mut [Op]) -> Result<(), writer::Refused> {
    let refuse = |reason: &'static str, message: String| writer::Refused { reason, message };
    // Where each path the batch moved was before it: (path now, path in the world).
    let mut moved: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    let origin = |moved: &[(Vec<String>, Vec<String>)], path: &[String]| -> Vec<String> {
        for (now, was) in moved.iter().rev() {
            if path.starts_with(now) {
                return was.iter().chain(&path[now.len()..]).cloned().collect();
            }
        }
        path.to_vec()
    };
    let mut gone: Vec<Vec<String>> = Vec::new();
    let mut added: Vec<(Vec<String>, String)> = Vec::new();
    let mut removed: Vec<(Vec<String>, String)> = Vec::new();
    for op in ops.iter_mut() {
        match op {
            Op::AddComponent { .. } | Op::RemoveComponent { .. } => {
                let (path, component, add, plain) = match &*op {
                    Op::AddComponent { path, component, text, .. } => (path.clone(), component.clone(), true, text.is_none()),
                    Op::RemoveComponent { path, component } => (path.clone(), component.clone(), false, false),
                    _ => unreachable!("matched"),
                };
                if gone.iter().any(|g| path.starts_with(g)) {
                    continue;
                }
                let was = origin(&moved, &path);
                let Ok(full) = schema.component(&component).map(|s| s.name) else { continue };
                let Some(nodes) = nodes_of(world, file, scene, &was) else {
                    return Err(refuse("invalid", format!("no open level shows {file}: open one that places it to change its components")));
                };
                let key = (was, full.clone());
                if add {
                    let has = nodes.iter().any(|n| n.components.contains_key(&full)) && !removed.contains(&key);
                    if has {
                        return Err(refuse("exists", format!("{} already has {full}", path.join("/"))));
                    }
                    if plain
                        && let Some(short) = shortest(schema, &component, &full)
                        && let Op::AddComponent { component, .. } = op
                    {
                        *component = short;
                    }
                    removed.retain(|r| r != &key);
                    added.push(key);
                } else {
                    // A component the file does not add has its source elsewhere: the prefab's.
                    let given = nodes.iter().any(|n| n.components.get(&full).and_then(|c| c.source.as_ref()).is_some_and(|s| s.file != file)) && !added.contains(&key);
                    if given {
                        return Err(refuse("prefab-internal", format!("{full} comes from {}'s prefab; .bsn cannot remove it", path.join("/"))));
                    }
                    added.retain(|a| a != &key);
                    removed.push(key);
                }
            }
            Op::Delete { path } | Op::Rename { path, .. } | Op::Reparent { path, .. } => {
                let was = origin(&moved, path);
                if scene.is_some() && !gone.iter().any(|g| path.starts_with(g)) && nodes_of(world, file, scene, &was).is_some_and(|n| n.iter().any(|n| made_in_tsx(n))) {
                    return Err(refuse("prefab-internal", format!("{} is made in TSX; edit it there", path.join("/"))));
                }
                match op {
                    Op::Delete { path } => gone.push(path.clone()),
                    Op::Rename { path, to } => moved.push((path[..path.len() - 1].iter().chain(std::iter::once(&*to)).cloned().collect(), was)),
                    Op::Reparent { path, parent, .. } => moved.push((parent.iter().chain(path.last()).cloned().collect(), was)),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reconciles every open level that is, or includes, `file`, now.
fn reconcile_affected(world: &mut World, file: &str) {
    for p in world.resource::<Levels>().affected(file) {
        reconcile_text(world, &p);
    }
}

fn reconcile_text(world: &mut World, level: &str) {
    reconcile_level(world, level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ts::backend::SourceLocation;

    fn at(file: &str) -> SourceLocation {
        SourceLocation { file: file.into(), line: 1, column: 1 }
    }

    #[test]
    fn a_scene_instance_a_sidecar_placed_is_the_sidecars() {
        let node = |source: Vec<SourceLocation>| TsxNode { id: 0, root: "main".into(), key_path: vec![], types: vec![], source, components: Default::default() };
        assert!(!made_in_tsx(&node(vec![at("src/enemy.tsx"), at("levels/main.bsn")])), "the call site is the sidecar's");
        assert!(made_in_tsx(&node(vec![at("src/main.tsx")])));
        assert!(made_in_tsx(&node(vec![])));
    }
}
