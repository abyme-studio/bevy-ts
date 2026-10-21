//! Project engines (native decisions 1, 2 and 4): the prebuilt engine plus the project's native
//! crate and adopted plugins, generated as a binary crate under `.bevy-ts/engine/`, built by
//! `cargo` only when its Rust inputs change, and cached by a content hash of those inputs in
//! `.bevy-ts/engines/<hash>/` and, optionally, a shared cache.

use crate::project::{NativeConfig, Project};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// This bevy-ts's version: the npm package's, the crates' and the native crate's pin.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The target triple this engine was built for.
pub const TARGET: &str = env!("BEVY_TS_TARGET");
/// The Bevy version this engine was built with.
pub const BEVY_VERSION: &str = env!("BEVY_TS_BEVY_VERSION");
/// Bumped when the generated engine crate changes shape, so old cache entries miss.
const GENERATOR: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// `bevy-ts dev`: the dev profile with dependencies optimised.
    Dev,
    /// `bevy-ts build`.
    Release,
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Profile::Dev => "dev",
            Profile::Release => "release",
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Profile::Dev => "debug",
            Profile::Release => "release",
        }
    }
}

pub fn exe_name() -> &'static str {
    if cfg!(windows) { "bevy-ts.exe" } else { "bevy-ts" }
}

/// Where the generated engine takes bevy-ts's own crates from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// crates.io at exactly [`VERSION`].
    Registry,
    /// A bevy-ts checkout (engine development, this repository's examples).
    Path(PathBuf),
}

impl Source {
    /// `BEVY_TS_CRATES=registry` or `=<checkout>`; otherwise the checkout this binary was built
    /// from when it still exists, else crates.io.
    pub fn detect() -> Source {
        match std::env::var("BEVY_TS_CRATES") {
            Ok(v) if v == "registry" => return Source::Registry,
            Ok(v) if !v.is_empty() => return Source::Path(PathBuf::from(v)),
            _ => {}
        }
        let root = PathBuf::from(env!("BEVY_TS_SOURCE_ROOT"));
        if root.join("crates/bevy-ts-cli/Cargo.toml").is_file() { Source::Path(root) } else { Source::Registry }
    }

    fn dep(&self, krate: &str) -> String {
        match self {
            Source::Registry => format!("{krate} = \"={VERSION}\""),
            Source::Path(root) => format!("{krate} = {{ path = {:?} }}", root.join("crates").join(krate).display().to_string()),
        }
    }

    fn has_port(&self, port: &str) -> bool {
        match self {
            Source::Registry => true,
            Source::Path(root) => root.join("crates").join(format!("bevy-ts-{port}")).join("Cargo.toml").is_file(),
        }
    }
}

/// The project's native crate, read from its `Cargo.toml`.
#[derive(Clone, Debug)]
pub struct NativePkg {
    pub name: String,
    pub version: String,
    /// The library's crate name (`[lib] name`, or the package name with `-` as `_`).
    pub lib: String,
    pub dir: PathBuf,
    /// The Cargo workspace root it belongs to (its own directory when it has none).
    pub workspace_root: PathBuf,
    /// Packages it depends on by path, transitively: their files enter the hash too.
    pub path_deps: Vec<PathBuf>,
}

fn read_toml(path: &Path) -> Result<toml::Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.parse::<toml::Table>().map_err(|e| format!("{}: {e}", path.display()))
}

fn path_deps(dir: &Path, manifest: &toml::Table, out: &mut Vec<PathBuf>) {
    for key in ["dependencies", "build-dependencies"] {
        let Some(deps) = manifest.get(key).and_then(|d| d.as_table()) else { continue };
        for spec in deps.values() {
            let Some(p) = spec.get("path").and_then(|p| p.as_str()) else { continue };
            let Ok(p) = dir.join(p).canonicalize() else { continue };
            if out.contains(&p) {
                continue;
            }
            out.push(p.clone());
            if let Ok(m) = read_toml(&p.join("Cargo.toml")) {
                path_deps(&p, &m, out);
            }
        }
    }
}

/// The workspace a package belongs to: the nearest ancestor whose `[workspace]` lists it in
/// `members` (exactly or by a trailing `/*`), or the package itself when it declares
/// `[workspace]` or no ancestor claims it.
fn workspace_root(dir: &Path, manifest: &toml::Table) -> PathBuf {
    if manifest.contains_key("workspace") {
        return dir.to_path_buf();
    }
    let mut at = dir.parent();
    while let Some(d) = at {
        if let Ok(m) = read_toml(&d.join("Cargo.toml"))
            && let Some(ws) = m.get("workspace").and_then(|w| w.as_table())
        {
            let rel = dir.strip_prefix(d).unwrap_or(dir).to_string_lossy().replace('\\', "/");
            let listed = |key: &str| {
                ws.get(key).and_then(|m| m.as_array()).is_some_and(|list| {
                    list.iter().filter_map(|v| v.as_str()).any(|m| {
                        m.trim_end_matches('/') == rel
                            || m.strip_suffix("/*").is_some_and(|p| rel.rsplit_once('/').is_some_and(|(parent, _)| parent == p))
                    })
                })
            };
            if listed("members") && !listed("exclude") {
                return d.to_path_buf();
            }
        }
        at = d.parent();
    }
    dir.to_path_buf()
}

impl NativePkg {
    pub fn read(dir: &Path) -> Result<NativePkg, String> {
        let dir = dir.canonicalize().map_err(|e| format!("native crate {}: {e}", dir.display()))?;
        let manifest = read_toml(&dir.join("Cargo.toml"))?;
        let package = manifest
            .get("package")
            .and_then(|p| p.as_table())
            .ok_or_else(|| format!("{}: no [package]; the native crate must be a library package", dir.display()))?;
        let name = package.get("name").and_then(|n| n.as_str()).ok_or("native crate has no package name")?.to_string();
        let version = package.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string();
        let lib = manifest
            .get("lib")
            .and_then(|l| l.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| name.replace('-', "_"));
        if !dir.join("src/lib.rs").is_file() && manifest.get("lib").and_then(|l| l.get("path")).is_none() {
            return Err(format!("native crate {name} has no library target (src/lib.rs)"));
        }
        let workspace_root = workspace_root(&dir, &manifest);
        let mut deps = Vec::new();
        path_deps(&dir, &manifest, &mut deps);
        // bevy-ts's own crates come from the checkout or crates.io; they are not project inputs.
        deps.retain(|p| !p.join("src/cli.rs").exists() && !p.ends_with("bevy-ts-native") && !p.ends_with("bevy-ts"));
        Ok(NativePkg { name, version, lib, dir, workspace_root, path_deps: deps })
    }

    fn lock_path(&self) -> PathBuf {
        self.workspace_root.join("Cargo.lock")
    }
}

/// Everything that decides a project engine.
#[derive(Clone, Debug)]
pub struct Plan {
    pub root: PathBuf,
    pub native: NativeConfig,
    pub pkg: Option<NativePkg>,
    pub features: Vec<String>,
    pub cache: Option<String>,
    pub profile: Profile,
    pub source: Source,
}

/// Files and directories whose contents decide the hash, for watching.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub files: BTreeSet<PathBuf>,
    pub dirs: BTreeSet<PathBuf>,
}

fn skip_dir(name: &str) -> bool {
    name == "target" || name == "node_modules" || name.starts_with('.')
}

/// Every file under `dir` Cargo would package, approximately: everything but `target/`,
/// `node_modules/` and hidden entries. Sorted.
fn package_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !skip_dir(&name) {
                package_files(&path, out);
            }
        } else if ft.is_file() && !name.starts_with('.') {
            out.push(path);
        }
    }
}

fn package_dirs(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    out.insert(dir.to_path_buf());
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.file_type().is_ok_and(|t| t.is_dir()) && !skip_dir(&name) {
            package_dirs(&e.path(), out);
        }
    }
}

impl Plan {
    /// The project's engine plan, or `None` when it runs on the prebuilt engine.
    pub fn new(project: &Project, profile: Profile) -> Result<Option<Plan>, String> {
        let Some(native) = project.native().cloned() else { return Ok(None) };
        let pkg = match &native.krate {
            Some(dir) => Some(NativePkg::read(&project.root.join(dir))?),
            None => None,
        };
        if pkg.is_some() && native.plugin.is_none() {
            return Err("bevy-ts.json: native.crate needs native.plugin, the path of its Bevy plugin (\"native::GamePlugin\")".into());
        }
        let source = Source::detect();
        for port in &native.ports {
            if !source.has_port(port) {
                return Err(format!("bevy-ts.json: no bevy-ts port named {port:?}"));
            }
        }
        Ok(Some(Plan {
            root: project.root.clone(),
            native,
            pkg,
            features: project.config.features.clone(),
            cache: project.config.engine_cache.clone(),
            profile,
            source,
        }))
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join(".bevy-ts/engine")
    }

    pub fn cache_dir(&self, hash: &str) -> PathBuf {
        self.root.join(".bevy-ts/engines").join(hash)
    }

    /// The generated engine's `Cargo.toml`.
    pub fn manifest(&self) -> String {
        let mut s = String::from(
            "# Generated by bevy-ts from bevy-ts.json. Do not edit; it is rewritten before every build.\n\
             [package]\nname = \"bevy-ts-engine\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
             [[bin]]\nname = \"bevy-ts-engine\"\npath = \"main.rs\"\n\n\
             # Its own workspace, whatever surrounds it.\n[workspace]\n\n[dependencies]\n",
        );
        for krate in ["bevy-ts-cli", "bevy-ts-native"] {
            let _ = writeln!(s, "{}", self.source.dep(krate));
        }
        if !self.features.is_empty() {
            let _ = writeln!(s, "bevy = {{ version = \"={BEVY_VERSION}\", default-features = false, features = {:?} }}", self.features);
        }
        if let Some(p) = &self.pkg {
            let _ = writeln!(s, "{} = {{ path = {:?} }}", p.name, p.dir.display().to_string());
        }
        for port in &self.native.ports {
            let _ = writeln!(s, "{}", self.source.dep(&format!("bevy-ts-{port}")));
        }
        for (name, c) in &self.native.crates {
            let _ = write!(s, "{name} = {{ version = {:?}", c.version);
            if !c.features.is_empty() {
                let _ = write!(s, ", features = {:?}", c.features);
            }
            if let Some(d) = c.default_features {
                let _ = write!(s, ", default-features = {d}");
            }
            s.push_str(" }\n");
        }
        s.push_str("\n[profile.dev]\nopt-level = 1\n\n[profile.dev.package.\"*\"]\nopt-level = 3\n");
        s
    }

    /// Every plugin the engine adds, as Rust paths: ports, adopted crates, the project's own.
    pub fn plugins(&self) -> Vec<String> {
        let mut v: Vec<String> = self.native.ports.iter().map(|p| format!("bevy_ts_{}::PortPlugin", p.replace('-', "_"))).collect();
        v.extend(self.native.crates.values().map(|c| c.plugin.clone()));
        v.extend(self.native.plugin.clone());
        v
    }

    /// The generated engine's `main.rs`: the prebuilt engine's entry with one plugin adding the
    /// ports', the adopted crates' and the project's plugins. Each is a unit or `Default` type;
    /// a glob-imported fallback const stands in the value namespace when it is not a unit.
    pub fn main_rs(&self, hash: &str) -> String {
        let mut s = String::from(
            "// Generated by bevy-ts from bevy-ts.json. Do not edit; it is rewritten before every build.\n\n\
             use bevy_ts_cli::__plugin::Make;\nuse bevy_ts_cli::bevy::app::{App, Plugin};\n\n",
        );
        let plugins = self.plugins();
        for (i, path) in plugins.iter().enumerate() {
            let last = path.rsplit("::").next().unwrap_or(path);
            let _ = write!(
                s,
                "#[allow(non_upper_case_globals, unused_imports, dead_code)]\nmod plugin_{i} {{\n    mod fallback {{\n        pub const {last}: bevy_ts_cli::__plugin::UseDefault = bevy_ts_cli::__plugin::UseDefault;\n    }}\n    pub use fallback::*;\n    pub use {path};\n}}\n\n"
            );
        }
        s.push_str("struct ProjectPlugins;\n\nimpl Plugin for ProjectPlugins {\n    fn build(&self, app: &mut App) {\n");
        for (i, path) in plugins.iter().enumerate() {
            let last = path.rsplit("::").next().unwrap_or(path);
            let _ = writeln!(s, "        app.add_plugins(Make::<plugin_{i}::{last}>::make(plugin_{i}::{last}));");
        }
        s.push_str("    }\n}\n\nfn main() {\n");
        let native = match &self.pkg {
            Some(p) => format!("Some(bevy_ts_cli::NativeIdentity {{ name: {:?}.into(), version: {:?}.into() }})", p.name, p.version),
            None => "None".into(),
        };
        let _ = writeln!(s, "    let identity = bevy_ts_cli::EngineIdentity::project({hash:?}, {native});");
        s.push_str("    bevy_ts_cli::run_engine(identity, ProjectPlugins);\n}\n");
        s
    }

    /// Files and directories that decide the hash.
    pub fn inputs(&self) -> Inputs {
        let mut i = Inputs::default();
        i.files.insert(self.root.join(crate::project::FILE));
        if let Some(p) = &self.pkg {
            i.files.insert(p.lock_path());
            i.files.insert(p.workspace_root.join("Cargo.toml"));
            for d in std::iter::once(&p.dir).chain(&p.path_deps) {
                package_dirs(d, &mut i.dirs);
            }
        }
        i
    }

    /// The engine hash: bevy-ts version, target, profile, features, the native config, and for a
    /// native crate its `Cargo.lock`, its workspace manifest and every packaged file of it and its
    /// path dependencies. Never TS, assets or the rest of `bevy-ts.json`.
    pub fn hash(&self) -> Result<String, String> {
        let mut h = Sha256::new();
        let mut feed = |label: &str, bytes: &[u8]| {
            h.update((label.len() as u64).to_le_bytes());
            h.update(label.as_bytes());
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(bytes);
        };
        feed("bevy-ts", VERSION.as_bytes());
        feed("generator", GENERATOR.as_bytes());
        feed("target", TARGET.as_bytes());
        feed("profile", self.profile.name().as_bytes());
        let mut features = self.features.clone();
        features.sort();
        feed("features", features.join(",").as_bytes());
        feed("ports", self.native.ports.join(",").as_bytes());
        for (name, c) in &self.native.crates {
            let spec = format!("{}|{}|{:?}|{}", c.version, c.features.join(","), c.default_features, c.plugin);
            feed(&format!("crate {name}"), spec.as_bytes());
        }
        if let Source::Path(root) = &self.source {
            // Engine development: the checkout's crates are inputs too. The binary that runs this
            // was built from them, so its size and modification time stand in for their contents.
            let stamp = std::env::current_exe()
                .and_then(std::fs::metadata)
                .map(|m| format!("{}|{:?}", m.len(), m.modified().ok()))
                .unwrap_or_default();
            feed("source", format!("{}|{stamp}", root.display()).as_bytes());
            // Ports built from the checkout are not part of that binary: their files count.
            for port in &self.native.ports {
                let dir = root.join("crates").join(format!("bevy-ts-{port}"));
                let mut files = Vec::new();
                package_files(&dir, &mut files);
                for f in files {
                    let rel = f.strip_prefix(&dir).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                    feed(&format!("port {port}:{rel}"), &std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?);
                }
            }
        }
        if let Some(p) = &self.pkg {
            let lock = p.lock_path();
            let bytes = std::fs::read(&lock).map_err(|e| format!("{}: {e}", lock.display()))?;
            feed("Cargo.lock", &bytes);
            let ws = p.workspace_root.join("Cargo.toml");
            feed("workspace Cargo.toml", &std::fs::read(&ws).map_err(|e| format!("{}: {e}", ws.display()))?);
            for (n, d) in std::iter::once(&p.dir).chain(&p.path_deps).enumerate() {
                let mut files = Vec::new();
                package_files(d, &mut files);
                for f in files {
                    let rel = f.strip_prefix(d).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                    let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                    feed(&format!("{n}:{rel}"), &bytes);
                }
            }
        }
        let digest = h.finalize();
        Ok(digest[..16].iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Compares the resolved `bevy-ts-native` and `bevy` in the native crate's `Cargo.lock` with
    /// this engine's, before anything compiles. Generates the lock first when there is none.
    pub fn check_lock(&self) -> Result<(), String> {
        let Some(p) = &self.pkg else { return Ok(()) };
        let lock = p.lock_path();
        if !lock.is_file() {
            let status = cargo()?
                .args(["generate-lockfile", "--manifest-path"])
                .arg(p.workspace_root.join("Cargo.toml"))
                .status()
                .map_err(|e| format!("cargo generate-lockfile: {e}"))?;
            if !status.success() {
                return Err(format!("cargo generate-lockfile failed for {}", p.name));
            }
        }
        check_lock_file(&lock)
    }
}

/// The versions a lock must resolve, by package.
fn expected() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![("bevy-ts-native", VERSION)];
    if !BEVY_VERSION.is_empty() {
        v.push(("bevy", BEVY_VERSION));
    }
    v
}

pub fn check_lock_file(lock: &Path) -> Result<(), String> {
    let t = read_toml(lock)?;
    let packages = t.get("package").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    for (name, want) in expected() {
        let found: Vec<&str> = packages
            .iter()
            .filter(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
            .filter_map(|p| p.get("version").and_then(|v| v.as_str()))
            .collect();
        if let Some(bad) = found.iter().find(|v| **v != want) {
            return Err(format!(
                "{}: {name} resolves to {bad}, but this bevy-ts needs ={want}; pin `{name} = \"={want}\"` and run `cargo update -p {name}`",
                lock.display()
            ));
        }
    }
    Ok(())
}

fn cargo() -> Result<Command, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    match Command::new(&cargo).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status() {
        Ok(s) if s.success() => Ok(Command::new(cargo)),
        _ => Err("no Rust toolchain: `cargo` is not on PATH".into()),
    }
}

/// SHA-256 of a file, hex.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

/// Copies `src` into `dir` as the engine binary with its checksum beside it.
fn install(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dst = dir.join(exe_name());
    let tmp = dir.join(format!("{}.tmp", exe_name()));
    std::fs::copy(src, &tmp).map_err(|e| format!("{} -> {}: {e}", src.display(), tmp.display()))?;
    let sum = sha256_file(&tmp)?;
    std::fs::write(dir.join(format!("{}.sha256", exe_name())), format!("{sum}\n")).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    Ok(dst)
}

/// Where an engine came from, for the log line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    LocalCache,
    SharedCache,
    Built,
}

/// Copies an engine from the shared directory cache (`<dir>/<hash>/<target>/`) into the local
/// one, verified against the SHA-256 beside it. `Ok(None)` on a miss. The checksum proves the
/// copy is intact, not who wrote it: whoever can write the cache can replace both.
pub fn fetch_shared(cache: &Path, hash: &str, into: &Path) -> Result<Option<PathBuf>, String> {
    let dir = cache.join(hash).join(TARGET);
    let src = dir.join(exe_name());
    if !src.is_file() {
        return Ok(None);
    }
    let tmp_dir = into.with_extension("fetch");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let bin = tmp_dir.join(exe_name());
    std::fs::copy(&src, &bin).map_err(|e| format!("{}: {e}", src.display()))?;
    let want = std::fs::read_to_string(dir.join(format!("{}.sha256", exe_name()))).unwrap_or_default();
    let want = want.split_whitespace().next().unwrap_or("").to_lowercase();
    let have = sha256_file(&bin)?;
    if want != have {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!(
            "shared cache engine {hash}: SHA-256 mismatch (recorded {}, got {have}); not run",
            if want.is_empty() { "nothing" } else { &want }
        ));
    }
    let dst = install(&bin, into)?;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    make_executable(&dst)?;
    Ok(Some(dst))
}

pub fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        p.set_mode(p.mode() | 0o755);
        std::fs::set_permissions(path, p).map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}

/// Writes `.bevy-ts/engine/` for a hash.
pub fn generate(plan: &Plan, hash: &str) -> Result<PathBuf, String> {
    let dir = plan.engine_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: &str, text: String| {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok::<_, String>(())
    };
    write("Cargo.toml", plan.manifest())?;
    write("main.rs", plan.main_rs(hash))?;
    if let Some(p) = &plan.pkg {
        // The project's lock decides the native crate's dependencies; cargo adds the engine's own.
        let lock = std::fs::read_to_string(p.lock_path()).map_err(|e| e.to_string())?;
        let engine_lock = dir.join("Cargo.lock");
        if !engine_lock.is_file() {
            std::fs::write(&engine_lock, lock).map_err(|e| e.to_string())?;
        }
    }
    Ok(dir)
}

fn target_dir(plan: &Plan) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| plan.engine_dir().join("target"))
}

/// What [`obtain`] reports while it works.
pub enum Event {
    /// A line of text: bevy-ts's own, or cargo's.
    Log(String),
    /// Crates cargo has finished, of about `total` (`done` never passes it).
    Progress { done: usize, total: usize },
}

/// Where [`obtain`] reports.
pub enum Sink<'a> {
    /// Text on stderr, cargo's output inherited: `bevy-ts dev`'s supervisor and `types`.
    Stderr,
    /// Each line and cargo's progress as an [`Event`]: `bevy-ts build`.
    Events(&'a mut dyn FnMut(Event)),
}

impl Sink<'_> {
    fn log(&mut self, text: &str) {
        match self {
            Sink::Stderr => eprintln!("{}", text.trim_end_matches('\n')),
            Sink::Events(f) => {
                for line in text.lines() {
                    f(Event::Log(line.to_string()));
                }
            }
        }
    }
}

/// The number of packages in the engine crate's resolve for this target: about how many units
/// cargo compiles.
fn package_count(manifest: &Path, target: &Path) -> Option<usize> {
    let out = cargo()
        .ok()?
        .args(["metadata", "--format-version", "1", "--filter-platform", TARGET, "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", target)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v["resolve"]["nodes"].as_array().map(Vec::len).filter(|n| *n > 0)
}

/// Runs cargo with JSON messages: each finished crate is a [`Event::Progress`] step, everything
/// else cargo prints a [`Event::Log`] line.
fn run_cargo_reporting(mut cmd: Command, total: Option<usize>, report: &mut dyn FnMut(Event)) -> Result<std::process::ExitStatus, String> {
    use std::io::BufRead;
    enum Line {
        Out(String),
        Err(String),
    }
    cmd.arg("--message-format=json-render-diagnostics").stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cargo build: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    let out = child.stdout.take().expect("piped");
    let err = child.stderr.take().expect("piped");
    let tx_err = tx.clone();
    std::thread::spawn(move || {
        for l in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(Line::Out(l));
        }
    });
    std::thread::spawn(move || {
        for l in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx_err.send(Line::Err(l));
        }
    });
    let mut done = 0;
    if let Some(total) = total {
        report(Event::Progress { done, total });
    }
    for line in rx {
        match line {
            Line::Err(l) => report(Event::Log(l)),
            Line::Out(l) => match serde_json::from_str::<serde_json::Value>(&l) {
                Ok(m) => match m["reason"].as_str() {
                    Some("compiler-artifact") => {
                        done += 1;
                        if let Some(total) = total {
                            report(Event::Progress { done: done.min(total), total });
                        }
                    }
                    Some("compiler-message") => {
                        for r in m["message"]["rendered"].as_str().unwrap_or_default().lines() {
                            report(Event::Log(r.to_string()));
                        }
                    }
                    _ => {}
                },
                Err(_) => report(Event::Log(l)),
            },
        }
    }
    child.wait().map_err(|e| format!("cargo build: {e}"))
}

/// Builds the engine for `hash` with `cargo` and installs it in the local cache. On
/// [`Sink::Stderr`] cargo's output is passed through; on [`Sink::Events`] it is reported.
pub fn build(plan: &Plan, hash: &str, sink: &mut Sink) -> Result<PathBuf, String> {
    plan.check_lock()?;
    let mut cmd = cargo().map_err(|e| format!("{e}; engine {hash} needs a Rust toolchain or a shared engine cache entry"))?;
    let dir = generate(plan, hash)?;
    sink.log(&format!("bevy-ts: building project engine {hash} ({})", plan.profile.name()));
    cmd.arg("build").arg("--manifest-path").arg(dir.join("Cargo.toml")).args(["--bin", "bevy-ts-engine"]);
    if plan.profile == Profile::Release {
        cmd.arg("--release");
    }
    cmd.env("CARGO_TARGET_DIR", target_dir(plan));
    let status = match sink {
        Sink::Stderr => cmd.status().map_err(|e| format!("cargo build: {e}"))?,
        Sink::Events(report) => {
            let total = package_count(&dir.join("Cargo.toml"), &target_dir(plan));
            run_cargo_reporting(cmd, total, &mut **report)?
        }
    };
    if !status.success() {
        return Err(format!("cargo build failed for project engine {hash}"));
    }
    let built = target_dir(plan).join(plan.profile.dir()).join(if cfg!(windows) { "bevy-ts-engine.exe" } else { "bevy-ts-engine" });
    install(&built, &plan.cache_dir(hash))
}

/// What a build will compile, printed before it starts.
pub fn describe(plan: &Plan, hash: &str, why: &str) -> String {
    let mut s = format!("bevy-ts: project engine {hash} ({}, {TARGET}): {why}\n", plan.profile.name());
    if let Some(p) = &plan.pkg {
        let _ = writeln!(s, "  native crate {} {} ({}), plugin {}", p.name, p.version, p.dir.display(), plan.native.plugin.as_deref().unwrap_or("?"));
    }
    for port in &plan.native.ports {
        let _ = writeln!(s, "  port bevy-ts-{port}");
    }
    for (name, c) in &plan.native.crates {
        let _ = writeln!(s, "  crate {name} {}, plugin {}", c.version, c.plugin);
    }
    let first = !target_dir(plan).join(plan.profile.dir()).exists();
    if first {
        s.push_str("  first build: compiles Bevy, rusty_v8 and the plugins once (minutes); later Rust edits recompile only the changed crates\n");
    }
    s
}

/// The engine binary for the plan's current hash: the local cache, then the shared directory,
/// then a build when `may_build`, which reports what it compiles and why first.
pub fn obtain(plan: &Plan, may_build: bool, sink: &mut Sink) -> Result<(PathBuf, String, Origin), String> {
    let hash = plan.hash()?;
    let local = plan.cache_dir(&hash).join(exe_name());
    if local.is_file() {
        return Ok((local, hash, Origin::LocalCache));
    }
    let mut why = format!("not in {}", plan.cache_dir(&hash).display());
    if let Some(cache) = &plan.cache {
        let dir = plan.root.join(cache);
        match fetch_shared(&dir, &hash, &plan.cache_dir(&hash)) {
            Ok(Some(p)) => return Ok((p, hash, Origin::SharedCache)),
            Ok(None) => why.push_str(&format!(" nor the shared cache {}", dir.join(&hash).join(TARGET).display())),
            Err(e) => {
                sink.log(&format!("bevy-ts: {e}"));
                why.push_str(&format!("; the shared cache copy failed verification ({e})"));
            }
        }
    }
    if !may_build {
        return Err(format!("project engine {hash} is {why}"));
    }
    sink.log(&describe(plan, &hash, &why));
    build(plan, &hash, sink).map(|p| (p, hash, Origin::Built))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;

    fn project_with_native(dir: &Path) -> Project {
        std::fs::create_dir_all(dir.join("native/src")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("bevy-ts.json"), r#"{ "native": { "crate": "native", "plugin": "thrusters_native::GamePlugin" } }"#).unwrap();
        std::fs::write(
            dir.join("native/Cargo.toml"),
            "[package]\nname = \"thrusters-native\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
        )
        .unwrap();
        std::fs::write(dir.join("native/src/lib.rs"), "pub struct GamePlugin;\n")
            .unwrap();
        std::fs::write(
            dir.join("native/Cargo.lock"),
            format!("version = 4\n\n[[package]]\nname = \"bevy-ts-native\"\nversion = \"{VERSION}\"\n"),
        )
        .unwrap();
        std::fs::write(dir.join("src/game.ts"), "export const x = 1;\n").unwrap();
        project::find(dir).unwrap().unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bevy-ts-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn ts_and_asset_edits_leave_the_hash_unchanged() {
        let d = tmp("hash");
        let p = project_with_native(&d);
        let plan = Plan::new(&p, Profile::Dev).unwrap().unwrap();
        let before = plan.hash().unwrap();
        std::fs::write(d.join("src/game.ts"), "export const x = 2;\n").unwrap();
        std::fs::write(d.join("assets/a.png"), b"png").unwrap();
        // Fields of bevy-ts.json other than native and features are not inputs either.
        std::fs::write(d.join("bevy-ts.json"), r#"{ "native": { "crate": "native", "plugin": "thrusters_native::GamePlugin" }, "engineCache": "/x" }"#).unwrap();
        let p = project::find(&d).unwrap().unwrap();
        let plan = Plan::new(&p, Profile::Dev).unwrap().unwrap();
        assert_eq!(plan.hash().unwrap(), before);
        // Build output under the native crate is not an input.
        std::fs::create_dir_all(d.join("native/target")).unwrap();
        std::fs::write(d.join("native/target/x"), b"x").unwrap();
        assert_eq!(plan.hash().unwrap(), before);
        // A Rust edit is.
        std::fs::write(d.join("native/src/lib.rs"), "pub struct GamePlugin; // edited\n").unwrap();
        assert_ne!(plan.hash().unwrap(), before);
        let release = Plan { profile: Profile::Release, ..plan.clone() };
        assert_ne!(release.hash().unwrap(), plan.hash().unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_mismatched_lock_names_crate_expected_and_found() {
        let d = tmp("lock");
        let p = project_with_native(&d);
        let plan = Plan::new(&p, Profile::Dev).unwrap().unwrap();
        plan.check_lock().unwrap();
        std::fs::write(d.join("native/Cargo.lock"), "version = 4\n\n[[package]]\nname = \"bevy-ts-native\"\nversion = \"0.0.9\"\n").unwrap();
        let e = plan.check_lock().unwrap_err();
        assert!(e.contains("bevy-ts-native") && e.contains("0.0.9") && e.contains(&format!("={VERSION}")), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn generated_engine_adds_ports_crates_and_the_native_entry() {
        let d = tmp("gen");
        let mut p = project_with_native(&d);
        let n = p.config.native.as_mut().unwrap();
        n.crates.insert(
            "bevy_kira_audio".into(),
            project::CrateSpec { version: "0.27".into(), features: vec![], default_features: None, plugin: "bevy_kira_audio::AudioPlugin".into() },
        );
        let plan = Plan::new(&p, Profile::Dev).unwrap().unwrap();
        let main = plan.main_rs("abc");
        assert!(main.contains("pub use bevy_kira_audio::AudioPlugin;"));
        assert!(main.contains("pub use thrusters_native::GamePlugin;"));
        assert!(main.contains("Make::<plugin_1::GamePlugin>::make(plugin_1::GamePlugin)"), "{main}");
        assert!(main.contains("run_engine(identity, ProjectPlugins)"));
        let manifest = plan.manifest();
        assert!(manifest.contains("thrusters-native = { path ="));
        assert!(manifest.contains("bevy_kira_audio = { version = \"0.27\" }"));
        assert!(manifest.contains("[workspace]"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_tampered_shared_engine_is_not_installed() {
        let d = tmp("shared");
        let cache = d.join("cache");
        let entry = cache.join("h1").join(TARGET);
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(entry.join(exe_name()), b"engine bytes").unwrap();
        let good = Sha256::digest(b"engine bytes").iter().map(|b| format!("{b:02x}")).collect::<String>();
        std::fs::write(entry.join(format!("{}.sha256", exe_name())), format!("{good}\n")).unwrap();
        let got = fetch_shared(&cache, "h1", &d.join("local/h1")).unwrap().unwrap();
        assert_eq!(std::fs::read(&got).unwrap(), b"engine bytes");
        let bad = cache.join("h2").join(TARGET);
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join(exe_name()), b"evil bytes").unwrap();
        std::fs::write(bad.join(format!("{}.sha256", exe_name())), format!("{good}\n")).unwrap();
        let e = fetch_shared(&cache, "h2", &d.join("local/h2")).unwrap_err();
        assert!(e.contains("mismatch"), "{e}");
        assert!(!d.join("local/h2").join(exe_name()).exists());
        assert!(fetch_shared(&cache, "missing", &d.join("local/m")).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
