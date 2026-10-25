//! `bevy-ts.json`: the project file. Found by walking up from the entry (or the working
//! directory). Only `native`, `features` and `engineCache` concern the engine build; `name` and
//! `entry` concern `bevy-ts build`; `levels`, `meshPicking` and `diagnostics` are read when the game runs.
//!
//! ```jsonc
//! {
//!   "name": "arena",                           // the built game's name; else the folder's
//!   "entry": "src/game.tsx",                   // the entry, relative to this file
//!   "native": {
//!     "crate": "native",                       // the project's own Rust, optional,
//!     "plugin": "native::GamePlugin",          // and its Bevy plugin
//!     "ports": ["rapier3d"],                   // curated bevy-ts ports
//!     "crates": { "bevy_kira_audio": { "version": "0.27", "plugin": "bevy_kira_audio::AudioPlugin" } }
//!   },
//!   "features": [],                            // Bevy features the engine adds
//!   "engineCache": "/mnt/team/engines",        // a shared directory of built engines
//!   "levels": { "prefabEdits": "override" },   // where an edit inside a placed prefab goes
//!   "meshPicking": true,                       // Bevy's MeshPickingPlugin; baked into a build
//!   "diagnostics": ["FrameTimeDiagnosticsPlugin"], // Bevy's diagnostics plugins; baked into a build
//!   "net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }, // hosts scripts may connect to
//!   "env": ["ABYME_URL", "ABYME_KEY"]          // the only variables `process.env` shows
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "bevy-ts.json";

/// An uncurated plugin crate adopted in config.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct CrateSpec {
    pub version: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default, rename = "defaultFeatures")]
    pub default_features: Option<bool>,
    /// The plugin to add: a path to a unit or `Default` plugin type.
    pub plugin: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct NativeConfig {
    /// Path to the project's native crate, relative to `bevy-ts.json`.
    #[serde(default, rename = "crate")]
    pub krate: Option<String>,
    /// The native crate's Bevy plugin, as a path (`native::GamePlugin`).
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub crates: BTreeMap<String, CrateSpec>,
}

impl NativeConfig {
    /// Whether the project needs a project engine at all.
    pub fn needs_engine(&self) -> bool {
        self.krate.is_some() || !self.ports.is_empty() || !self.crates.is_empty()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectConfig {
    /// The game's name in a build.
    #[serde(default)]
    pub name: Option<String>,
    /// The entry module, relative to the project root.
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub native: Option<NativeConfig>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default, rename = "engineCache")]
    pub engine_cache: Option<String>,
    #[serde(default)]
    pub levels: LevelsConfig,
    /// Adds Bevy's `MeshPickingPlugin`: 3D meshes take pointer events. Off by default, as in Bevy.
    #[serde(default, rename = "meshPicking", deserialize_with = "mesh_picking")]
    pub mesh_picking: bool,
    /// Bevy's diagnostics plugins to add, by Bevy's names (docs/diagnostics.md). None by default.
    #[serde(default, deserialize_with = "diagnostics")]
    pub diagnostics: Vec<String>,
    /// What scripts may connect to (docs/networking.md).
    #[serde(default)]
    pub net: NetConfig,
    /// The environment variables `process.env` shows; read when the engine starts.
    #[serde(default)]
    pub env: Vec<String>,
}

/// `meshPicking` as a bool, its error naming the field (serde's names only the type).
fn mesh_picking<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => Ok(b),
        v => Err(serde::de::Error::custom(format!("meshPicking must be true or false, not {v}"))),
    }
}

/// `diagnostics` as a list of plugin names bevy-ts adds, its error naming the bad one.
fn diagnostics<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let names = Vec::<String>::deserialize(d).map_err(|e| serde::de::Error::custom(format!("diagnostics must be a list of plugin names: {e}")))?;
    crate::diagnostics::check_names(&names).map_err(serde::de::Error::custom)?;
    Ok(names)
}

/// `net` in `bevy-ts.json` and `game.json`: deny by default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetConfig {
    /// `host`, `host:port`, a URL, or `${NAME}` read from the environment at start.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// `levels` in `bevy-ts.json`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct LevelsConfig {
    /// `override` (the default) or `source`: where a viewport edit inside a placed prefab goes.
    #[serde(default, rename = "prefabEdits")]
    pub prefab_edits: Option<String>,
}

/// A project: its root (the directory holding `bevy-ts.json`) and config.
#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub config: ProjectConfig,
}

impl Project {
    /// The native config when the project needs a project engine.
    pub fn native(&self) -> Option<&NativeConfig> {
        self.config.native.as_ref().filter(|n| n.needs_engine())
    }

    /// `.bevy-ts/` under the root.
    pub fn state_dir(&self) -> PathBuf {
        self.root.join(".bevy-ts")
    }
}

/// Strips `//` and `/* */` comments outside strings, so `bevy-ts.json` may be JSONC.
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    if n == '\n' {
                        out.push('\n');
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn parse(text: &str) -> Result<ProjectConfig, String> {
    serde_json::from_str(&strip_comments(text)).map_err(|e| e.to_string())
}

/// Finds `bevy-ts.json` in `start` or an ancestor. `Ok(None)` when there is none.
pub fn find(start: &Path) -> Result<Option<Project>, String> {
    let start = std::path::absolute(start).map_err(|e| e.to_string())?;
    let mut dir = if start.is_dir() { start.as_path() } else { start.parent().unwrap_or(&start) };
    loop {
        let file = dir.join(FILE);
        if file.is_file() {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let config = parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            return Ok(Some(Project { root: dir.to_path_buf(), config }));
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_with_ports_and_crates() {
        let c = parse(
            r#"{ "native": {
                "ports": ["rapier3d"], // curated
                /* uncurated */ "crates": { "bevy_kira_audio": { "version": "0.27", "plugin": "a::B" } }
            }, "engineCache": "/team/engines" }"#,
        )
        .unwrap();
        let n = c.native.unwrap();
        assert_eq!(n.ports, vec!["rapier3d"]);
        assert_eq!(n.crates["bevy_kira_audio"].plugin, "a::B");
        assert!(n.krate.is_none() && n.needs_engine());
        assert_eq!(c.engine_cache.as_deref(), Some("/team/engines"));
    }

    #[test]
    fn no_native_needs_no_engine() {
        let c = parse(r#"{ "native": {} }"#).unwrap();
        assert!(!c.native.unwrap().needs_engine());
        let s = parse(r#"{ "a": "// not a comment" }"#);
        assert!(s.is_ok());
    }

    #[test]
    fn name_and_entry_are_optional() {
        let c = parse(r#"{ "name": "arena", "entry": "src/main.ts" }"#).unwrap();
        assert_eq!(c.name.as_deref(), Some("arena"));
        assert_eq!(c.entry.as_deref(), Some("src/main.ts"));
        let c = parse("{}").unwrap();
        assert!(c.name.is_none() && c.entry.is_none());
    }

    #[test]
    fn diagnostics_is_a_list_of_bevy_plugins_none_by_default() {
        assert!(parse("{}").unwrap().diagnostics.is_empty());
        let c = parse(r#"{ "diagnostics": ["FrameTimeDiagnosticsPlugin", "RenderAssetDiagnosticPlugin<RenderMesh>"] }"#).unwrap();
        assert_eq!(c.diagnostics, ["FrameTimeDiagnosticsPlugin", "RenderAssetDiagnosticPlugin<RenderMesh>"]);
        let e = parse(r#"{ "diagnostics": ["FrameTimeDiagnosticPlugin"] }"#).unwrap_err();
        assert!(e.contains("\"FrameTimeDiagnosticPlugin\" is not a diagnostics plugin") && e.contains("FrameTimeDiagnosticsPlugin"), "{e}");
        let e = parse(r#"{ "diagnostics": ["LogDiagnosticsPlugin", "LogDiagnosticsPlugin"] }"#).unwrap_err();
        assert!(e.contains("listed twice"), "{e}");
    }

    #[test]
    fn mesh_picking_is_a_bool_off_by_default() {
        assert!(!parse("{}").unwrap().mesh_picking);
        assert!(parse(r#"{ "meshPicking": true }"#).unwrap().mesh_picking);
        assert!(!parse(r#"{ "meshPicking": false }"#).unwrap().mesh_picking);
        let e = parse(r#"{ "meshPicking": "yes" }"#).unwrap_err();
        assert!(e.contains("meshPicking") && e.contains("\"yes\""), "{e}");
    }
}
