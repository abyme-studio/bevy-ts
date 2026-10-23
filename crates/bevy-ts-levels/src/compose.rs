//! A level file to a `tsx` description (design decision 3): each entity an `<entity>` keyed by
//! its `#Name`, each patch a component element whose fields are controlled with provenance
//! `override`, `Children [ … ]` child nodes, `@Comp { … }` expanded by its TS scene component in
//! one host call, and `:"x.bsn"` expanded from that file's tree. Pure: the engine supplies the
//! files, the types and the expansion, so every rule here is tested without one.

use crate::syntax::{self, EntityList, Entry, Fields, LineIndex, ParsedFile, Span, ValueKind};
use bevy::reflect::Reflect;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::layout::FieldType;
use bevy_ts_tsx::desc::{ComponentDesc, ComponentKind, NodeDesc, binding_id};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

/// What a field or prop holds.
#[derive(Clone, Debug, PartialEq)]
pub enum ValueType {
    /// A native-layout field type.
    Field(FieldType),
    /// A string: a scene prop only.
    String,
    /// A `Handle<T>`: a string asset path (decision 12).
    Handle,
    /// A reflected field of a type levels cannot write, by type path.
    Unsupported(String),
}

#[derive(Clone, Debug)]
pub struct FieldSchema {
    pub name: String,
    pub ty: ValueType,
    /// The value an unset field takes, in the description's JSON form.
    pub default: Value,
}

/// A component a level can patch, as `tsx` names it.
#[derive(Clone, Debug)]
pub struct ComponentSchema {
    /// The element type in the description (`Transform`, `enemies::Health`).
    pub name: String,
    pub kind: ComponentKind,
    pub fields: Vec<FieldSchema>,
    /// `Name`: a patch sets the node's name.
    pub is_name: bool,
}

/// A TS scene component (`class Grunt extends SceneComponent`): its fields and its scene's props.
#[derive(Clone, Debug)]
pub struct SceneSchema {
    pub component: ComponentSchema,
    pub props: Vec<FieldSchema>,
    /// Props with no default that are not optional: an instance must set them.
    pub required: Vec<String>,
}

/// The types a level is checked against: Bevy's registry and the generation's TS components.
pub trait Schema {
    /// A component by full path or by a short name only one type has; the error names every
    /// candidate of an ambiguous short name.
    fn component(&self, path: &str) -> Result<ComponentSchema, String>;
    fn scene(&self, path: &str) -> Result<SceneSchema, String>;
}

/// One scene component instance to expand: the component, its props and fields together as a
/// JSX call would pass them.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandRequest {
    pub component: String,
    pub props: Map<String, Value>,
    pub at: SourceLocation,
}

/// One instance's expansion: its description, or the errors its scene threw.
pub type Expansion = Result<NodeDesc, Vec<ScriptError>>;

/// Expands scene component instances, all of a reconcile's in one host call (decision 5).
pub trait Expander {
    /// One expansion per request, in order; `Err` when the call itself failed.
    fn expand(&mut self, requests: &[ExpandRequest]) -> Result<Vec<Expansion>, Vec<ScriptError>>;
}

/// Level files by asset path.
pub trait Files {
    fn parsed(&mut self, path: &str) -> Result<Arc<ParsedFile>, String>;
}

/// A place in a level that owns an entity: the file, the entity's name path in it, its line.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub file: String,
    pub path: Vec<String>,
    pub at: SourceLocation,
    /// An included file's one top entity, merged into the entity that includes it (which then
    /// has its key). A file with several tops puts them under that entity instead.
    pub merged: bool,
}

/// A scene component instance's props: every prop its scene declares, with the level's value or
/// the default.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePlacement {
    pub component: String,
    pub props: Vec<PlacedProp>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedProp {
    pub name: String,
    /// The prop's type, as the manifest writes it (`{ "kind": "f32" }`).
    pub ty: Value,
    pub value: Value,
    /// Where the level sets it; `None` when it takes its default.
    pub location: Option<SourceLocation>,
}

/// Why a level entry is a placeholder (`level-placeholders`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Reflect)]
#[serde(rename_all = "lowercase")]
pub enum MissingReason {
    /// No live code registers the type, or the class is no longer a scene component.
    Unregistered,
    /// Its code threw, or the live generation is the empty one a failed TS build left.
    Failed,
    /// The code is live, but saved fields or props do not fit it.
    Mismatch,
}

/// A place in a file, as a reflected value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Reflect)]
pub struct MissingAt {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl From<&SourceLocation> for MissingAt {
    fn from(l: &SourceLocation) -> Self {
        MissingAt { file: l.file.clone(), line: l.line, column: l.column }
    }
}

impl MissingAt {
    pub fn location(&self) -> SourceLocation {
        SourceLocation { file: self.file.clone(), line: self.line, column: self.column }
    }
}

/// An entry under a missing scene instance that has no base of its own: kept, not spawned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Reflect)]
pub struct KeptOverride {
    /// Its name path below the instance.
    pub path: Vec<String>,
    /// Its patches' saved text, keyed `Component.field`.
    pub fields: BTreeMap<String, String>,
}

/// One placeholder entry: a component a level writes whose code is missing, failed or does not
/// fit. Serialized as the protocol's `MissingComponent`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Reflect)]
pub struct MissingComponent {
    /// The name the file writes.
    pub component: String,
    pub reason: MissingReason,
    /// One line of why.
    pub message: String,
    /// The saved fields and props as written (a prop keeps its `@`); a mismatch lists only the
    /// misfits.
    pub fields: BTreeMap<String, String>,
    /// Where the level writes it.
    pub location: MissingAt,
    /// Where its code was expected, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<MissingAt>,
    /// A mismatch: why each listed field does not fit.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub misfit: BTreeMap<String, String>,
    /// A missing scene component: the entries below it kept and not spawned.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<KeptOverride>,
    /// The entity's own scene component, which did not expand.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub scene: bool,
}

impl MissingComponent {
    fn new(component: &str, reason: MissingReason, message: &str, fields: BTreeMap<String, String>, at: &SourceLocation) -> Self {
        MissingComponent {
            component: component.to_string(),
            reason,
            message: message.to_string(),
            fields,
            location: at.into(),
            expected: None,
            misfit: BTreeMap::new(),
            overrides: Vec::new(),
            scene: false,
        }
    }

    /// The entry as a level diagnostic.
    pub fn warning(&self) -> ScriptError {
        ScriptError::at(format!("warning: {}", self.message), Some(self.location.location()))
    }
}

/// A composed level.
#[derive(Clone, Debug, Default)]
pub struct Composed {
    pub nodes: Vec<NodeDesc>,
    /// The level frames of every node the level made or overrode, outermost first, by key path.
    pub owners: BTreeMap<Vec<String>, Vec<Frame>>,
    /// The scene component instances' props as placed, by key path.
    pub scenes: BTreeMap<Vec<String>, ScenePlacement>,
    /// Placeholder entries, by key path (`level-placeholders`).
    pub missing: BTreeMap<Vec<String>, Vec<MissingComponent>>,
    /// The level and every file it includes, for reload fan-out.
    pub files: BTreeSet<String>,
    pub warnings: Vec<ScriptError>,
}

/// Resolves a path written in `from` (an asset path): `./` and `../` against `from`'s directory,
/// anything else against the asset root, as `AssetPath::resolve` does.
pub fn resolve_path(from: &str, path: &str) -> String {
    if !(path.starts_with("./") || path.starts_with("../")) {
        return path.to_string();
    }
    let (body, label) = match path.split_once('#') {
        Some((b, l)) => (b, Some(l)),
        None => (path, None),
    };
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in body.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = parts.join("/");
    if let Some(l) = label {
        out.push('#');
        out.push_str(l);
    }
    out
}

struct FileCtx {
    path: String,
    parsed: Arc<ParsedFile>,
    index: LineIndex,
}

impl FileCtx {
    fn at(&self, span: Span) -> SourceLocation {
        let (line, column) = self.index.line_col(&self.parsed.source, span.start);
        SourceLocation { file: self.path.clone(), line, column }
    }

    fn err(&self, message: impl Into<String>, span: Span) -> ScriptError {
        ScriptError::at(message, Some(self.at(span)))
    }

    fn text(&self, span: Span) -> String {
        self.parsed.text(span).to_string()
    }

    /// Saved fields as written, keyed by name (a prop keeps its `@`) or tuple index.
    fn written(&self, fields: &Fields) -> BTreeMap<String, String> {
        match fields {
            Fields::Unit => BTreeMap::new(),
            Fields::Named { fields, .. } => fields.iter().map(|f| (if f.prop { format!("@{}", f.name) } else { f.name.clone() }, self.text(f.value.span))).collect(),
            Fields::Tuple { values, .. } => values.iter().enumerate().map(|(i, v)| (i.to_string(), self.text(v.span))).collect(),
        }
    }
}

struct LField {
    name: String,
    value: syntax::Value,
    span: Span,
}

struct LPatch {
    schema: ComponentSchema,
    /// The component as the file writes it.
    written: String,
    fields: Vec<LField>,
    span: Span,
    ctx: Arc<FileCtx>,
}

enum Base {
    None,
    Include(Vec<LNode>),
    Scene(usize),
    /// A scene component that is a placeholder: entries below with no base are kept.
    Missing,
}

struct LNode {
    key: String,
    name: Option<String>,
    ctx: Arc<FileCtx>,
    span: Span,
    name_path: Vec<String>,
    base: Base,
    patches: Vec<LPatch>,
    children: Vec<LNode>,
    /// Placeholder entries found while reading.
    missing: Vec<MissingComponent>,
    /// Every patch's saved text keyed `Component.field`, in case the node is kept as an override
    /// (tolerant only).
    kept: BTreeMap<String, String>,
}

/// Where an instance is placed, for a placeholder when its scene throws.
#[derive(Clone)]
struct Site {
    written: String,
    fields: BTreeMap<String, String>,
    at: SourceLocation,
}

struct Env<'a> {
    files: &'a mut dyn Files,
    schema: &'a dyn Schema,
    tolerant: bool,
    requests: Vec<ExpandRequest>,
    scenes: Vec<ScenePlacement>,
    sites: Vec<Site>,
    stack: Vec<String>,
    seen: BTreeSet<String>,
    warnings: Vec<ScriptError>,
}

/// Missing code under `bevy-ts dev` (`tolerant`) is a placeholder entry; otherwise the error.
fn placeholder(tolerant: bool, out: &mut Vec<MissingComponent>, error: ScriptError, entry: MissingComponent) -> Result<(), ScriptError> {
    if !tolerant {
        return Err(error);
    }
    out.push(entry);
    Ok(())
}

/// A mismatch entry from its misfits: key, saved text, why.
fn mismatch(component: &str, at: &SourceLocation, misfits: &[(String, String, ScriptError)]) -> MissingComponent {
    let message = match misfits {
        [(_, _, e)] => e.message.clone(),
        many => format!("{} saved fields do not fit {component}: {}", many.len(), many.iter().map(|(k, ..)| k.as_str()).collect::<Vec<_>>().join(", ")),
    };
    let mut m = MissingComponent::new(component, MissingReason::Mismatch, &message, misfits.iter().map(|(k, t, _)| (k.clone(), t.clone())).collect(), at);
    m.misfit = misfits.iter().map(|(k, _, e)| (k.clone(), e.message.clone())).collect();
    m
}

/// Composes the level at `path`. `tolerant` (`bevy-ts dev`) makes missing code placeholders.
pub fn compose(path: &str, files: &mut dyn Files, schema: &dyn Schema, expander: &mut dyn Expander, tolerant: bool) -> Result<Composed, Vec<ScriptError>> {
    let prepared = prepare(path, files, schema, tolerant)?;
    let expansions = if prepared.requests.is_empty() { Vec::new() } else { expander.expand(&prepared.requests)? };
    prepared.finish(expansions)
}

/// A level read and typed, waiting for its scene component instances to be expanded: the first
/// half of [`compose`], for callers whose expansion needs what the reading borrowed (the world).
pub struct Prepared {
    top: Vec<LNode>,
    tolerant: bool,
    /// One per scene component instance, in order; [`Prepared::finish`] takes one description each.
    pub requests: Vec<ExpandRequest>,
    /// Each request's props as placed.
    scenes: Vec<ScenePlacement>,
    sites: Vec<Site>,
    files: BTreeSet<String>,
    warnings: Vec<ScriptError>,
}

/// Reads and types the level at `path` and every file it includes. Only a broken file fails it
/// when `tolerant`: a parse error, a duplicate name, a cycle or a missing include.
pub fn prepare(path: &str, files: &mut dyn Files, schema: &dyn Schema, tolerant: bool) -> Result<Prepared, Vec<ScriptError>> {
    let mut env = Env { files, schema, tolerant, requests: Vec::new(), scenes: Vec::new(), sites: Vec::new(), stack: Vec::new(), seen: BTreeSet::new(), warnings: Vec::new() };
    let top = load(&mut env, path, None).map_err(|e| vec![e])?;
    Ok(Prepared { top, tolerant, requests: env.requests, scenes: env.scenes, sites: env.sites, files: env.seen, warnings: env.warnings })
}

/// An instance's expansion with what it was placed with, taken once by [`finish`].
type Slot = Option<(Expansion, ScenePlacement, Site)>;

struct Fin<'a> {
    expansions: &'a mut [Slot],
    tolerant: bool,
}

impl Prepared {
    /// Builds the description from the instances' expansions, one per request. A failed one is
    /// the errors it was, or a `failed` placeholder when tolerant.
    pub fn finish(self, expansions: Vec<Expansion>) -> Result<Composed, Vec<ScriptError>> {
        if expansions.len() != self.requests.len() {
            return Err(vec![ScriptError::new(format!("expansion returned {} descriptions for {} instances", expansions.len(), self.requests.len()))]);
        }
        let mut expansions: Vec<Slot> = expansions.into_iter().zip(self.scenes).zip(self.sites).map(|((x, p), s)| Some((x, p, s))).collect();
        let mut fin = Fin { expansions: &mut expansions, tolerant: self.tolerant };
        let mut composed = Composed { files: self.files, warnings: self.warnings, ..Default::default() };
        for n in self.top {
            let node = finish(n, None, &mut fin, &[], &[], &mut composed)?;
            composed.nodes.push(node);
        }
        Ok(composed)
    }
}

impl Composed {
    /// Each placeholder entry as a warning with its place, once its entries are final.
    pub fn warn_missing(&mut self) {
        let warnings: Vec<ScriptError> = self.missing.values().flatten().map(MissingComponent::warning).collect();
        self.warnings.extend(warnings);
    }
}

fn load(env: &mut Env, path: &str, at: Option<(&FileCtx, Span)>) -> Result<Vec<LNode>, ScriptError> {
    let locate = |m: String| match at {
        Some((ctx, span)) => ctx.err(m, span),
        None => ScriptError::new(m),
    };
    if env.stack.iter().any(|p| p == path) {
        let mut cycle = env.stack.clone();
        cycle.push(path.to_string());
        return Err(locate(format!("level inclusion cycle: {}", cycle.join(" -> "))));
    }
    let parsed = env.files.parsed(path).map_err(|e| locate(format!("{path}: {e}")))?;
    let ctx = Arc::new(FileCtx { path: path.to_string(), index: LineIndex::new(&parsed.source), parsed });
    env.seen.insert(path.to_string());
    env.stack.push(path.to_string());
    let entities = ctx.parsed.entities.clone();
    let out = build_list(env, &ctx, &entities, &[]);
    env.stack.pop();
    out
}

fn build_list(env: &mut Env, ctx: &Arc<FileCtx>, list: &EntityList, prefix: &[String]) -> Result<Vec<LNode>, ScriptError> {
    let mut names = HashSet::new();
    let mut unnamed = 0;
    let mut out = Vec::new();
    for e in &list.items {
        if e.entries.is_empty() {
            continue;
        }
        let (key, name) = match e.name() {
            Some(n) => {
                if !names.insert(n.name.clone()) {
                    return Err(ctx.err(format!("two siblings are named {:?}; names are unique among siblings", n.name), n.span));
                }
                (n.name.clone(), Some(n.name.clone()))
            }
            None => {
                let key = format!("#{unnamed}");
                unnamed += 1;
                env.warnings.push(ctx.err(format!("unnamed entity, identified by position ({key}); name it, or the writer names it on its first edit"), e.span));
                (key, None)
            }
        };
        let mut name_path = prefix.to_vec();
        name_path.push(key.clone());
        let mut node = LNode {
            key,
            name,
            ctx: ctx.clone(),
            span: e.span,
            name_path: name_path.clone(),
            base: Base::None,
            patches: Vec::new(),
            children: Vec::new(),
            missing: Vec::new(),
            kept: BTreeMap::new(),
        };
        for entry in &e.entries {
            match entry {
                Entry::Name(_) => {}
                Entry::Include(i) => {
                    let target = resolve_path(&ctx.path, &i.path);
                    node.base = Base::Include(load(env, &target, Some((ctx, i.span)))?);
                }
                Entry::Prefab(p) => {
                    let written = p.path.joined();
                    let at = ctx.at(p.span);
                    let whole = |reason, message: &str| {
                        let mut m = MissingComponent::new(&written, reason, message, ctx.written(&p.fields), &at);
                        m.scene = true;
                        m
                    };
                    let scene = match env.schema.scene(&written) {
                        Ok(s) => s,
                        Err(m) => {
                            placeholder(env.tolerant, &mut node.missing, ctx.err(&m, p.path.span), whole(MissingReason::Unregistered, &m))?;
                            node.base = Base::Missing;
                            continue;
                        }
                    };
                    let mut props = Map::new();
                    let mut set_at = BTreeMap::new();
                    let mut misfits = Vec::new();
                    for f in p.fields.named() {
                        let (list, what) = if f.prop { (&scene.props, "scene prop") } else { (&scene.component.fields, "field") };
                        let fit = match list.iter().find(|s| s.name == f.name) {
                            Some(s) => convert(ctx, &format!("{}.{}", scene.component.name, f.name), &f.value, &s.ty, &s.default),
                            None => {
                                let known: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
                                Err(ctx.err(format!("{} has no {what} {} ({what}s: {})", scene.component.name, f.name, known.join(", ")), f.name_span))
                            }
                        };
                        match fit {
                            Ok(v) => {
                                props.insert(f.name.clone(), v);
                                if f.prop {
                                    set_at.insert(f.name.clone(), ctx.at(f.name_span));
                                }
                            }
                            Err(e) => misfits.push((if f.prop { format!("@{}", f.name) } else { f.name.clone() }, ctx.text(f.value.span), e)),
                        }
                    }
                    let required = scene.required.iter().find(|r| !props.contains_key(*r));
                    // A required prop unset makes the whole instance a placeholder, which lists
                    // every saved field; strict, the first misfit is the error, as it was.
                    if let Some((_, _, e)) = misfits.first()
                        && (!env.tolerant || required.is_none())
                    {
                        placeholder(env.tolerant, &mut node.missing, e.clone(), mismatch(&written, &at, &misfits))?;
                    }
                    if let Some(r) = required {
                        let m = format!("{} needs @{r}", scene.component.name);
                        placeholder(env.tolerant, &mut node.missing, ctx.err(&m, p.span), whole(MissingReason::Mismatch, &m))?;
                        node.base = Base::Missing;
                        continue;
                    }
                    let placed = scene
                        .props
                        .iter()
                        .map(|s| PlacedProp {
                            name: s.name.clone(),
                            ty: match &s.ty {
                                ValueType::Field(t) => serde_json::to_value(t).unwrap_or(Value::Null),
                                _ => json!({ "kind": "string" }),
                            },
                            value: props.get(&s.name).cloned().unwrap_or_else(|| s.default.clone()),
                            location: set_at.get(&s.name).cloned(),
                        })
                        .collect();
                    env.scenes.push(ScenePlacement { component: scene.component.name.clone(), props: placed });
                    env.sites.push(Site { written: written.clone(), fields: ctx.written(&p.fields), at: at.clone() });
                    env.requests.push(ExpandRequest { component: scene.component.name.clone(), props, at });
                    node.base = Base::Scene(env.requests.len() - 1);
                }
                Entry::Patch(p) => {
                    let written = p.path.joined();
                    if env.tolerant {
                        for (k, v) in ctx.written(&p.fields) {
                            node.kept.insert(format!("{written}.{k}"), v);
                        }
                        if matches!(p.fields, Fields::Unit) {
                            node.kept.entry(written.clone()).or_default();
                        }
                    }
                    let schema = match env.schema.component(&written) {
                        Ok(s) => s,
                        Err(m) => {
                            let entry = MissingComponent::new(&written, MissingReason::Unregistered, &m, ctx.written(&p.fields), &ctx.at(p.span));
                            placeholder(env.tolerant, &mut node.missing, ctx.err(&m, p.path.span), entry)?;
                            continue;
                        }
                    };
                    let fields = match &p.fields {
                        Fields::Unit => Vec::new(),
                        Fields::Named { fields, .. } => fields.iter().map(|f| LField { name: f.name.clone(), value: f.value.clone(), span: f.name_span }).collect(),
                        Fields::Tuple { values, .. } if schema.is_name => {
                            values.iter().map(|v| LField { name: "0".into(), value: v.clone(), span: v.span }).collect()
                        }
                        Fields::Tuple { values, .. } => {
                            if values.len() > schema.fields.len() {
                                let e = ctx.err(format!("{} takes {} values, got {}", schema.name, schema.fields.len(), values.len()), p.span);
                                let extra: Vec<(String, String, ScriptError)> = values.iter().enumerate().skip(schema.fields.len()).map(|(i, v)| (i.to_string(), ctx.text(v.span), e.clone())).collect();
                                placeholder(env.tolerant, &mut node.missing, e, mismatch(&written, &ctx.at(p.span), &extra))?;
                            }
                            values.iter().zip(&schema.fields).map(|(v, s)| LField { name: s.name.clone(), value: v.clone(), span: v.span }).collect()
                        }
                    };
                    node.patches.push(LPatch { schema, written, fields, span: p.span, ctx: ctx.clone() });
                }
                Entry::Related(r) => {
                    if r.path.last() != "Children" {
                        return Err(ctx.err(format!("{} lists are not supported in levels; use Children [ … ]", r.path.joined()), r.path.span));
                    }
                    node.children.extend(build_list(env, ctx, &r.list, &name_path)?);
                }
            }
        }
        // Children of a node written in several lists keep one name space.
        let mut seen = HashSet::new();
        for c in &node.children {
            if c.name.is_some() && !seen.insert(c.key.clone()) {
                return Err(c.ctx.err(format!("two siblings are named {:?}; names are unique among siblings", c.key), c.span));
            }
        }
        out.push(node);
    }
    Ok(out)
}

/// The finished description of a level node at `parent` (a key path), with its frames.
fn empty(key: &str, at: &SourceLocation) -> NodeDesc {
    NodeDesc {
        key: key.to_string(),
        ty: "entity".into(),
        name: None,
        ref_id: None,
        slot: None,
        on: BTreeMap::new(),
        source: vec![at.clone()],
        components: Vec::new(),
        children: Vec::new(),
    }
}

/// A node under a missing scene instance, kept as text with every entry below it.
fn keep(l: &LNode, prefix: &[String], out: &mut Vec<KeptOverride>) {
    let mut path = prefix.to_vec();
    path.push(l.key.clone());
    out.push(KeptOverride { path: path.clone(), fields: l.kept.clone() });
    for c in &l.children {
        keep(c, &path, out);
    }
}

fn finish(l: LNode, key: Option<String>, fin: &mut Fin, parent: &[String], frames: &[Frame], out: &mut Composed) -> Result<NodeDesc, Vec<ScriptError>> {
    let key = key.unwrap_or_else(|| l.key.clone());
    let mut path = parent.to_vec();
    path.push(key.clone());
    let mut chain = frames.to_vec();
    set_frame(&mut chain, Frame { file: l.ctx.path.clone(), path: l.name_path.clone(), at: l.ctx.at(l.span), merged: false });
    let at = l.ctx.at(l.span);
    let mut missing = l.missing;
    let mut placeholder_scene = matches!(l.base, Base::Missing);
    let mut node = match l.base {
        Base::None | Base::Missing => empty(&key, &at),
        Base::Scene(i) => {
            let (x, placed, site) = fin.expansions[i].take().expect("each expansion is used once");
            match x {
                Ok(mut n) => {
                    out.scenes.insert(path.clone(), placed);
                    n.key = key.clone();
                    // The level is the outermost call site; tsx's expansion already names the `@` entry.
                    if n.source.last().is_none_or(|s| s.file != at.file) {
                        n.source.push(at.clone());
                    }
                    n
                }
                Err(errors) if !fin.tolerant => return Err(errors),
                Err(errors) => {
                    let first = errors.first();
                    let message = first.map_or_else(|| format!("{} did not expand", site.written), |e| e.message.clone());
                    let mut m = MissingComponent::new(&site.written, MissingReason::Failed, &message, site.fields, &site.at);
                    m.scene = true;
                    m.expected = first.and_then(|e| e.location.as_ref()).filter(|l| **l != site.at).map(MissingAt::from);
                    missing.push(m);
                    placeholder_scene = true;
                    empty(&key, &at)
                }
            }
        }
        Base::Include(mut nodes) if nodes.len() == 1 => {
            let inner = nodes.pop().expect("one");
            let file = inner.ctx.path.clone();
            let mut n = finish(inner, Some(key.clone()), fin, parent, &chain, out)?;
            if let Some(f) = out.owners.get_mut(&path).and_then(|c| c.iter_mut().find(|f| f.file == file)) {
                f.merged = true;
            }
            n.source.push(at.clone());
            n
        }
        Base::Include(nodes) => {
            let mut children = Vec::new();
            for inner in nodes {
                let below = descend(&chain, &inner.key);
                children.push(finish(inner, None, fin, &path, &below, out)?);
            }
            NodeDesc { children, ..empty(&key, &at) }
        }
    };
    if l.name.is_some() {
        node.name = l.name.clone();
    }
    // An included root already holds its inner frames, which its children descend from too.
    match out.owners.get(&path) {
        Some(inner) => chain = inner.clone(),
        None => {
            out.owners.insert(path.clone(), chain.clone());
        }
    }
    for p in &l.patches {
        missing.extend(apply_patch(&mut node, p, fin.tolerant).map_err(|e| vec![e])?);
    }
    for child in l.children {
        // Under a missing scene, an entry with no base of its own may be a merge target: kept.
        if placeholder_scene && matches!(child.base, Base::None) {
            let mut kept = Vec::new();
            keep(&child, &[], &mut kept);
            if let Some(m) = missing.iter_mut().rev().find(|m| m.scene) {
                m.overrides.extend(kept);
            }
            continue;
        }
        merge_child(&mut node, child, fin, &path, &chain, out)?;
    }
    if !missing.is_empty() {
        out.missing.entry(path).or_default().extend(missing);
    }
    Ok(node)
}

/// A child's frames: each of its parent's, one name deeper.
fn descend(chain: &[Frame], key: &str) -> Vec<Frame> {
    chain
        .iter()
        .map(|f| {
            let mut f = f.clone();
            f.path.push(key.to_string());
            f
        })
        .collect()
}

/// One frame per file: a node written in a file it already has a frame in replaces it.
fn set_frame(chain: &mut Vec<Frame>, frame: Frame) {
    match chain.iter_mut().find(|f| f.file == frame.file) {
        Some(f) => *f = frame,
        None => chain.push(frame),
    }
}

/// E3: a named child that matches a child of the base (by key, or a quoted `tsx` key segment)
/// patches it; anything else is a new child.
fn merge_child(node: &mut NodeDesc, child: LNode, fin: &mut Fin, path: &[String], chain: &[Frame], out: &mut Composed) -> Result<(), Vec<ScriptError>> {
    let existing = child.name.as_ref().and_then(|n| node.children.iter().position(|c| &c.key == n));
    let Some(i) = existing else {
        let below = descend(chain, &child.key);
        node.children.push(finish(child, None, fin, path, &below, out)?);
        return Ok(());
    };
    if !matches!(child.base, Base::None) {
        return Err(vec![child.ctx.err(format!("{} overrides an included child, so it cannot include anything itself", child.key), child.span)]);
    }
    let mut cpath = path.to_vec();
    cpath.push(child.key.clone());
    let mut cchain = descend(chain, &child.key);
    set_frame(&mut cchain, Frame { file: child.ctx.path.clone(), path: child.name_path.clone(), at: child.ctx.at(child.span), merged: false });
    out.owners.insert(cpath.clone(), cchain.clone());
    let target = &mut node.children[i];
    let mut missing = child.missing;
    for p in &child.patches {
        missing.extend(apply_patch(target, p, fin.tolerant).map_err(|e| vec![e])?);
    }
    if !missing.is_empty() {
        out.missing.entry(cpath.clone()).or_default().extend(missing);
    }
    for grandchild in child.children {
        merge_child(target, grandchild, fin, &cpath, &cchain, out)?;
    }
    Ok(())
}

/// `initialHp` for `hp`.
fn initial_prop(field: &str) -> String {
    let mut c = field.chars();
    match c.next() {
        Some(f) => format!("initial{}{}", f.to_uppercase(), c.as_str()),
        None => "initial".into(),
    }
}

/// Layers a patch over a node. A field that does not fit is the error it was, or, when
/// tolerant, left out and listed in the mismatch entry this returns.
fn apply_patch(node: &mut NodeDesc, p: &LPatch, tolerant: bool) -> Result<Option<MissingComponent>, ScriptError> {
    let ctx = &p.ctx;
    if p.schema.is_name {
        let Some(LField { value: syntax::Value { kind: ValueKind::Str(s), .. }, .. }) = p.fields.first() else {
            return Err(ctx.err("Name takes a string: Name(\"…\")", p.span));
        };
        node.name = Some(s.clone());
        return Ok(None);
    }
    let mut misfits = Vec::new();
    let i = match node.components.iter().position(|c| c.ty == p.schema.name) {
        Some(i) => i,
        None => {
            node.components.push(ComponentDesc {
                ty: p.schema.name.clone(),
                kind: p.schema.kind,
                props: Map::new(),
                source: Some(ctx.at(p.span)),
                overrides: BTreeMap::new(),
            });
            node.components.len() - 1
        }
    };
    let c = &mut node.components[i];
    for f in &p.fields {
        let misfit = |e: ScriptError| if tolerant { Ok((f.name.clone(), ctx.text(f.value.span), e)) } else { Err(e) };
        let Some(s) = p.schema.fields.iter().find(|s| s.name == f.name) else {
            let known: Vec<&str> = p.schema.fields.iter().map(|s| s.name.as_str()).collect();
            misfits.push(misfit(ctx.err(format!("{} has no field {} (fields: {})", p.schema.name, f.name, known.join(", ")), f.span))?);
            continue;
        };
        let current = c.props.get(&f.name);
        if current.is_some_and(|v| binding_id(v).is_some()) {
            let site = c.source.as_ref().map_or_else(|| "its element".to_string(), |s| s.to_string());
            return Err(ctx.err(
                format!(
                    "{}.{} is bound in TSX at {site}; a bound field has one owner, its binding, so a level cannot override it (make the binding read a prop and set the prop)",
                    p.schema.name, f.name
                ),
                f.span,
            ));
        }
        let initial = initial_prop(&f.name);
        let base = current.or_else(|| c.props.get(&initial)).cloned().unwrap_or_else(|| s.default.clone());
        let v = match convert(ctx, &format!("{}.{}", p.schema.name, f.name), &f.value, &s.ty, &base) {
            Ok(v) => v,
            Err(e) => {
                misfits.push(misfit(e)?);
                continue;
            }
        };
        c.props.remove(&initial);
        c.props.insert(f.name.clone(), v);
        c.overrides.insert(f.name.clone(), ctx.at(f.span));
    }
    Ok((!misfits.is_empty()).then(|| mismatch(&p.written, &ctx.at(p.span), &misfits)))
}

const LANES: [&str; 4] = ["x", "y", "z", "w"];

fn lanes(base: &Value, n: usize) -> Vec<f64> {
    match base {
        Value::Array(a) => (0..n).map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(0.0)).collect(),
        Value::Object(o) => LANES[..n].iter().map(|k| o.get(*k).and_then(Value::as_f64).unwrap_or(0.0)).collect(),
        _ => vec![0.0; n],
    }
}

/// `Vec3::ZERO`, `Quat::IDENTITY` and the other glam constants a level may name.
pub fn constant(name: &str, n: usize) -> Option<Vec<f64>> {
    let axis = |i: usize, s: f64| {
        (i < n).then(|| {
            let mut v = vec![0.0; n];
            v[i] = s;
            v
        })
    };
    match name {
        "ZERO" => Some(vec![0.0; n]),
        "ONE" => Some(vec![1.0; n]),
        "NEG_ONE" => Some(vec![-1.0; n]),
        "IDENTITY" if n == 4 => axis(3, 1.0),
        "X" => axis(0, 1.0),
        "Y" => axis(1, 1.0),
        "Z" => axis(2, 1.0),
        "W" => axis(3, 1.0),
        "NEG_X" => axis(0, -1.0),
        "NEG_Y" => axis(1, -1.0),
        "NEG_Z" => axis(2, -1.0),
        _ => None,
    }
}

fn number(ctx: &FileCtx, v: &syntax::Value) -> Result<f64, ScriptError> {
    match v.kind {
        ValueKind::Float(f) => Ok(f),
        ValueKind::Int(i) => Ok(i as f64),
        _ => Err(ctx.err("expected a number", v.span)),
    }
}

/// A `.bsn` value as the description's JSON, layered over `base` for partial vectors.
fn convert(ctx: &FileCtx, field: &str, v: &syntax::Value, ty: &ValueType, base: &Value) -> Result<Value, ScriptError> {
    let ty = match ty {
        ValueType::String | ValueType::Handle => {
            return match &v.kind {
                ValueKind::Str(s) if matches!(ty, ValueType::Handle) => Ok(Value::String(resolve_path(&ctx.path, s))),
                ValueKind::Str(s) => Ok(Value::String(s.clone())),
                _ => Err(ctx.err("expected a string", v.span)),
            };
        }
        ValueType::Unsupported(t) => return Err(ctx.err(format!("{field}: {t} fields cannot be set from a level"), v.span)),
        ValueType::Field(t) => t,
    };
    let out = match ty {
        FieldType::F32 | FieldType::F64 => json!(number(ctx, v)?),
        FieldType::I8 | FieldType::U8 | FieldType::I16 | FieldType::U16 | FieldType::I32 | FieldType::U32 => match v.kind {
            ValueKind::Int(i) => json!(i as i64),
            ValueKind::Float(_) => return Err(ctx.err("expected an integer, found a float", v.span)),
            _ => return Err(ctx.err("expected an integer", v.span)),
        },
        FieldType::Bool => match v.kind {
            ValueKind::Bool(b) => json!(b),
            _ => return Err(ctx.err("expected true or false", v.span)),
        },
        FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => {
            let n = ty.lanes().expect("vector");
            let ValueKind::Type(p) = &v.kind else { return Err(ctx.err(format!("expected a {}", vector_name(ty)), v.span)) };
            let mut out = lanes(base, n);
            match &p.fields {
                Fields::Unit => {
                    out = constant(p.path.last(), n).ok_or_else(|| ctx.err(format!("unknown constant {}", p.path.joined()), p.path.span))?;
                }
                Fields::Named { fields, .. } => {
                    for f in fields {
                        let i = LANES[..n].iter().position(|l| *l == f.name).ok_or_else(|| ctx.err(format!("{} has no field {}", vector_name(ty), f.name), f.name_span))?;
                        out[i] = number(ctx, &f.value)?;
                    }
                }
                Fields::Tuple { values, .. } => {
                    if values.len() != n {
                        return Err(ctx.err(format!("{} takes {n} values", vector_name(ty)), p.span));
                    }
                    for (i, x) in values.iter().enumerate() {
                        out[i] = number(ctx, x)?;
                    }
                }
            }
            json!(out)
        }
        FieldType::Enum { variants, .. } => {
            let ValueKind::Type(p) = &v.kind else { return Err(ctx.err(format!("expected a variant ({})", variants.join(", ")), v.span)) };
            let name = p.path.last();
            if !matches!(p.fields, Fields::Unit) || !variants.iter().any(|x| x == name) {
                return Err(ctx.err(format!("unknown variant {} (variants: {})", p.path.joined(), variants.join(", ")), v.span));
            }
            json!(name)
        }
        FieldType::Entity => return Err(ctx.err("entity fields cannot be set from a level", v.span)),
        FieldType::Array { .. } => return Err(ctx.err("list values are not supported in .bsn files", v.span)),
        // `Timer { duration: 2.0, mode: Repeating }`: its fields over `base`, the rest zero.
        FieldType::Timer | FieldType::Stopwatch => {
            let (layout, name) = (ty.fields().expect("a struct"), ty.ts_name());
            let ValueKind::Type(p) = &v.kind else { return Err(ctx.err(format!("expected a {name}"), v.span)) };
            let mut out = match base {
                Value::Object(o) => o.clone(),
                _ => serde_json::Map::new(),
            };
            match &p.fields {
                Fields::Unit => {}
                Fields::Named { fields, .. } => {
                    for f in fields {
                        let sub = layout.field(&f.name).ok_or_else(|| ctx.err(format!("{name} has no field {}", f.name), f.name_span))?;
                        let at = format!("{field}.{}", f.name);
                        let below = out.get(&f.name).cloned().unwrap_or(Value::Null);
                        out.insert(f.name.clone(), convert(ctx, &at, &f.value, &ValueType::Field(sub.ty.clone()), &below)?);
                    }
                }
                Fields::Tuple { .. } => return Err(ctx.err(format!("{name} takes named fields: {name} {{ duration: 2.0 }}"), p.span)),
            }
            Value::Object(out)
        }
    };
    let mut scratch = vec![0u8; ty.size()];
    bevy_ts::value::encode_field(field, ty, &out, &mut scratch).map_err(|e| ctx.err(e.to_string(), v.span))?;
    Ok(out)
}

fn vector_name(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Vec2 => "Vec2",
        FieldType::Vec3 => "Vec3",
        FieldType::Vec4 => "Vec4",
        _ => "Quat",
    }
}

#[cfg(test)]
mod tests;
