//! The annotation reader: system signatures read from syntax alone, with no type checker.
//!
//! It reads the param types of every top-level function (and every `const x = system(...)`),
//! follows type aliases (generic ones by substituting the arguments as written), interfaces and
//! object types used as param bundles, and relative imports, all by name. The result is appended
//! after the module's code as `__bevyts.bind(...)` and `__bevyts.declare(...)` statements; no
//! function body is touched, so source maps and breakpoints are unchanged.
//!
//! A component, resource, message, event or state named in a param is a reference
//! `["<module path>", "<local name>"]` into the module whose source text names it. Each module
//! binds, as thunks, the value names its own text contributes, so circular imports and
//! declaration order do not matter.

use crate::compile::line_col;
use bevy_ts::backend::{ScriptError, SourceLocation};
use oxc::allocator::Allocator;
use oxc::ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportDefaultDeclarationKind, Expression, FormalParameters, ImportDeclarationSpecifier, Program,
    Statement, TSImportTypeQualifier, TSLiteral, TSSignature, TSTupleElement, TSType, TSTypeName, TSTypeParameterDeclaration,
};
use oxc::parser::Parser;
use oxc::span::{SourceType, Span};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
#[cfg(test)]
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// The port param-kind registry: the params Rust crates export with `app.ts_param` in this
/// process (`ReadRapierContext`). A port's param reads as `{ kind: "port", name, args }`.
pub fn port_param_kinds() -> Vec<String> {
    bevy_ts::host::param_kinds()
}

/// Param kinds bevy-ts itself declares, recognised by name.
const KINDS: &[&str] = &[
    "Query", "Res", "ResMut", "Local", "Commands", "MessageReader", "MessageWriter", "State", "NextState", "On", "App", "Mut", "Option",
    "Entity", "With", "Without", "Changed", "Added", "Add", "Insert", "Remove",
];

/// Field type names, which are types only and never bound.
const FIELD_TYPES: &[&str] = &[
    "f32", "f64", "i8", "u8", "i16", "u16", "i32", "u32", "bool", "vec2", "vec3", "vec4", "quat", "entity", "Vec2", "Vec3", "Vec3Like", "Vec4", "Quat",
];

/// Utility types that compute a type; the reader names them in its error.
const COMPUTED: &[&str] = &[
    "ReturnType", "Parameters", "ConstructorParameters", "InstanceType", "Pick", "Omit", "Partial", "Required", "Readonly", "Record",
    "Exclude", "Extract", "NonNullable", "Awaited",
];

/// The nearest ancestor of `entry` holding `package.json`, `tsconfig.json` or `bevy-ts.json`,
/// else the entry's directory. Identities are paths relative to it.
pub fn project_root(entry: &Path) -> PathBuf {
    let dir = entry.parent().unwrap_or(Path::new("/"));
    dir.ancestors()
        .find(|d| ["package.json", "tsconfig.json", "bevy-ts.json"].iter().any(|f| d.join(f).is_file()))
        .unwrap_or(dir)
        .to_path_buf()
}

/// `to` relative to the directory `from`, with `/` separators.
fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to_c: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to_c).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = from[common..].iter().map(|_| "..".to_string()).collect();
    parts.extend(to_c[common..].iter().map(|c| c.as_os_str().to_string_lossy().to_string()));
    parts.join("/")
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// A type as written, owned, with where it was written.
#[derive(Clone, Debug)]
enum Ty {
    Ref { name: String, args: Vec<Ty>, span: Span },
    Tuple(Vec<Ty>, Span),
    /// An object type's members; `None` for a member that is not a plain property.
    Object(Vec<Member>, Span),
    /// `import("bevy-ts").App`.
    App,
    /// A union of string literals: an enum field type.
    Strings(Vec<String>, Span),
    /// `boolean`, `string` or `number`.
    Keyword(&'static str, Span),
    /// Anything else, unreadable; `what` says what it is.
    Other { what: &'static str, span: Span },
}

fn ty(t: &TSType) -> Ty {
    let other = |what| Ty::Other { what, span: oxc::span::GetSpan::span(t) };
    match t {
        TSType::TSTypeReference(r) => match &r.type_name {
            TSTypeName::IdentifierReference(id) => Ty::Ref {
                name: id.name.to_string(),
                args: r.type_arguments.as_ref().map(|a| a.params.iter().map(ty).collect()).unwrap_or_default(),
                span: r.span,
            },
            _ => other("a qualified name"),
        },
        TSType::TSTupleType(t) => Ty::Tuple(
            t.element_types
                .iter()
                .map(|e| match e {
                    TSTupleElement::TSOptionalType(o) => Ty::Other { what: "an optional tuple element", span: o.span },
                    TSTupleElement::TSRestType(r) => Ty::Other { what: "a rest tuple element", span: r.span },
                    e => match e.to_ts_type() {
                        TSType::TSNamedTupleMember(m) => match m.element_type.as_ts_type() {
                            Some(t) if !m.optional => ty(t),
                            _ => Ty::Other { what: "an optional tuple element", span: m.span },
                        },
                        t => ty(t),
                    },
                })
                .collect(),
            t.span,
        ),
        TSType::TSTypeLiteral(l) => Ty::Object(members(&l.members), l.span),
        TSType::TSImportType(i) => match &i.qualifier {
            Some(TSImportTypeQualifier::Identifier(q)) if q.name == "App" => Ty::App,
            _ => other("an import type"),
        },
        TSType::TSParenthesizedType(p) => ty(&p.type_annotation),
        TSType::TSConditionalType(_) => other("a conditional type"),
        TSType::TSMappedType(_) => other("a mapped type"),
        TSType::TSIndexedAccessType(_) => other("an indexed access type"),
        TSType::TSTypeQuery(_) => other("a typeof type"),
        TSType::TSTypeOperatorType(_) => other("a type operator"),
        TSType::TSUnionType(u) => {
            let lits: Option<Vec<String>> = u
                .types
                .iter()
                .map(|t| match t {
                    TSType::TSLiteralType(l) => match &l.literal {
                        TSLiteral::StringLiteral(s) => Some(s.value.to_string()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            match lits {
                Some(v) => Ty::Strings(v, u.span),
                None => other("a union type"),
            }
        }
        TSType::TSBooleanKeyword(k) => Ty::Keyword("boolean", k.span),
        TSType::TSStringKeyword(k) => Ty::Keyword("string", k.span),
        TSType::TSNumberKeyword(k) => Ty::Keyword("number", k.span),
        TSType::TSIntersectionType(_) => other("an intersection type"),
        TSType::TSFunctionType(_) | TSType::TSConstructorType(_) => other("a function type"),
        TSType::TSArrayType(_) => other("an array type"),
        TSType::TSLiteralType(l) => match &l.literal {
            TSLiteral::StringLiteral(s) => Ty::Strings(vec![s.value.to_string()], l.span),
            _ => other("a literal type"),
        },
        TSType::TSTemplateLiteralType(_) => other("a literal type"),
        _ => other("a built-in type"),
    }
}

/// An object type's member: its name, its type (`None` if it is not a plain property), and `?`.
type Member = (String, Option<Ty>, Span, bool);

fn members(m: &[TSSignature]) -> Vec<Member> {
    m.iter()
        .map(|s| match s {
            TSSignature::TSPropertySignature(p) if !p.computed => match p.key.static_name() {
                Some(name) => (name.to_string(), p.type_annotation.as_ref().map(|a| ty(&a.type_annotation)), p.span, p.optional),
                None => (String::new(), None, p.span, false),
            },
            s => (String::new(), None, oxc::span::GetSpan::span(s), false),
        })
        .collect()
}

/// A type alias or interface: its type params and body.
#[derive(Debug)]
struct Named {
    params: Vec<String>,
    body: Ty,
}

#[derive(Debug)]
struct Import {
    spec: String,
    imported: String,
    type_only: bool,
    span: Span,
}

/// A function the reader declares.
#[derive(Debug)]
struct Func {
    name: String,
    span: Span,
    /// `const x = system(...)`: declared with values, no params to read.
    setup: bool,
    params: Vec<Param>,
}

#[derive(Debug)]
struct Param {
    name: String,
    ty: Option<Ty>,
    span: Span,
}

/// Bases a data class extends, from `bevy-ts`. `States(...)` and `SystemSet` are read too.
const DATA: &[&str] = &["Component", "SceneComponent", "ReflectComponent", "Resource", "Message", "Event", "EntityEvent"];

/// A class declaring a component, resource, message, event, state machine or system set.
#[derive(Debug)]
struct DataClass {
    name: String,
    span: Span,
    /// The bevy-ts base: one of [`DATA`], `States` or `SystemSet`.
    base: String,
    states: Vec<String>,
    path: Option<String>,
    propagate: bool,
    fields: Vec<FieldSrc>,
    scene: Option<SceneSrc>,
    /// Failures found while extracting: (where, what).
    errors: Vec<(Span, String)>,
}

#[derive(Debug)]
struct FieldSrc {
    name: String,
    ty: Option<Ty>,
    default: Option<Value>,
    span: Span,
}

/// `static scene({ speed = 2 }: { speed?: f32 })`: the props type and the destructured defaults.
#[derive(Debug)]
struct SceneSrc {
    ty: Option<Ty>,
    defaults: HashMap<String, Option<Value>>,
    span: Span,
}

/// A literal default: a number, a bool, a string, a vector as an array or `vec3(...)`/`quat(...)`.
fn literal(e: &Expression) -> Option<Value> {
    let num = |e: &Expression| match e {
        Expression::NumericLiteral(n) => Some(n.value),
        Expression::UnaryExpression(u) if u.operator.as_str() == "-" => match &u.argument {
            Expression::NumericLiteral(n) => Some(-n.value),
            _ => None,
        },
        _ => None,
    };
    // Whole numbers as integers, so the JSON reads as written.
    let number = |n: f64| if n.fract() == 0.0 && n.abs() < 9e15 { json!(n as i64) } else { json!(n) };
    if let Some(n) = num(e) {
        return Some(number(n));
    }
    match e {
        Expression::BooleanLiteral(b) => Some(json!(b.value)),
        Expression::StringLiteral(s) => Some(json!(s.value.as_str())),
        Expression::ArrayExpression(a) => a.elements.iter().map(|el| el.as_expression().and_then(num).map(number)).collect::<Option<Vec<_>>>().map(Value::Array),
        Expression::CallExpression(c) => {
            let Expression::Identifier(callee) = &c.callee else { return None };
            let args = c.arguments.iter().map(|a| a.as_expression().and_then(num)).collect::<Option<Vec<_>>>()?;
            let (lanes, fill): (usize, &[f64]) = match callee.name.as_str() {
                "vec2" => (2, &[0.0, 0.0]),
                "vec3" => (3, &[0.0, 0.0, 0.0]),
                "vec4" => (4, &[0.0, 0.0, 0.0, 0.0]),
                "quat" => (4, &[0.0, 0.0, 0.0, 1.0]),
                _ => return None,
            };
            Some(Value::Array((0..lanes).map(|i| number(args.get(i).copied().unwrap_or(fill[i]))).collect()))
        }
        Expression::ParenthesizedExpression(p) => literal(&p.expression),
        _ => None,
    }
}

/// The `bevy-ts` export a local name is, if it is imported from `bevy-ts`.
fn bevy_ts_name<'f>(f: &'f FileInfo, local: &str) -> Option<&'f str> {
    f.imports.get(local).filter(|i| i.spec == "bevy-ts" || i.spec.starts_with("bevy-ts/")).map(|i| i.imported.as_str())
}

/// Reads a class extending a bevy-ts data base, `States(...)` or `SystemSet`.
fn data_class(c: &Class, f: &FileInfo) -> Option<DataClass> {
    let id = c.id.as_ref()?;
    let (base, states) = match &c.heritage.as_ref()?.expression {
        Expression::Identifier(b) => {
            let name = bevy_ts_name(f, b.name.as_str())?;
            if !DATA.contains(&name) && name != "SystemSet" {
                return None;
            }
            (name.to_string(), Vec::new())
        }
        Expression::CallExpression(call) => {
            let Expression::Identifier(callee) = &call.callee else { return None };
            if bevy_ts_name(f, callee.name.as_str()) != Some("States") {
                return None;
            }
            let values = call.arguments.iter().filter_map(|a| match a.as_expression() {
                Some(Expression::StringLiteral(s)) => Some(s.value.to_string()),
                _ => None,
            });
            ("States".to_string(), values.collect())
        }
        _ => return None,
    };
    let mut d = DataClass { name: id.name.to_string(), span: id.span, base, states, path: None, propagate: false, fields: Vec::new(), scene: None, errors: Vec::new() };
    for el in &c.body.body {
        match el {
            ClassElement::PropertyDefinition(p) if !p.computed => {
                let Some(name) = p.key.static_name() else { continue };
                if p.r#static {
                    match (name.as_ref(), &p.value) {
                        ("path", Some(Expression::StringLiteral(s))) => d.path = Some(s.value.to_string()),
                        ("path", _) => d.errors.push((p.span, "static path must be a string literal".into())),
                        ("propagate", Some(Expression::BooleanLiteral(b))) => d.propagate = b.value,
                        _ => {}
                    }
                    continue;
                }
                let default = p.value.as_ref().and_then(literal);
                d.fields.push(FieldSrc { name: name.to_string(), ty: p.type_annotation.as_ref().map(|a| ty(&a.type_annotation)), default, span: p.span });
            }
            ClassElement::MethodDefinition(m) if m.r#static && m.key.static_name().as_deref() == Some("scene") => {
                let mut scene = SceneSrc { ty: None, defaults: HashMap::new(), span: m.span };
                if let Some(param) = m.value.params.items.first() {
                    scene.ty = param.type_annotation.as_ref().map(|a| ty(&a.type_annotation));
                    if let BindingPattern::ObjectPattern(o) = &param.pattern {
                        for prop in &o.properties {
                            let Some(key) = prop.key.static_name() else { continue };
                            let default = match &prop.value {
                                BindingPattern::AssignmentPattern(a) => literal(&a.right).or(Some(Value::Null)),
                                _ => None,
                            };
                            scene.defaults.insert(key.to_string(), default);
                        }
                    }
                }
                d.scene = Some(scene);
            }
            _ => {}
        }
    }
    Some(d)
}

/// The Rust-style module path of a root-relative file: `src/units/tank.ts` is `units::tank`.
pub fn module_path(rel: &str) -> String {
    let rel = rel.strip_prefix("src/").unwrap_or(rel);
    let rel = rel.rsplit_once('.').map_or(rel, |(stem, ext)| if ext.contains('/') { rel } else { stem });
    rel.split('/').map(|seg| seg.replace(['-', '.'], "_")).collect::<Vec<_>>().join("::")
}

/// What the reader needs from one file.
#[derive(Debug)]
pub(crate) struct FileInfo {
    path: PathBuf,
    /// Root-relative path: the module's identity prefix.
    rel: String,
    source: String,
    named: HashMap<String, Named>,
    imports: HashMap<String, Import>,
    /// `export { a as b } from "./x"`: exported name to (specifier, name there).
    reexports: HashMap<String, (String, String)>,
    /// Top-level value bindings.
    values: BTreeSet<String>,
    /// Local name to the names it is exported as.
    exports: HashMap<String, Vec<String>>,
    functions: Vec<Func>,
    classes: Vec<DataClass>,
    imports_bevy_ts: bool,
}

fn type_params(p: &Option<oxc::allocator::Box<TSTypeParameterDeclaration>>) -> Vec<String> {
    p.as_ref().map(|p| p.params.iter().map(|t| t.name.name.to_string()).collect()).unwrap_or_default()
}

fn params(p: &FormalParameters) -> Vec<Param> {
    let mut out: Vec<Param> = p
        .items
        .iter()
        .map(|i| Param {
            name: match &i.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.to_string(),
                _ => "{…}".into(),
            },
            ty: i.type_annotation.as_ref().map(|a| ty(&a.type_annotation)),
            span: i.span,
        })
        .collect();
    if let Some(r) = &p.rest {
        out.push(Param { name: "...".into(), ty: Some(Ty::Other { what: "a rest param", span: r.span }), span: r.span });
    }
    out
}

/// Collects what the reader needs from a parsed file. Call it before types are stripped.
pub(crate) fn extract(program: &Program, path: &Path, source: &str, root: &Path) -> FileInfo {
    let mut f = FileInfo {
        path: path.to_path_buf(),
        rel: relative(root, path),
        source: source.to_string(),
        named: HashMap::new(),
        imports: HashMap::new(),
        reexports: HashMap::new(),
        values: BTreeSet::new(),
        exports: HashMap::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        imports_bevy_ts: false,
    };
    for stmt in &program.body {
        if let Statement::ImportDeclaration(d) = stmt {
            let spec = d.source.value.to_string();
            f.imports_bevy_ts |= spec == "bevy-ts" || spec.starts_with("bevy-ts/");
            for s in d.specifiers.iter().flatten() {
                let (local, imported, kind_type, span) = match s {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => (&s.local, s.imported.name().to_string(), s.import_kind.is_type(), s.span),
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => (&s.local, "default".into(), false, s.span),
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => (&s.local, "*".into(), false, s.span),
                };
                let import = Import { spec: spec.clone(), imported, type_only: d.import_kind.is_type() || kind_type, span };
                f.imports.insert(local.name.to_string(), import);
            }
        }
    }
    let system = |name: &str, f: &FileInfo| {
        f.imports.get(name).map_or(name == "system", |i| i.imported == "system" && (i.spec == "bevy-ts" || i.spec.starts_with("bevy-ts/")))
    };
    let declare = |d: &Declaration, exported: bool, f: &mut FileInfo| {
        let export = |name: &str, f: &mut FileInfo| {
            if exported {
                f.exports.entry(name.to_string()).or_default().push(name.to_string());
            }
        };
        match d {
            Declaration::VariableDeclaration(v) if !v.declare => {
                for d in &v.declarations {
                    for id in d.id.get_binding_identifiers() {
                        f.values.insert(id.name.to_string());
                        export(id.name.as_str(), f);
                    }
                    let BindingPattern::BindingIdentifier(id) = &d.id else { continue };
                    let (setup, ps) = match &d.init {
                        Some(Expression::CallExpression(c)) => match &c.callee {
                            Expression::Identifier(callee) if system(callee.name.as_str(), f) => (true, Vec::new()),
                            _ => continue,
                        },
                        Some(Expression::ArrowFunctionExpression(a)) => (false, params(&a.params)),
                        Some(Expression::FunctionExpression(fun)) => (false, params(&fun.params)),
                        _ => continue,
                    };
                    f.functions.push(Func { name: id.name.to_string(), span: id.span, setup, params: ps });
                }
            }
            Declaration::FunctionDeclaration(fun) if !fun.declare && fun.body.is_some() => {
                if let Some(id) = &fun.id {
                    f.values.insert(id.name.to_string());
                    export(id.name.as_str(), f);
                    f.functions.push(Func { name: id.name.to_string(), span: id.span, setup: false, params: params(&fun.params) });
                }
            }
            Declaration::ClassDeclaration(c) if !c.declare => {
                if let Some(id) = &c.id {
                    f.values.insert(id.name.to_string());
                    export(id.name.as_str(), f);
                }
                if let Some(d) = data_class(c, f) {
                    f.classes.push(d);
                }
            }
            Declaration::TSEnumDeclaration(e) if !e.declare => {
                f.values.insert(e.id.name.to_string());
                export(e.id.name.as_str(), f);
            }
            Declaration::TSTypeAliasDeclaration(a) => {
                f.named.insert(a.id.name.to_string(), Named { params: type_params(&a.type_parameters), body: ty(&a.type_annotation) });
            }
            Declaration::TSInterfaceDeclaration(i) => {
                let body = if i.extends.is_empty() {
                    Ty::Object(members(&i.body.body), i.span)
                } else {
                    Ty::Other { what: "an interface that extends another", span: i.span }
                };
                f.named.insert(i.id.name.to_string(), Named { params: type_params(&i.type_parameters), body });
            }
            _ => {}
        }
    };
    for stmt in &program.body {
        match stmt {
            Statement::ExportDeclaration(e) => declare(&e.declaration, true, &mut f),
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(fun) if fun.body.is_some() => {
                    if let Some(id) = &fun.id {
                        f.values.insert(id.name.to_string());
                        f.exports.entry(id.name.to_string()).or_default().push("default".into());
                        f.functions.push(Func { name: id.name.to_string(), span: id.span, setup: false, params: params(&fun.params) });
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        f.values.insert(id.name.to_string());
                        f.exports.entry(id.name.to_string()).or_default().push("default".into());
                    }
                    if let Some(d) = data_class(c, &f) {
                        f.classes.push(d);
                    }
                }
                ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) if i.extends.is_empty() => {
                    let body = Ty::Object(members(&i.body.body), i.span);
                    f.named.insert(i.id.name.to_string(), Named { params: type_params(&i.type_parameters), body });
                }
                _ => {}
            },
            Statement::ExportNamedDeclaration(e) if !e.export_kind.is_type() => {
                for s in e.specifiers.iter().filter(|s| !s.export_kind.is_type()) {
                    f.exports.entry(s.local.name().to_string()).or_default().push(s.exported.name().to_string());
                }
            }
            Statement::ExportFromDeclaration(e) => {
                for s in &e.specifiers {
                    f.reexports.insert(s.exported.name().to_string(), (e.source.value.to_string(), s.local.name().to_string()));
                }
            }
            s => {
                if let Some(d) = s.as_declaration() {
                    declare(d, false, &mut f);
                }
            }
        }
    }
    f
}

/// Where a type is read: the file whose text wrote it, and the generic arguments in force.
#[derive(Clone)]
struct Scope {
    file: Rc<FileInfo>,
    subst: Rc<HashMap<String, (Ty, Scope)>>,
}

/// A failure to read a param.
struct Fail {
    file: Rc<FileInfo>,
    span: Span,
    message: String,
    /// A name imported with `import type`: fails the compile rather than the registration.
    type_import: bool,
}

/// What a type names once aliases, substitutions and imports are followed.
enum Head {
    Kind { name: String, args: Vec<Ty>, span: Span, scope: Scope },
    Value(Value),
    Bundle(Vec<Member>, Scope),
    Tuple(Vec<Ty>, Scope),
    App,
}

/// The appended declarations for one module.
#[derive(Default)]
pub(crate) struct Declarations {
    /// JS to append after the module's code.
    pub code: String,
    /// Relative specifiers the declarations need evaluated (modules whose names are bound).
    pub imports: Vec<String>,
    /// Data classes by local name, with the declaration their static block hands
    /// `__bevyts.data(this, ...)` (JSON).
    pub classes: Vec<(String, String)>,
}

struct Reader {
    root: PathBuf,
    ports: Vec<String>,
    files: HashMap<PathBuf, Option<Rc<FileInfo>>>,
    /// Modules (by path) a declaration referenced.
    referenced: BTreeSet<PathBuf>,
}

/// Reads one module's declarations. `Err` holds compile errors: names a param needs that are
/// imported with `import type`. Every other failure is carried lazily as an `error` declaration.
pub(crate) fn read(file: FileInfo, root: &Path) -> Result<Declarations, Vec<ScriptError>> {
    read_with(file, root, port_param_kinds())
}

fn read_with(file: FileInfo, root: &Path, ports: Vec<String>) -> Result<Declarations, Vec<ScriptError>> {
    let file = Rc::new(file);
    let mut r = Reader { root: root.to_path_buf(), ports, files: HashMap::new(), referenced: BTreeSet::new() };
    r.files.insert(file.path.clone(), Some(file.clone()));
    let scope = Scope { file: file.clone(), subst: Rc::default() };
    let mut declares = Vec::new();
    let mut errors = Vec::new();
    let mut any_read = false;
    for func in &file.functions {
        let exported = file.exports.get(&func.name).and_then(|e| e.first());
        let (line, column) = line_col(&file.source, func.span.start);
        let mut d = json!({ "at": { "file": file.rel, "line": line, "column": column } });
        match exported {
            Some(export) => d["id"] = json!(format!("{}#{export}", file.rel)),
            None => {
                d["error"] = json!(format!(
                    "{} is not exported; a system, observer or plugin is an exported function, identified as module#export",
                    func.name
                ));
            }
        }
        if func.setup {
            d["kind"] = json!("setup");
        } else {
            d["kind"] = json!("system");
            if exported.is_some() {
                match r.params(&func.params, &scope) {
                    Ok(params) => {
                        any_read |= !params.is_empty();
                        d["params"] = Value::Array(params);
                    }
                    Err(f) if f.type_import => errors.push(r.script_error(&f)),
                    Err(f) => d["error"] = json!(r.message(&f)),
                }
            }
        }
        declares.push(format!(";__bevyts.declare({}, {d});", func.name));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let classes: Vec<_> = file.classes.iter().map(|c| (c.name.clone(), r.class(c, &scope).to_string())).collect();
    if !file.imports_bevy_ts && !any_read {
        // A module that never names bevy-ts declares nothing: helpers in plain modules stay plain.
        return Ok(Declarations { code: String::new(), imports: Vec::new(), classes });
    }
    let mut code = String::new();
    let binds = r.binds(&file);
    if !binds.is_empty() {
        let thunks: Vec<_> = binds.iter().map(|n| format!("{n}: () => {n}")).collect();
        code.push_str(&format!(";__bevyts.bind({}, {{ {} }});\n", json!(file.rel), thunks.join(", ")));
    }
    let dir = file.path.parent().unwrap_or(Path::new("/"));
    let mut imports = Vec::new();
    for p in r.referenced.iter().filter(|p| **p != file.path) {
        let rel = relative(dir, p);
        let spec = if rel.starts_with("..") { rel } else { format!("./{rel}") };
        code.push_str(&format!(";import {};\n", json!(spec)));
        imports.push(spec);
    }
    for d in declares {
        code.push_str(&d);
        code.push('\n');
    }
    Ok(Declarations { code, imports, classes })
}

impl Reader {
    fn location(&self, f: &Fail) -> (u32, u32) {
        line_col(&f.file.source, f.span.start)
    }

    fn message(&self, f: &Fail) -> String {
        let (line, column) = self.location(f);
        format!("{}:{line}:{column}: {}", f.file.rel, f.message)
    }

    fn script_error(&self, f: &Fail) -> ScriptError {
        let (line, column) = self.location(f);
        ScriptError::at(f.message.clone(), Some(SourceLocation { file: f.file.path.display().to_string(), line, column }))
    }

    /// The value names a module binds: every value its own param types and type declarations name.
    fn binds(&self, file: &FileInfo) -> BTreeSet<String> {
        fn walk(t: &Ty, file: &FileInfo, params: &[String], out: &mut BTreeSet<String>) {
            match t {
                Ty::Ref { name, args, .. } => {
                    let value = file.values.contains(name) || file.imports.get(name).is_some_and(|i| !i.type_only && i.imported != "*");
                    if value && !params.contains(name) && !KINDS.contains(&name.as_str()) && !FIELD_TYPES.contains(&name.as_str()) {
                        out.insert(name.clone());
                    }
                    args.iter().for_each(|a| walk(a, file, params, out));
                }
                Ty::Tuple(ts, _) => ts.iter().for_each(|a| walk(a, file, params, out)),
                Ty::Object(ms, _) => ms.iter().filter_map(|m| m.1.as_ref()).for_each(|a| walk(a, file, params, out)),
                Ty::App | Ty::Other { .. } | Ty::Strings(..) | Ty::Keyword(..) => {}
            }
        }
        let mut out = BTreeSet::new();
        for n in file.named.values() {
            walk(&n.body, file, &n.params, &mut out);
        }
        for f in &file.functions {
            for p in f.params.iter().filter_map(|p| p.ty.as_ref()) {
                walk(p, file, &[], &mut out);
            }
        }
        out
    }

    fn load(&mut self, path: &Path) -> Option<Rc<FileInfo>> {
        if let Some(f) = self.files.get(path) {
            return f.clone();
        }
        let info = std::fs::read_to_string(path).ok().and_then(|source| {
            let allocator = Allocator::default();
            let source_type = SourceType::from_path(path).ok()?;
            let parsed = Parser::new(&allocator, &source, source_type).parse();
            Some(Rc::new(extract(&parsed.program, path, &source, &self.root)))
        });
        self.files.insert(path.to_path_buf(), info.clone());
        info
    }

    /// The type alias or interface `name` means in `file`, following relative imports.
    fn lookup(&mut self, file: &Rc<FileInfo>, name: &str, depth: u32) -> Option<(Rc<FileInfo>, String)> {
        if depth > 32 {
            return None;
        }
        if file.named.contains_key(name) {
            return Some((file.clone(), name.to_string()));
        }
        let (spec, there) = match file.imports.get(name) {
            Some(i) => (i.spec.clone(), i.imported.clone()),
            None => file.reexports.get(name).cloned()?,
        };
        if !(spec.starts_with("./") || spec.starts_with("../")) {
            return None;
        }
        let target = crate::compile::resolve_relative(&file.path, &spec)?;
        let target = self.load(&normalize(&target))?;
        self.lookup(&target, &there, depth + 1)
    }

    /// A data class's declaration (design decision 13): kind, name, fields and scene props.
    fn class(&mut self, c: &DataClass, scope: &Scope) -> Value {
        let file = &scope.file;
        let at = |span: Span| {
            let (line, column) = line_col(&file.source, span.start);
            (line, column)
        };
        let (line, column) = at(c.span);
        let export = file.exports.get(&c.name).and_then(|e| e.first()).cloned();
        let kind = match c.base.as_str() {
            "Component" => "component",
            "SceneComponent" => "sceneComponent",
            "ReflectComponent" => "reflectComponent",
            "Resource" => "resource",
            "Message" => "message",
            "Event" => "event",
            "EntityEvent" => "entityEvent",
            "States" => "state",
            _ => "set",
        };
        let shown = export.as_deref().filter(|e| *e != "default").unwrap_or(&c.name);
        let name = c.path.clone().unwrap_or_else(|| format!("{}::{shown}", module_path(&file.rel)));
        let mut d = json!({ "kind": kind, "name": name, "at": { "file": file.rel, "line": line, "column": column } });
        if let Some(e) = &export {
            d["id"] = json!(format!("{}#{e}", file.rel));
        }
        let mut errors: Vec<(Span, String)> = c.errors.clone();
        if kind == "state" {
            if c.states.is_empty() {
                errors.push((c.span, "States(...) takes the values as string literals: States(\"Menu\", \"Playing\")".into()));
            }
            d["values"] = json!(c.states);
        } else if kind != "set" {
            let mut fields = Vec::new();
            for f in &c.fields {
                if kind == "entityEvent" && f.name == "entity" {
                    continue; // the target, carried beside the payload
                }
                let Some(t) = &f.ty else {
                    errors.push((f.span, format!("field {} of {} needs a type (f32, u32, bool, Entity, Vec3, Quat, a string-literal union, ...)", f.name, c.name)));
                    continue;
                };
                match self.field_ty(t, scope, false, 0) {
                    Ok(ty) => {
                        let mut field = json!({ "name": f.name, "type": ty });
                        if let Some(v) = &f.default {
                            field["default"] = v.clone();
                        }
                        fields.push(field);
                    }
                    Err((span, why)) => errors.push((span, format!("{} field {}: {why}", c.name, f.name))),
                }
            }
            d["fields"] = json!(fields);
            if kind == "entityEvent" {
                d["propagate"] = json!(c.propagate);
            }
        }
        if let Some(sc) = &c.scene {
            if kind != "sceneComponent" {
                errors.push((sc.span, format!("{}: static scene belongs to a SceneComponent", c.name)));
            }
            match self.props(sc, scope) {
                Ok(props) => {
                    for p in &props {
                        if c.fields.iter().any(|f| Some(f.name.as_str()) == p["name"].as_str()) {
                            errors.push((sc.span, format!("{}: {} is both a field and a scene prop", c.name, p["name"].as_str().unwrap_or(""))));
                        }
                    }
                    d["scene"] = json!({ "props": props });
                }
                Err((span, why)) => errors.push((span, format!("{} scene props: {why}", c.name))),
            }
        } else if kind == "sceneComponent" {
            d["scene"] = json!({ "props": [] });
        }
        if let Some((span, why)) = errors.first() {
            let (line, column) = at(*span);
            d["error"] = json!(format!("{}:{line}:{column}: {why}", file.rel));
        }
        d
    }

    /// A scene's props: each property of its props type, with the destructured default.
    fn props(&mut self, sc: &SceneSrc, scope: &Scope) -> Result<Vec<Value>, (Span, String)> {
        let Some(t) = &sc.ty else {
            return if sc.defaults.is_empty() { Ok(Vec::new()) } else { Err((sc.span, "annotate the props type: scene({ speed = 2 }: { speed?: f32 })".into())) };
        };
        let (members, s) = self.object(t, scope, 0)?;
        let mut out = Vec::new();
        for (name, t, span, optional) in members {
            let Some(t) = t else { return Err((span, "props are plain named properties".into())) };
            let mut p = json!({ "name": name, "type": self.field_ty(&t, &s, true, 0)?, "optional": optional });
            match sc.defaults.get(&name) {
                Some(Some(Value::Null)) => p["hasDefault"] = json!(true),
                Some(Some(v)) => p["default"] = v.clone(),
                _ => {}
            }
            out.push(p);
        }
        Ok(out)
    }

    /// The members of an object type, following aliases and interfaces by name.
    fn object(&mut self, t: &Ty, scope: &Scope, depth: u32) -> Result<(Vec<Member>, Scope), (Span, String)> {
        match t {
            Ty::Object(ms, _) => Ok((ms.clone(), scope.clone())),
            Ty::Ref { name, args, span } if args.is_empty() && depth < 32 => match self.lookup(&scope.file, name, 0) {
                Some((file, there)) => {
                    let body = file.named[&there].body.clone();
                    self.object(&body, &Scope { file, subst: Rc::default() }, depth + 1)
                }
                None => Err((*span, format!("{name} is not an object type declared in a project module"))),
            },
            t => Err((span_of(t), "the props type is an object type, an alias of one or an interface".into())),
        }
    }

    /// A field type: `f32`, `bool`, `Vec3`, `Entity`, a string-literal union, or an alias of one.
    /// `string` only where `strings` (scene props).
    fn field_ty(&mut self, t: &Ty, scope: &Scope, strings: bool, depth: u32) -> Result<Value, (Span, String)> {
        const NAMED: &[(&str, &str)] = &[
            ("f32", "f32"), ("f64", "f64"), ("i8", "i8"), ("u8", "u8"), ("i16", "i16"), ("u16", "u16"), ("i32", "i32"), ("u32", "u32"),
            ("bool", "bool"), ("Vec2", "vec2"), ("vec2", "vec2"), ("Vec3", "vec3"), ("vec3", "vec3"), ("Vec3Like", "vec3"),
            ("Vec4", "vec4"), ("vec4", "vec4"), ("Quat", "quat"), ("quat", "quat"), ("Entity", "entity"), ("entity", "entity"),
        ];
        let bad = |span: Span, text: &str| {
            (span, format!("{text} is not a field type (f32, f64, i8..u32, bool, Vec2, Vec3, Vec4, Quat, Entity, a string-literal union){}", if strings { " or string" } else { "" }))
        };
        if depth > 32 {
            return Err((span_of(t), "type aliases nest too deep (a cycle?)".into()));
        }
        match t {
            Ty::Strings(v, _) => Ok(json!({ "kind": "enum", "variants": v })),
            Ty::Keyword("boolean", _) => Ok(json!({ "kind": "bool" })),
            Ty::Keyword("string", _) if strings => Ok(json!({ "kind": "string" })),
            Ty::Keyword("string", span) => Err((*span, "strings are not supported; components hold fixed-size data only".into())),
            Ty::Keyword(k, span) => Err(bad(*span, k)),
            Ty::Ref { name, args, span } if args.is_empty() => {
                if let Some((t, s)) = scope.subst.get(name) {
                    let (t, s) = (t.clone(), s.clone());
                    return self.field_ty(&t, &s, strings, depth + 1);
                }
                if let Some((_, k)) = NAMED.iter().find(|(n, _)| n == name) {
                    return Ok(json!({ "kind": k }));
                }
                match self.lookup(&scope.file, name, 0) {
                    Some((file, there)) => {
                        let body = file.named[&there].body.clone();
                        self.field_ty(&body, &Scope { file, subst: Rc::default() }, strings, depth + 1)
                    }
                    None => Err(bad(*span, name)),
                }
            }
            t => {
                let span = span_of(t);
                Err(bad(span, &scope.file.source[span.start as usize..span.end as usize]))
            }
        }
    }

    fn unreadable(&self, scope: &Scope, span: Span, what: &str) -> Fail {
        let text = &scope.file.source[span.start as usize..span.end as usize];
        self.fail(scope, span, format!("`{text}` is {what}; a param type is a bevy-ts param kind, an alias of one or a param bundle. Use system(() => ...) for computed types"))
    }

    fn fail(&self, scope: &Scope, span: Span, message: impl Into<String>) -> Fail {
        Fail { file: scope.file.clone(), span, message: message.into(), type_import: false }
    }

    fn head(&mut self, t: &Ty, scope: &Scope, depth: u32) -> Result<Head, Fail> {
        if depth > 64 {
            return Err(self.fail(scope, span_of(t), "type aliases nest too deep (a cycle?)"));
        }
        let (name, args, span) = match t {
            Ty::Ref { name, args, span } => (name, args, *span),
            Ty::Tuple(ts, _) => return Ok(Head::Tuple(ts.clone(), scope.clone())),
            Ty::Object(ms, _) => return Ok(Head::Bundle(ms.clone(), scope.clone())),
            Ty::App => return Ok(Head::App),
            Ty::Strings(_, span) => return Err(self.unreadable(scope, *span, "a string-literal union")),
            Ty::Keyword(_, span) => return Err(self.unreadable(scope, *span, "a built-in type")),
            Ty::Other { what, span } => {
                let text = &scope.file.source[span.start as usize..span.end as usize];
                return Err(self.fail(scope, *span, format!("`{text}` is {what}; a param type is a bevy-ts param kind, an alias of one or a param bundle. Use system(() => ...) for computed types")));
            }
        };
        if let Some((t, s)) = scope.subst.get(name) {
            if !args.is_empty() {
                return Err(self.fail(scope, span, format!("{name} is a type parameter and takes no arguments")));
            }
            let (t, s) = (t.clone(), s.clone());
            return self.head(&t, &s, depth + 1);
        }
        if KINDS.contains(&name.as_str()) || self.ports.iter().any(|p| p == name) {
            return Ok(Head::Kind { name: name.clone(), args: args.clone(), span, scope: scope.clone() });
        }
        if let Some((file, there)) = self.lookup(&scope.file, name, 0) {
            let named = &file.named[&there];
            if args.len() != named.params.len() {
                return Err(self.fail(scope, span, format!("{name} takes {} type arguments, given {}", named.params.len(), args.len())));
            }
            let subst = named.params.iter().cloned().zip(args.iter().map(|a| (a.clone(), scope.clone()))).collect::<HashMap<_, _>>();
            let body = named.body.clone();
            let inner = Scope { file: file.clone(), subst: Rc::new(subst) };
            return self.head(&body, &inner, depth + 1);
        }
        if let Some(i) = scope.file.imports.get(name) {
            if i.type_only {
                return Err(Fail {
                    file: scope.file.clone(),
                    span: i.span,
                    message: format!("{name} is imported as a type; system params need its value"),
                    type_import: true,
                });
            }
            if i.imported != "*" {
                return self.value(name, args, span, scope);
            }
        }
        if scope.file.values.contains(name) {
            return self.value(name, args, span, scope);
        }
        if COMPUTED.contains(&name.as_str()) {
            let text = &scope.file.source[span.start as usize..span.end as usize];
            return Err(self.fail(scope, span, format!("`{text}` is a computed type ({name}); the reader follows names only. Use system(() => ...) instead")));
        }
        Err(self.fail(scope, span, format!("{name} is not a system param kind, an alias of one, a param bundle or a value declared in this module")))
    }

    fn value(&mut self, name: &str, args: &[Ty], span: Span, scope: &Scope) -> Result<Head, Fail> {
        if !args.is_empty() {
            return Err(self.fail(scope, span, format!("{name} is a value and takes no type arguments")));
        }
        self.referenced.insert(scope.file.path.clone());
        Ok(Head::Value(json!([scope.file.rel, name])))
    }

    fn params(&mut self, ps: &[Param], scope: &Scope) -> Result<Vec<Value>, Fail> {
        ps.iter()
            .map(|p| match &p.ty {
                Some(t) => self.param(t, scope),
                None => Err(self.fail(scope, p.span, format!("param {} has no type; annotate it with a param kind such as Query<[...]>", p.name))),
            })
            .collect()
    }

    fn arg<'t>(&self, name: &str, args: &'t [Ty], i: usize, span: Span, scope: &Scope) -> Result<&'t Ty, Fail> {
        args.get(i).ok_or_else(|| self.fail(scope, span, format!("{name} needs {} type argument{}", i + 1, if i == 0 { "" } else { "s" })))
    }

    /// A component, resource, message, event or state name.
    fn reference(&mut self, t: &Ty, scope: &Scope, what: &str) -> Result<Value, Fail> {
        match self.head(t, scope, 0)? {
            Head::Value(v) => Ok(v),
            Head::Kind { name, span, scope, .. } => Err(self.fail(&scope, span, format!("{name}<...> is not {what}"))),
            _ => Err(self.fail(scope, span_of(t), format!("expected {what}"))),
        }
    }

    fn param(&mut self, t: &Ty, scope: &Scope) -> Result<Value, Fail> {
        let (name, args, span, scope) = match self.head(t, scope, 0)? {
            Head::Kind { name, args, span, scope } => (name, args, span, scope),
            Head::Value(v) => {
                return Err(self.fail(scope, span_of(t), format!("{} is a value, not a system param; query it with Query<[{}]>", v[1], v[1].as_str().unwrap_or(""))));
            }
            Head::Bundle(members, s) => {
                let mut fields = Vec::new();
                for (name, t, span, optional) in members {
                    let (Some(t), false) = (t, optional) else {
                        return Err(self.fail(&s, span, "a param bundle holds plain named properties only"));
                    };
                    fields.push(json!({ "name": name, "param": self.param(&t, &s)? }));
                }
                return Ok(json!({ "kind": "bundle", "fields": fields }));
            }
            Head::Tuple(..) => return Err(self.fail(scope, span_of(t), "a tuple is not a system param; Query takes one as its data")),
            Head::App => return Ok(json!({ "kind": "app" })),
        };
        let s = &scope;
        match name.as_str() {
            "Query" => {
                let data = match self.head(self.arg(&name, &args, 0, span, s)?, s, 0)? {
                    Head::Tuple(ts, ds) => ts.iter().map(|d| self.datum(d, &ds)).collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(self.fail(s, span, "Query's data is a tuple: Query<[A, Mut<B>]>")),
                };
                let filter = match args.get(1) {
                    None => Vec::new(),
                    Some(f) => match self.head(f, s, 0)? {
                        Head::Tuple(ts, fs) => ts.iter().map(|f| self.filter(f, &fs)).collect::<Result<Vec<_>, _>>()?,
                        _ => vec![self.filter(f, s)?],
                    },
                };
                Ok(json!({ "kind": "query", "data": data, "filter": filter }))
            }
            "Res" | "ResMut" => {
                let r = self.reference(self.arg(&name, &args, 0, span, s)?, s, "a resource")?;
                Ok(json!({ "kind": if name == "Res" { "res" } else { "resMut" }, "resource": r }))
            }
            "Local" => {
                let fields = match self.head(self.arg(&name, &args, 0, span, s)?, s, 0)? {
                    Head::Bundle(members, ls) => members
                        .into_iter()
                        .map(|(n, t, sp, _)| match t.as_ref().map(|t| self.field_ty(t, &ls, false, 0)) {
                            Some(Ok(ft)) => Ok(json!({ "name": n, "type": ft })),
                            Some(Err((sp, why))) => Err(self.fail(&ls, sp, format!("Local field {n}: {why}"))),
                            None => Err(self.fail(&ls, sp, format!("Local field {n} needs a field type"))),
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(self.fail(s, span, "Local takes an object of field types: Local<{ n: u32 }>")),
                };
                Ok(json!({ "kind": "local", "fields": fields }))
            }
            "Commands" => Ok(json!({ "kind": "commands" })),
            "App" => Ok(json!({ "kind": "app" })),
            "MessageReader" | "MessageWriter" => {
                let m = self.reference(self.arg(&name, &args, 0, span, s)?, s, "a message")?;
                Ok(json!({ "kind": if name == "MessageReader" { "messageReader" } else { "messageWriter" }, "message": m }))
            }
            "State" | "NextState" => {
                let st = self.reference(self.arg(&name, &args, 0, span, s)?, s, "a state")?;
                Ok(json!({ "kind": if name == "State" { "state" } else { "nextState" }, "state": st }))
            }
            "On" => match self.head(self.arg(&name, &args, 0, span, s)?, s, 0)? {
                Head::Kind { name: l, args: largs, span: lspan, scope: ls } if matches!(l.as_str(), "Add" | "Insert" | "Remove") => {
                    let c = self.reference(self.arg(&l, &largs, 0, lspan, &ls)?, &ls, "a component")?;
                    Ok(json!({ "kind": "on", "lifecycle": l.to_lowercase(), "component": c }))
                }
                Head::Value(e) => Ok(json!({ "kind": "on", "event": e })),
                _ => Err(self.fail(s, span, "On takes an event or Add/Insert/Remove<Component>")),
            },
            port if self.ports.iter().any(|p| p == port) => {
                let refs = args.iter().map(|a| self.reference(a, s, "a value")).collect::<Result<Vec<_>, _>>()?;
                Ok(json!({ "kind": "port", "name": port, "args": refs }))
            }
            other => Err(self.fail(s, span, format!("{other}<...> is not a system param; it belongs inside a Query or On"))),
        }
    }

    fn datum(&mut self, t: &Ty, scope: &Scope) -> Result<Value, Fail> {
        match self.head(t, scope, 0)? {
            Head::Value(c) => Ok(json!({ "component": c, "mut": false, "optional": false })),
            Head::Kind { name, args, span, scope } => match name.as_str() {
                "Entity" => Ok(json!({ "entity": true })),
                "Mut" | "Option" => {
                    let mut d = self.datum(self.arg(&name, &args, 0, span, &scope)?, &scope)?;
                    if d.get("entity").is_some() {
                        return Err(self.fail(&scope, span, format!("{name}<Entity> is not query data")));
                    }
                    d[if name == "Mut" { "mut" } else { "optional" }] = json!(true);
                    Ok(d)
                }
                _ => Err(self.fail(&scope, span, format!("{name}<...> is not query data; data is a component, Mut<C>, Option<C> or Entity"))),
            },
            _ => Err(self.fail(scope, span_of(t), "query data is a component, Mut<C>, Option<C> or Entity")),
        }
    }

    fn filter(&mut self, t: &Ty, scope: &Scope) -> Result<Value, Fail> {
        match self.head(t, scope, 0)? {
            Head::Kind { name, args, span, scope } if matches!(name.as_str(), "With" | "Without" | "Changed" | "Added") => {
                let c = self.reference(self.arg(&name, &args, 0, span, &scope)?, &scope, "a component")?;
                let mut m = serde_json::Map::new();
                m.insert(name.to_lowercase(), c);
                Ok(Value::Object(m))
            }
            _ => Err(self.fail(scope, span_of(t), "a query filter is With<C>, Without<C>, Changed<C> or Added<C>, or a tuple of them")),
        }
    }
}

fn span_of(t: &Ty) -> Span {
    match t {
        Ty::Ref { span, .. } | Ty::Other { span, .. } | Ty::Tuple(_, span) | Ty::Object(_, span) | Ty::Strings(_, span) | Ty::Keyword(_, span) => *span,
        Ty::App => Span::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{Compiler, compile_module};

    fn tmp(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join("bevy-ts-reader").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        for (f, src) in files {
            std::fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
            std::fs::write(dir.join(f), src).unwrap();
        }
        dir.canonicalize().unwrap()
    }

    fn compile(path: &Path, root: &Path) -> String {
        let source = std::fs::read_to_string(path).unwrap();
        compile_module(path, &source, false, root).unwrap_or_else(|e| panic!("{e:?}")).code
    }

    /// The declarations in compiled code, by function name.
    fn declared(code: &str) -> BTreeMap<String, Value> {
        code.lines()
            .filter_map(|l| l.strip_prefix(";__bevyts.declare("))
            .map(|l| {
                let (name, rest) = l.split_once(", ").unwrap();
                (name.to_string(), serde_json::from_str(rest.strip_suffix(");").unwrap()).unwrap())
            })
            .collect()
    }

    fn bind_line(code: &str) -> &str {
        code.lines().find(|l| l.starts_with(";__bevyts.bind(")).unwrap_or("")
    }

    fn c(r: &Value) -> Value {
        json!({ "component": r, "mut": false, "optional": false })
    }

    fn m(r: &Value) -> Value {
        json!({ "component": r, "mut": true, "optional": false })
    }

    fn bench(file: &str) -> (PathBuf, PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/dx/examples").canonicalize().unwrap();
        (root.join(file), root)
    }

    #[test]
    fn reads_the_prototype_systems() {
        let (path, root) = bench("systems.ts");
        let code = compile(&path, &root);
        let d = declared(&code);
        let r = |n: &str| json!(["systems.ts", n]);
        let params = |f: &str| d[f]["params"].clone();
        assert_eq!(d["chase"]["id"], json!("systems.ts#chase"));
        assert_eq!(d["chase"]["kind"], json!("system"));
        let line = std::fs::read_to_string(&path).unwrap().lines().position(|l| l.starts_with("export function chase(")).unwrap() + 1;
        assert_eq!(d["chase"]["at"], json!({ "file": "systems.ts", "line": line, "column": 17 }));
        assert_eq!(
            params("chase"),
            json!([
                { "kind": "res", "resource": r("Time") },
                { "kind": "query", "data": [c(&r("Transform"))], "filter": [{ "with": r("Player") }] },
                { "kind": "query", "data": [c(&r("Enemy")), m(&r("Transform"))], "filter": [{ "with": r("Alive") }, { "without": r("Player") }] },
            ])
        );
        assert_eq!(
            params("regen"),
            json!([{ "kind": "query", "data": [{ "entity": true }, m(&r("Health")), { "component": r("Enemy"), "mut": false, "optional": true }], "filter": [{ "changed": r("Health") }] }])
        );
        assert_eq!(
            params("separate"),
            json!([
                { "kind": "query", "data": [{ "entity": true }, m(&r("Transform"))], "filter": [{ "with": r("Enemy") }] },
                { "kind": "query", "data": [{ "entity": true }, c(&r("Transform"))], "filter": [{ "with": r("Enemy") }] },
            ])
        );
        assert_eq!(params("drag"), json!([{ "kind": "query", "data": [m(&r("Velocity"))], "filter": [{ "with": r("Collider") }] }]));
        // Reflect tier is the type checker's to refuse; the syntax reads.
        assert_eq!(params("bad"), json!([{ "kind": "query", "data": [m(&r("Collider"))], "filter": [] }]));
        assert_eq!(params("spawnWave"), json!([{ "kind": "commands" }, { "kind": "local", "fields": [{ "name": "n", "type": { "kind": "u32" } }] }]));
        assert_eq!(params("pause"), json!([{ "kind": "nextState", "state": r("Game") }]));
        assert_eq!(
            params("applyDamage"),
            json!([
                { "kind": "messageReader", "message": r("Damage") },
                { "kind": "query", "data": [m(&r("Health"))], "filter": [] },
                { "kind": "resMut", "resource": r("Score") },
                { "kind": "commands" },
            ])
        );
        assert_eq!(
            params("onExplode"),
            json!([
                { "kind": "on", "event": r("Explode") },
                { "kind": "query", "data": [{ "entity": true }, c(&r("Transform"))], "filter": [{ "with": r("Enemy") }] },
                { "kind": "messageWriter", "message": r("Damage") },
                { "kind": "commands" },
            ])
        );
        assert_eq!(
            params("onHealthAdded"),
            json!([{ "kind": "on", "lifecycle": "add", "component": r("Health") }, { "kind": "query", "data": [m(&r("Health"))], "filter": [] }])
        );
        assert_eq!(params("enemyPlugin"), json!([{ "kind": "app" }]));
        assert_eq!(d.len(), 11, "{d:#?}");
        assert_eq!(
            bind_line(&code),
            r#";__bevyts.bind("systems.ts", { Alive: () => Alive, Collider: () => Collider, Damage: () => Damage, Enemy: () => Enemy, Explode: () => Explode, Game: () => Game, Health: () => Health, Player: () => Player, Score: () => Score, Time: () => Time, Transform: () => Transform, Velocity: () => Velocity });"#
        );
    }

    #[test]
    fn reads_the_prototype_fallback() {
        let (path, root) = bench("fallback.ts");
        let d = declared(&compile(&path, &root));
        let err = d["computed"]["error"].as_str().unwrap();
        assert!(err.starts_with("fallback.ts:10:29: `ReturnType<typeof makeQuery>` is a computed type (ReturnType)"), "{err}");
        assert!(d["computed"].get("params").is_none());
        assert_eq!(d["drift"]["kind"], json!("setup"));
        assert_eq!(d["drift"]["id"], json!("fallback.ts#drift"));
        assert_eq!(d.len(), 2, "{d:#?}");
    }

    #[test]
    fn declarations_follow_the_code_and_leave_mapped_lines_alone() {
        let (path, root) = bench("systems.ts");
        let source = std::fs::read_to_string(&path).unwrap();
        let c = compile_module(&path, &source, false, &root).unwrap();
        let map: Value = serde_json::from_str(&c.source_map).unwrap();
        let mapped_lines = map["mappings"].as_str().unwrap().split(';').count();
        let first = c.code.lines().position(|l| l.starts_with(";__bevyts.")).unwrap();
        assert!(first >= mapped_lines, "declarations start at line {first}, inside the {mapped_lines} mapped lines");
        assert!(c.code.contains("function chase(time, player, enemies) {"), "the body is untouched");
    }

    #[test]
    fn an_alias_from_another_file() {
        let dir = tmp(
            "alias",
            &[
                (
                    "src/enemies.ts",
                    "import { Transform, Component, type Query, type Mut, type With } from \"bevy-ts\";\nexport class Alive extends Component {}\nexport type Movers<T> = Query<[T, Mut<Transform>], With<Alive>>;\n",
                ),
                ("src/enemy.ts", "import { Component } from \"bevy-ts\";\nexport class Enemy extends Component {}\n"),
                (
                    "src/game.ts",
                    "import type { Movers } from \"./enemies.ts\";\nimport { Enemy } from \"./enemy.ts\";\nexport function move(q: Movers<Enemy>) {}\n",
                ),
            ],
        );
        let code = compile(&dir.join("src/game.ts"), &dir);
        assert_eq!(
            declared(&code)["move"]["params"],
            json!([{ "kind": "query", "data": [c(&json!(["src/game.ts", "Enemy"])), m(&json!(["src/enemies.ts", "Transform"]))], "filter": [{ "with": ["src/enemies.ts", "Alive"] }] }])
        );
        assert_eq!(bind_line(&code), r#";__bevyts.bind("src/game.ts", { Enemy: () => Enemy });"#);
        // The alias's module binds what its own text names, and is evaluated for it.
        assert!(code.contains(";import \"./enemies.ts\";"), "{code}");
        let enemies = compile(&dir.join("src/enemies.ts"), &dir);
        assert_eq!(bind_line(&enemies), r#";__bevyts.bind("src/enemies.ts", { Alive: () => Alive, Transform: () => Transform });"#);
        let mut compiler = Compiler::new(dir.join("src/game.ts"), false).unwrap();
        compiler.declarations = true;
        let set = compiler.compile_graph().unwrap();
        assert_eq!(set.modules.len(), 3);
    }

    #[test]
    fn a_type_only_import_fails_the_compile_at_its_line() {
        let dir = tmp(
            "type-import",
            &[
                ("src/enemy.ts", "export class Enemy {}\n"),
                ("src/game.ts", "import type { Query } from \"bevy-ts\";\n\nimport type { Enemy } from \"./enemy\";\nexport function chase(q: Query<[Enemy]>) {}\n"),
            ],
        );
        let path = dir.join("src/game.ts");
        let e = compile_module(&path, &std::fs::read_to_string(&path).unwrap(), false, &dir).err().unwrap();
        assert_eq!(e[0].message, "Enemy is imported as a type; system params need its value");
        let at = e[0].location.clone().unwrap();
        assert!(at.file.ends_with("src/game.ts"), "{at:?}");
        assert_eq!((at.line, at.column), (3, 15));
    }

    #[test]
    fn a_computed_param_type_names_itself() {
        let dir = tmp(
            "pick",
            &[(
                "src/game.ts",
                "import { type Query, Component } from \"bevy-ts\";\nclass A extends Component {}\ninterface Bundle { a: Query<[A]> }\nexport function f(\n  b: Pick<Bundle, \"a\">,\n) {}\n",
            )],
        );
        let d = declared(&compile(&dir.join("src/game.ts"), &dir));
        let err = d["f"]["error"].as_str().unwrap();
        assert!(err.starts_with("src/game.ts:5:6: `Pick<Bundle, \"a\">` is a computed type (Pick)"), "{err}");
    }

    #[test]
    fn bundles_locals_states_and_lazy_errors() {
        let src = r#"import { type Query, type Res, type State, type Local, type Mut, type Option, type On, type Remove, type Commands, Component, Resource, States, type f32 } from "bevy-ts";
export class Pos extends Component { x: f32 = 0 }
export class Clock extends Resource { declare t: f32 }
export class Mode extends States("A", "B") {}
interface Movers { q: Query<[Option<Mut<Pos>>]>; clock: Res<Clock> }
type Counter<T> = Local<{ n: T; at: vec3 }>;
type Env = { mode: State<Mode>; cmd: Commands };
function step(m: Movers, e: Env, c: Counter<u32>, gone: On<Remove<Pos>>) {}
export { step as tick };
function helper(x: number) {}
export function untyped(x) {}
export function wrong(p: Pos) {}
export function local(l: Local<{ n: number }>) {}
export const arrow = (q: Query<[Pos]>) => {};
"#;
        let dir = tmp("bundles", &[("src/game.ts", src)]);
        let d = declared(&compile(&dir.join("src/game.ts"), &dir));
        let pos = json!(["src/game.ts", "Pos"]);
        assert_eq!(d["step"]["id"], json!("src/game.ts#tick"));
        assert_eq!(
            d["step"]["params"],
            json!([
                { "kind": "bundle", "fields": [
                    { "name": "q", "param": { "kind": "query", "data": [{ "component": pos, "mut": true, "optional": true }], "filter": [] } },
                    { "name": "clock", "param": { "kind": "res", "resource": ["src/game.ts", "Clock"] } },
                ] },
                { "kind": "bundle", "fields": [
                    { "name": "mode", "param": { "kind": "state", "state": ["src/game.ts", "Mode"] } },
                    { "name": "cmd", "param": { "kind": "commands" } },
                ] },
                { "kind": "local", "fields": [{ "name": "n", "type": { "kind": "u32" } }, { "name": "at", "type": { "kind": "vec3" } }] },
                { "kind": "on", "lifecycle": "remove", "component": pos },
            ])
        );
        assert_eq!(d["arrow"]["params"], json!([{ "kind": "query", "data": [c(&pos)], "filter": [] }]));
        let err = |f: &str| d[f]["error"].as_str().unwrap().to_string();
        assert!(err("helper").starts_with("helper is not exported"), "{}", err("helper"));
        assert!(d["helper"].get("id").is_none());
        assert!(err("untyped").contains("param x has no type"), "{}", err("untyped"));
        assert!(err("wrong").starts_with("src/game.ts:12:26: \"Pos\" is a value, not a system param"), "{}", err("wrong"));
        assert!(err("local").contains("Local field n: number is not a field type"), "{}", err("local"));
    }

    #[test]
    fn a_plain_module_declares_nothing() {
        let dir = tmp("plain", &[("src/util.ts", "export function scale(n: number): number {\n  return n * 3;\n}\n")]);
        assert!(!compile(&dir.join("src/util.ts"), &dir).contains("__bevyts"));
    }

    #[test]
    fn port_param_kinds_come_from_the_registry() {

        let src = "import { Collider } from \"bevy-ts/rapier\";\nexport function cast(r: ReadRapierContext<Collider>) {}\n";
        let path = Path::new("/p/src/game.ts");
        let allocator = Allocator::default();
        let program = Parser::new(&allocator, src, SourceType::ts()).parse().program;
        let read = |ports: &[&str]| declared(&read_with(extract(&program, path, src, Path::new("/p")), Path::new("/p"), ports.iter().map(|s| s.to_string()).collect()).ok().unwrap().code);
        assert_eq!(read(&["ReadRapierContext"])["cast"]["params"], json!([{ "kind": "port", "name": "ReadRapierContext", "args": [["src/game.ts", "Collider"]] }]));
        assert!(read(&[])["cast"]["error"].as_str().unwrap().contains("ReadRapierContext is not a system param kind"));
    }

    /// The `__bevyts.data(this, ...)` declarations in compiled code, by class name.
    fn data(code: &str) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        let flat: String = code.lines().map(str::trim).collect::<Vec<_>>().join("");
        for part in flat.split("class ").skip(1) {
            let name: String = part.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if let Some(rest) = part.split_once("__bevyts.data(this, ").map(|(_, r)| r) {
                let mut de = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
                if let Some(Ok(v)) = de.next() {
                    out.insert(name, v);
                }
            }
        }
        out
    }

    #[test]
    fn reads_the_prototype_classes() {
        let (path, root) = bench("systems.ts");
        let d = data(&compile(&path, &root));
        assert_eq!(
            d["Enemy"],
            json!({ "kind": "component", "name": "systems::Enemy", "id": "systems.ts#Enemy", "at": d["Enemy"]["at"], "fields": [{ "name": "speed", "type": { "kind": "f32" }, "default": 2 }] })
        );
        assert_eq!(d["Health"]["fields"].as_array().unwrap().len(), 2);
        assert_eq!(d["Alive"]["fields"], json!([]));
        assert_eq!(d["Score"]["kind"], json!("resource"));
        assert_eq!(d["Game"], json!({ "kind": "state", "name": "systems::Game", "id": "systems.ts#Game", "at": d["Game"]["at"], "values": ["Menu", "Playing", "Paused"] }));
        assert_eq!(d["Damage"]["fields"], json!([{ "name": "target", "type": { "kind": "entity" } }, { "name": "amount", "type": { "kind": "f32" } }]));
        assert_eq!(d["Explode"]["kind"], json!("entityEvent"));
        assert_eq!(d["Explode"]["propagate"], json!(false));
        assert_eq!(d["Shield"]["name"], json!("game::Shield"), "static path pins the name");
        assert_eq!(d["EnemySet"]["kind"], json!("set"));

        let (path, root) = bench("prefab.tsx");
        let d = data(&compile(&path, &root));
        assert_eq!(d["Grunt"]["kind"], json!("sceneComponent"));
        assert_eq!(d["Grunt"]["fields"], json!([{ "name": "team", "type": { "kind": "u8" }, "default": 0 }]));
        assert_eq!(
            d["Grunt"]["scene"],
            json!({ "props": [
                { "name": "at", "type": { "kind": "vec3" }, "optional": true, "default": [0, 0, 0] },
                { "name": "speed", "type": { "kind": "f32" }, "optional": true, "default": 2 },
            ] })
        );
    }

    #[test]
    fn class_fields_props_and_their_errors() {
        let src = r#"import { Component, SceneComponent, EntityEvent, type f32, type u8, type Entity } from "bevy-ts";
type Team = "red" | "blue";
interface Props { label: string; size?: f32; team?: Team }
export class Badge extends Component { team: Team = "red"; declare owner: Entity; on: boolean = true }
export class Sign extends SceneComponent {
  height: f32 = 1;
  static scene({ label, size = 2, team = pick() }: Props) { return null; }
}
export class Bad extends Component { name: string = "x" }
export class Bare extends Component { hp = 3 }
export class Clash extends SceneComponent { size: f32 = 1; static scene({ size = 1 }: { size?: f32 }) { return null; } }
export class Boom extends EntityEvent { static propagate = true; declare entity: Entity; declare radius: f32 }
export default class Main extends Component { n: u8 = 1 }
class Local extends Component { x: f32 = vec3(1, 2).x }
"#;
        let dir = tmp("classes", &[("src/units/my-tank.ts", src)]);
        let code = compile(&dir.join("src/units/my-tank.ts"), &dir);
        let d = data(&code);
        assert_eq!(d["Badge"]["name"], json!("units::my_tank::Badge"));
        assert_eq!(
            d["Badge"]["fields"],
            json!([
                { "name": "team", "type": { "kind": "enum", "variants": ["red", "blue"] }, "default": "red" },
                { "name": "owner", "type": { "kind": "entity" } },
                { "name": "on", "type": { "kind": "bool" }, "default": true },
            ])
        );
        assert_eq!(
            d["Sign"]["scene"]["props"],
            json!([
                { "name": "label", "type": { "kind": "string" }, "optional": false },
                { "name": "size", "type": { "kind": "f32" }, "optional": true, "default": 2 },
                { "name": "team", "type": { "kind": "enum", "variants": ["red", "blue"] }, "optional": true, "hasDefault": true },
            ])
        );
        let err = |c: &str| d[c]["error"].as_str().unwrap_or_default().to_string();
        assert!(err("Bad").starts_with("src/units/my-tank.ts:9:"), "{}", err("Bad"));
        assert!(err("Bad").contains("Bad field name: strings are not supported"), "{}", err("Bad"));
        assert!(err("Bare").contains("field hp of Bare needs a type"), "{}", err("Bare"));
        assert!(err("Clash").contains("size is both a field and a scene prop"), "{}", err("Clash"));
        assert_eq!(d["Boom"]["fields"], json!([{ "name": "radius", "type": { "kind": "f32" } }]), "an entity event's target is not a field");
        assert_eq!(d["Boom"]["propagate"], json!(true));
        assert_eq!(d["Main"]["name"], json!("units::my_tank::Main"));
        assert_eq!(d["Main"]["id"], json!("src/units/my-tank.ts#default"));
        assert_eq!(d["Local"]["name"], json!("units::my_tank::Local"));
        assert!(d["Local"].get("id").is_none());
        assert_eq!(d["Local"]["fields"], json!([{ "name": "x", "type": { "kind": "f32" } }]), "a computed default is the runtime's");
    }

    #[test]
    fn module_paths_are_rust_style() {
        assert_eq!(module_path("src/game.ts"), "game");
        assert_eq!(module_path("src/enemies.tsx"), "enemies");
        assert_eq!(module_path("src/units/tank.ts"), "units::tank");
        assert_eq!(module_path("levels/my-level.ts"), "levels::my_level");
    }

    #[test]
    fn the_project_root_holds_a_manifest() {
        let dir = tmp("root", &[("src/game.ts", "")]);
        assert_eq!(project_root(&dir.join("src/game.ts")), dir);
        assert_eq!(relative(&dir, &dir.join("src/game.ts")), "src/game.ts");
        assert_eq!(relative(&dir.join("src/a"), &dir.join("src/b/c.ts")), "../b/c.ts");
    }
}
