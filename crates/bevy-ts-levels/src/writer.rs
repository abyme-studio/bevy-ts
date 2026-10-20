//! The writer (design decisions 2 and 7): operations on name paths become text splices into the
//! file's own bytes. Nothing is re-emitted; new text copies the indentation and separators of its
//! nearest sibling. Every result is re-parsed, and refused unless the tree changed only where the
//! batch said.

use crate::syntax::{self, Entity, EntityList, Entry, Fields, ParsedFile, Patch, Span, ValueKind, lex::quote};
use std::collections::BTreeMap;

/// A value the writer prints.
#[derive(Clone, Debug, PartialEq)]
pub enum WValue {
    /// `f32` is set when the value came from an `f32` field: printed in `f32`'s shortest form.
    Float { v: f64, f32: bool },
    Int(i64),
    Bool(bool),
    Str(String),
    /// `Vec3 { x: 1.0, z: 2.0 }`: a typed value with the named lanes given.
    Vector { ty: String, lanes: Vec<(String, f64)> },
    /// An enum variant or a constant, written as a path (`Visibility::Hidden`).
    Path(String),
}

impl WValue {
    pub fn print(&self) -> String {
        match self {
            WValue::Float { v, f32 } => float(*v, *f32),
            WValue::Int(i) => i.to_string(),
            WValue::Bool(b) => b.to_string(),
            WValue::Str(s) => quote(s),
            WValue::Vector { ty, lanes } => {
                let inner: Vec<String> = lanes.iter().map(|(n, v)| format!("{n}: {}", float(*v, true))).collect();
                format!("{ty} {{ {} }}", inner.join(", "))
            }
            WValue::Path(p) => p.clone(),
        }
    }
}

/// The shortest form that round-trips, always with a `.`.
pub fn float(v: f64, f32: bool) -> String {
    let mut s = if f32 { format!("{}", v as f32) } else { format!("{v}") };
    if !s.contains(['.', 'e', 'E', 'N', 'i']) {
        s.push_str(".0");
    }
    s
}

/// What a new entity is.
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    None,
    /// `@Comp { @prop: v, field: v }`: props first, then fields.
    Scene { component: String, props: Vec<(String, WValue)>, fields: Vec<(String, WValue)> },
    /// `:"path.bsn"`.
    Include(String),
}

/// A component patch for a new entity: `Transform { translation: Vec3 { x: 1.0 } }`.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPatch {
    pub component: String,
    pub fields: Vec<(String, WValue)>,
}

impl NewPatch {
    fn print(&self) -> String {
        if self.fields.is_empty() {
            return self.component.clone();
        }
        let f: Vec<String> = self.fields.iter().map(|(n, v)| format!("{n}: {}", v.print())).collect();
        format!("{} {{ {} }}", self.component, f.join(", "))
    }
}

/// One operation of a batch, on a name path from the file's top (`["EnemyB", "eye"]`). A
/// positional segment `#n` names the n-th unnamed sibling.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Sets `component.field…` (a field path, `["translation", "x"]`). `containers[i]` is the
    /// type written when `field[i + 1]` needs a new typed value (`Vec3`). A path that ends inside
    /// a prefab (no such entity in the file) is created as named override entries.
    Set { path: Vec<String>, component: String, field: Vec<String>, value: WValue, containers: Vec<String> },
    /// Removes an override: a field, or a lane of one. A patch left with no fields goes too.
    Revert { path: Vec<String>, component: String, field: Vec<String> },
    /// Sets a scene prop (`@hp`) or a scene component field on an instance.
    SetProp { path: Vec<String>, prop: String, value: WValue, field: bool },
    /// Removes a scene prop (`@hp`) or a scene component field from an instance's `@Comp { … }`.
    RevertProp { path: Vec<String>, prop: String, field: bool },
    /// Adds an entity under `parent` (the top level when empty).
    Add { parent: Vec<String>, name: String, base: Base, patches: Vec<NewPatch> },
    Delete { path: Vec<String> },
    Rename { path: Vec<String>, to: String },
    Reparent { path: Vec<String>, parent: Vec<String> },
    /// Puts raw entity text back under `parent` at `index`: the inverse of a delete.
    Restore { parent: Vec<String>, index: usize, text: String },
}

/// A batch's result: the new text, the changed lines (1-based, inclusive, in the new text) and
/// the batch that undoes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub text: String,
    pub lines: Vec<(u32, u32)>,
    pub inverse: Vec<Op>,
    /// Name paths the batch renamed, in order (a `rename`, or an unnamed entity the write named
    /// and the positional ids after it): the engine moves their nodes so the entities stay.
    pub renamed: Vec<(Vec<String>, Vec<String>)>,
}

/// Why a batch was refused.
#[derive(Clone, Debug, PartialEq)]
pub struct Refused {
    /// `not-found`, `prefab-internal`, `exists`, `invalid` or `internal`.
    pub reason: &'static str,
    pub message: String,
}

fn refuse(reason: &'static str, message: impl Into<String>) -> Refused {
    Refused { reason, message: message.into() }
}

/// Applies a batch to `src`, one operation after another, each re-parsed and checked. The whole
/// batch applies or none of it.
pub fn apply(src: &str, ops: &[Op]) -> Result<Written, Refused> {
    apply_with(src, ops, false)
}

/// [`apply`] for a sidecar level (decision 11), where a `set` on a name path the file lacks
/// creates it from the top: the file layers over a TSX scene, so every path is an override.
pub fn apply_sidecar(src: &str, ops: &[Op]) -> Result<Written, Refused> {
    apply_with(src, ops, true)
}

fn apply_with(src: &str, ops: &[Op], create_top: bool) -> Result<Written, Refused> {
    let mut text = src.to_string();
    let mut inverse = Vec::new();
    let mut changed: Vec<(usize, usize)> = Vec::new();
    let mut renamed = Vec::new();
    for op in ops {
        let file = syntax::parse(&text).map_err(|d| refuse("internal", format!("the file does not parse: {}", d.message)))?;
        let (splices, inv, touched, renames) = plan(&file, op, create_top)?;
        renamed.extend(renames);
        let (next, ranges) = splice(&text, &splices);
        let parsed = syntax::parse(&next).map_err(|d| refuse("internal", format!("the edit would not parse: {} ({op:?})", d.message)))?;
        verify(&file, &parsed, &touched)?;
        // Earlier changed ranges move with this op's splices.
        changed = changed.into_iter().map(|(s, e)| (shift(&splices, s), shift(&splices, e))).collect();
        changed.extend(ranges);
        inverse.splice(0..0, inv);
        text = next;
    }
    let index = syntax::LineIndex::new(&text);
    let mut lines: Vec<(u32, u32)> = changed.iter().map(|&(s, e)| (index.line(s), index.line(e.max(s + 1) - 1).max(index.line(s)))).collect();
    lines.sort();
    lines.dedup();
    Ok(Written { text, lines, inverse, renamed })
}

fn shift(splices: &[(usize, usize, String)], at: usize) -> usize {
    let mut out = at as isize;
    for (s, e, t) in splices {
        if *e <= at {
            out += t.len() as isize - (*e - *s) as isize;
        }
    }
    out as usize
}

/// Applies non-overlapping splices; returns the text and each insertion's new range.
fn splice(src: &str, splices: &[(usize, usize, String)]) -> (String, Vec<(usize, usize)>) {
    let mut sorted: Vec<&(usize, usize, String)> = splices.iter().collect();
    sorted.sort_by_key(|(s, e, _)| (*s, *e));
    let mut out = String::with_capacity(src.len() + 64);
    let mut ranges = Vec::new();
    let mut at = 0;
    for (s, e, t) in sorted {
        out.push_str(&src[at..*s]);
        ranges.push((out.len(), out.len() + t.len()));
        out.push_str(t);
        at = *e;
    }
    out.push_str(&src[at..]);
    (out, ranges)
}

type Renames = Vec<(Vec<String>, Vec<String>)>;
type Plan = (Vec<(usize, usize, String)>, Vec<Op>, Vec<Vec<String>>, Renames);

/// An entity found by name path, with the list it is in.
struct Found<'a> {
    entity: &'a Entity,
    list: &'a EntityList,
    index: usize,
}

fn find_in<'a>(list: &'a EntityList, seg: &str) -> Option<(usize, &'a Entity)> {
    if let Some(n) = seg.strip_prefix('#').and_then(|n| n.parse::<usize>().ok()) {
        return list.items.iter().enumerate().filter(|(_, e)| e.name().is_none() && !e.entries.is_empty()).nth(n);
    }
    list.items.iter().enumerate().find(|(_, e)| e.name().is_some_and(|x| x.name == seg))
}

fn children(e: &Entity) -> Option<&EntityList> {
    e.related("Children").map(|r| &r.list)
}

fn find<'a>(file: &'a ParsedFile, path: &[String]) -> Option<Found<'a>> {
    let mut list = &file.entities;
    let mut found = None;
    for seg in path {
        let (index, entity) = find_in(list, seg)?;
        found = Some(Found { entity, list, index });
        list = children(entity).unwrap_or(&EMPTY);
    }
    found
}

static EMPTY: EntityList = EntityList { items: Vec::new(), separators: Vec::new() };

fn line_start(src: &str, off: usize) -> usize {
    src[..off].rfind('\n').map_or(0, |i| i + 1)
}

fn indent_at(src: &str, off: usize) -> String {
    let s = line_start(src, off);
    src[s..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Where text placed after `end` on its own line goes: the end of `end`'s line when only
/// whitespace and comments follow on it, else `end` itself (with `false`).
fn after_line(file: &ParsedFile, end: usize) -> (usize, bool) {
    let src = &file.source;
    let eol = src[end..].find('\n').map_or(src.len(), |i| end + i);
    let next = file.tokens.iter().find(|t| t.span.start >= end).map_or(src.len(), |t| t.span.start);
    if next >= eol {
        // Only trivia to the end of the line; a block comment may run past it.
        let mut stop = eol;
        for c in &file.comments {
            if c.start < eol && c.end > stop && c.start >= end {
                stop = c.end;
            }
        }
        (stop, true)
    } else {
        (end, false)
    }
}

/// The indentation of an entity's entries: its first entry's line.
fn entity_indent(file: &ParsedFile, e: &Entity) -> String {
    indent_at(&file.source, e.span.start)
}

/// The indentation for children of `parent` (or the top level): a sibling's, or the parent's
/// plus four spaces.
fn child_indent(file: &ParsedFile, parent: Option<&Entity>, list: &EntityList) -> String {
    if let Some(first) = list.items.iter().find(|e| !e.entries.is_empty()) {
        return entity_indent(file, first);
    }
    match parent {
        Some(p) => format!("{}    ", entity_indent(file, p)),
        None => String::new(),
    }
}

fn name_text(name: &str) -> String {
    let ident = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ident && name != "true" && name != "false" { format!("#{name}") } else { format!("#{}", quote(name)) }
}

/// The patch for `component` on an entity: by full path, or by last segment.
fn patch_of<'a>(e: &'a Entity, component: &str) -> Option<&'a Patch> {
    let short = component.rsplit("::").next().unwrap_or(component);
    e.patches().find(|p| p.path.joined() == component).or_else(|| e.patches().find(|p| p.path.last() == short && (p.path.segments.len() == 1 || component.ends_with(&p.path.joined()))))
}

/// How the file names `component` in a new patch: as an existing patch of it does, else as given.
fn written_name(file: &ParsedFile, component: &str) -> String {
    let short = component.rsplit("::").next().unwrap_or(component);
    let mut found = None;
    walk(&file.entities, &mut |e| {
        if found.is_none()
            && let Some(p) = e.patches().find(|p| p.path.last() == short && component.ends_with(&p.path.joined()))
        {
            found = Some(p.path.joined());
        }
    });
    found.unwrap_or_else(|| component.to_string())
}

fn walk(list: &EntityList, f: &mut impl FnMut(&Entity)) {
    for e in &list.items {
        f(e);
        if let Some(c) = children(e) {
            walk(c, f);
        }
    }
}

/// The name the writer gives an unnamed entity: its prefab's or first component's short name and
/// its number among unnamed siblings of that name (`#Crate3`), unique among its siblings.
fn generated_name(list: &EntityList, e: &Entity) -> String {
    let base = |e: &Entity| -> String {
        e.entries
            .iter()
            .find_map(|x| match x {
                Entry::Prefab(p) => Some(p.path.last().to_string()),
                Entry::Patch(p) => Some(p.path.last().to_string()),
                Entry::Include(i) => Some(i.path.rsplit('/').next().unwrap_or("Level").trim_end_matches(".bsn").to_string()),
                _ => None,
            })
            .unwrap_or_else(|| "Entity".into())
    };
    let b = base(e);
    let mut n = 0;
    for x in &list.items {
        if x.name().is_none() && !x.entries.is_empty() && base(x) == b {
            n += 1;
        }
        if std::ptr::eq(x, e) {
            break;
        }
    }
    let taken = |s: &str| list.items.iter().any(|x| x.name().is_some_and(|m| m.name == s));
    let mut name = format!("{b}{n}");
    while taken(&name) {
        n += 1;
        name = format!("{b}{n}");
    }
    name
}

/// Names the target and its unnamed ancestors if they have no name; returns the splices and the
/// path with real names.
fn ensure_named(file: &ParsedFile, path: &[String], splices: &mut Vec<(usize, usize, String)>, renames: &mut Renames) -> Vec<String> {
    let mut list = &file.entities;
    let mut out: Vec<String> = Vec::new();
    for seg in path {
        let Some((_, e)) = find_in(list, seg) else {
            out.push(seg.clone());
            list = &EMPTY;
            continue;
        };
        if e.name().is_none() {
            let name = generated_name(list, e);
            splices.push((e.span.start, e.span.start, format!("{} ", name_text(&name))));
            // The positional ids after it shift down by one.
            let k: usize = seg.strip_prefix('#').and_then(|n| n.parse().ok()).unwrap_or(0);
            let unnamed = list.items.iter().filter(|x| x.name().is_none() && !x.entries.is_empty()).count();
            let at = |s: String| {
                let mut p = out.clone();
                p.push(s);
                p
            };
            renames.push((at(seg.clone()), at(name.clone())));
            for j in k + 1..unnamed {
                renames.push((at(format!("#{j}")), at(format!("#{}", j - 1))));
            }
            out.push(name);
        } else {
            out.push(seg.clone());
        }
        list = children(e).unwrap_or(&EMPTY);
    }
    out
}

fn plan(file: &ParsedFile, op: &Op, create_top: bool) -> Result<Plan, Refused> {
    let (s, i, t, mut renames) = match op {
        Op::Set { path, component, field, value, containers } => plan_set(file, path, component, field, value, containers, create_top),
        Op::Revert { path, component, field } => plan_revert(file, path, component, field),
        Op::SetProp { path, prop, value, field } => plan_set_prop(file, path, prop, value, *field),
        Op::RevertProp { path, prop, field } => plan_revert_prop(file, path, prop, *field),
        Op::Add { parent, name, base, patches } => plan_add(file, parent, name, base, patches),
        Op::Delete { path } => plan_delete(file, path),
        Op::Rename { path, to } => plan_rename(file, path, to),
        Op::Reparent { path, parent } => plan_reparent(file, path, parent),
        Op::Restore { parent, index, text } => plan_restore(file, parent, *index, text),
    }?;
    if let Op::Rename { path, to } = op {
        let mut new = path.clone();
        *new.last_mut().expect("a path") = to.clone();
        renames.push((path.clone(), new));
    }
    Ok((s, i, t, renames))
}

/// The existing value at a field path inside a patch's fields, if any.
fn value_at<'a>(fields: &'a Fields, field: &[String]) -> Option<&'a syntax::Value> {
    let (first, rest) = field.split_first()?;
    let v = match fields {
        Fields::Named { .. } => &fields.get(first)?.value,
        Fields::Tuple { values, .. } => values.get(first.parse::<usize>().ok()?)?,
        Fields::Unit => return None,
    };
    if rest.is_empty() {
        return Some(v);
    }
    match &v.kind {
        ValueKind::Type(p) => value_at(&p.fields, rest),
        _ => None,
    }
}

/// Splices that set `field` to `value` inside `fields` (written after `path_end`, the type path's
/// end, for a unit patch). Returns whether it wrote anything.
fn set_in_fields(file: &ParsedFile, fields: &Fields, path_end: usize, field: &[String], value: &WValue, containers: &[String], out: &mut Vec<(usize, usize, String)>) {
    let (first, rest) = field.split_first().expect("a field");
    let existing = match fields {
        Fields::Named { .. } => fields.get(first).map(|f| &f.value),
        Fields::Tuple { values, .. } => first.parse::<usize>().ok().and_then(|i| values.get(i)),
        Fields::Unit => None,
    };
    let new_value = |rest: &[String]| -> String {
        // A new value for `first`: the leaf, or typed containers down to it.
        if rest.is_empty() {
            return value.print();
        }
        let mut s = value.print();
        for (i, seg) in rest.iter().enumerate().rev() {
            s = format!("{} {{ {seg}: {s} }}", containers.get(i).map_or("", String::as_str));
        }
        s
    };
    if let Some(v) = existing {
        if rest.is_empty() {
            out.push((v.span.start, v.span.end, value.print()));
            return;
        }
        match &v.kind {
            ValueKind::Type(p) if !matches!(p.fields, Fields::Unit) => {
                set_in_fields(file, &p.fields, p.path.span.end, rest, value, containers.get(1..).unwrap_or(&[]), out);
            }
            ValueKind::Type(p) => {
                // A constant (`Vec3::ZERO`): written out lane by lane, then the lane set.
                let n = match containers.first().map(String::as_str) {
                    Some("Vec2") => 2,
                    Some("Vec4") | Some("Quat") => 4,
                    _ => 3,
                };
                let lanes = crate::compose::constant(p.path.last(), n).unwrap_or(vec![0.0; n]);
                let names = ["x", "y", "z", "w"];
                let parts: Vec<String> = (0..n)
                    .map(|i| if rest[0] == names[i] && rest.len() == 1 { format!("{}: {}", names[i], value.print()) } else { format!("{}: {}", names[i], float(lanes[i], true)) })
                    .collect();
                out.push((v.span.start, v.span.end, format!("{} {{ {} }}", containers.first().map_or("Vec3", String::as_str), parts.join(", "))));
            }
            _ => out.push((v.span.start, v.span.end, new_value(rest))),
        }
        return;
    }
    let entry = format!("{first}: {}", new_value(rest));
    add_field(file, fields, path_end, &entry, out);
}

/// Adds `name: value` text to a patch's fields, in their style.
fn add_field(file: &ParsedFile, fields: &Fields, path_end: usize, entry: &str, out: &mut Vec<(usize, usize, String)>) {
    let src = &file.source;
    match fields {
        Fields::Unit => out.push((path_end, path_end, format!(" {{ {entry} }}"))),
        Fields::Tuple { .. } => {}
        Fields::Named { open, close, fields, trailing_comma } => {
            let Some(last) = fields.last() else {
                out.push((open.end, close.start, format!(" {entry} ")));
                return;
            };
            let one_line = !src[open.start..close.end].contains('\n');
            if one_line {
                out.push((last.value.span.end, last.value.span.end, format!(", {entry}")));
                return;
            }
            let indent = indent_at(src, last.span.start);
            // The comma after the last field, if any.
            let comma_end = if *trailing_comma { file.tokens.iter().find(|t| t.span.start >= last.span.end).map_or(last.span.end, |t| t.span.end) } else { last.span.end };
            if !trailing_comma {
                out.push((last.span.end, last.span.end, ",".into()));
            }
            let (at, own_line) = after_line(file, comma_end);
            let tail = if *trailing_comma { "," } else { "" };
            if own_line {
                out.push((at, at, format!("\n{indent}{entry}{tail}")));
            } else {
                out.push((at, at, format!(" {entry}{tail}")));
            }
        }
    }
}

/// Where a new head entry (a patch) goes on an entity: on its own line after its last
/// non-relationship entry.
fn add_entry(file: &ParsedFile, e: &Entity, text: &str, out: &mut Vec<(usize, usize, String)>) {
    let indent = entity_indent(file, e);
    let last_head = e.entries.iter().rev().find(|x| !matches!(x, Entry::Related(_))).map(|x| x.span().end);
    let first_related = e.entries.iter().find(|x| matches!(x, Entry::Related(_))).map(|x| x.span().start);
    match (last_head, first_related) {
        (Some(end), related) => {
            let (at, own_line) = after_line(file, end);
            if own_line {
                out.push((at, at, format!("\n{indent}{text}")));
            } else if let Some(r) = related {
                out.push((r, r, format!("{text}\n{indent}")));
            } else {
                out.push((end, end, format!(" {text}")));
            }
        }
        (None, Some(r)) => out.push((r, r, format!("{text}\n{indent}"))),
        (None, None) => out.push((e.span.start, e.span.start, text.to_string())),
    }
}

/// Creates the missing tail of `path` (override entries inside a prefab) under the deepest
/// entity that exists, with `body` as the last one's patch text.
fn create_path(file: &ParsedFile, path: &[String], body: &str, create_top: bool, out: &mut Vec<(usize, usize, String)>) -> Result<(), Refused> {
    let mut depth = 0;
    let mut list = &file.entities;
    let mut parent: Option<&Entity> = None;
    for seg in path {
        match find_in(list, seg) {
            Some((_, e)) => {
                parent = Some(e);
                list = children(e).unwrap_or(&EMPTY);
                depth += 1;
            }
            None => break,
        }
    }
    if parent.is_none() && !create_top {
        return Err(refuse("not-found", format!("no entity {}", path.join("/"))));
    }
    let missing = &path[depth..];
    // Nested override entries, innermost holding the body.
    let base_indent = match parent {
        Some(p) => child_indent(file, Some(p), children(p).unwrap_or(&EMPTY)),
        None => child_indent(file, None, &file.entities),
    };
    let mut text = String::new();
    for (i, seg) in missing.iter().enumerate() {
        let ind = format!("{base_indent}{}", "    ".repeat(i));
        if i > 0 {
            text.push_str(&format!("\n{ind}"));
        }
        text.push_str(&name_text(seg));
        if i + 1 == missing.len() {
            text.push_str(&format!("\n{ind}{body}"));
        } else {
            text.push_str(&format!("\n{ind}Children ["));
        }
    }
    for i in (0..missing.len().saturating_sub(1)).rev() {
        text.push_str(&format!("\n{base_indent}{}]", "    ".repeat(i)));
    }
    match parent {
        Some(p) => append_child(file, p, &text, out),
        None => append_top(file, &text, &base_indent, out),
    }
    Ok(())
}

/// Appends entity text to `parent`'s `Children` list (made if missing), `--` before it when the
/// list has entities.
fn append_child(file: &ParsedFile, parent: &Entity, text: &str, out: &mut Vec<(usize, usize, String)>) {
    let indent = entity_indent(file, parent);
    match parent.related("Children") {
        Some(r) => {
            let list = &r.list;
            let inner = child_indent(file, Some(parent), list);
            let last = list.items.iter().rev().find(|e| !e.entries.is_empty());
            match last {
                Some(last) => {
                    let has_trailing = list.separators.len() >= list.items.len();
                    let (at, _) = after_line(file, if has_trailing { *list.separators.last().map(|s| &s.end).expect("separator") } else { last.span.end });
                    let sep = if has_trailing { String::new() } else { format!("\n{inner}--") };
                    let trail = if has_trailing { format!("\n{inner}--") } else { String::new() };
                    out.push((at, at, format!("{sep}\n{inner}{}{trail}", text)));
                }
                None => {
                    let one_line = !file.source[r.open.start..r.close.end].contains('\n');
                    if one_line {
                        out.push((r.open.end, r.close.start, format!("\n{inner}{text}\n{indent}")));
                    } else {
                        out.push((r.open.end, r.open.end, format!("\n{inner}{text}")));
                    }
                }
            }
        }
        None => {
            let inner = format!("{indent}    ");
            add_entry(file, parent, &format!("Children [\n{inner}{text}\n{indent}]"), out);
        }
    }
}

fn plan_set(file: &ParsedFile, path: &[String], component: &str, field: &[String], value: &WValue, containers: &[String], create_top: bool) -> Result<Plan, Refused> {
    if field.is_empty() {
        return Err(refuse("invalid", "set needs a field"));
    }
    let mut out = Vec::new();
    let mut renames = Vec::new();
    let named = ensure_named(file, path, &mut out, &mut renames);
    let inverse;
    match find(file, path) {
        Some(f) => {
            let e = f.entity;
            match patch_of(e, component) {
                Some(p) => {
                    inverse = match value_at(&p.fields, field) {
                        Some(old) => to_wvalue(file, old).map(|v| Op::Set { path: named.clone(), component: component.into(), field: field.to_vec(), value: v, containers: containers.to_vec() }),
                        None => Some(Op::Revert { path: named.clone(), component: component.into(), field: field.to_vec() }),
                    };
                    set_in_fields(file, &p.fields, p.path.span.end, field, value, containers, &mut out);
                }
                None => {
                    let name = written_name(file, component);
                    let mut body = String::new();
                    let mut s = value.print();
                    for (i, seg) in field.iter().enumerate().rev() {
                        s = if i == 0 { format!("{seg}: {s}") } else { format!("{} {{ {seg}: {s} }}", containers.get(i - 1).map_or("", String::as_str)) };
                    }
                    body.push_str(&format!("{name} {{ {s} }}"));
                    add_entry(file, e, &body, &mut out);
                    inverse = Some(Op::Revert { path: named.clone(), component: component.into(), field: field.to_vec() });
                }
            }
        }
        None => {
            let name = written_name(file, component);
            let mut s = value.print();
            for (i, seg) in field.iter().enumerate().rev() {
                s = if i == 0 { format!("{seg}: {s}") } else { format!("{} {{ {seg}: {s} }}", containers.get(i - 1).map_or("", String::as_str)) };
            }
            create_path(file, path, &format!("{name} {{ {s} }}"), create_top, &mut out)?;
            inverse = Some(Op::Revert { path: named.clone(), component: component.into(), field: field.to_vec() });
        }
    }
    Ok((dedupe(out), inverse.into_iter().collect(), vec![named], renames))
}

/// One splice per range: naming an entity and inserting at its start may coincide.
fn dedupe(mut v: Vec<(usize, usize, String)>) -> Vec<(usize, usize, String)> {
    v.sort_by_key(|(s, e, _)| (*s, *e));
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    for (s, e, t) in v {
        if let Some(last) = out.last_mut()
            && last.0 == s
            && last.1 == s
            && e == s
        {
            last.2.push_str(&t);
            continue;
        }
        out.push((s, e, t));
    }
    out
}

/// A parsed value back as a writer value, for inverses.
fn to_wvalue(file: &ParsedFile, v: &syntax::Value) -> Option<WValue> {
    Some(match &v.kind {
        ValueKind::Float(f) => WValue::Float { v: *f, f32: false },
        ValueKind::Int(i) => WValue::Int(*i as i64),
        ValueKind::Bool(b) => WValue::Bool(*b),
        ValueKind::Str(s) => WValue::Str(s.clone()),
        ValueKind::Type(p) => match &p.fields {
            Fields::Unit => WValue::Path(p.path.joined()),
            Fields::Named { fields, .. } => WValue::Vector {
                ty: p.path.joined(),
                lanes: fields.iter().map(|f| Some((f.name.clone(), number(&f.value)?))).collect::<Option<Vec<_>>>()?,
            },
            Fields::Tuple { .. } => return Some(WValue::Path(file.text(v.span).to_string())),
        },
    })
}

fn number(v: &syntax::Value) -> Option<f64> {
    match v.kind {
        ValueKind::Float(f) => Some(f),
        ValueKind::Int(i) => Some(i as f64),
        _ => None,
    }
}

/// The span to remove for a field: the field and the comma after it, or before it when last.
fn field_removal(file: &ParsedFile, fields: &Fields, name: &str) -> Option<(usize, usize)> {
    let Fields::Named { fields: list, open, close, .. } = fields else { return None };
    let i = list.iter().position(|f| f.name == name)?;
    let f = &list[i];
    let next_tok = |at: usize| file.tokens.iter().find(|t| t.span.start >= at);
    let one_line = !file.source[open.start..close.end].contains('\n');
    if let Some(t) = next_tok(f.span.end).filter(|t| t.kind == syntax::lex::Kind::Comma) {
        if one_line || i + 1 < list.len() {
            // `a: 1, ` up to the next field.
            let end = next_tok(t.span.end).map_or(t.span.end, |n| if i + 1 < list.len() { n.span.start } else { t.span.end });
            return Some((f.span.start, end));
        }
        // The last field of a multi-line patch: its whole line.
        let start = line_start(&file.source, f.span.start);
        let (end, _) = after_line(file, t.span.end);
        return Some((start.saturating_sub(1).max(open.end), end));
    }
    if i > 0 {
        // The last field without a trailing comma: from the previous field's end.
        return Some((list[i - 1].span.end, f.span.end));
    }
    Some((f.span.start, f.span.end))
}

fn plan_revert(file: &ParsedFile, path: &[String], component: &str, field: &[String]) -> Result<Plan, Refused> {
    let f = find(file, path).ok_or_else(|| refuse("not-found", format!("no entity {}", path.join("/"))))?;
    let p = patch_of(f.entity, component).ok_or_else(|| refuse("not-found", format!("{} has no {component} override", path.join("/"))))?;
    let old = value_at(&p.fields, field).ok_or_else(|| refuse("not-found", format!("{} has no {component}.{} override", path.join("/"), field.join("."))))?;
    let inverse = to_wvalue(file, old).map(|v| Op::Set { path: path.to_vec(), component: component.into(), field: field.to_vec(), value: v, containers: Vec::new() });
    // The fields that hold the leaf, and whether removing it empties them.
    let (holder, holder_end, leaf) = if field.len() == 1 {
        (&p.fields, p.path.span.end, &field[0])
    } else {
        let parent = value_at(&p.fields, &field[..field.len() - 1]).expect("the leaf exists");
        let ValueKind::Type(t) = &parent.kind else { return Err(refuse("invalid", "not a typed value")) };
        (&t.fields, t.path.span.end, &field[field.len() - 1])
    };
    let mut out = Vec::new();
    if holder.named().len() == 1 && holder.get(leaf).is_some() {
        if field.len() == 1 {
            let only = f.entity.entries.iter().all(|x| matches!(x, Entry::Name(_)) || x == &Entry::Patch(p.clone()));
            if only && path.len() > 1 {
                // An override entry with nothing left (`#eye` under an instance) goes whole, and
                // its parent's `Children [ … ]` with it when it was the only one.
                let parent = find(file, &path[..path.len() - 1]).expect("the parent of a found entity");
                let siblings = f.list.items.iter().filter(|e| !e.entries.is_empty()).count();
                match parent.entity.related("Children") {
                    Some(r) if siblings == 1 => out.push(entry_removal(file, parent.entity, &Entry::Related(r.clone()))),
                    _ => {
                        let (s, e) = entity_removal(file, f.list, f.index);
                        out.push((s, e, String::new()));
                    }
                }
            } else {
                // The patch has nothing left: it goes with its line.
                out.push(entry_removal(file, f.entity, &Entry::Patch(p.clone())));
            }
        } else {
            return plan_revert(file, path, component, &field[..field.len() - 1]).map(|(s, _, t, n)| (s, inverse.clone().into_iter().collect(), t, n));
        }
    } else {
        let (s, e) = field_removal(file, holder, leaf).ok_or_else(|| refuse("invalid", "a tuple field cannot be reverted alone"))?;
        out.push((s, e, String::new()));
    }
    let _ = holder_end;
    Ok((out, inverse.into_iter().collect(), vec![path.to_vec()], Vec::new()))
}

/// Removes an entry of an entity: its own line when it has one, else the entry and the space
/// before it.
fn entry_removal(file: &ParsedFile, e: &Entity, entry: &Entry) -> (usize, usize, String) {
    let span = entry.span();
    let src = &file.source;
    let ls = line_start(src, span.start);
    let alone_before = src[ls..span.start].trim().is_empty();
    let (eol, alone_after) = after_line(file, span.end);
    if alone_before && alone_after && e.entries.len() > 1 {
        let end = if src[eol..].starts_with('\n') { eol + 1 } else { eol };
        return (ls, end, String::new());
    }
    let trivia = file.trivia_before(span.start);
    (trivia.start, span.end, String::new())
}

fn plan_set_prop(file: &ParsedFile, path: &[String], prop: &str, value: &WValue, is_field: bool) -> Result<Plan, Refused> {
    let f = find(file, path).ok_or_else(|| refuse("not-found", format!("no entity {}", path.join("/"))))?;
    let Some(Entry::Prefab(p)) = f.entity.inclusion() else { return Err(refuse("invalid", format!("{} is not a scene component instance", path.join("/")))) };
    let existing = p.fields.named().iter().find(|x| x.name == prop && x.prop == !is_field);
    let mut out = Vec::new();
    let inverse = match existing {
        Some(x) => {
            let inv = to_wvalue(file, &x.value).map(|v| Op::SetProp { path: path.to_vec(), prop: prop.into(), value: v, field: is_field });
            out.push((x.value.span.start, x.value.span.end, value.print()));
            inv
        }
        None => {
            let at = if is_field { "" } else { "@" };
            add_field(file, &p.fields, p.path.span.end, &format!("{at}{prop}: {}", value.print()), &mut out);
            Some(Op::RevertProp { path: path.to_vec(), prop: prop.into(), field: is_field })
        }
    };
    Ok((out, inverse.into_iter().collect(), vec![path.to_vec()], Vec::new()))
}

fn plan_revert_prop(file: &ParsedFile, path: &[String], prop: &str, is_field: bool) -> Result<Plan, Refused> {
    let f = find(file, path).ok_or_else(|| refuse("not-found", format!("no entity {}", path.join("/"))))?;
    let Some(Entry::Prefab(p)) = f.entity.inclusion() else { return Err(refuse("invalid", format!("{} is not a scene component instance", path.join("/")))) };
    let at = if is_field { "" } else { "@" };
    let named = p.fields.named();
    let Some(x) = named.iter().find(|x| x.name == prop && x.prop == !is_field) else {
        return Err(refuse("not-found", format!("{} sets no {at}{prop}", path.join("/"))));
    };
    let inverse = to_wvalue(file, &x.value).map(|v| Op::SetProp { path: path.to_vec(), prop: prop.into(), value: v, field: is_field });
    let mut out = Vec::new();
    match &p.fields {
        // The only one: the braces go with it (`@Comp`).
        Fields::Named { close, .. } if named.len() == 1 => out.push((p.path.span.end, close.end, String::new())),
        fields => {
            let (s, e) = field_removal(file, fields, prop).ok_or_else(|| refuse("invalid", "not a named field"))?;
            out.push((s, e, String::new()));
        }
    }
    Ok((out, inverse.into_iter().collect(), vec![path.to_vec()], Vec::new()))
}

fn entity_text(name: &str, base: &Base, patches: &[NewPatch], indent: &str) -> String {
    let mut lines = vec![name_text(name)];
    match base {
        Base::None => {}
        Base::Include(p) => lines.push(format!(":{}", quote(p))),
        Base::Scene { component, props, fields } => {
            let mut f: Vec<String> = props.iter().map(|(n, v)| format!("@{n}: {}", v.print())).collect();
            f.extend(fields.iter().map(|(n, v)| format!("{n}: {}", v.print())));
            lines.push(if f.is_empty() { format!("@{component}") } else { format!("@{component} {{ {} }}", f.join(", ")) });
        }
    }
    lines.extend(patches.iter().map(NewPatch::print));
    lines.join(&format!("\n{indent}"))
}

fn plan_add(file: &ParsedFile, parent: &[String], name: &str, base: &Base, patches: &[NewPatch]) -> Result<Plan, Refused> {
    let mut out = Vec::new();
    let mut path = parent.to_vec();
    path.push(name.to_string());
    if parent.is_empty() {
        if find_in(&file.entities, name).is_some() {
            return Err(refuse("exists", format!("an entity named {name} is already at the top level")));
        }
        let list = &file.entities;
        let indent = child_indent(file, None, list);
        let text = entity_text(name, base, patches, &indent);
        append_top(file, &text, &indent, &mut out);
    } else {
        let p = find(file, parent).ok_or_else(|| refuse("not-found", format!("no entity {}", parent.join("/"))))?;
        if children(p.entity).and_then(|c| find_in(c, name)).is_some() {
            return Err(refuse("exists", format!("{} already has a child named {name}", parent.join("/"))));
        }
        let indent = child_indent(file, Some(p.entity), children(p.entity).unwrap_or(&EMPTY));
        let text = entity_text(name, base, patches, &indent);
        append_child(file, p.entity, &text, &mut out);
    }
    Ok((out, vec![Op::Delete { path: path.clone() }], vec![path], Vec::new()))
}

/// Appends a top-level entity, after a `--` when the file has one.
fn append_top(file: &ParsedFile, text: &str, indent: &str, out: &mut Vec<(usize, usize, String)>) {
    let list = &file.entities;
    let src = &file.source;
    let Some(last) = list.items.iter().rev().find(|e| !e.entries.is_empty()) else {
        let at = src.len();
        let lead = if src.is_empty() || src.ends_with('\n') { "" } else { "\n" };
        out.push((at, at, format!("{lead}{indent}{text}\n")));
        return;
    };
    let has_trailing = list.separators.len() >= list.items.len();
    let end = if has_trailing { list.separators.last().expect("separator").end } else { last.span.end };
    let (at, _) = after_line(file, end);
    let sep = if has_trailing { String::new() } else { format!("\n{indent}--") };
    let trail = if has_trailing { format!("\n{indent}--") } else { String::new() };
    out.push((at, at, format!("{sep}\n{indent}{text}{trail}")));
}

/// The bytes of an entity a delete removes: its leading comments (those on lines of their own
/// before it), its span, and one adjacent `--` with its line.
fn entity_removal(file: &ParsedFile, list: &EntityList, index: usize) -> (usize, usize) {
    let src = &file.source;
    let e = &list.items[index];
    let trivia = file.trivia_before(e.span.start);
    // Comments after the first newline of the trivia belong to the entity.
    let own = src[trivia.start..trivia.end].find('\n').map_or(trivia.start, |i| trivia.start + i + 1);
    let mut start = own.min(line_start(src, e.span.start));
    let (mut end, _) = after_line(file, e.span.end);
    if let Some(sep) = list.separators.get(index) {
        let (sep_end, _) = after_line(file, sep.end);
        end = sep_end;
    } else if index > 0
        && let Some(sep) = list.separators.get(index - 1)
    {
        start = line_start(src, sep.start);
        let prev_end = list.items[index - 1].span.end;
        let (prev_eol, _) = after_line(file, prev_end);
        start = start.max(prev_eol);
    }
    if src[end..].starts_with('\n') && (start == 0 || src[..start].ends_with('\n')) {
        end += 1;
    }
    (start, end)
}

fn plan_delete(file: &ParsedFile, path: &[String]) -> Result<Plan, Refused> {
    let Some(f) = find(file, path) else {
        // Under an instance and not in the file: it is the prefab's (Godot cannot delete an
        // instanced child either).
        let inside = (1..path.len()).rev().find_map(|n| find(file, &path[..n])).is_some_and(|a| a.entity.inclusion().is_some());
        return Err(if inside { refuse("prefab-internal", format!("{} belongs to its prefab; a level cannot delete it", path.join("/"))) } else { refuse("not-found", format!("no entity {}", path.join("/"))) });
    };
    let (start, end) = entity_removal(file, f.list, f.index);
    let text = file.text(f.entity.span).to_string();
    let parent = path[..path.len() - 1].to_vec();
    Ok((vec![(start, end, String::new())], vec![Op::Restore { parent, index: f.index, text }], vec![path.to_vec()], Vec::new()))
}

fn plan_restore(file: &ParsedFile, parent: &[String], _index: usize, text: &str) -> Result<Plan, Refused> {
    let mut out = Vec::new();
    let name = syntax::parse(text).ok().and_then(|p| p.entities.items.first().and_then(|e| e.name().map(|n| n.name.clone()))).unwrap_or_default();
    let mut path = parent.to_vec();
    path.push(name.clone());
    if parent.is_empty() {
        let indent = child_indent(file, None, &file.entities);
        append_top(file, text, &indent, &mut out);
    } else {
        let p = find(file, parent).ok_or_else(|| refuse("not-found", format!("no entity {}", parent.join("/"))))?;
        append_child(file, p.entity, text, &mut out);
    }
    Ok((out, vec![Op::Delete { path: path.clone() }], vec![path], Vec::new()))
}

fn plan_rename(file: &ParsedFile, path: &[String], to: &str) -> Result<Plan, Refused> {
    let f = find(file, path).ok_or_else(|| refuse("not-found", format!("no entity {}", path.join("/"))))?;
    if find_in(f.list, to).is_some() {
        return Err(refuse("exists", format!("a sibling is already named {to}")));
    }
    let mut new_path = path.to_vec();
    *new_path.last_mut().expect("a path") = to.to_string();
    let out = match f.entity.name() {
        Some(n) => vec![(n.span.start, n.span.end, name_text(to))],
        None => vec![(f.entity.span.start, f.entity.span.start, format!("{} ", name_text(to)))],
    };
    let back = f.entity.name().map_or_else(|| path.last().cloned().unwrap_or_default(), |n| n.name.clone());
    Ok((out, vec![Op::Rename { path: new_path.clone(), to: back }], vec![path.to_vec(), new_path], Vec::new()))
}

fn plan_reparent(file: &ParsedFile, path: &[String], parent: &[String]) -> Result<Plan, Refused> {
    let f = find(file, path).ok_or_else(|| refuse("not-found", format!("no entity {}", path.join("/"))))?;
    if parent.starts_with(path) {
        return Err(refuse("invalid", "an entity cannot move under itself"));
    }
    let name = f.entity.name().map(|n| n.name.clone()).ok_or_else(|| refuse("invalid", "name the entity before moving it"))?;
    let (start, end) = entity_removal(file, f.list, f.index);
    // Re-indent the entity's lines to the new place.
    let old_indent = entity_indent(file, f.entity);
    let mut out = vec![(start, end, String::new())];
    let new_indent = if parent.is_empty() {
        child_indent(file, None, &file.entities)
    } else {
        let p = find(file, parent).ok_or_else(|| refuse("not-found", format!("no entity {}", parent.join("/"))))?;
        if children(p.entity).and_then(|c| find_in(c, &name)).is_some() {
            return Err(refuse("exists", format!("{} already has a child named {name}", parent.join("/"))));
        }
        child_indent(file, Some(p.entity), children(p.entity).unwrap_or(&EMPTY))
    };
    let body: Vec<String> = file
        .text(f.entity.span)
        .lines()
        .enumerate()
        .map(|(i, l)| if i == 0 { l.to_string() } else { format!("{new_indent}{}", l.strip_prefix(old_indent.as_str()).unwrap_or(l.trim_start())) })
        .collect();
    let text = body.join("\n");
    if parent.is_empty() {
        append_top(file, &text, &new_indent, &mut out);
    } else {
        let p = find(file, parent).expect("checked");
        append_child(file, p.entity, &text, &mut out);
    }
    let mut new_path = parent.to_vec();
    new_path.push(name);
    Ok((out, vec![Op::Reparent { path: new_path.clone(), parent: path[..path.len() - 1].to_vec() }], vec![path.to_vec(), new_path], Vec::new()))
}

/// The head of every named entity (its entries but its relationship lists), by name path, as
/// token text without trivia.
fn fingerprint(file: &ParsedFile) -> BTreeMap<Vec<String>, String> {
    fn go(file: &ParsedFile, list: &EntityList, prefix: &[String], out: &mut BTreeMap<Vec<String>, String>) {
        for e in &list.items {
            let Some(n) = e.name() else { continue };
            let mut path = prefix.to_vec();
            path.push(n.name.clone());
            let mut head = String::new();
            for entry in &e.entries {
                if let Entry::Related(r) = entry {
                    head.push_str(&format!("{}[] ", r.path.joined()));
                    continue;
                }
                let span: Span = entry.span();
                let from = file.tokens.partition_point(|t| t.span.start < span.start);
                for t in file.tokens[from..].iter().take_while(|t| t.span.end <= span.end) {
                    head.push_str(file.text(t.span));
                    head.push(' ');
                }
            }
            out.insert(path.clone(), head);
            if let Some(c) = children(e) {
                go(file, c, &path, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    go(file, &file.entities, &[], &mut out);
    out
}

/// The new tree may differ from the old only inside the touched paths (and their unnamed
/// ancestors, which the write names).
fn verify(old: &ParsedFile, new: &ParsedFile, touched: &[Vec<String>]) -> Result<(), Refused> {
    let inside = |p: &Vec<String>| touched.iter().any(|t| p.starts_with(t));
    // A write names unnamed ancestors of its target: new paths on the way to it are expected.
    let on_the_way = |p: &Vec<String>| touched.iter().any(|t| t.starts_with(p));
    let a = fingerprint(old);
    let b = fingerprint(new);
    for (p, head) in &a {
        if inside(p) {
            continue;
        }
        // A touched ancestor's children list may gain or lose an entity; its head may not change.
        match b.get(p) {
            Some(h) if h == head => {}
            Some(h) if touched.iter().any(|t| t.starts_with(p)) && h.replace("Children[] ", "") == head.replace("Children[] ", "") => {}
            other => return Err(refuse("internal", format!("the edit would change {} ({head:?} -> {other:?})", p.join("/")))),
        }
    }
    for p in b.keys() {
        if !a.contains_key(p) && !inside(p) && !on_the_way(p) {
            return Err(refuse("internal", format!("the edit would add {}", p.join("/"))));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
