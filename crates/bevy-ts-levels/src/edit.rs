//! Writes in the engine (design decision 8): the disk wins. A batch names the content hash it was
//! made against; when the file on disk has moved on, the world first follows the disk, then the
//! batch is replayed by name path on the new text, or refused as a conflict. Never a text merge.

use crate::compose::Schema;
use crate::compose::MissingReason;
use crate::engine::{BsnLevel, Levels, MissingComponents, WorldSchema, content_hash, open_sidecar, reconcile_level, root_name};
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
pub(crate) fn load_text(world: &mut World, file: &str, text: &str) -> Result<(), String> {
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
        shorten(&schema, &mut ops);
        // The writer first: it refuses a batch that reuses a path it changed, so each op's path
        // names the entity the world has now. The refusal of the earliest op wins; on one op,
        // the world's says more than the file's `not-found` (a prefab's component, or a scene's
        // TSX entity, is not in the file either).
        match writer::apply(&disk, &ops, scene.is_some(), Some(&resolve)) {
            Ok(w) => check_world(world, &schema, &req.file, scene.as_deref(), &ops).map(|()| w),
            Err(e) => match check_world(world, &schema, &req.file, scene.as_deref(), &ops[..=e.at]) {
                Err(w) if w.at < e.at || e.reason == "not-found" => Err(w),
                _ => Err(e),
            },
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
    // Renames and moves keep their entities: the retained tree moves, in every placement of the
    // file, before the reload sees it.
    // Where a path lands is read from the new text: an included file that now has one top
    // merges it into the including entity.
    let places = placements(world, &req.file, scene.as_deref()).unwrap_or_default();
    let top_after = syntax::parse(&written.text).ok().and_then(|f| single_top(&f));
    for p in &places {
        let after = p.after(top_after.clone());
        for (from, to) in &written.renamed {
            if let (Some(a), Some(b)) = (p.key_of(from), after.key_of(to)) {
                bevy_ts_tsx::rekey(world, &p.root, &a, &b);
            }
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
    // What the batch added, placed as the reload placed it: in the file's own root when it is
    // open, else its first inclusion.
    let places = placements(world, &req.file, scene.as_deref()).unwrap_or_default();
    let tree = world.resource::<TsxTree>();
    let created = written.created.iter().map(|p| (p.clone(), places.iter().filter_map(|pl| tree.entity(node_id(&pl.root, &pl.key_of(p)?))).next())).collect();
    WriteResult { id: req.id, file: req.file.clone(), status, reason: None, message: String::new(), hash: Some(hash_text(hash)), lines: written.lines, disk: None, inverse: written.inverse, created }
}

/// The `app.scene` root whose sidecar level is `file`, if any. None for a level `app.level`
/// declares: it is a level of its own.
pub(crate) fn sidecar_of(world: &World, file: &str) -> Option<String> {
    if world.resource::<Levels>().declared.contains(file) {
        return None;
    }
    let scenes: Vec<String> = world.resource::<TsxTree>().scenes.keys().cloned().collect();
    scenes.into_iter().find(|s| bevy_ts_tsx::scene_level(world, s).as_deref() == Some(file))
}

/// The shortest suffix of `component` that resolves to the type `full`.
fn shortest(schema: &WorldSchema, component: &str, full: &str) -> Option<String> {
    let segments: Vec<&str> = component.split("::").collect();
    (1..=segments.len()).map(|k| segments[segments.len() - k..].join("::")).find(|c| schema.component(c).is_ok_and(|s| s.name == full))
}

/// Where a file's entities are in the world: an open level that is the file, or one inclusion
/// of it in an open level.
#[derive(Clone, Debug)]
struct Placement {
    /// The tsx root.
    root: String,
    /// The key of the entity that includes the file; empty for the file's own root.
    key: Vec<String>,
    included: bool,
    /// An included file's one top entity, merged into the including entity (compose's
    /// `Frame::merged`); a file with several tops puts them under it.
    top: Option<String>,
    /// The files of the including frames above the file, outermost first.
    above: Vec<String>,
}

impl Placement {
    fn key_of(&self, path: &[String]) -> Option<Vec<String>> {
        let rest = match &self.top {
            Some(top) => path.split_first().filter(|(first, _)| *first == top)?.1,
            None => path,
        };
        Some(self.key.iter().chain(rest).cloned().collect())
    }

    /// The placement once the file has `top` as its one top entity, or several (None).
    fn after(&self, top: Option<String>) -> Placement {
        Placement { top: if self.included { top } else { None }, ..self.clone() }
    }
}

/// The name path segment of a file's one top entity, when it has exactly one.
fn single_top(file: &ParsedFile) -> Option<String> {
    let mut tops = file.entities.items.iter().filter(|e| !e.entries.is_empty());
    let (first, None) = (tops.next()?, tops.next()) else { return None };
    Some(first.name().map_or_else(|| "#0".to_string(), |n| n.name.clone()))
}

/// Every placement of `file`: its own root first, then each inclusion in an open level, as
/// compose placed it; or by key path in a sidecar's scene. None when no open level shows the file.
fn placements(world: &World, file: &str, scene: Option<&str>) -> Option<Vec<Placement>> {
    if let Some(scene) = scene {
        return Some(vec![Placement { root: scene.to_string(), key: Vec::new(), included: false, top: None, above: Vec::new() }]);
    }
    let levels = world.resource::<Levels>();
    let mut out: Vec<Placement> = Vec::new();
    if let Some(r) = levels.roots.get(file) {
        out.push(Placement { root: r.overlay.clone().unwrap_or_else(|| root_name(file)), key: Vec::new(), included: false, top: None, above: Vec::new() });
    }
    let mut shown = !out.is_empty();
    for (level, r) in levels.roots.iter().filter(|(p, r)| p.as_str() != file && r.files.contains(file)) {
        shown = true;
        let root = r.overlay.clone().unwrap_or_else(|| root_name(level));
        for (key, frames) in &r.owners {
            let Some(i) = frames.iter().position(|f| f.file == file && f.path.len() == 1) else { continue };
            let f = &frames[i];
            let (key, top) = if f.merged { (key.clone(), Some(f.path[0].clone())) } else { (key[..key.len().saturating_sub(1)].to_vec(), None) };
            if !out.iter().any(|p| p.root == root && p.key == key) {
                out.push(Placement { root: root.clone(), key, included: true, top, above: frames[..i].iter().map(|f| f.file.clone()).collect() });
            }
        }
    }
    shown.then_some(out)
}

/// The nodes the entity at `path` is in each placement, with the files above it there.
fn nodes_of<'w>(world: &'w World, places: &'w [Placement], path: &[String]) -> Vec<(&'w TsxNode, &'w [String])> {
    let tree = world.resource::<TsxTree>();
    places
        .iter()
        .filter_map(|p| {
            let e = tree.entity(node_id(&p.root, &p.key_of(path)?))?;
            Some((world.get::<TsxNode>(e)?, &p.above[..]))
        })
        .collect()
}

/// A new patch names its component in the shortest form that resolves to the same type.
fn shorten(schema: &WorldSchema, ops: &mut [Op]) {
    for op in ops {
        if let Op::AddComponent { component, text: None, .. } = op
            && let Ok(full) = schema.component(component).map(|s| s.name)
            && let Some(short) = shortest(schema, component, &full)
        {
            *component = short;
        }
    }
}

/// Whether a node was made by the scene's TSX rather than a level: its outermost source, the
/// call site, is not a `.bsn` (a scene instance a sidecar placed has the prefab's TSX first).
fn made_in_tsx(n: &TsxNode) -> bool {
    n.source.last().is_none_or(|s| !s.file.ends_with(".bsn"))
}

/// Checks that need the world (design decision 1), on a batch the writer accepted: a component
/// the entity has cannot be added; one its prefab gives cannot be removed; a sidecar's TSX
/// entities are the scene's to rename, delete and move. In an included file, the entity has and
/// its prefab gives only what comes from the file or below it, not the including level's
/// overrides. A component removed then added in the batch counts as removed.
fn check_world(world: &World, schema: &WorldSchema, file: &str, scene: Option<&str>, ops: &[Op]) -> Result<(), writer::Refused> {
    let refuse = |at: usize, reason: &'static str, message: String| writer::Refused { at, reason, message };
    let places = placements(world, file, scene);
    let mut added: Vec<(&[String], String)> = Vec::new();
    let mut removed: Vec<(&[String], String)> = Vec::new();
    // A placeholder of the entity at `path` (`level-placeholders`): `component`, or its scene.
    let placeholder = |path: &[String], component: Option<&str>| -> bool {
        let tree = world.resource::<TsxTree>();
        let short = |c: &str| c.rsplit("::").next().unwrap_or(c).to_string();
        places.as_deref().unwrap_or_default().iter().filter_map(|p| tree.entity(node_id(&p.root, &p.key_of(path)?))).filter_map(|e| world.get::<MissingComponents>(e)).flat_map(|m| &m.0).any(|m| match component {
            Some(c) => m.reason != MissingReason::Mismatch && (m.component == c || short(&m.component) == short(c)),
            // An unset required prop (a mismatch) is fixed by setting it.
            None => m.scene && m.reason != MissingReason::Mismatch,
        })
    };
    let unknown = |at: usize, what: &str, component: &str| refuse(at, "unknown", format!("no live code registers {what} {component}"));
    for (at, op) in ops.iter().enumerate() {
        // Writes to a placeholder's fields are refused: its text stays as saved.
        match op {
            Op::Set { path, component, .. } | Op::Revert { path, component, .. } if placeholder(path, Some(component)) => {
                return Err(refuse(at, "missing", format!("{component} on {} is a placeholder: its code is missing, so its saved text is kept as it is", path.join("/"))));
            }
            Op::SetProp { path, .. } | Op::RevertProp { path, .. } if placeholder(path, None) => {
                return Err(refuse(at, "missing", format!("{}'s scene component is a placeholder: its code is missing, so its props are kept as saved", path.join("/"))));
            }
            Op::Set { component, .. } | Op::AddComponent { component, text: None, .. } if schema.component(component).is_err() => return Err(unknown(at, "a component named", component)),
            Op::Add { base, patches, .. } => {
                if let writer::Base::Scene { component, .. } = base
                    && schema.scene(component).is_err()
                {
                    return Err(unknown(at, "a scene component named", component));
                }
                if let Some(p) = patches.iter().find(|p| schema.component(&p.component).is_err()) {
                    return Err(unknown(at, "a component named", &p.component));
                }
            }
            _ => {}
        }
        match op {
            Op::AddComponent { path, component, .. } | Op::RemoveComponent { path, component } => {
                let Ok(full) = schema.component(component).map(|s| s.name) else { continue };
                let Some(places) = &places else {
                    return Err(refuse(at, "invalid", format!("no open level shows {file}: open one that places it to change its components")));
                };
                // Where each placement's component comes from, if from the file or below it.
                let sources: Vec<Option<&str>> = nodes_of(world, places, path)
                    .into_iter()
                    .filter_map(|(n, above)| n.components.get(&full).map(|c| (c, above)))
                    .filter(|(c, above)| !c.source.as_ref().is_some_and(|s| above.contains(&s.file)))
                    .map(|(c, _)| c.source.as_ref().map(|s| s.file.as_str()))
                    .collect();
                let key = (&path[..], full.clone());
                if matches!(op, Op::AddComponent { .. }) {
                    if !sources.is_empty() && !removed.contains(&key) {
                        return Err(refuse(at, "exists", format!("{} already has {full}", path.join("/"))));
                    }
                    removed.retain(|r| r != &key);
                    added.push(key);
                } else {
                    if sources.iter().any(|s| s.is_some_and(|s| s != file)) && !added.contains(&key) {
                        return Err(refuse(at, "prefab-internal", format!("{full} comes from {}'s prefab; .bsn cannot remove it", path.join("/"))));
                    }
                    added.retain(|a| a != &key);
                    removed.push(key);
                }
            }
            Op::Delete { path } | Op::Rename { path, .. } | Op::Reparent { path, .. } if scene.is_some() => {
                let places = places.as_deref().unwrap_or_default();
                if nodes_of(world, places, path).iter().any(|(n, _)| made_in_tsx(n)) {
                    return Err(refuse(at, "prefab-internal", format!("{} is made in TSX; edit it there", path.join("/"))));
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
