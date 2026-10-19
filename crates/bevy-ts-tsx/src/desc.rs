//! The scene description JS sends: one JSON document per generation in the manifest's `tsx`
//! section, one per runtime mount in a `tsx/mount` command. Prefabs are already expanded and
//! keys already assigned; Rust diffs these against what it last applied.

use bevy_ts::backend::SourceLocation;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The `tsx` manifest section: every `app.scene` of the generation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneSection {
    #[serde(default)]
    pub roots: Vec<RootDesc>,
    /// Errors the JSX runtime found while describing (a component outside an entity, a
    /// duplicate key), each at its element. Any fails the generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<bevy_ts::backend::ScriptError>,
}

/// One `app.scene(name, fn)`: its top-level entities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RootDesc {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    #[serde(default)]
    pub nodes: Vec<NodeDesc>,
}

/// One entity, or a control-flow slot (`<Show>`, `<For>`) whose content JS mounts later.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDesc {
    /// The explicit key, or `type#n` among siblings of the same type.
    pub key: String,
    /// `entity`, or the outermost prefab that produced it.
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The `ref={...}` this entity fills, by id within the generation.
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<u32>,
    /// Set on a slot: not an entity, a place where JS mounts groups of nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u32>,
    /// Picking handlers by Bevy event name (`PointerClick`), as handler ids of the generation.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub on: std::collections::BTreeMap<String, u32>,
    /// The element that made it, then each prefab call site above it, innermost first.
    #[serde(default)]
    pub source: Vec<SourceLocation>,
    #[serde(default)]
    pub components: Vec<ComponentDesc>,
    #[serde(default)]
    pub children: Vec<NodeDesc>,
}

impl NodeDesc {
    pub fn location(&self) -> Option<SourceLocation> {
        self.source.first().cloned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    /// A TS component from `component()`.
    Ts,
    /// A Rust component typed by bevy-ts (`Transform`, `Name`).
    Rust,
    /// `<mesh>`, `<material>`.
    Intrinsic,
}

/// One component element on an entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentDesc {
    #[serde(rename = "type")]
    pub ty: String,
    pub kind: ComponentKind,
    #[serde(default)]
    pub props: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

/// A node's identity: a stable hash (FNV-1a, 64 bit) of its root and key path, the same in
/// every run so protocol clients can hold on to it.
pub fn node_id(root: &str, path: &[String]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    // Length-prefixed, so no choice of key text can collide with another path.
    for seg in std::iter::once(root).chain(path.iter().map(String::as_str)) {
        eat(&(seg.len() as u32).to_le_bytes());
        eat(seg.as_bytes());
    }
    h
}

/// A file as the protocol shows it: project-relative when under the working directory, never a
/// `file://` URL.
pub fn display_file(file: &str) -> String {
    Files::new().display(file)
}

/// Shows files relative to the working directory, read once; each distinct file is worked out
/// once, as a section names the same few files thousands of times.
pub struct Files {
    cwd: Option<std::path::PathBuf>,
    seen: std::collections::HashMap<String, String>,
}

impl Files {
    pub fn new() -> Self {
        Files { cwd: std::env::current_dir().ok(), seen: Default::default() }
    }

    pub fn display(&mut self, file: &str) -> String {
        if let Some(d) = self.seen.get(file) {
            return d.clone();
        }
        let path = file.strip_prefix("file://").unwrap_or(file);
        let path = path.split('?').next().unwrap_or(path);
        let shown = match self.cwd.as_ref().and_then(|cwd| std::path::Path::new(path).strip_prefix(cwd).ok()) {
            Some(rel) => rel.display().to_string(),
            None => path.to_string(),
        };
        self.seen.insert(file.to_string(), shown.clone());
        shown
    }
}

impl Default for Files {
    fn default() -> Self {
        Files::new()
    }
}

/// Makes every source location in a node tree project-relative.
pub fn normalize_sources(nodes: &mut [NodeDesc]) {
    normalize_with(&mut Files::new(), nodes);
}

pub fn normalize_with(files: &mut Files, nodes: &mut [NodeDesc]) {
    for n in nodes {
        for s in &mut n.source {
            s.file = files.display(&s.file);
        }
        for c in &mut n.components {
            if let Some(s) = &mut c.source {
                s.file = files.display(&s.file);
            }
        }
        normalize_with(files, &mut n.children);
    }
}

/// A binding placeholder in a prop: `{"$b": id}`. Its value comes from the host call.
pub fn binding_id(v: &Value) -> Option<u32> {
    let o = v.as_object()?;
    if o.len() > 2 || (o.len() == 2 && !o.contains_key("$s")) {
        return None;
    }
    o.get("$b")?.as_u64().map(|n| n as u32)
}

/// The site of a binding on a native-layout field: `{"$b": id, "$s": site}`.
pub fn site_id(v: &Value) -> Option<u32> {
    binding_id(v)?;
    v.get("$s")?.as_u64().map(|n| n as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_ids_are_stable_and_path_sensitive() {
        let a = node_id("arena", &["a".into()]);
        assert_eq!(a, node_id("arena", &["a".into()]));
        assert_ne!(a, node_id("arena", &["b".into()]));
        assert_ne!(node_id("arena", &["a".into(), "b".into()]), node_id("arena", &["a\0b".into()]));
        assert_ne!(a, node_id("other", &["a".into()]));
    }
}
