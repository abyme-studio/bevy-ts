//! Writes in the engine (design decision 8): the disk wins. A batch names the content hash it was
//! made against; when the file on disk has moved on, the world first follows the disk, then the
//! batch is replayed by name path on the new text, or refused as a conflict. Never a text merge.

use crate::engine::{BsnLevel, Levels, content_hash, reconcile_level, root_name};
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
}

impl WriteResult {
    fn refused(req: &WriteRequest, status: &'static str, reason: &str, message: impl Into<String>) -> Self {
        WriteResult { id: req.id, file: req.file.clone(), status, reason: Some(reason.into()), message: message.into(), hash: None, lines: Vec::new(), disk: None, inverse: Vec::new() }
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
    let written = match writer::apply(&disk, &req.ops) {
        Ok(w) => w,
        Err(e) if status == "rebased" && e.reason == "not-found" => return WriteResult::refused(req, "conflict", "target removed", e.message),
        Err(e) => return WriteResult::refused(req, "refused", e.reason, e.message),
    };
    if let Err(e) = write_atomic(&disk_path, &written.text) {
        return WriteResult::refused(req, "refused", "io", format!("{}: {e}", disk_path.display()));
    }
    // Renames keep their entities: the retained tree moves before the reload sees the file.
    let roots: Vec<String> = world.resource::<Levels>().roots.keys().filter(|p| *p == &req.file).cloned().collect();
    for root in roots {
        for (from, to) in &written.renamed {
            bevy_ts_tsx::rekey(world, &root_name(&root), from, to);
        }
    }
    let hash = content_hash(written.text.as_bytes());
    if let Err(e) = load_text(world, &req.file, &written.text) {
        return WriteResult::refused(req, "refused", "internal", e);
    }
    reconcile_affected(world, &req.file);
    WriteResult { id: req.id, file: req.file.clone(), status, reason: None, message: String::new(), hash: Some(hash_text(hash)), lines: written.lines, disk: None, inverse: written.inverse }
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
