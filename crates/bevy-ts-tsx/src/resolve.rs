//! Component elements resolved against the components they name: which fields are controlled,
//! which initial, the bytes to insert, and the errors (with TS locations) that fail a load.

use crate::desc::{ComponentDesc, ComponentKind, NodeDesc, RootDesc, binding_id};
use crate::ui::{self, UiKind};
use crate::intrinsics::{MaterialProps, MeshProps};
use bevy::ecs::component::ComponentId;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::components::{PendingComponents, TsComponents};
use bevy_ts::layout::{ComponentLayout, FieldType, NativeLayouts};
use bevy_ts::manifest::ComponentDecl;
use bevy_ts::value;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

/// The name `<entity name>` and `<Name value>` resolve to.
pub const NAME: &str = "Name";
pub const MESH: &str = "mesh";
pub const MATERIAL: &str = "material";

/// Where TS components come from: the pending generation while checking, the live registry
/// while applying.
pub trait TsLookup {
    fn ts(&self, name: &str) -> Option<(ComponentId, ComponentLayout, ComponentDecl)>;
}

impl TsLookup for PendingComponents {
    fn ts(&self, name: &str) -> Option<(ComponentId, ComponentLayout, ComponentDecl)> {
        let (id, layout) = self.lookup(name)?;
        Some((id, layout.clone(), self.decl(name)?.clone()))
    }
}

impl TsLookup for TsComponents {
    fn ts(&self, name: &str) -> Option<(ComponentId, ComponentLayout, ComponentDecl)> {
        self.live(name).map(|c| (c.id, c.layout.clone(), c.decl.clone()))
    }
}

/// What a component element writes.
#[derive(Clone, Debug)]
pub enum Target {
    /// A component with a native layout: a TS component or `Transform`. `defaults` is a whole
    /// value with every field at its declared default.
    Native { id: ComponentId, layout: ComponentLayout, defaults: Vec<u8> },
    Name,
    Mesh(MeshProps),
    Material(MaterialProps),
    Ui(UiKind),
}

/// A component element, checked.
#[derive(Clone, Debug)]
pub struct Resolved {
    /// Its identity on the entity: the component name, or the intrinsic.
    pub key: String,
    pub target: Target,
    /// Fields written on insert and whenever their declared value changes.
    pub controlled: BTreeMap<String, Value>,
    /// Fields written only on insert.
    pub initial: BTreeMap<String, Value>,
    /// Fields given neither way, with the default they take on insert.
    pub defaults: BTreeMap<String, Value>,
    /// Fields written only by a binding (`f={() => ...}`), by binding id.
    pub bound: BTreeMap<String, u32>,
    /// The site of each bound native-layout field (design decision 11).
    pub sites: BTreeMap<String, u32>,
    pub source: Option<SourceLocation>,
}

impl Resolved {
    /// The bytes to insert: defaults, then initial and controlled values.
    pub fn insert_bytes(&self) -> Option<Vec<u8>> {
        let Target::Native { layout, defaults, .. } = &self.target else { return None };
        let mut bytes = defaults.clone();
        for (f, v) in self.initial.iter().chain(self.controlled.iter()) {
            let fl = layout.field(f).expect("checked field");
            value::encode_field(f, &fl.ty, v, &mut bytes[fl.offset..fl.offset + fl.ty.size()]).expect("checked value");
        }
        Some(bytes)
    }
}

fn err(message: impl Into<String>, at: &Option<SourceLocation>) -> ScriptError {
    ScriptError::at(message, at.clone())
}

/// `initialHp` → `hp`.
fn initial_field(prop: &str) -> Option<String> {
    let rest = prop.strip_prefix("initial")?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    first.is_uppercase().then(|| first.to_lowercase().chain(chars).collect())
}

fn transform_defaults(layout: &ComponentLayout) -> Vec<u8> {
    let mut bytes = vec![0u8; layout.size];
    for (f, v) in [("translation", json!([0, 0, 0])), ("rotation", json!([0, 0, 0, 1])), ("scale", json!([1, 1, 1]))] {
        let fl = layout.field(f).expect("Transform field");
        value::encode_field(f, &fl.ty, &v, &mut bytes[fl.offset..fl.offset + fl.ty.size()]).expect("Transform default");
    }
    bytes
}

/// Controlled, initial and default fields of one element.
type Split = (BTreeMap<String, Value>, BTreeMap<String, Value>, BTreeMap<String, Value>);

/// Splits props into controlled and initial fields of a native-layout component and checks
/// every value against its field type.
fn split_fields(
    c: &ComponentDesc,
    fields: &[(String, FieldType, Option<Value>)],
) -> Result<Split, ScriptError> {
    let field = |name: &str| fields.iter().find(|(n, ..)| n == name);
    let mut controlled = BTreeMap::new();
    let mut initial = BTreeMap::new();
    for (prop, v) in &c.props {
        if let Some((name, ty, _)) = field(prop) {
            check_value(c, name, ty, v)?;
            controlled.insert(name.clone(), v.clone());
        } else if let Some(f) = initial_field(prop)
            && let Some((name, ty, _)) = field(&f)
        {
            check_value(c, name, ty, v)?;
            initial.insert(name.clone(), v.clone());
        } else {
            let known: Vec<&str> = fields.iter().map(|(n, ..)| n.as_str()).collect();
            return Err(err(format!("{} has no field {prop} (fields: {})", c.ty, known.join(", ")), &c.source));
        }
    }
    for f in initial.keys() {
        if controlled.contains_key(f) {
            let cap: String = f.chars().take(1).flat_map(char::to_uppercase).chain(f.chars().skip(1)).collect();
            return Err(err(format!("{}.{f} is given both as {f} and initial{cap}; pick one", c.ty), &c.source));
        }
    }
    let defaults = fields
        .iter()
        .filter(|(n, ..)| !controlled.contains_key(n) && !initial.contains_key(n))
        .map(|(n, ty, d)| (n.clone(), d.clone().unwrap_or_else(|| value::zero(ty))))
        .collect();
    Ok((controlled, initial, defaults))
}

fn check_value(c: &ComponentDesc, field: &str, ty: &FieldType, v: &Value) -> Result<(), ScriptError> {
    let mut scratch = vec![0u8; ty.size()];
    value::encode_field(&format!("{}.{field}", c.ty), ty, v, &mut scratch).map_err(|e| err(e.to_string(), &c.source))
}

fn parse_props<T: serde::de::DeserializeOwned>(c: &ComponentDesc) -> Result<T, ScriptError> {
    serde_json::from_value(Value::Object(c.props.clone())).map_err(|e| err(format!("<{}>: {e}", c.ty), &c.source))
}

/// Resolves one component element: bound props first, then the rest by kind.
pub fn resolve(c: &ComponentDesc, ts: &dyn TsLookup, native: &NativeLayouts) -> Result<Resolved, ScriptError> {
    let mut bound = BTreeMap::new();
    let mut sites = BTreeMap::new();
    let mut rest = c.clone();
    for (prop, v) in &c.props {
        let Some(id) = binding_id(v) else { continue };
        if initial_field(prop).is_some() {
            return Err(err(format!("{}.{prop}: an initial value is written once, so it takes a value, not a binding", c.ty), &c.source));
        }
        bound.insert(prop.clone(), id);
        if let Some(site) = crate::desc::site_id(v) {
            sites.insert(prop.clone(), site);
        }
        rest.props.remove(prop);
    }
    let mut r = resolve_unbound(&rest, ts, native)?;
    if bound.is_empty() {
        return Ok(r);
    }
    match &r.target {
        Target::Native { layout, .. } => {
            for f in bound.keys() {
                if layout.field(f).is_none() {
                    let known: Vec<&str> = layout.fields.iter().map(|f| f.name.as_str()).collect();
                    return Err(err(format!("{} has no field {f} (fields: {})", c.ty, known.join(", ")), &c.source));
                }
                r.defaults.remove(f);
            }
        }
        Target::Ui(kind) => {
            for f in bound.keys() {
                ui::check(*kind, &Map::from_iter([(f.clone(), Value::Null)]))
                    .or_else(|e| if e.contains("has no prop") { Err(e) } else { Ok(()) })
                    .map_err(|e| err(format!("<{}> {e}", c.ty), &c.source))?;
            }
        }
        _ => return Err(err(format!("<{}> props cannot be bound yet; bind a component field or a UI prop", c.ty), &c.source)),
    }
    r.bound = bound;
    r.sites = sites;
    Ok(r)
}

fn resolve_unbound(c: &ComponentDesc, ts: &dyn TsLookup, native: &NativeLayouts) -> Result<Resolved, ScriptError> {
    let resolved = |target, (controlled, initial, defaults)| Resolved {
        key: c.ty.clone(),
        target,
        controlled,
        initial,
        defaults,
        bound: BTreeMap::new(),
        sites: BTreeMap::new(),
        source: c.source.clone(),
    };
    match c.kind {
        ComponentKind::Ts => {
            let (id, layout, decl) =
                ts.ts(&c.ty).ok_or_else(|| err(format!("unknown component {}: no component() declares it", c.ty), &c.source))?;
            let fields: Vec<_> = decl.fields.iter().map(|f| (f.name.clone(), f.ty.clone(), f.default.clone())).collect();
            let defaults = value::encode_component(&decl, &layout, &Map::new()).map_err(|e| err(e.to_string(), &c.source))?;
            Ok(resolved(Target::Native { id, layout, defaults }, split_fields(c, &fields)?))
        }
        ComponentKind::Rust if c.ty == NAME => {
            let mut props = c.props.clone();
            let v = props.remove("value");
            if let Some(extra) = props.keys().next() {
                return Err(err(format!("Name has no prop {extra}; use <Name value=\"...\" />"), &c.source));
            }
            let Some(Value::String(s)) = v else { return Err(err("<Name> needs value=\"...\" (a string)", &c.source)) };
            Ok(resolved(Target::Name, (BTreeMap::from([("value".to_string(), Value::String(s))]), BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Rust => {
            let n = native.get(&c.ty).ok_or_else(|| {
                err(format!("<{}>: tsx sets Rust components with a native layout (Transform) and Name only", c.ty), &c.source)
            })?;
            let fields: Vec<_> = n.layout.fields.iter().map(|f| (f.name.clone(), f.ty.clone(), None)).collect();
            let defaults = if c.ty == "Transform" { transform_defaults(&n.layout) } else { vec![0; n.layout.size] };
            let mut split = split_fields(c, &fields)?;
            // Defaults read back from the default value, for provenance.
            for (f, v) in split.2.iter_mut() {
                let fl = n.layout.field(f).expect("layout field");
                *v = value::decode_field(&fl.ty, &defaults[fl.offset..fl.offset + fl.ty.size()]);
            }
            Ok(resolved(Target::Native { id: n.id, layout: n.layout.clone(), defaults }, split))
        }
        ComponentKind::Intrinsic if c.ty == MESH => {
            let props: MeshProps = parse_props(c)?;
            props.check().map_err(|m| err(format!("<mesh>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Mesh(props), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MATERIAL => {
            let props: MaterialProps = parse_props(c)?;
            props.check().map_err(|m| err(format!("<material>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Material(props), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic => {
            let kind = UiKind::from_tag(&c.ty).ok_or_else(|| err(format!("unknown intrinsic <{}>", c.ty), &c.source))?;
            ui::check(kind, &c.props).map_err(|m| err(format!("<{}> {m}", c.ty), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Ui(kind), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
    }
}

/// An entity's component elements, resolved, with `<entity name>` as a `Name` element.
pub fn resolve_node(node: &NodeDesc, ts: &dyn TsLookup, native: &NativeLayouts) -> Result<Vec<Resolved>, ScriptError> {
    let mut out = Vec::with_capacity(node.components.len() + 1);
    let mut seen = HashSet::new();
    if let Some(name) = &node.name {
        seen.insert(NAME.to_string());
        out.push(Resolved {
            key: NAME.into(),
            target: Target::Name,
            controlled: BTreeMap::from([("value".to_string(), Value::String(name.clone()))]),
            initial: BTreeMap::new(),
            bound: BTreeMap::new(),
            sites: BTreeMap::new(),
            defaults: BTreeMap::new(),
            source: node.location(),
        });
    }
    for c in &node.components {
        if !seen.insert(c.ty.clone()) {
            return Err(err(format!("{} is given twice on one entity", c.ty), &c.source));
        }
        out.push(resolve(c, ts, native)?);
    }
    // Mesh `color` and a `<material>` on one entity would both own the material.
    let mesh_color = out.iter().any(|r| matches!(&r.target, Target::Mesh(MeshProps { color: Some(_), .. })));
    if mesh_color && let Some(m) = out.iter().find(|r| matches!(r.target, Target::Material(_))) {
        return Err(err("give the colour on <material> or on <mesh color>, not both", &m.source));
    }
    Ok(out)
}

/// Checks a whole section: duplicate roots and sibling keys, and every component element.
pub fn check_roots(roots: &[RootDesc], ts: &dyn TsLookup, native: &NativeLayouts) -> Vec<ScriptError> {
    let mut errors = Vec::new();
    let mut names = HashSet::new();
    for r in roots {
        if !names.insert(r.name.as_str()) {
            errors.push(err(format!("scene {} is declared twice", r.name), &r.location));
        }
        check_nodes(&r.nodes, ts, native, &mut errors);
    }
    errors
}

pub fn check_nodes(nodes: &[NodeDesc], ts: &dyn TsLookup, native: &NativeLayouts, errors: &mut Vec<ScriptError>) {
    let mut keys = HashSet::new();
    for n in nodes {
        if !keys.insert(n.key.as_str()) {
            errors.push(err(format!("two siblings have the key {:?}", n.key), &n.location()));
        }
        if let Err(e) = resolve_node(n, ts, native) {
            errors.push(e);
        }
        check_nodes(&n.children, ts, native, errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_props_name_their_field() {
        assert_eq!(initial_field("initialHp").as_deref(), Some("hp"));
        assert_eq!(initial_field("initialMaxSpeed").as_deref(), Some("maxSpeed"));
        assert_eq!(initial_field("initial"), None);
        assert_eq!(initial_field("initialize"), None);
        assert_eq!(initial_field("hp"), None);
    }
}
