//! Host-side TS: type stripping and source maps with oxc, walking the module graph from the
//! entry through relative imports. No type checking; that is the editor's and `tsc`'s job.

use bevy_ts::backend::{CompiledModule, ModuleSet, ScriptError, SourceLocation};
use bevy_ts::generation::ModuleSource;
use oxc::allocator::Allocator;
use oxc::ast::ast::Statement;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{SourceType, Span};
use oxc::transformer::{JsxRuntime, TransformOptions, Transformer};
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Specifiers the engine serves itself.
pub const BUILTINS: &[&str] = &["bevy-ts", "bevy-ts/bevy", "bevy-ts/jsx-runtime", "bevy-ts/jsx-dev-runtime"];

/// One compiled file.
pub struct Compiled {
    pub code: String,
    pub source_map: String,
    /// Relative imports left after type stripping, with where they are.
    pub imports: Vec<(String, SourceLocation)>,
}

/// A 1-based line and column for a byte offset.
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let before = &source[..(offset as usize).min(source.len())];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
    (line, col)
}

fn display(path: &Path) -> String {
    match std::env::current_dir().ok().and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf)) {
        Some(p) => p.display().to_string(),
        None => path.display().to_string(),
    }
}

fn location(path: &Path, source: &str, span: Span) -> SourceLocation {
    let (line, column) = line_col(source, span.start);
    SourceLocation { file: display(path), line, column }
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Compiles one TS or TSX file. `dev` uses the development JSX transform, so elements carry
/// their source location.
pub fn compile_file(path: &Path, source: &str, dev: bool) -> Result<Compiled, Vec<ScriptError>> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).map_err(|e| vec![ScriptError::new(format!("{}: {e}", display(path)))])?;
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
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program).semantic;
    let mut options = TransformOptions::default();
    options.jsx.runtime = JsxRuntime::Automatic;
    options.jsx.import_source = Some("bevy-ts".into());
    options.jsx.development = dev;
    // Keep value imports even when unused (verbatimModuleSyntax): only `import type` goes.
    options.typescript.only_remove_type_imports = true;
    let ret = Transformer::new(&allocator, path, &options).build_with_scoping(semantic.into_scoping(), &mut program);
    if ret.diagnostics.has_errors() {
        return Err(ret.diagnostics.errors().map(|d| ScriptError::at(d.message.to_string(), None)).collect());
    }

    let mut imports = Vec::new();
    let mut add = |spec: &str, span: Span| imports.push((spec.to_string(), location(path, source, span)));
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(d) => add(d.source.value.as_str(), d.source.span),
            Statement::ExportAllDeclaration(d) => add(d.source.value.as_str(), d.source.span),
            Statement::ExportFromDeclaration(d) => add(d.source.value.as_str(), d.source.span),
            _ => {}
        }
    }

    let out = Codegen::new()
        .with_options(CodegenOptions { source_map_path: Some(path.to_path_buf()), ..CodegenOptions::default() })
        .build(&program);
    let map = out.map.map(|m| m.to_json_string()).unwrap_or_default();
    // Point the map at the TS file by URL, with its text, so stacks and debuggers show TS.
    let mut map: serde_json::Value = serde_json::from_str(&map).unwrap_or(serde_json::json!({"version": 3, "mappings": ""}));
    map["sources"] = serde_json::json!([file_url(path)]);
    map["sourcesContent"] = serde_json::json!([source]);
    let map = map.to_string();
    // Inline too, so debuggers attached over CDP map breakpoints to the TS file themselves.
    let code = format!("{}\n//# sourceMappingURL=data:application/json;base64,{}\n", out.code, base64(map.as_bytes()));
    Ok(Compiled { code, source_map: map, imports })
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

/// Finds the file a relative import names, as the engine's loader does.
pub fn resolve_relative(from: &Path, spec: &str) -> Option<PathBuf> {
    let base = from.parent()?.join(spec);
    let s = base.to_string_lossy().to_string();
    let mut candidates = vec![base.clone()];
    if let Some(stem) = s.strip_suffix(".js").or_else(|| s.strip_suffix(".jsx")) {
        candidates.push(PathBuf::from(format!("{stem}.ts")));
        candidates.push(PathBuf::from(format!("{stem}.tsx")));
    }
    for ext in [".ts", ".tsx", "/index.ts", "/index.tsx"] {
        candidates.push(PathBuf::from(format!("{s}{ext}")));
    }
    candidates.into_iter().find(|c| c.is_file()).map(|c| c.canonicalize().unwrap_or_else(|_| normalize(&c)))
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

/// Compiles an entry and every module it reaches through relative imports.
pub struct Compiler {
    pub entry: PathBuf,
    pub dev: bool,
    /// Every file the last compile read, for the watcher.
    files: Mutex<BTreeSet<PathBuf>>,
}

impl Compiler {
    pub fn new(entry: impl AsRef<Path>, dev: bool) -> std::io::Result<Self> {
        let entry = std::env::current_dir()?.join(entry);
        // Canonical, so paths match what the file watcher reports (/var vs /private/var).
        let entry = entry.canonicalize().unwrap_or_else(|_| normalize(&entry));
        Ok(Compiler { entry, dev, files: Mutex::new(BTreeSet::new()) })
    }

    /// Files the last compile read (also those that failed).
    pub fn files(&self) -> BTreeSet<PathBuf> {
        self.files.lock().unwrap().clone()
    }

    pub fn compile_graph(&self) -> Result<ModuleSet, Vec<ScriptError>> {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([self.entry.clone()]);
        let mut modules = Vec::new();
        let mut errors = Vec::new();
        let mut files = BTreeSet::new();
        while let Some(path) = queue.pop_front() {
            if !seen.insert(path.clone()) {
                continue;
            }
            files.insert(path.clone());
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(ScriptError::new(format!("{}: {e}", display(&path))));
                    continue;
                }
            };
            match compile_file(&path, &source, self.dev) {
                Ok(c) => {
                    for (spec, at) in &c.imports {
                        if BUILTINS.contains(&spec.as_str()) {
                            continue;
                        }
                        if !is_relative(spec) {
                            errors.push(ScriptError::at(
                                format!("cannot import {spec:?}: bare npm specifiers are not supported; import relative files or bevy-ts modules"),
                                Some(at.clone()),
                            ));
                            continue;
                        }
                        match resolve_relative(&path, spec) {
                            Some(p) => queue.push_back(p),
                            None => errors.push(ScriptError::at(format!("cannot find module {spec:?}"), Some(at.clone()))),
                        }
                    }
                    modules.push(CompiledModule { path: path.clone(), code: c.code, source_map: Some(c.source_map) });
                }
                Err(e) => errors.extend(e),
            }
        }
        *self.files.lock().unwrap() = files;
        if errors.is_empty() { Ok(ModuleSet { entry: self.entry.clone(), modules }) } else { Err(errors) }
    }
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
        assert!(e[0].message.contains("\"lodash\""), "{e:?}");
        let loc = e[0].location.clone().unwrap();
        assert!(loc.file.ends_with("game.ts"));
        assert_eq!(loc.line, 2);
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
}
