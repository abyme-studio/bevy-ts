//! Host-side TS: type stripping and source maps with oxc, walking the module graph from the
//! entry through its static imports. Every import is resolved here, npm packages included, and
//! rewritten to a relative path, so the loader and a shipped game resolve nothing else. No type
//! checking; that is the editor's and `tsc`'s job.

use crate::reader;
use bevy_ts::backend::{CompiledModule, ModuleSet, ScriptError, SourceLocation};
use bevy_ts::generation::ModuleSource;
use oxc::allocator::{Allocator, FromIn};
use oxc::ast::ast::Statement;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{SourceType, Span};
use oxc::transformer::{JsxRuntime, TransformOptions, Transformer};
use oxc_resolver::{ResolveContext, ResolveError, ResolveOptions, Resolver};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Specifiers the engine serves itself.
pub const BUILTINS: &[&str] = &["bevy-ts", "bevy-ts/bevy", "bevy-ts/jsx-runtime", "bevy-ts/jsx-dev-runtime"];

/// One compiled file.
pub struct Compiled {
    pub code: String,
    pub source_map: String,
    /// The file has `import` or `export`: a `.js` file without either is CommonJS.
    pub module_syntax: bool,
}

/// A 1-based line and column for a byte offset.
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let before = &source[..(offset as usize).min(source.len())];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
    (line, col)
}



fn location(path: &Path, source: &str, span: Span) -> SourceLocation {
    let (line, column) = line_col(source, span.start);
    SourceLocation { file: bevy_ts::backend::display_path(path), line, column }
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Compiles one TS or TSX file, with identities relative to its project root. `dev` uses the
/// development JSX transform, so elements carry their source location.
pub fn compile_file(path: &Path, source: &str, dev: bool) -> Result<Compiled, Vec<ScriptError>> {
    compile_module(path, source, dev, &reader::project_root(path))
}

/// Compiles one file for `bevy-ts transform <file>`: the module as the engine would load it,
/// declarations included.
pub fn transform(path: &Path, dev: bool) -> Result<String, Vec<ScriptError>> {
    let path = std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf());
    let path = path.canonicalize().unwrap_or_else(|_| normalize(&path));
    let source = std::fs::read_to_string(&path).map_err(|e| vec![ScriptError::new(format!("{}: {e}", bevy_ts::backend::display_path(&path)))])?;
    compile_module(&path, &source, dev, &reader::project_root(&path)).map(|c| c.code)
}

/// Compiles one TS or TSX file outside a graph. System signatures are read before types are
/// stripped and their declarations appended after the code, with identities relative to `root`.
/// Every import is left as written: the bundler plugins call this through `bevy-ts transform`,
/// and the bundler resolves with its own conditions and pre-bundling.
pub fn compile_module(path: &Path, source: &str, dev: bool, root: &Path) -> Result<Compiled, Vec<ScriptError>> {
    let resolver = resolver(dev);
    let options = Options { dev, root, declare: true, shipped: None, package: None, resolver: &resolver };
    compile_with(path, source, &options, &mut |_, _| Ok(None))
}

/// Whether the engine's compiles append declarations. Off until `core.js` defines
/// `__bevyts.bind`/`declare` (the runtime half of systems-api); `transform` always appends.
pub const ENGINE_DECLARATIONS: bool = true;

/// How one file compiles.
struct Options<'a> {
    dev: bool,
    root: &'a Path,
    declare: bool,
    /// For a shipped game: the file's shipped path, which its source map names, with no
    /// `sourcesContent`, so the game ships no TS text.
    shipped: Option<String>,
    /// Package code (script-runtime: "Package code is library code"): the package's name.
    package: Option<String>,
    /// For the reader's type lookups through relative imports.
    resolver: &'a Resolver,
}

/// What an import's specifier becomes: `None` keeps it as written.
type Rewrite<'r> = dyn FnMut(&str, &SourceLocation) -> Result<Option<String>, ScriptError> + 'r;

fn compile_with(path: &Path, source: &str, options: &Options, rewrite: &mut Rewrite) -> Result<Compiled, Vec<ScriptError>> {
    let Options { dev, root, declare, ref shipped, ref package, resolver } = *options;
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).map_err(|e| vec![ScriptError::new(format!("{}: {e}", bevy_ts::backend::display_path(path)))])?;
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.diagnostics.has_errors() {
        return Err(parsed
            .diagnostics
            .errors()
            .map(|d| {
                let span = d.labels.first().map(|l| Span::new(l.offset(), l.offset() + l.len()));
                let loc = span.map(|s| location(path, source, s));
                ScriptError::at(format!("syntax error: {}", d.message), loc)
            })
            .collect());
    }
    let module_syntax = parsed.module_record.has_module_syntax;
    let mut program = parsed.program;
    let info = reader::extract(&program, path, source, root);
    if let Some(name) = package
        && let Some(span) = info.declares_ecs()
    {
        return Err(vec![ScriptError::at(
            format!("package {name:?} declares an ECS type or registers a system; packages cannot declare ECS types yet"),
            Some(location(path, source, span)),
        )]);
    }
    let info = if shipped.is_some() { info.with_relative_locations() } else { info };
    let semantic = SemanticBuilder::new().build(&program).semantic;
    let mut transform = TransformOptions::default();
    transform.jsx.runtime = JsxRuntime::Automatic;
    transform.jsx.import_source = Some("bevy-ts".into());
    transform.jsx.development = dev;
    // Keep value imports even when unused (verbatimModuleSyntax): only `import type` goes.
    transform.typescript.only_remove_type_imports = true;
    let ret = Transformer::new(&allocator, path, &transform).build_with_scoping(semantic.into_scoping(), &mut program);
    if ret.diagnostics.has_errors() {
        return Err(ret.diagnostics.errors().map(|d| ScriptError::at(d.message.to_string(), None)).collect());
    }

    // Each import's string becomes the relative path to the file it resolves to. Only the value
    // changes, never the span, so the source map is unchanged.
    let mut errors = Vec::new();
    for stmt in program.body.iter_mut() {
        let source_literal = match stmt {
            Statement::ImportDeclaration(d) => &mut d.source,
            Statement::ExportAllDeclaration(d) => &mut d.source,
            Statement::ExportFromDeclaration(d) => &mut d.source,
            _ => continue,
        };
        match rewrite(source_literal.value.as_str(), &location(path, source, source_literal.span)) {
            Ok(Some(spec)) => {
                source_literal.value = oxc::str::Str::from_in(spec.as_str(), &allocator);
                source_literal.raw = None;
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }

    let mut declarations = if declare { reader::read(info, root, resolver)? } else { reader::Declarations::default() };
    // The imports the declaration pass appends are relative to this file too.
    let end = location(path, source, Span::new(source.len() as u32, source.len() as u32));
    for spec in &declarations.imports {
        match rewrite(spec, &end) {
            Ok(Some(to)) => {
                let (from, to) = (format!(";import {};", serde_json::json!(spec)), format!(";import {};", serde_json::json!(to)));
                declarations.code = declarations.code.replace(&from, &to);
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    inject_class_declarations(&allocator, &mut program, &declarations.classes);

    let out = Codegen::new()
        .with_options(CodegenOptions { source_map_path: Some(path.to_path_buf()), ..CodegenOptions::default() })
        .build(&program);
    let map = out.map.map(|m| m.to_json_string()).unwrap_or_default();
    // Point the map at the TS file by URL, with its text, so stacks and debuggers show TS.
    let mut map: serde_json::Value = serde_json::from_str(&map).unwrap_or(serde_json::json!({"version": 3, "mappings": ""}));
    if let Some(shipped) = shipped {
        map["sources"] = serde_json::json!([shipped]);
        if let Some(m) = map.as_object_mut() {
            m.remove("sourcesContent");
            m.remove("file");
        }
    } else {
        map["sources"] = serde_json::json!([file_url(path)]);
        map["sourcesContent"] = serde_json::json!([source]);
    }
    let map = map.to_string();
    // Inline too, so debuggers attached over CDP map breakpoints to the TS file themselves.
    // Declarations go after the code, so every mapped line keeps its place.
    let code = format!("{}{}\n//# sourceMappingURL=data:application/json;base64,{}\n", out.code, declarations.code, base64(map.as_bytes()));
    Ok(Compiled { code, source_map: map, module_syntax })
}

/// Gives each data class a trailing `static { __bevyts.data(this, {...}); }`, so its name, fields
/// and scene props are on the class from the moment it is defined (tsx reads `componentName` at
/// module load). The block has no source spans, so the source map is unchanged.
fn inject_class_declarations<'a>(allocator: &'a Allocator, program: &mut oxc::ast::ast::Program<'a>, classes: &[(String, String)]) {
    use oxc::ast::ast::{Declaration, ExportDefaultDeclarationKind};
    use oxc::ast_visit::VisitMut;
    struct NoSpans;
    impl<'a> VisitMut<'a> for NoSpans {
        fn visit_span(&mut self, span: &mut Span) {
            *span = Span::default();
        }
    }
    for (name, decl) in classes {
        let snippet = allocator.alloc_str(&format!("class _ {{ static {{ __bevyts.data(this, {decl}); }} }}"));
        let mut parsed = Parser::new(allocator, snippet, SourceType::mjs()).parse();
        let Some(Statement::ClassDeclaration(mut holder)) = parsed.program.body.pop() else { continue };
        let Some(mut block) = holder.body.body.pop() else { continue };
        NoSpans.visit_class_element(&mut block);
        let class = program.body.iter_mut().find_map(|stmt| {
            let c = match stmt {
                Statement::ClassDeclaration(c) => c,
                Statement::ExportDeclaration(e) => match &mut e.declaration {
                    Declaration::ClassDeclaration(c) => c,
                    _ => return None,
                },
                Statement::ExportDefaultDeclaration(e) => match &mut e.declaration {
                    ExportDefaultDeclarationKind::ClassDeclaration(c) => c,
                    _ => return None,
                },
                _ => return None,
            };
            (c.id.as_ref().is_some_and(|id| id.name == name.as_str())).then_some(c)
        });
        if let Some(c) = class {
            c.body.body.push(block);
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes standard base64, padding optional; other bytes are skipped.
pub fn decode_base64(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for v in s.bytes().filter_map(|c| T.iter().position(|t| *t == c)) {
        acc = (acc << 6 | v as u32) & 0xffffff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// The source map a compiled module carries inline, if any.
pub fn inline_source_map(code: &str) -> Option<String> {
    let (_, b64) = code.rsplit_once("//# sourceMappingURL=data:application/json;base64,")?;
    String::from_utf8(decode_base64(b64.trim())).ok()
}

/// The extension of a shader file a module imports (script-runtime: a `.wgsl` import).
pub const SHADER_EXT: &str = ".wgsl";

/// The Node-style resolver for a mode (script-packages design 1): `development` under
/// `bevy-ts dev`, `production` under `bevy-ts build`. Not `node`, `browser`, `require` or
/// `types`: scripts are not Node, have no DOM, load ES modules only and never load declarations.
pub fn resolver(dev: bool) -> Resolver {
    let strings = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mode = if dev { "development" } else { "production" };
    Resolver::new(ResolveOptions {
        condition_names: strings(&["bevy-ts", "import", "module", "default", mode]),
        extensions: strings(&[".ts", ".tsx", ".js", ".mjs", ".jsx"]),
        // A TS author writes `./x.js` for `./x.ts`.
        extension_alias: vec![(".js".into(), strings(&[".ts", ".tsx", ".js"])), (".jsx".into(), strings(&[".tsx", ".jsx"]))],
        main_files: strings(&["index"]),
        symlinks: true,
        builtin_modules: true,
        node_path: false,
        ..ResolveOptions::default()
    })
}

/// The file `spec` names from the module at `from`, if any, by its real path.
pub fn resolve_path(resolver: &Resolver, from: &Path, spec: &str) -> Option<PathBuf> {
    resolver.resolve(from.parent()?, spec).ok().map(|r| r.into_path_buf())
}

/// Specifiers the engine serves without a file.
fn is_builtin(spec: &str) -> bool {
    BUILTINS.contains(&spec) || bevy_ts::modules::is_registered(spec)
}

/// The import that reaches `to` from `from`: `./x.ts` or `../y/z.ts`.
fn specifier(from: &Path, to: &Path) -> String {
    let rel = reader::relative(from.parent().unwrap_or(Path::new("/")), to);
    if rel.starts_with("..") { rel } else { format!("./{rel}") }
}

/// The package a bare specifier names: `@scope/name` or `name`.
fn package_name(spec: &str) -> &str {
    let n = if spec.starts_with('@') { 2 } else { 1 };
    match spec.match_indices('/').nth(n - 1) {
        Some((i, _)) => &spec[..i],
        None => spec,
    }
}

/// Package code: outside the project root, or under a `node_modules` folder in it
/// (script-runtime: "Package code is library code").
pub fn is_package(root: &Path, path: &Path) -> bool {
    match path.strip_prefix(root) {
        Ok(rel) => rel.components().any(|c| c.as_os_str() == "node_modules"),
        Err(_) => true,
    }
}

/// The nearest `package.json` above `path` that has a name: its folder, name and version.
fn package_of(path: &Path) -> Option<(PathBuf, String, String)> {
    path.ancestors().skip(1).find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let name = json["name"].as_str()?.to_string();
        let version = json["version"].as_str().unwrap_or("0.0.0").to_string();
        Some((dir.to_path_buf(), name, version))
    })
}

/// Where each file sits for its imports (script-packages design 3).
#[derive(Clone, Debug)]
pub enum Layout {
    /// `bevy-ts dev`: a file's real path.
    Identity,
    /// `bevy-ts build`: game code at its path under the root, package code at
    /// `node_modules/<name>@<version>/<path in its package>`. Never a hidden folder, which
    /// `bevy-ts play` skips (pnpm's real paths run through `node_modules/.pnpm/`).
    Game { root: PathBuf },
}

impl Layout {
    /// The path `path` takes in this layout.
    pub fn place(&self, path: &Path) -> Result<PathBuf, String> {
        let Layout::Game { root } = self else { return Ok(path.to_path_buf()) };
        if !is_package(root, path) {
            return Ok(path.strip_prefix(root).expect("game code is under the root").to_path_buf());
        }
        let (dir, name, version) = package_of(path)
            .ok_or_else(|| format!("{} is outside the project {} and in no package with a name", path.display(), root.display()))?;
        let rel = path.strip_prefix(&dir).expect("a package's file is under its folder");
        Ok(Path::new("node_modules").join(format!("{name}@{version}")).join(rel))
    }
}

/// Why a resolution failed, at the import.
fn resolve_error(spec: &str, e: ResolveError, at: &SourceLocation) -> ScriptError {
    let bare = !is_relative(spec);
    let message = match e {
        ResolveError::Builtin { resolved, .. } => format!("cannot import {spec:?}: {resolved} is a Node built-in, and scripts have no Node APIs"),
        ResolveError::NotFound(_) if bare => format!("cannot import {spec:?}: package {:?} is not installed", package_name(spec)),
        ResolveError::NotFound(_) if spec.ends_with(SHADER_EXT) => format!("cannot find shader file {spec:?}"),
        ResolveError::NotFound(_) => format!("cannot find module {spec:?}"),
        ResolveError::PackagePathNotExported { subpath, .. } => {
            format!("cannot import {spec:?}: package {:?} does not export {subpath:?}", package_name(spec))
        }
        e => format!("cannot import {spec:?}: {e}"),
    };
    ScriptError::at(message, Some(at.clone()))
}

/// A `.wgsl` import's synthetic module: its default export is an opaque shader-file value naming
/// the file. The WGSL stays on the host; script materials read it there.
/// The hash [`Compiler::hashes`] keeps of a file's bytes.
pub fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

pub fn shader_module(path: &Path) -> String {
    let p = serde_json::to_string(&path.to_string_lossy()).expect("a string");
    format!("export default Object.freeze({{ kind: \"shaderFile\", path: {p} }});\n")
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/')
}

/// A file the walk reached, with the import that reached it.
struct Reached {
    path: PathBuf,
    from: Option<(String, SourceLocation)>,
    /// A `.js` file whose package does not say `"type": "module"`: CommonJS unless it has
    /// `import` or `export`. Holds the package's name.
    maybe_cjs: Option<String>,
}

/// Compiles an entry and every module it reaches through static imports.
pub struct Compiler {
    pub entry: PathBuf,
    /// The project root identities are relative to.
    pub root: PathBuf,
    /// Append system declarations to each module.
    pub declarations: bool,
    pub dev: bool,
    /// Compile for a shipped game: production JSX and conditions, the game layout, and source
    /// maps without source text.
    pub release: bool,
    resolver: Resolver,
    /// Every file the last compile read, for the watcher.
    files: Mutex<BTreeSet<PathBuf>>,
    /// Every path the last compile looked for and did not find.
    missing: Mutex<BTreeSet<PathBuf>>,
    /// A hash of each source the last compile read, as it read it.
    hashes: Mutex<HashMap<PathBuf, u64>>,
}

impl Compiler {
    pub fn new(entry: impl AsRef<Path>, dev: bool) -> std::io::Result<Self> {
        Self::with_mode(entry, dev, false)
    }

    /// A compiler for `bevy-ts build`: as `bevy-ts dev`'s, but with production JSX and
    /// conditions, the game layout, and source maps that carry no source text.
    pub fn for_release(entry: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::with_mode(entry, false, true)
    }

    fn with_mode(entry: impl AsRef<Path>, dev: bool, release: bool) -> std::io::Result<Self> {
        let entry = std::env::current_dir()?.join(entry);
        // Canonical, so paths match what the file watcher reports (/var vs /private/var).
        let entry = entry.canonicalize().unwrap_or_else(|_| normalize(&entry));
        let root = reader::project_root(&entry);
        Ok(Compiler {
            entry,
            root,
            declarations: ENGINE_DECLARATIONS,
            dev,
            release,
            resolver: resolver(dev),
            files: Mutex::new(BTreeSet::new()),
            missing: Mutex::new(BTreeSet::new()),
            hashes: Mutex::new(HashMap::new()),
        })
    }

    /// Where each file sits: its real path, or under `game/scripts/` for a release.
    pub fn layout(&self) -> Layout {
        if self.release { Layout::Game { root: self.root.clone() } } else { Layout::Identity }
    }

    /// Files the last compile read (also those that failed), and the `package.json` files its
    /// resolver read.
    pub fn files(&self) -> BTreeSet<PathBuf> {
        self.files.lock().unwrap().clone()
    }

    /// Paths the last compile's resolver looked for and did not find: where an install lands.
    pub fn missing(&self) -> BTreeSet<PathBuf> {
        self.missing.lock().unwrap().clone()
    }

    /// A hash of each source the last compile read, of the text it compiled: the watcher tells a
    /// change from a late report of an older write by it. Files it did not read itself (each
    /// `package.json`, shaders) have none.
    pub fn hashes(&self) -> HashMap<PathBuf, u64> {
        self.hashes.lock().unwrap().clone()
    }

    pub fn compile_graph(&self) -> Result<ModuleSet, Vec<ScriptError>> {
        // Its cache would hide an install or an edited `package.json`.
        self.resolver.clear_cache();
        let layout = self.layout();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([Reached { path: self.entry.clone(), from: None, maybe_cjs: None }]);
        let mut modules = Vec::new();
        let mut errors = Vec::new();
        let mut files = BTreeSet::new();
        let mut hashes = HashMap::new();
        let mut context = ResolveContext::default();
        // Shipped paths, so two files never ship at one.
        let mut shipped: HashMap<PathBuf, PathBuf> = HashMap::new();
        while let Some(Reached { path, from, maybe_cjs }) = queue.pop_front() {
            if !seen.insert(path.clone()) {
                continue;
            }
            files.insert(path.clone());
            let place = match layout.place(&path) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(ScriptError::at(e, from.map(|(_, at)| at)));
                    continue;
                }
            };
            if let Some(other) = shipped.insert(place.clone(), path.clone()) {
                // Two pnpm peer variants of one `name@version` share the path when identical. The
                // second's imports are not walked: identical bytes import the same specifiers.
                if std::fs::read(&other).ok() == std::fs::read(&path).ok() {
                    continue;
                }
                errors.push(ScriptError::new(format!(
                    "{} and {} would both ship at {}",
                    other.display(),
                    path.display(),
                    place.display()
                )));
                continue;
            }
            if path.to_string_lossy().ends_with(SHADER_EXT) {
                modules.push(CompiledModule { code: shader_module(&path), path, source_map: None });
                continue;
            }
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(ScriptError::new(format!("{}: {e}", bevy_ts::backend::display_path(&path))));
                    continue;
                }
            };
            hashes.insert(path.clone(), content_hash(source.as_bytes()));
            let package = is_package(&self.root, &path).then(|| package_of(&path).map_or_else(|| bevy_ts::backend::display_path(&path), |(_, n, _)| n));
            let options = Options {
                dev: self.dev,
                root: &self.root,
                declare: self.declarations && package.is_none(),
                shipped: self.release.then(|| place.to_string_lossy().replace('\\', "/")),
                package,
                resolver: &self.resolver,
            };
            let mut reached = Vec::new();
            let compiled = compile_with(&path, &source, &options, &mut |spec, at| {
                if is_builtin(spec) {
                    return Ok(None);
                }
                let dir = path.parent().unwrap_or(Path::new("/"));
                let r = self.resolver.resolve_with_context(dir, spec, None, &mut context).map_err(|e| resolve_error(spec, e, at))?;
                let target = r.path().to_path_buf();
                let name = target.to_string_lossy();
                let package_name = || r.package_json().and_then(|p| p.name().map(str::to_string)).unwrap_or_else(|| package_name(spec).to_string());
                if name.ends_with(".json") {
                    return Err(ScriptError::at(format!("cannot import {spec:?}: scripts cannot import JSON files"), Some(at.clone())));
                }
                if name.ends_with(".cjs") {
                    return Err(cjs_error(spec, &package_name(), &target, at));
                }
                let maybe_cjs = (name.ends_with(".js") && r.package_json().and_then(|p| p.r#type()) != Some(oxc_resolver::PackageType::Module)).then(package_name);
                let to = layout.place(&target).map_err(|e| ScriptError::at(e, Some(at.clone())))?;
                reached.push(Reached { path: target, from: Some((spec.to_string(), at.clone())), maybe_cjs });
                Ok(Some(specifier(&place, &to)))
            });
            match compiled {
                Ok(c) => {
                    if let (Some(package), Some((spec, at))) = (&maybe_cjs, &from)
                        && !c.module_syntax
                    {
                        errors.push(cjs_error(spec, package, &path, at));
                        continue;
                    }
                    queue.extend(reached);
                    modules.push(CompiledModule { path: path.clone(), code: c.code, source_map: Some(c.source_map) });
                }
                Err(e) => errors.extend(e),
            }
        }
        files.extend(context.file_dependencies.iter().filter(|p| p.ends_with("package.json")).cloned());
        *self.files.lock().unwrap() = files;
        *self.hashes.lock().unwrap() = hashes;
        *self.missing.lock().unwrap() = context.missing_dependencies.into_iter().collect();
        if errors.is_empty() { Ok(ModuleSet { entry: self.entry.clone(), modules }) } else { Err(errors) }
    }
}

fn cjs_error(spec: &str, package: &str, file: &Path, at: &SourceLocation) -> ScriptError {
    ScriptError::at(
        format!(
            "cannot import {spec:?}: {} in package {package:?} is CommonJS; bevy-ts loads ES modules only",
            bevy_ts::backend::display_path(file)
        ),
        Some(at.clone()),
    )
}

impl ModuleSource for Compiler {
    fn compile(&self) -> Result<ModuleSet, Vec<ScriptError>> {
        self.compile_graph()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join("bevy-ts-compile").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (f, src) in files {
            std::fs::write(dir.join(f), src).unwrap();
        }
        dir
    }

    #[test]
    fn strips_types_and_keeps_a_type_error() {
        let c = compile_file(Path::new("/x/game.ts"), "const n: number = 'not a number' as any as string;\nexport type T = { a: f32 };\n", false).unwrap();
        assert!(!c.code.contains(": number"), "{}", c.code);
        assert!(c.source_map.contains("file:///x/game.ts"));
    }

    #[test]
    fn syntax_errors_carry_line_and_column() {
        let e = compile_file(Path::new("/x/game.ts"), "const a = 1;\nconst = 2;\nconst b = 3;\n", false).err().unwrap();
        let loc = e[0].location.clone().unwrap();
        assert_eq!(loc.line, 2, "{e:?}");
    }

    #[test]
    fn tsx_dev_carries_source_location() {
        let src = "import { app } from \"bevy-ts\";\n\n\n\n\n\nconst a = (\n  <entity name=\"a\" />\n);\n";
        let src = src.replacen("const a = (\n  <entity", "  const a = <entity", 1).replacen(" />\n);", " />;", 1);
        let c = compile_file(Path::new("/x/game.tsx"), &src, true).unwrap();
        assert!(c.code.contains("bevy-ts/jsx-dev-runtime"), "{}", c.code);
        assert!(c.code.contains("/x/game.tsx"), "{}", c.code);
        assert!(c.code.contains("lineNumber: 7"), "{}", c.code);
        assert!(c.code.contains("columnNumber: 13"), "{}", c.code);
        let prod = compile_file(Path::new("/x/game.tsx"), &src, false).unwrap();
        assert!(prod.code.contains("bevy-ts/jsx-runtime"), "{}", prod.code);
    }

    #[test]
    fn a_bare_import_fails_with_its_location() {
        let dir = tmp("bare", &[("game.ts", "import { app } from \"bevy-ts\";\nimport _ from \"lodash\";\n")]);
        let e = Compiler::new(dir.join("game.ts"), false).unwrap().compile_graph().unwrap_err();
        assert!(e[0].message.contains("\"lodash\"") && e[0].message.contains("not installed"), "{e:?}");
        let loc = e[0].location.clone().unwrap();
        assert!(loc.file.ends_with("game.ts"));
        assert_eq!(loc.line, 2);
    }

    /// A project folder with nested files; `package.json` at its root.
    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join("bevy-ts-packages").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        for (f, src) in [("package.json", "{\"name\": \"game\", \"type\": \"module\"}")].iter().chain(files) {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, src).unwrap();
        }
        dir.canonicalize().unwrap()
    }

    fn graph(dir: &Path, dev: bool) -> Result<ModuleSet, Vec<ScriptError>> {
        let c = if dev { Compiler::new(dir.join("src/game.ts"), true) } else { Compiler::for_release(dir.join("src/game.ts")) };
        c.unwrap().compile_graph()
    }

    fn code_of<'a>(set: &'a ModuleSet, end: &str) -> &'a str {
        &set.modules.iter().find(|m| m.path.ends_with(end)).unwrap_or_else(|| panic!("no {end}")).code
    }

    fn errors(dir: &Path) -> String {
        format!("{:?}", graph(dir, true).unwrap_err())
    }

    #[test]
    fn exports_conditions_pick_bevy_ts_and_the_mode() {
        let dir = project(
            "conditions",
            &[
                ("src/game.ts", "import { a } from \"pick\";\nimport { m } from \"mode\";\nexport const x = [a, m];\n"),
                ("node_modules/pick/package.json", r#"{"name": "pick", "type": "module", "exports": {".": {"node": "./node.js", "bevy-ts": "./bevy.js", "default": "./index.js"}}}"#),
                ("node_modules/pick/node.js", "export const a = 'node';\n"),
                ("node_modules/pick/bevy.js", "export const a = 'bevy';\n"),
                ("node_modules/pick/index.js", "export const a = 'index';\n"),
                ("node_modules/mode/package.json", r#"{"name": "mode", "type": "module", "exports": {".": {"development": "./dev.js", "production": "./prod.js"}}}"#),
                ("node_modules/mode/dev.js", "export const m = 'dev';\n"),
                ("node_modules/mode/prod.js", "export const m = 'prod';\n"),
            ],
        );
        let dev = graph(&dir, true).unwrap();
        let game = code_of(&dev, "src/game.ts");
        assert!(game.contains("\"../node_modules/pick/bevy.js\""), "{game}");
        assert!(game.contains("\"../node_modules/mode/dev.js\""), "{game}");
        let release = graph(&dir, false).unwrap();
        let game = code_of(&release, "src/game.ts");
        assert!(game.contains("\"../node_modules/mode@0.0.0/prod.js\""), "{game}");
        assert!(release.modules.iter().any(|m| m.path.ends_with("mode/prod.js")));
        assert!(!release.modules.iter().any(|m| m.path.ends_with("mode/dev.js")));
    }

    #[test]
    fn a_scoped_package_and_main_resolve() {
        let dir = project(
            "scoped",
            &[
                ("src/game.ts", "import { a } from \"@s/p/sub\";\nimport { b } from \"plain\";\nexport const x = [a, b];\n"),
                ("node_modules/@s/p/package.json", r#"{"name": "@s/p", "version": "1.2.3", "type": "module", "exports": {"./sub": "./src/sub.ts"}}"#),
                ("node_modules/@s/p/src/sub.ts", "export const a: number = 1;\n"),
                ("node_modules/plain/package.json", r#"{"name": "plain", "version": "2.0.0", "type": "module", "main": "lib/main.js"}"#),
                ("node_modules/plain/lib/main.js", "export const b = 2;\n"),
            ],
        );
        let set = graph(&dir, true).unwrap();
        let sub = code_of(&set, "src/sub.ts");
        assert!(!sub.contains(": number"), "types stripped: {sub}");
        let release = graph(&dir, false).unwrap();
        let game = code_of(&release, "src/game.ts");
        assert!(game.contains("\"../node_modules/@s/p@1.2.3/src/sub.ts\""), "{game}");
        assert!(game.contains("\"../node_modules/plain@2.0.0/lib/main.js\""), "{game}");
    }

    #[cfg(unix)]
    #[test]
    fn a_pnpm_link_loads_the_real_file_and_ships_without_pnpm_folders() {
        let dir = project(
            "pnpm",
            &[
                ("src/game.ts", "import { a } from \"dep\";\nexport const x = a;\n"),
                ("node_modules/.pnpm/dep@1.0.0/node_modules/dep/package.json", r#"{"name": "dep", "version": "1.0.0", "type": "module", "exports": "./index.js"}"#),
                ("node_modules/.pnpm/dep@1.0.0/node_modules/dep/index.js", "export const a = 1;\n"),
            ],
        );
        std::os::unix::fs::symlink(".pnpm/dep@1.0.0/node_modules/dep", dir.join("node_modules/dep")).unwrap();
        let set = graph(&dir, true).unwrap();
        assert!(set.modules.iter().any(|m| m.path == dir.join("node_modules/.pnpm/dep@1.0.0/node_modules/dep/index.js")));
        let release = Compiler::for_release(dir.join("src/game.ts")).unwrap();
        let layout = release.layout();
        let real = dir.join("node_modules/.pnpm/dep@1.0.0/node_modules/dep/index.js");
        assert_eq!(layout.place(&real).unwrap(), Path::new("node_modules/dep@1.0.0/index.js"));
        assert!(code_of(&release.compile_graph().unwrap(), "src/game.ts").contains("\"../node_modules/dep@1.0.0/index.js\""));
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_package_reaches_outside_itself() {
        let base = std::env::temp_dir().join("bevy-ts-packages").join("outside");
        let _ = std::fs::remove_dir_all(&base);
        let write = |f: &str, src: &str| {
            let p = base.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, src).unwrap();
        };
        write("web/packages/client/package.json", r#"{"name": "@abyme/client", "type": "module", "exports": {"./connect": "./src/connect.ts"}}"#);
        write("web/packages/client/src/connect.ts", "import { P } from \"../../kernel/src/protocol.ts\";\nexport const connect = (): number => P;\n");
        write("web/packages/kernel/package.json", r#"{"name": "@abyme/kernel", "type": "module"}"#);
        write("web/packages/kernel/src/protocol.ts", "export const P = 1;\n");
        write("game/package.json", r#"{"name": "game", "type": "module"}"#);
        write("game/src/game.ts", "import { connect } from \"@abyme/client/connect\";\nexport const x = connect();\n");
        std::fs::create_dir_all(base.join("game/node_modules/@abyme")).unwrap();
        std::os::unix::fs::symlink(base.join("web/packages/client"), base.join("game/node_modules/@abyme/client")).unwrap();
        let base = base.canonicalize().unwrap();
        let set = graph(&base.join("game"), true).unwrap();
        let protocol = base.join("web/packages/kernel/src/protocol.ts");
        assert!(set.modules.iter().any(|m| m.path == protocol), "{:?}", set.modules.iter().map(|m| &m.path).collect::<Vec<_>>());
        let game = code_of(&set, "game/src/game.ts");
        assert!(game.contains("\"../../web/packages/client/src/connect.ts\""), "{game}");
        let release = graph(&base.join("game"), false).unwrap();
        let connect = code_of(&release, "client/src/connect.ts");
        assert!(connect.contains("\"../../kernel@0.0.0/src/protocol.ts\""), "{connect}");
        let map: serde_json::Value = serde_json::from_str(&inline_source_map(connect).unwrap()).unwrap();
        assert_eq!(map["sources"], serde_json::json!(["node_modules/@abyme/client@0.0.0/src/connect.ts"]));
    }

    #[test]
    fn an_unexported_subpath_fails_naming_the_package() {
        let dir = project(
            "unexported",
            &[
                ("src/game.ts", "\nimport { a } from \"@s/p/src/internal.ts\";\nexport const x = a;\n"),
                ("node_modules/@s/p/package.json", r#"{"name": "@s/p", "type": "module", "exports": {"./sub": "./src/sub.ts"}}"#),
                ("node_modules/@s/p/src/internal.ts", "export const a = 1;\n"),
            ],
        );
        let e = graph(&dir, true).unwrap_err();
        assert!(e[0].message.contains("\"@s/p/src/internal.ts\"") && e[0].message.contains("does not export"), "{e:?}");
        assert!(e[0].message.contains("\"@s/p\""), "{e:?}");
        assert_eq!(e[0].location.as_ref().unwrap().line, 2);
    }

    #[test]
    fn a_commonjs_package_is_refused() {
        let dir = project(
            "cjs",
            &[
                ("src/game.ts", "import pad from \"left-pad\";\nexport const x = pad;\n"),
                ("node_modules/left-pad/package.json", r#"{"name": "left-pad", "main": "index.js"}"#),
                ("node_modules/left-pad/index.js", "module.exports = function pad() {};\n"),
            ],
        );
        let e = graph(&dir, true).unwrap_err();
        let m = &e[0].message;
        assert!(m.contains("\"left-pad\"") && m.contains("index.js") && m.contains("ES modules only"), "{e:?}");
        assert_eq!(e[0].location.as_ref().unwrap().line, 1);
        assert!(e[0].location.as_ref().unwrap().file.ends_with("game.ts"));
    }

    #[test]
    fn node_builtins_are_refused() {
        for spec in ["node:fs", "fs"] {
            let dir = project(&format!("builtin-{spec}").replace(':', "-"), &[("src/game.ts", &format!("import * as fs from \"{spec}\";\nexport const x = fs;\n"))]);
            let e = graph(&dir, true).unwrap_err();
            assert!(e[0].message.contains(&format!("{spec:?}")) && e[0].message.contains("no Node APIs"), "{e:?}");
            assert_eq!(e[0].location.as_ref().unwrap().line, 1);
        }
    }

    #[test]
    fn a_json_import_is_refused() {
        let dir = project("json", &[("src/game.ts", "import data from \"./data.json\";\nexport const x = data;\n"), ("src/data.json", "{}")]);
        assert!(errors(&dir).contains("JSON"), "{}", errors(&dir));
    }

    #[test]
    fn a_package_declaring_a_component_fails() {
        let dir = project(
            "declares",
            &[
                ("src/game.ts", "import { Health } from \"hp\";\nexport const x = Health;\n"),
                ("node_modules/hp/package.json", r#"{"name": "hp", "type": "module", "exports": "./index.ts"}"#),
                ("node_modules/hp/index.ts", "import { Component, f32 } from \"bevy-ts\";\n\nexport class Health extends Component { hp: f32 = 1; }\n"),
            ],
        );
        let e = graph(&dir, true).unwrap_err();
        assert!(e[0].message.contains("\"hp\"") && e[0].message.contains("cannot declare ECS types"), "{e:?}");
        let at = e[0].location.as_ref().unwrap();
        assert!(at.file.ends_with("hp/index.ts") && at.line == 3, "{at:?}");
    }

    #[test]
    fn the_rewrite_leaves_the_source_map_unchanged() {
        let src = "import { k } from \"./util.js\";\nexport const x: number = k;\nexport const y = x + 1;\n";
        let dir = project("map", &[("src/game.ts", src), ("src/util.ts", "export const k = 1;\n")]);
        let set = graph(&dir, true).unwrap();
        let rewritten = code_of(&set, "src/game.ts");
        assert!(rewritten.contains("\"./util.ts\""), "{rewritten}");
        // The same file compiled with its import left as written.
        let kept = compile_file(Path::new("/elsewhere/game.ts"), src, true).unwrap();
        let mappings = |code: &str| {
            let map: serde_json::Value = serde_json::from_str(&inline_source_map(code).unwrap()).unwrap();
            map["mappings"].as_str().unwrap().split(';').map(str::to_string).collect::<Vec<_>>()
        };
        let (a, b) = (mappings(rewritten), mappings(&kept.code));
        assert_eq!(a.len(), b.len());
        assert_eq!(a[1..], b[1..], "lines after the import keep their mappings");
        assert_eq!(a[0].split(',').next(), b[0].split(',').next());
    }

    #[test]
    fn transform_keeps_imports_as_written() {
        let dir = project(
            "transform",
            &[
                ("src/game.ts", "import { a } from \"dep\";\nimport { k } from \"./util.js\";\nexport const x = [a, k];\n"),
                ("src/util.ts", "export const k = 1;\n"),
                ("node_modules/dep/package.json", r#"{"name": "dep", "type": "module", "exports": {"browser": "./browser.js", "default": "./index.js"}}"#),
                ("node_modules/dep/index.js", "export const a = 1;\n"),
            ],
        );
        let code = transform(&dir.join("src/game.ts"), true).unwrap();
        assert!(code.contains("from \"dep\"") && code.contains("from \"./util.js\""), "the bundler resolves: {code}");
    }

    #[test]
    fn extensionless_js_resolves() {
        let dir = project("js", &[("src/game.ts", "import { k } from \"./lib\";\nexport const x = k;\n"), ("src/lib.mjs", "export const k = 1;\n")]);
        assert!(code_of(&graph(&dir, true).unwrap(), "src/game.ts").contains("\"./lib.mjs\""));
    }

    #[test]
    fn a_wgsl_import_is_a_shader_file_value() {
        let dir = tmp("wgsl", &[("game.ts", "import glow from \"./glow.wgsl\";\nexport const g = glow;\n"), ("glow.wgsl", "fn f() {}\n")]);
        let c = Compiler::new(dir.join("game.ts"), false).unwrap();
        let set = c.compile_graph().unwrap();
        let wgsl = dir.join("glow.wgsl").canonicalize().unwrap();
        let m = set.modules.iter().find(|m| m.path == wgsl).expect("a synthetic module");
        assert!(m.code.contains("\"shaderFile\""), "{}", m.code);
        assert!(m.code.contains(&*wgsl.to_string_lossy()), "{}", m.code);
        assert!(!m.code.contains("fn f"), "WGSL stays on the host: {}", m.code);
        assert!(c.files().contains(&wgsl), "the shader file is watched");
    }

    #[test]
    fn a_missing_wgsl_import_fails_at_its_line() {
        let dir = tmp("wgsl-missing", &[("game.ts", "import { app } from \"bevy-ts\";\n\nimport glow from \"./glw.wgsl\";\n")]);
        let e = Compiler::new(dir.join("game.ts"), false).unwrap().compile_graph().unwrap_err();
        assert!(e[0].message.contains("\"./glw.wgsl\""), "{e:?}");
        let loc = e[0].location.clone().unwrap();
        assert!(loc.file.ends_with("game.ts"));
        assert_eq!(loc.line, 3);
    }

    #[test]
    fn walks_relative_imports() {
        let dir = tmp(
            "graph",
            &[
                ("game.ts", "import { k } from \"./util.js\";\nimport type { T } from \"./types\";\nexport const x: number = k;\n"),
                ("util.ts", "export const k = 1;\n"),
                ("types.ts", "export type T = number;\n"),
            ],
        );
        let set = Compiler::new(dir.join("game.ts"), false).unwrap().compile_graph().unwrap();
        let names: Vec<_> = set.modules.iter().map(|m| m.path.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["game.ts", "util.ts"]);
    }

    #[test]
    fn a_release_compile_ships_no_ts_source() {
        let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/src/game.tsx");
        let compiler = Compiler::for_release(&arena).unwrap();
        let set = compiler.compile_graph().unwrap();
        assert!(set.modules.len() >= 4, "{}", set.modules.len());
        for m in &set.modules {
            let source = std::fs::read_to_string(&m.path).unwrap();
            let rel = m.path.strip_prefix(&compiler.root).unwrap().to_string_lossy().to_string();
            let map: serde_json::Value = serde_json::from_str(&inline_source_map(&m.code).unwrap()).unwrap();
            assert_eq!(map["sources"], serde_json::json!([rel]), "{}", m.path.display());
            assert!(map.get("sourcesContent").is_none(), "{map}");
            assert!(!m.code.contains(&source), "{} holds its TS source", m.path.display());
            assert!(!m.code.contains("jsx-dev-runtime"), "{}", m.path.display());
            let root = compiler.root.to_string_lossy();
            assert!(!m.code.contains(&*root), "{} names the project folder", m.path.display());
            // A typed signature does not survive.
            assert!(!m.code.contains("Query<["), "{}", m.path.display());
        }
    }
}
