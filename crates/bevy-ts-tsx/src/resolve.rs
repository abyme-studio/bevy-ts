//! Component elements resolved against the components they name: which fields are controlled,
//! which initial, the bytes to insert, and the errors (with TS locations) that fail a load.

use crate::desc::{ComponentDesc, ComponentKind, NodeDesc, RootDesc, binding_id};
use crate::ui::{self, UiKind};
use crate::intrinsics::{Material2dProps, MaterialProps, Mesh2dProps, MeshProps};
use bevy::ecs::component::ComponentId;
use bevy::reflect::TypeRegistry;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::components::{PendingComponents, TsComponents};
use bevy_ts::layout::{ComponentLayout, FieldType, NativeLayouts};
use bevy_ts::manifest::ComponentDecl;
use bevy_ts::value;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

/// The name `<entity name>` and `<Name value>` resolve to.
pub const NAME: &str = "Name";
pub const MESH: &str = "mesh";
pub const MATERIAL: &str = "material";
pub const MESH2D: &str = "mesh2d";
pub const MATERIAL2D: &str = "material2d";

/// The asset intrinsics, whose props are rebuilt as a whole and cannot be bound.
pub fn asset_intrinsic(tag: &str) -> bool {
    [MESH, MATERIAL, MESH2D, MATERIAL2D].contains(&tag)
}

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
    /// A component with a native layout: a TS component or `Transform`. `defaults` is a Rust
    /// component's whole default value; a TS component's value is encoded from `decl` instead,
    /// so its heap fields (`string`) are built in Rust. `type_path` is a Rust component's, for
    /// its holes written through reflection.
    Native { id: ComponentId, layout: ComponentLayout, defaults: Vec<u8>, type_path: Option<String>, decl: Option<Box<bevy_ts::manifest::ComponentDecl>> },
    Name,
    Mesh(MeshProps),
    Material(Box<MaterialProps>),
    Mesh2d(Mesh2dProps),
    Material2d(Material2dProps),
    /// `<material kind={K} ...params />`: a script material instance of kind `kind`. On a glTF
    /// scene root, `gltfMaterial` limits it to the scene's meshes with that material name.
    ScriptMaterial { kind: String, props: Map<String, Value>, gltf_material: Option<String> },
    /// `<material asset={ref} />`: an owned script material, `Kind.add`'s ref number `asset`.
    OwnedMaterial { asset: u64, gltf_material: Option<String> },
    /// A reflect-tier Rust component, by type path: written field by field through reflection.
    Reflect { type_path: String },
    /// Bevy's state-scoped component of a TS state value (`<DespawnOnExit value={Game.Playing} />`),
    /// by kind (`bevy_ts::states::SCOPED`): the machine's slot is found on insert.
    StateScoped { kind: u8 },
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
    /// A layout-tier Rust component's holes given as props (`color`), written through reflection
    /// after the bytes, on insert and whenever their declared value changes.
    pub reflected: BTreeMap<String, Value>,
    /// Fields given neither way, with the default they take on insert.
    pub defaults: BTreeMap<String, Value>,
    /// Fields written only by a binding (`f={() => ...}`), by binding id.
    pub bound: BTreeMap<String, u32>,
    /// The site of each bound native-layout field (design decision 11).
    pub sites: BTreeMap<String, u32>,
    pub source: Option<SourceLocation>,
    /// Controlled fields a level file sets, with where (provenance `override`).
    pub overrides: BTreeMap<String, SourceLocation>,
}

impl Resolved {
    /// `override` for a field a level file sets, else `controlled`.
    pub fn controlled_provenance(&self, field: &str) -> crate::tree::Provenance {
        if self.overrides.contains_key(field) { crate::tree::Provenance::Override } else { crate::tree::Provenance::Controlled }
    }

    /// The value to insert: defaults, then initial and controlled values.
    pub fn insert_bytes(&self) -> Option<bevy_ts::heap::HeapBytes> {
        let Target::Native { layout, defaults, decl, .. } = &self.target else { return None };
        if let Some(decl) = decl {
            let values: Map<String, Value> = self.initial.iter().chain(self.controlled.iter()).map(|(k, v)| (k.clone(), v.clone())).collect();
            return Some(value::encode_component(decl, layout, &values).expect("checked values"));
        }
        let mut bytes = defaults.clone();
        for (f, v) in self.initial.iter().chain(self.controlled.iter()) {
            let fl = layout.field(f).expect("checked field");
            value::encode_field(f, &fl.ty, v, &mut bytes[fl.offset..fl.offset + fl.ty.size()]).expect("checked value");
        }
        Some(bevy_ts::heap::HeapBytes::plain(bytes))
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
            return Err(err(format!("{} has no field {prop} (fields: {})", c.ty, known.join(", ")), &at(c, prop)));
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
    value::encode_field(&format!("{}.{field}", c.ty), ty, v, &mut scratch).map_err(|e| err(e.to_string(), &at(c, field)))
}

/// Where a prop was set: the level line of an override, else the element.
fn at(c: &ComponentDesc, prop: &str) -> Option<SourceLocation> {
    c.overrides.get(prop).cloned().or_else(|| c.source.clone())
}

fn parse_props<T: serde::de::DeserializeOwned>(c: &ComponentDesc) -> Result<T, ScriptError> {
    serde_json::from_value(Value::Object(c.props.clone())).map_err(|e| err(format!("<{}>: {e}", c.ty), &c.source))
}

/// What `<material kind>` names: its kind, its params and textures, and its `gltfMaterial`.
pub struct ScriptMaterialDesc {
    pub kind: String,
    pub props: Map<String, Value>,
    pub gltf_material: Option<String>,
}

/// `<material kind={K} .../>`'s kind name (`K` describes as `{ name }`), its `gltfMaterial` and its
/// other props. Checked against the kind's schema by the tsx section's check, which sees the
/// generation's materials.
pub fn script_material(c: &ComponentDesc) -> Result<ScriptMaterialDesc, ScriptError> {
    let mut props = c.props.clone();
    let kind = match props.remove("kind") {
        Some(Value::String(s)) => s,
        Some(Value::Object(o)) => match o.get("name") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err(err("<material kind={...}> takes a kind from material() in bevy-ts/material", &c.source)),
        },
        _ => return Err(err("<material kind={...}> takes a kind from material() in bevy-ts/material", &c.source)),
    };
    let gltf_material = match props.remove("gltfMaterial") {
        None => None,
        Some(Value::String(s)) => Some(s),
        Some(_) => return Err(err("<material gltfMaterial> takes a glTF material name, a string", &c.source)),
    };
    Ok(ScriptMaterialDesc { kind, props, gltf_material })
}

/// `<material asset={ref} gltfMaterial? />`: the ref number of an owned script material
/// (`Kind.add`) and its `gltfMaterial`. Any other prop, or a ref of another asset type, fails.
pub fn owned_material(c: &ComponentDesc) -> Result<(u64, Option<String>), ScriptError> {
    let path = <bevy_ts_material::ScriptMaterial as bevy::reflect::TypePath>::type_path();
    if let Some(p) = c.props.keys().find(|p| !["asset", "gltfMaterial"].contains(&p.as_str())) {
        return Err(err(format!("<material asset> takes only asset and gltfMaterial, not {p}: the asset holds its params"), &c.source));
    }
    let v = &c.props["asset"];
    if binding_id(v).is_some() {
        return Err(err("<material asset> cannot be bound; assets.set the ref instead", &c.source));
    }
    let n = match bevy_ts::reflect_asset::parse_ref(v) {
        Some(Ok(n)) => n,
        Some(Err(m)) => return Err(err(format!("<material asset>: {m}"), &c.source)),
        None => return Err(err(format!("<material asset> takes a {path} ref from Kind.add, not {v}"), &c.source)),
    };
    match v.get("type").and_then(Value::as_str) {
        Some(t) if t == path => {}
        t => return Err(err(format!("<material asset>: asset ref {n} is a {}, not a {path}", t.unwrap_or("ref of no type")), &c.source)),
    }
    let gltf_material = match c.props.get("gltfMaterial") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(err("<material gltfMaterial> takes a glTF material name, a string", &c.source)),
    };
    Ok((n, gltf_material))
}

/// A one-field tuple struct's element (`<WorldAssetRoot value="..." />`): its field is `value`
/// (native-components; reflection writes it as `.0`), and no other prop exists. A level's patch
/// (`WorldAssetRoot("...")`) arrives as `0`, Bevy's reflection path, and is taken as `value`.
fn newtype_props(c: &ComponentDesc) -> Result<ComponentDesc, ScriptError> {
    let mut out = c.clone();
    out.props = Map::new();
    for (prop, v) in &c.props {
        let field = match prop.as_str() {
            "value" | "0" => "value",
            "initialValue" => "initialValue",
            _ => return Err(err(format!("{} takes `value`, not {prop}: it is a tuple struct of one field (<{} value={{...}} />)", c.ty, c.ty), &at(c, prop))),
        };
        out.props.insert(field.into(), v.clone());
    }
    if let Some(at) = out.overrides.remove("0") {
        out.overrides.insert("value".into(), at);
    }
    Ok(out)
}

/// Resolves one component element: bound props first, then the rest by kind.
pub fn resolve(c: &ComponentDesc, ts: &dyn TsLookup, native: &NativeLayouts, registry: &TypeRegistry) -> Result<Resolved, ScriptError> {
    let newtype;
    let c = if c.kind == ComponentKind::Rust && c.ty != NAME && native.is_newtype(&c.ty) {
        newtype = newtype_props(c)?;
        &newtype
    } else {
        c
    };
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
    let mut r = resolve_unbound(&rest, ts, native, registry)?;
    if bound.is_empty() {
        return Ok(r);
    }
    match &r.target {
        Target::Native { layout, .. } => {
            for f in bound.keys() {
                // Only a layout-tier hole refuses a binding; a reflect-tier prop binds (below).
                if native.get(&c.ty).is_some_and(|n| n.reflect_holes.iter().any(|h| &h.name == f)) {
                    return Err(err(format!("{}.{f} is written through reflection, so it takes a value, not a binding", c.ty), &at(c, f)));
                }
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
        // A reflect-tier prop binds: each value is checked against the type by reflection as it
        // arrives and written over the current value.
        Target::Reflect { .. } => {}
        // Instance params may be bound; the tsx check, which sees the kinds, fails any other.
        Target::ScriptMaterial { .. } => {}
        _ => return Err(err(format!("<{}> props cannot be bound yet; bind a component field or a UI prop", c.ty), &c.source)),
    }
    r.bound = bound;
    r.sites = sites;
    Ok(r)
}

/// Checks props written through reflection against a default of the component.
fn check_reflected(c: &ComponentDesc, registry: &TypeRegistry, type_path: &str, props: &Map<String, Value>) -> Result<(), ScriptError> {
    crate::reflected::check(registry, type_path, props).map_err(|m| {
        let prop = props.keys().find(|p| m.contains(&format!(".{p}:"))).map_or("", |p| p.as_str());
        err(m, &at(c, prop))
    })
}

fn resolve_unbound(c: &ComponentDesc, ts: &dyn TsLookup, native: &NativeLayouts, registry: &TypeRegistry) -> Result<Resolved, ScriptError> {
    let resolved = |target, (controlled, initial, defaults)| Resolved {
        key: c.ty.clone(),
        target,
        controlled,
        initial,
        reflected: BTreeMap::new(),
        defaults,
        bound: BTreeMap::new(),
        sites: BTreeMap::new(),
        source: c.source.clone(),
        overrides: c.overrides.clone(),
    };
    match c.kind {
        ComponentKind::Ts => {
            let (id, layout, decl) =
                ts.ts(&c.ty).ok_or_else(|| err(format!("unknown component {}: no component() declares it", c.ty), &c.source))?;
            let fields: Vec<_> = decl.fields.iter().map(|f| (f.name.clone(), f.ty.clone(), f.default.clone())).collect();
            value::encode_component(&decl, &layout, &Map::new()).map_err(|e| err(e.to_string(), &c.source))?;
            Ok(resolved(Target::Native { id, layout, defaults: Vec::new(), type_path: None, decl: Some(Box::new(decl)) }, split_fields(c, &fields)?))
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
        ComponentKind::Rust if bevy_ts::states::scoped_kind(&c.ty).is_some() => {
            let kind = bevy_ts::states::scoped_kind(&c.ty).expect("checked");
            // TSX gives the state value as `{ state, value }`; a level as "game::Game.Playing".
            let v = match c.props.get("value") {
                Some(Value::String(s)) => s.rsplit_once('.').map(|(state, value)| serde_json::json!({ "state": state, "value": value })),
                Some(v) if v["state"].is_string() && v["value"].is_string() => Some(v.clone()),
                _ => None,
            };
            let Some(v) = v.filter(|_| c.props.len() == 1) else {
                return Err(err(format!("<{}> takes one prop, value: a state value (<{} value={{Game.Playing}} />, or \"game::Game.Playing\" in a level)", c.ty, c.ty), &c.source));
            };
            let controlled = BTreeMap::from([("value".to_string(), v)]);
            Ok(resolved(Target::StateScoped { kind }, (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Rust if reflect_path(native, &c.ty).is_some() => {
            let type_path = reflect_path(native, &c.ty).expect("checked");
            // `initialF` writes field `f` on insert only; reflection checks the field then.
            let (mut controlled, mut initial) = (BTreeMap::new(), BTreeMap::new());
            for (k, v) in &c.props {
                match initial_field(k) {
                    Some(f) => initial.insert(f, v.clone()),
                    None => controlled.insert(k.clone(), v.clone()),
                };
            }
            if let Some(f) = initial.keys().find(|f| controlled.contains_key(*f)) {
                return Err(err(format!("{}.{f} is given both as {f} and its initial form; pick one", c.ty), &c.source));
            }
            let written: Map<String, Value> = initial.iter().chain(controlled.iter()).map(|(k, v)| (k.clone(), v.clone())).collect();
            check_reflected(c, registry, &type_path, &written)?;
            Ok(resolved(Target::Reflect { type_path }, (controlled, initial, BTreeMap::new())))
        }
        ComponentKind::Rust => {
            let n = native.get(&c.ty).ok_or_else(|| {
                err(format!("<{}>: tsx sets Rust components with a native layout (Transform) and Name only", c.ty), &c.source)
            })?;
            let fields: Vec<_> = n.layout.fields.iter().map(|f| (f.name.clone(), f.ty.clone(), None)).collect();
            let defaults = n.defaults.clone();
            // Holes reflection writes are props of their own; an opaque hole is never set.
            let mut layout_props = c.clone();
            let mut reflected = Map::new();
            for (prop, v) in &c.props {
                if n.reflect_holes.iter().any(|h| &h.name == prop) {
                    reflected.insert(prop.clone(), v.clone());
                    layout_props.props.remove(prop);
                } else if let Some(why) = n.holes.iter().find_map(|h| h.strip_prefix(&format!("{prop}: "))) {
                    return Err(err(format!("{}.{prop} is {why}, so TSX cannot set it", c.ty), &at(c, prop)));
                }
            }
            check_reflected(c, registry, &n.type_path, &reflected)?;
            let mut split = split_fields(&layout_props, &fields)?;
            // Defaults read back from the default value, for provenance.
            for (f, v) in split.2.iter_mut() {
                let fl = n.layout.field(f).expect("layout field");
                // SAFETY: a native layout has no heap field.
                *v = unsafe { value::decode_field(&fl.ty, &defaults[fl.offset..fl.offset + fl.ty.size()]) };
            }
            let mut r = resolved(Target::Native { id: n.id, layout: n.layout.clone(), defaults, type_path: Some(n.type_path.clone()), decl: None }, split);
            r.reflected = reflected.into_iter().collect();
            Ok(r)
        }
        ComponentKind::Intrinsic if c.ty == MESH => {
            let props: MeshProps = parse_props(c)?;
            props.check().map_err(|m| err(format!("<mesh>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Mesh(props), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MATERIAL && c.props.contains_key("asset") => {
            let (asset, gltf_material) = owned_material(c)?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::OwnedMaterial { asset, gltf_material }, (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MATERIAL && c.props.contains_key("kind") => {
            let ScriptMaterialDesc { kind, props, gltf_material } = script_material(c)?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::ScriptMaterial { kind, props, gltf_material }, (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MATERIAL => {
            let props: MaterialProps = parse_props(c)?;
            props.check().map_err(|m| err(format!("<material>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Material(Box::new(props)), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MESH2D => {
            let props: Mesh2dProps = parse_props(c)?;
            props.0.check().map_err(|m| err(format!("<mesh2d>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Mesh2d(props), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic if c.ty == MATERIAL2D => {
            let props: Material2dProps = parse_props(c)?;
            props.check().map_err(|m| err(format!("<material2d>: {m}"), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Material2d(props), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
        ComponentKind::Intrinsic => {
            let kind = UiKind::from_tag(&c.ty).ok_or_else(|| err(format!("unknown intrinsic <{}>", c.ty), &c.source))?;
            ui::check(kind, &c.props).map_err(|m| err(format!("<{}> {m}", c.ty), &c.source))?;
            let controlled = c.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(resolved(Target::Ui(kind), (controlled, BTreeMap::new(), BTreeMap::new())))
        }
    }
}

/// The type path a Rust component is written by reflection under: reflect-tier components, and
/// layout-tier ones whose holes may own heap data (a handle), which are never inserted from bytes.
pub fn reflect_path(native: &NativeLayouts, ty: &str) -> Option<String> {
    match native.get(ty) {
        Some(n) => (!n.insertable).then(|| n.type_path.clone()),
        None => native.reflect_tier(ty).map(|r| r.type_path.clone()),
    }
}

/// An entity's component elements, resolved, with `<entity name>` as a `Name` element.
pub fn resolve_node(node: &NodeDesc, ts: &dyn TsLookup, native: &NativeLayouts, registry: &TypeRegistry) -> Result<Vec<Resolved>, ScriptError> {
    let mut out = Vec::with_capacity(node.components.len() + 1);
    let mut seen = HashSet::new();
    if let Some(name) = &node.name {
        seen.insert(NAME.to_string());
        out.push(Resolved {
            key: NAME.into(),
            target: Target::Name,
            controlled: BTreeMap::from([("value".to_string(), Value::String(name.clone()))]),
            initial: BTreeMap::new(),
            reflected: BTreeMap::new(),
            bound: BTreeMap::new(),
            sites: BTreeMap::new(),
            defaults: BTreeMap::new(),
            source: node.location(),
            overrides: BTreeMap::new(),
        });
    }
    for c in &node.components {
        if !seen.insert(c.ty.clone()) {
            return Err(err(format!("{} is given twice on one entity", c.ty), &c.source));
        }
        out.push(resolve(c, ts, native, registry)?);
    }
    // Mesh `color` and a `<material>` on one entity would both own the material.
    let mesh_color = out.iter().any(|r| matches!(&r.target, Target::Mesh(MeshProps { color: Some(_), .. })));
    if mesh_color && let Some(m) = out.iter().find(|r| matches!(r.target, Target::Material(_) | Target::ScriptMaterial { .. } | Target::OwnedMaterial { .. })) {
        return Err(err("give the colour on <material> or on <mesh color>, not both", &m.source));
    }
    let mesh2d_color = out.iter().any(|r| matches!(&r.target, Target::Mesh2d(Mesh2dProps(MeshProps { color: Some(_), .. }))));
    if mesh2d_color && let Some(m) = out.iter().find(|r| matches!(r.target, Target::Material2d(_))) {
        return Err(err("give the colour on <material2d> or on <mesh2d color>, not both", &m.source));
    }
    // Bevy keeps 2D and 3D apart: a `Mesh2d` draws only with a 2D material, a `Mesh3d` with a 3D one.
    let is_3d = |r: &&Resolved| matches!(r.target, Target::Mesh(_) | Target::Material(_) | Target::ScriptMaterial { .. });
    let is_2d = |r: &&Resolved| matches!(r.target, Target::Mesh2d(_) | Target::Material2d(_));
    if let (Some(a), Some(b)) = (out.iter().find(is_3d), out.iter().find(is_2d)) {
        return Err(err(format!("<{}> and <{}> on one entity: pair <mesh> with <material>, <mesh2d> with <material2d>", a.key, b.key), &b.source));
    }
    // World text owns its entity's mesh and material.
    let world_text = out.iter().any(|r| matches!(r.target, Target::Ui(UiKind::WorldText)));
    if world_text && let Some(m) = out.iter().find(|r| matches!(r.target, Target::Mesh(_) | Target::Material(_) | Target::ScriptMaterial { .. } | Target::OwnedMaterial { .. })) {
        return Err(err(format!("<worldText> and <{}> on one entity: a label draws its own mesh and material; give it an entity of its own", m.key), &node.location()));
    }
    Ok(out)
}

/// Checks a whole section: duplicate roots and sibling keys, and every component element.
pub fn check_roots(roots: &[RootDesc], ts: &dyn TsLookup, native: &NativeLayouts, registry: &TypeRegistry) -> Vec<ScriptError> {
    let mut errors = Vec::new();
    let mut names = HashSet::new();
    for r in roots {
        if !names.insert(r.name.as_str()) {
            errors.push(err(format!("scene {} is declared twice", r.name), &r.location));
        }
        check_nodes(&r.nodes, ts, native, registry, &mut errors);
    }
    errors
}

pub fn check_nodes(nodes: &[NodeDesc], ts: &dyn TsLookup, native: &NativeLayouts, registry: &TypeRegistry, errors: &mut Vec<ScriptError>) {
    let mut keys = HashSet::new();
    for n in nodes {
        if !keys.insert(n.key.as_str()) {
            errors.push(err(format!("two siblings have the key {:?}", n.key), &n.location()));
        }
        if let Err(e) = resolve_node(n, ts, native, registry) {
            errors.push(e);
        }
        check_nodes(&n.children, ts, native, registry, errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoTs;

    impl TsLookup for NoTs {
        fn ts(&self, _: &str) -> Option<(ComponentId, ComponentLayout, ComponentDecl)> {
            None
        }
    }

    fn node(components: Value) -> NodeDesc {
        serde_json::from_value(serde_json::json!({ "key": "e", "type": "entity", "source": [{ "file": "game.tsx", "line": 7, "column": 3 }], "components": components })).unwrap()
    }

    #[test]
    fn world_text_beside_a_mesh_or_material_names_both() {
        let owned = <bevy_ts_material::ScriptMaterial as bevy::reflect::TypePath>::type_path();
        for other in [
            serde_json::json!({ "type": "mesh", "kind": "intrinsic", "props": { "shape": "cuboid" } }),
            serde_json::json!({ "type": "material", "kind": "intrinsic", "props": {} }),
            serde_json::json!({ "type": "material", "kind": "intrinsic", "props": { "asset": { "$asset": 1, "type": owned } } }),
        ] {
            let n = node(serde_json::json!([other, { "type": "worldText", "kind": "intrinsic", "props": { "text": "A" } }]));
            let e = resolve_node(&n, &NoTs, &NativeLayouts::default(), &TypeRegistry::default()).unwrap_err();
            let ty = other["type"].as_str().unwrap();
            assert!(e.message.contains("<worldText>") && e.message.contains(&format!("<{ty}>")), "{}", e.message);
            assert_eq!(e.location.as_ref().map(|l| l.line), Some(7));
        }
        let alone = node(serde_json::json!([{ "type": "worldText", "kind": "intrinsic", "props": { "text": "A", "size": 0.5 } }]));
        assert!(resolve_node(&alone, &NoTs, &NativeLayouts::default(), &TypeRegistry::default()).is_ok());
    }

    #[test]
    fn initial_props_name_their_field() {
        assert_eq!(initial_field("initialHp").as_deref(), Some("hp"));
        assert_eq!(initial_field("initialMaxSpeed").as_deref(), Some("maxSpeed"));
        assert_eq!(initial_field("initial"), None);
        assert_eq!(initial_field("initialize"), None);
        assert_eq!(initial_field("hp"), None);
    }
}
