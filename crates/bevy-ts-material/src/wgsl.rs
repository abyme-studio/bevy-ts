//! A material's shader source, and the check that keeps a broken file from replacing a working
//! shader. Bevy compiles material shaders as WESL and its pipeline cache drops the old pipeline the
//! moment a shader is replaced, so a save is checked here first, on the host, with the shader defs
//! the live pipeline was built with (captured from the render world, [`PipelineWatch`]).
//!
//! The check compiles the file with Bevy's own shader modules, keeps only what it imports (the
//! prelude), renames those declarations back to the names the file imports them as, and hands naga
//! the prelude followed by the file verbatim. naga's line `n` is then the file's line `n - k`, `k`
//! the prelude's lines; past the file's last line is the generated params code.

use crate::layout::Bindings;
use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy::shader::{ShaderDefVal, ShaderImport, Source};
use bevy_ts::backend::{ScriptError, SourceLocation};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use wesl::Mangler;
use wesl::syntax::{GlobalDeclaration, GlobalDeclarationNode, Ident, ImportContent, ImportStatement, ModulePath, PathOrigin, TranslationUnit};

/// The author's file, then the params and texture declarations. Appended, not prepended: WGSL
/// declarations are order-free and WESL imports must come first, so the author's lines keep their
/// numbers and anything past their last line is generated. A kind with instance params also needs
/// Bevy's `get_tag`, imported at the start of the author's first line
/// ([`INSTANCE_IMPORT`](crate::layout::INSTANCE_IMPORT), or its 2D form), which shifts only that
/// line's columns.
pub fn combine(text: &str, bindings: &Bindings, material: &str) -> String {
    let mut s = if bindings.has_instance() { format!("{}{text}", bindings.space().instance_import()) } else { text.to_string() };
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&bindings.wgsl(material));
    s
}

/// A pass's file, then its params struct and screen bindings, appended as [`combine`]'s are.
pub fn combine_pass(text: &str, layout: &crate::layout::PassLayout, pass: &str) -> String {
    let mut s = text.to_string();
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&layout.wgsl(pass));
    s
}

/// What the check found.
#[derive(Debug)]
pub enum Checked {
    Ok,
    /// Errors in the author's file or the generated params: the shader must not be replaced.
    Errors(Vec<ScriptError>),
    /// The check could not decide (the file uses a WESL feature it does not model, or an error
    /// lies in Bevy's modules under these defs). The shader is replaced; the renderer decides.
    Unverified(String),
}

/// What a checked shader belongs to: a material or a pass, as its errors name it.
pub struct Subject {
    /// `material game::Glow`, `pass game::Dusk`.
    pub label: String,
    /// Its declaration in TS, where an error in the generated declarations points.
    pub at: Option<SourceLocation>,
    /// The names the generated declarations use.
    pub generated: Vec<String>,
}

/// Checks a combined source under `defs`. `shaders` holds Bevy's shader modules.
pub fn check(shaders: &Assets<Shader>, combined: &str, file: &Path, decl: &Subject, defs: &[ShaderDefVal]) -> Checked {
    let author_lines = combined.find("\n// bevy-ts: the params").map_or(combined.lines().count(), |i| combined[..i].lines().count());
    // The prepended import's columns are not the author's.
    let shift = [crate::layout::INSTANCE_IMPORT, crate::layout::INSTANCE_IMPORT_2D].into_iter().find(|i| combined.starts_with(i)).map_or(0, |i| i.len() as u32);
    let at = |line: usize, column: u32| {
        let column = if line == 1 { column.saturating_sub(shift).max(1) } else { column };
        Some(SourceLocation { file: bevy_ts::backend::display_path(file), line: line as u32, column })
    };
    let fail = |m: String, loc| Checked::Errors(vec![ScriptError::at(format!("{}: {m}", decl.label), loc)]);

    // The file as WESL: its imports, and the names it declares.
    let tu = match TranslationUnit::from_str(combined) {
        Ok(tu) => tu,
        Err(e) => {
            let (line, col) = line_col(combined, e.span.start);
            return if line <= author_lines {
                fail(e.error.to_string(), at(line, col))
            } else {
                fail(format!("{} in the declarations generated from its params", e.error), decl.at.clone())
            };
        }
    };
    let Some(import_end) = imports_end(combined) else { return Checked::Unverified("a conditional import".into()) };
    if uses_unmodelled_wesl(&combined[import_end..]) {
        return Checked::Unverified("inline module paths or conditional translation".into());
    }
    let main_path = ModulePath { origin: PathOrigin::Absolute, components: vec!["bevy_ts_check".into()] };
    let imports = flatten_imports(&tu.imports, &main_path);
    let main_names: HashSet<String> = tu.global_declarations.iter().filter_map(decl_ident).map(|i| i.name().clone()).collect();
    // The file declaring a name the params generate (a texture named like its own `const`): the
    // schema's error, named exactly, before anything is compiled.
    let generated = &decl.generated;
    let mut declared: HashMap<String, usize> = HashMap::new();
    for i in tu.global_declarations.iter().filter_map(decl_ident) {
        *declared.entry(i.name().clone()).or_default() += 1;
    }
    if let Some(n) = generated.iter().find(|n| declared.get(*n).copied().unwrap_or_default() > 1) {
        return fail(format!("the file declares `{n}`, which its params, textures or instance params generate"), decl.at.clone());
    }

    // Bevy's modules, by module path, as its shader cache resolves them.
    let mut modules: HashMap<ModulePath, &str> = HashMap::new();
    for (_, s) in shaders.iter() {
        if let (Some(p), Source::Wesl(src) | Source::Wgsl(src)) = (module_path(&s.import_path), &s.source) {
            modules.insert(p, src.as_ref());
        }
    }
    modules.insert(main_path.clone(), combined);
    let mut options = wesl::CompileOptions { imports: true, condcomp: true, visibility: false, strip: true, keep_main: true, ..Default::default() };
    let mut constants = String::new();
    for d in defs {
        match d {
            ShaderDefVal::Bool(k, v) => {
                options.features.flags.insert(k.to_string(), (*v).into());
            }
            ShaderDefVal::Int(k, v) => {
                options.features.flags.insert(k.to_string(), true.into());
                constants.push_str(&format!("const {k} = {v};\n"));
            }
            ShaderDefVal::UInt(k, v) => {
                options.features.flags.insert(k.to_string(), true.into());
                constants.push_str(&format!("const {k} = {v}u;\n"));
            }
        }
    }
    let resolver = Resolver { modules, constants };
    let compiled = match wesl::compile(&main_path, &options, &resolver) {
        Ok(c) => c,
        Err(e) => {
            let d = wesl::error::Diagnostic::from(e);
            let ours = d.detail.module_path.as_ref() == Some(&main_path);
            return match (ours, d.detail.span) {
                (true, Some(span)) => {
                    let (line, col) = line_col(combined, span.start);
                    if line <= author_lines {
                        fail(d.error.to_string(), at(line, col))
                    } else {
                        fail(format!("{} in the declarations generated from its params", d.error), decl.at.clone())
                    }
                }
                _ if matches!(*d.error, wesl::Error::ResolveError(_)) => fail(d.error.to_string(), decl.at.clone()),
                _ => Checked::Unverified(d.error.to_string()),
            };
        }
    };

    // The prelude: what the file imports, under the names it imports them as.
    let mut prelude = compiled.syntax;
    prelude.global_declarations.retain(|d| decl_ident(d).is_none_or(|i| !main_names.contains(&*i.name())));
    let mangler = wesl::mangler::EscapeMangler;
    for (local, (path, item)) in &imports {
        let mangled = mangler.mangle(path, item);
        // An import the file does not use was stripped; nothing to rename.
        if let Some(mut ident) = prelude.global_declarations.iter().filter_map(decl_ident).find(|i| *i.name() == mangled) {
            ident.rename(local.clone());
        }
    }
    let prelude = format!("{prelude}\n");
    let k = prelude.matches('\n').count();
    let mut body = combined.to_string();
    blank(&mut body, 0..import_end);
    let source = prelude + &body;

    let map = |line: u32, column: u32, message: String| -> Checked {
        let n = line as usize;
        if n <= k {
            Checked::Unverified(format!("in Bevy's modules: {message}"))
        } else if n - k <= author_lines {
            fail(message, at(n - k, column))
        } else {
            fail(format!("{message} in the declarations generated from its params"), decl.at.clone())
        }
    };
    let module = match naga::front::wgsl::parse_str(&source) {
        Ok(m) => m,
        Err(e) => {
            // The first label in the author's part, else the first: a type the error names may be
            // declared in the prelude while the mistake is in the file.
            let in_file = |span: naga::Span| span.location(&source).line_number as usize > k;
            let label = e.labels().find(|(span, _)| in_file(*span)).or_else(|| e.labels().next());
            let loc = label.map(|(span, _)| span.location(&source)).or_else(|| e.location(&source));
            let label = label.map(|(_, l)| l.to_string()).filter(|l| !l.is_empty());
            let msg = match label {
                Some(l) if l != e.message() => format!("{}: {l}", e.message()),
                _ => e.message().to_string(),
            };
            return match loc {
                Some(l) => map(l.line_number, l.line_position, msg),
                None => fail(msg, decl.at.clone()),
            };
        }
    };
    let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    if let Err(e) = validator.validate(&module) {
        let mut msg = e.as_inner().to_string();
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e.as_inner());
        while let Some(s) = src {
            msg.push_str(&format!(": {s}"));
            src = s.source();
        }
        // The innermost span is the expression; the outer one the function.
        // The innermost span in the author's part, else the innermost.
        let spans: Vec<_> = e.spans().map(|(span, _)| span.location(&source)).filter(|l| l.line_number > 0).collect();
        let loc = spans.iter().rev().find(|l| l.line_number as usize > k).or(spans.last()).copied();
        return match loc {
            Some(l) => map(l.line_number, l.line_position, msg),
            None => fail(msg, decl.at.clone()),
        };
    }
    Checked::Ok
}

/// A global declaration's name.
fn decl_ident(d: &GlobalDeclarationNode) -> Option<Ident> {
    match &**d {
        GlobalDeclaration::Declaration(d) => Some(d.ident.clone()),
        GlobalDeclaration::TypeAlias(t) => Some(t.ident.clone()),
        GlobalDeclaration::Struct(s) => Some(s.ident.clone()),
        GlobalDeclaration::Function(f) => Some(f.ident.clone()),
        _ => None,
    }
}

fn line_col(source: &str, offset: usize) -> (usize, u32) {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
    (line, col)
}

/// Replaces `range` with spaces, keeping its newlines, so every line keeps its number.
fn blank(s: &mut String, range: std::ops::Range<usize>) {
    let blanked: String = s[range.clone()].chars().map(|c| if c == '\n' { '\n' } else { ' ' }).collect();
    s.replace_range(range, &blanked);
}

/// Skips whitespace and comments.
fn skip_trivia(s: &str, mut i: usize) -> usize {
    loop {
        let rest = &s[i..];
        let trimmed = rest.trim_start();
        i += rest.len() - trimmed.len();
        if trimmed.starts_with("//") {
            i += trimmed.find('\n').unwrap_or(trimmed.len());
        } else if trimmed.starts_with("/*") {
            i += trimmed.find("*/").map_or(trimmed.len(), |e| e + 2);
        } else {
            return i;
        }
    }
}

/// The byte where the leading import statements end. `None` for a conditional import.
fn imports_end(s: &str) -> Option<usize> {
    let mut i = skip_trivia(s, 0);
    let mut end = 0;
    loop {
        let rest = &s[i..];
        if rest.starts_with("@if") || rest.starts_with("@elif") || rest.starts_with("@else") {
            let after = skip_trivia(s, i + rest.find(')').map_or(rest.len(), |p| p + 1));
            if s[after..].starts_with("import") {
                return None;
            }
            return Some(end);
        }
        let is_import = rest.starts_with("import") && !rest[6..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
        if !is_import {
            return Some(end);
        }
        let semi = rest.find(';')?;
        i += semi + 1;
        end = i;
        i = skip_trivia(s, i);
    }
}

/// Inline `a::b` paths or `@if` outside the imports: WESL the check does not model.
fn uses_unmodelled_wesl(body: &str) -> bool {
    let mut code = String::new();
    let mut i = 0;
    while i < body.len() {
        let j = skip_trivia(body, i);
        if j > i {
            code.push(' ');
            i = j;
            continue;
        }
        let c = body[i..].chars().next().unwrap();
        code.push(c);
        i += c.len_utf8();
    }
    code.contains("::") || code.contains("@if") || code.contains("@elif") || code.contains("@else")
}

/// Local name to (module path, item), as WESL resolves an import statement.
fn flatten_imports(imports: &[ImportStatement], main: &ModulePath) -> HashMap<String, (ModulePath, String)> {
    fn rec(content: &ImportContent, path: ModulePath, out: &mut HashMap<String, (ModulePath, String)>) {
        match content {
            ImportContent::Item(item) => {
                let local = item.rename.as_ref().unwrap_or(&item.ident).name().clone();
                out.insert(local, (path, item.ident.name().clone()));
            }
            ImportContent::Collection(coll) => {
                for import in coll {
                    rec(&import.content, path.clone().join(import.path.iter().cloned()), out);
                }
            }
        }
    }
    let mut out = HashMap::new();
    for import in imports {
        match &import.path {
            Some(p) => rec(&import.content, main.join_path(p), &mut out),
            None => {
                if let ImportContent::Collection(coll) = &import.content {
                    for import in coll {
                        let mut parts = import.path.iter().cloned();
                        if let Some(pkg) = parts.next() {
                            rec(&import.content, ModulePath { origin: PathOrigin::Package(pkg), components: parts.collect() }, &mut out);
                        }
                    }
                }
            }
        }
    }
    out
}

/// A shader's module path, as Bevy's shader cache derives it.
fn module_path(import: &ShaderImport) -> Option<ModulePath> {
    match import {
        ShaderImport::Custom(name) => {
            let mut segments = name.split("::");
            let package = segments.next().filter(|s| !s.is_empty())?;
            let components = segments.map(|s| (!s.is_empty()).then(|| s.to_string())).collect::<Option<Vec<_>>>()?;
            Some(ModulePath { origin: PathOrigin::Package(package.to_string()), components })
        }
        ShaderImport::AssetPath(path) => {
            let components: Vec<String> = path.split('/').filter(|c| !c.is_empty()).map(str::to_string).collect();
            (!components.is_empty()).then_some(ModulePath { origin: PathOrigin::Absolute, components })
        }
    }
}

struct Resolver<'a> {
    modules: HashMap<ModulePath, &'a str>,
    constants: String,
}

impl wesl::Resolver for Resolver<'_> {
    fn resolve_source<'b>(&'b self, path: &ModulePath) -> Result<Cow<'b, str>, wesl::error::ResolveError> {
        if path.origin == PathOrigin::Package("constants".into()) && path.components.is_empty() {
            return Ok(Cow::Borrowed(&self.constants));
        }
        // Bevy registers sub-packages as `pkg/sub`; its cache canonicalizes to the last part.
        let canonical = match &path.origin {
            PathOrigin::Package(p) if p.contains('/') => {
                ModulePath { origin: PathOrigin::Package(p.rsplit('/').next().unwrap().to_string()), components: path.components.clone() }
            }
            _ => path.clone(),
        };
        self.modules
            .get(&canonical)
            .map(|s| Cow::Borrowed(*s))
            .ok_or_else(|| wesl::error::ResolveError::ModuleNotFound(path.clone(), "no such shader module".into()))
    }
}

/// What the render world saw of our shaders' pipelines: the defs each was built with, and failures.
#[derive(Resource, Clone, Default)]
pub struct PipelineWatch(pub Arc<Mutex<WatchState>>);

#[derive(Default)]
pub struct WatchState {
    /// Shader assets that are material stages.
    pub ours: HashSet<AssetId<Shader>>,
    /// The defs of a pipeline that compiled, per stage shader.
    pub defs: HashMap<AssetId<Shader>, Vec<ShaderDefVal>>,
    /// Stage shaders with a built pipeline as of the last render frame: what draws now.
    pub drawing: HashSet<AssetId<Shader>>,
    /// Each stage's source version: bumped by the main world when it replaces the shader.
    pub version: HashMap<AssetId<Shader>, u64>,
    /// The versions the render world extracted with the shader assets of the same frame.
    extracted: HashMap<AssetId<Shader>, u64>,
    /// The versions extracted one frame earlier. Bevy may apply an extracted shader a frame late
    /// (its cache lock busy), so a failure is tagged with the version that has had a whole frame
    /// to apply: one frame of delay, never an old source's failure reported against a new one.
    applied: HashMap<AssetId<Shader>, u64>,
    /// Pipelines that failed: the shader, the source version it was built from, the pipeline's
    /// defs and the renderer's message.
    pub failed: Vec<Failure>,
    /// Failures already passed on, so each is told once per version.
    seen: HashSet<(AssetId<Shader>, u64, String)>,
}

/// A pipeline the renderer failed to build from a stage's shader.
pub struct Failure {
    pub shader: AssetId<Shader>,
    pub version: u64,
    pub defs: Vec<ShaderDefVal>,
    pub message: String,
}

impl WatchState {
    /// A stage's shader was replaced: a newer source version.
    pub fn replaced(&mut self, id: AssetId<Shader>) {
        self.ours.insert(id);
        *self.version.entry(id).or_default() += 1;
    }
}

/// Render world, at extract: the source versions that go with the shader assets this frame
/// extracts.
pub fn extract_versions(watch: Res<PipelineWatch>) {
    let mut w = watch.0.lock().unwrap();
    let now = w.version.clone();
    w.applied = std::mem::replace(&mut w.extracted, now);
}

/// Render world, every frame: records our pipelines' defs and failures.
pub fn watch_pipelines(cache: Res<bevy::render::render_resource::PipelineCache>, watch: Res<PipelineWatch>) {
    use bevy::render::render_resource::{CachedPipelineState, PipelineDescriptor};
    let mut w = watch.0.lock().unwrap();
    if w.ours.is_empty() {
        return;
    }
    let mut drawing = HashSet::new();
    for p in cache.pipelines() {
        let PipelineDescriptor::RenderPipelineDescriptor(d) = &p.descriptor else { continue };
        let stages = std::iter::once((d.vertex.shader.id(), &d.vertex.shader_defs)).chain(d.fragment.as_ref().map(|f| (f.shader.id(), &f.shader_defs)));
        for (id, defs) in stages {
            if !w.ours.contains(&id) {
                continue;
            }
            match &p.state {
                // Defs belong to the pipeline, not the source: any compiled one will do.
                CachedPipelineState::Ok(_) => {
                    drawing.insert(id);
                    w.defs.insert(id, defs.clone());
                }
                // Bevy leaves a pipeline waiting on its shader or an import in `Err` until its next
                // pass queues it again: not a failure.
                CachedPipelineState::Err(bevy::shader::ShaderCacheError::ShaderNotLoaded(_) | bevy::shader::ShaderCacheError::ShaderImportNotYetAvailable) => {}
                CachedPipelineState::Err(e) => {
                    let message = e.to_string();
                    let version = w.applied.get(&id).copied().unwrap_or_default();
                    if w.seen.insert((id, version, message.clone())) {
                        w.failed.push(Failure { shader: id, version, defs: defs.clone(), message });
                    }
                }
                _ => {}
            }
        }
    }
    w.drawing = drawing;
}
