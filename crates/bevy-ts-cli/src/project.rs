//! `bevy-ts.json`: the project file. Found by walking up from the entry (or the working
//! directory). Only `native`, `features` and `engineCache` concern the engine build; `name` and
//! `entry` concern `bevy-ts build`; `levels` and the switches (`meshPicking`, `tabNavigation`,
//! `directionalNavigation`, `imageSampler`) and `diagnostics` are read when the game runs.
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
//!   "tabNavigation": true,                     // Bevy's TabNavigationPlugin: Tab moves focus
//!   "directionalNavigation": true,             // Bevy's DirectionalNavigationPlugin
//!   "diagnostics": ["FrameTimeDiagnosticsPlugin"], // Bevy's diagnostics plugins; baked into a build
//!   "imageSampler": "nearest",                 // ImagePlugin's default sampler; baked into a build
//!   "assetSources": { "stardew": "~/dev/assets/stardew" }, // folders loaded as `stardew://...`
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
    /// The Bevy plugins a project opts into.
    #[serde(flatten)]
    pub switches: Switches,
    /// Bevy's diagnostics plugins to add, by Bevy's names (docs/diagnostics.md). None by default.
    #[serde(default, deserialize_with = "diagnostics")]
    pub diagnostics: Vec<String>,
    /// What scripts may connect to (docs/networking.md).
    #[serde(default)]
    pub net: NetConfig,
    /// The environment variables `process.env` shows; read when the engine starts.
    #[serde(default)]
    pub env: Vec<String>,
    /// Extra asset folders, each a Bevy asset source loaded as `name://path` (docs/assets.md):
    /// name to folder as written, absolute, `~/` or relative to `bevy-ts.json`.
    #[serde(default, rename = "assetSources", deserialize_with = "asset_sources")]
    pub asset_sources: BTreeMap<String, String>,
}

/// `assetSources`, each name checked: Bevy's prefix, so lowercase letters, digits and `-`, and not
/// `embedded`, Bevy's own.
fn asset_sources<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let map = BTreeMap::<String, String>::deserialize(d).map_err(|e| serde::de::Error::custom(format!("assetSources must map names to folders: {e}")))?;
    for name in map.keys() {
        check_source_name(name).map_err(serde::de::Error::custom)?;
    }
    Ok(map)
}

pub fn check_source_name(name: &str) -> Result<(), String> {
    if name == "embedded" {
        return Err("assetSources: \"embedded\" is Bevy's own source; name it otherwise".into());
    }
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(format!("assetSources: {name:?} is not a source name: lowercase letters, digits and -"));
    }
    Ok(())
}

/// A source folder as written, resolved: `~/` under `home`, a relative one under `base`, made
/// canonical when it exists (Bevy's watcher reports canonical paths, as for the default root).
pub fn resolve_source(folder: &str, base: &Path, home: Option<&Path>) -> PathBuf {
    let path = match (folder.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => base.join(folder),
    };
    let path = std::path::absolute(&path).unwrap_or(path);
    path.canonicalize().unwrap_or(path)
}

/// This machine's home folder, for `~/`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Bevy plugins a project opts into, each off by default as Bevy keeps it out of
/// `DefaultPlugins`, and `ImagePlugin`'s default sampler: read from `bevy-ts.json` under `dev` and from `game.json` when playing, where
/// `bevy-ts build` writes each that is on. None is an engine hash input: the prebuilt engine has
/// every plugin, and `base_app` adds the ones switched on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Switches {
    /// `MeshPickingPlugin`: 3D meshes and `<mesh2d>`s take pointer events.
    #[serde(default, rename = "meshPicking", deserialize_with = "mesh_picking", skip_serializing_if = "is_false")]
    pub mesh_picking: bool,
    /// `TabNavigationPlugin`: Tab and Shift-Tab move focus in tab order.
    #[serde(default, rename = "tabNavigation", deserialize_with = "tab_navigation", skip_serializing_if = "is_false")]
    pub tab_navigation: bool,
    /// `DirectionalNavigationPlugin`: the map `DirectionalNavigation` navigates.
    #[serde(default, rename = "directionalNavigation", deserialize_with = "directional_navigation", skip_serializing_if = "is_false")]
    pub directional_navigation: bool,
    /// `imageSampler`: `ImagePlugin`'s sampler for every image whose sampler is `Default`.
    #[serde(default, rename = "imageSampler", skip_serializing_if = "DefaultSampler::is_linear")]
    pub image_sampler: DefaultSampler,
}

/// `imageSampler` in `bevy-ts.json`: Bevy's default, linear, or `ImagePlugin::default_nearest()`
/// for pixel art. An image given its own sampler keeps it.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultSampler {
    #[default]
    Linear,
    Nearest,
}

impl DefaultSampler {
    fn is_linear(&self) -> bool {
        *self == DefaultSampler::Linear
    }
}

impl<'de> Deserialize<'de> for DefaultSampler {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) if s == "linear" => Ok(DefaultSampler::Linear),
            serde_json::Value::String(s) if s == "nearest" => Ok(DefaultSampler::Nearest),
            v => Err(serde::de::Error::custom(format!("imageSampler must be \"nearest\" or \"linear\", not {v}"))),
        }
    }
}

fn is_false(b: &bool) -> bool {
    !b
}

/// A switch as a bool, its error naming the field (serde's names only the type).
fn switch<'de, D: serde::Deserializer<'de>>(d: D, field: &str) -> Result<bool, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => Ok(b),
        v => Err(serde::de::Error::custom(format!("{field} must be true or false, not {v}"))),
    }
}

fn mesh_picking<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "meshPicking")
}

fn tab_navigation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "tabNavigation")
}

fn directional_navigation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "directionalNavigation")
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

    /// `assetSources`, each folder resolved on this machine.
    pub fn asset_sources(&self) -> Vec<(String, PathBuf)> {
        let home = home();
        self.config.asset_sources.iter().map(|(name, folder)| (name.clone(), resolve_source(folder, &self.root, home.as_deref()))).collect()
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
    fn switches_are_bools_off_by_default() {
        assert_eq!(parse("{}").unwrap().switches, Switches::default());
        let on = parse(r#"{ "meshPicking": true, "tabNavigation": true, "directionalNavigation": true }"#).unwrap().switches;
        assert_eq!(on, Switches { mesh_picking: true, tab_navigation: true, directional_navigation: true, image_sampler: DefaultSampler::Linear });
        assert!(!parse(r#"{ "meshPicking": false }"#).unwrap().switches.mesh_picking);
        for field in ["meshPicking", "tabNavigation", "directionalNavigation"] {
            let e = parse(&format!(r#"{{ "{field}": "yes" }}"#)).unwrap_err();
            assert!(e.contains(field) && e.contains("\"yes\""), "{e}");
        }
    }

    #[test]
    fn image_sampler_is_nearest_or_linear() {
        assert_eq!(parse("{}").unwrap().switches.image_sampler, DefaultSampler::Linear);
        assert_eq!(parse(r#"{ "imageSampler": "nearest" }"#).unwrap().switches.image_sampler, DefaultSampler::Nearest);
        assert_eq!(parse(r#"{ "imageSampler": "linear" }"#).unwrap().switches.image_sampler, DefaultSampler::Linear);
        let e = parse(r#"{ "imageSampler": "Nearest" }"#).unwrap_err();
        assert!(e.contains("imageSampler") && e.contains("\"Nearest\""), "{e}");
        // Written to game.json only when nearest.
        assert_eq!(serde_json::to_string(&Switches::default()).unwrap(), "{}");
        let on = Switches { image_sampler: DefaultSampler::Nearest, ..Default::default() };
        assert_eq!(serde_json::to_string(&on).unwrap(), r#"{"imageSampler":"nearest"}"#);
    }

    #[test]
    fn asset_source_names_are_checked() {
        let c = parse(r#"{ "assetSources": { "stardew": "~/dev/assets/stardew", "ui-2": "art" } }"#).unwrap();
        assert_eq!(c.asset_sources.len(), 2);
        for bad in ["Stardew", "embedded", "", "a_b", "a/b"] {
            let e = parse(&format!(r#"{{ "assetSources": {{ "{bad}": "x" }} }}"#)).unwrap_err();
            assert!(e.contains("assetSources") && e.contains(&format!("\"{bad}\"")), "{bad}: {e}");
        }
    }

    #[test]
    fn a_source_folder_resolves_from_home_the_project_or_as_written() {
        let (base, home) = (Path::new("/nowhere/game"), Path::new("/nowhere/me"));
        assert_eq!(resolve_source("~/dev/art", base, Some(home)), PathBuf::from("/nowhere/me/dev/art"));
        assert_eq!(resolve_source("../art", base, Some(home)), PathBuf::from("/nowhere/game/../art"));
        assert_eq!(resolve_source("/srv/art", base, Some(home)), PathBuf::from("/srv/art"));
        // An existing folder is made canonical.
        let here = std::env::current_dir().unwrap();
        assert_eq!(resolve_source(".", &here, None), here.canonicalize().unwrap());
    }
}
