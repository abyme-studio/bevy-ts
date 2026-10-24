//! Reflected assets for scripts and TSX: every type registered with `ReflectAsset`, built, read
//! and edited through the one reflected-value writer and reader (`reflect_value`).
//!
//! Scripts call in while a system runs or TSX renders, when the world is only `&World`, so an
//! asset cannot enter `Assets<T>` at the call. Instead each script-made asset gets a UUID handle at
//! once (`UntypedHandle::Uuid`, no world needed), its value is checked and queued in
//! [`ScriptAssets`], and [`flush`] inserts it with `&mut World` when the run ends, before the run's
//! commands apply. `get` reads a queued value, so a script sees its `add` and `set` at once.
//!
//! A ref is `{ "$asset": n, "type": path }` in JSON, `n` counting up from 1 and never reused in a
//! session. Script-made assets live until the script reloads or stops ([`clear`] removes them);
//! a ref used after that is a "stale asset ref". Asset values (`T.of(props)`,
//! `{ "$of": path, "props": ... }`) are cached by their type and the props' [`canonical`] JSON, so
//! every slot given equal props shares one asset.

use bevy::asset::uuid::Uuid;
use crate::reflect_value;
use bevy::asset::{AssetServer, ReflectAsset, UntypedAssetId, UntypedHandle};
use bevy::prelude::*;
use bevy::reflect::{PartialReflect, ReflectFromReflect, TypeRegistry};
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

/// One session's assets past this many log a warning: `assets.add` in a per-frame system grows
/// memory until reload.
const REFS_WARN: usize = 10_000;

/// The high half of every script-made asset's UUID; the low half is its ref number.
const UUID_TAG: u64 = 0x6265_7679_2d74_7341; // "bevy-tsA"

/// An asset type found by its type path.
#[derive(Clone)]
pub struct AssetType {
    /// The asset's own type (`StandardMaterial`), not its handle's.
    pub id: TypeId,
    pub path: &'static str,
    pub reflect: ReflectAsset,
}

/// The asset type registered as `type_path`, refused when it is not registered or not an asset.
pub fn asset_type(registry: &TypeRegistry, type_path: &str) -> Result<AssetType, String> {
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not a registered type"))?;
    asset_type_of(registry, reg.type_id()).ok_or_else(|| format!("{type_path} is not a reflected asset"))
}

fn asset_type_of(registry: &TypeRegistry, id: TypeId) -> Option<AssetType> {
    let reg = registry.get(id)?;
    let reflect = reg.data::<ReflectAsset>()?.clone();
    Some(AssetType { id, path: reg.type_info().type_path(), reflect })
}

/// The type path of an asset type, for messages.
fn path_of(registry: &TypeRegistry, id: TypeId) -> String {
    registry.get(id).map_or_else(|| format!("{id:?}"), |r| r.type_info().type_path().to_string())
}

/// One script-made asset.
struct Made {
    handle: UntypedHandle,
    /// Its asset type, to remove it on [`clear`].
    reflect: ReflectAsset,
}

/// A value waiting for [`flush`]: a new asset, or a changed one.
struct Queued {
    added: bool,
    value: Box<dyn Reflect>,
}

#[derive(Default)]
struct Table {
    /// The last ref given; refs below it that are not in `refs` are stale.
    last: u64,
    refs: HashMap<u64, Made>,
    by_id: HashMap<UntypedAssetId, u64>,
    values: HashMap<(TypeId, String), u64>,
    queued: BTreeMap<u64, Queued>,
    /// The JSON each hand-mapped asset a script made was last written from, for entries whose
    /// read gives it back (`AnimationClip`).
    sources: HashMap<UntypedAssetId, Value>,
    /// Assets of refs a reload made stale, kept for the entities that still use them until the
    /// next restart or stop.
    retired: Vec<Made>,
    warned: bool,
}

/// The ref table: every asset scripts made this session, by ref number and by asset id, the
/// asset-value cache, and the values waiting for [`flush`]. Behind a lock: scripts reach it
/// through `&World` during a run.
#[derive(Resource, Default, Clone)]
pub struct ScriptAssets(Arc<Mutex<Table>>);

impl ScriptAssets {
    fn table(&self) -> MutexGuard<'_, Table> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A new ref for an asset of `ty`, with its UUID handle; its value is queued as added.
    fn make(&self, ty: &AssetType, value: Box<dyn Reflect>) -> (u64, UntypedHandle) {
        let mut t = self.table();
        t.last += 1;
        let n = t.last;
        let handle = UntypedHandle::Uuid { type_id: ty.id, uuid: Uuid::from_u64_pair(UUID_TAG, n) };
        t.by_id.insert(handle.id(), n);
        t.refs.insert(n, Made { handle: handle.clone(), reflect: ty.reflect.clone() });
        t.queued.insert(n, Queued { added: true, value });
        if t.refs.len() > REFS_WARN && !t.warned {
            t.warned = true;
            warn!("bevy-ts: scripts hold {} assets this session; add an asset once and assets.set it after", t.refs.len());
        }
        (n, handle)
    }

    /// The handle of ref `n`.
    pub fn handle(&self, n: u64) -> Result<UntypedHandle, String> {
        let t = self.table();
        match t.refs.get(&n) {
            Some(m) => Ok(m.handle.clone()),
            None if n >= 1 && n <= t.last => Err(format!("stale asset ref {n}: assets live until the script reloads or stops")),
            None => Err(format!("unknown asset ref {n}")),
        }
    }

    /// The handle of ref `n` for a `Handle<T>` slot of asset type `asset`; a ref of another asset
    /// type fails naming both.
    pub fn handle_for(&self, registry: &TypeRegistry, asset: TypeId, n: u64) -> Result<UntypedHandle, String> {
        let h = self.handle(n)?;
        if h.type_id() != asset {
            return Err(format!("asset ref {n} is a {}, not a {}", path_of(registry, h.type_id()), path_of(registry, asset)));
        }
        Ok(h)
    }

    /// The ref of an asset a script made, if any.
    pub fn ref_of(&self, id: UntypedAssetId) -> Option<u64> {
        self.table().by_id.get(&id).copied()
    }

    /// The ref of the cached value of `asset` with these canonical props.
    pub fn cached(&self, asset: TypeId, key: &str) -> Option<u64> {
        self.table().values.get(&(asset, key.to_string())).copied()
    }

    /// Caches ref `n` as the value of `asset` with these canonical props.
    pub fn cache(&self, asset: TypeId, key: String, n: u64) {
        self.table().values.insert((asset, key), n);
    }

    /// The value of ref `n` waiting for [`flush`], cloned, if any.
    fn queued_value(&self, n: u64) -> Option<Box<dyn PartialReflect>> {
        self.table().queued.get(&n).and_then(|q| q.value.reflect_clone().ok()).map(|v| v.into_partial_reflect())
    }

    /// Queues `value` as ref `n`'s new value; a queued add stays an add.
    fn queue_set(&self, n: u64, value: Box<dyn Reflect>) {
        let mut t = self.table();
        let added = t.queued.get(&n).is_some_and(|q| q.added);
        t.queued.insert(n, Queued { added, value });
    }

    /// The JSON a hand-mapped asset was last written from, if a script wrote it.
    pub fn source(&self, id: UntypedAssetId) -> Option<Value> {
        self.table().sources.get(&id).cloned()
    }

    fn set_source(&self, id: UntypedAssetId, json: Value) {
        self.table().sources.insert(id, json);
    }

    /// How many assets the table holds.
    pub fn len(&self) -> usize {
        self.table().refs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Puts every queued value in its `Assets<T>`: an add inserted at its UUID, a set applied once.
/// Runs after each system run and entry call, before its commands apply.
pub fn flush(world: &mut World) {
    let Some(assets) = world.get_resource::<ScriptAssets>() else { return };
    let queued: Vec<(Queued, UntypedHandle, ReflectAsset)> = {
        let mut t = assets.table();
        let queued = std::mem::take(&mut t.queued);
        queued.into_iter().filter_map(|(n, q)| t.refs.get(&n).map(|m| (q, m.handle.clone(), m.reflect.clone()))).collect()
    };
    for (q, handle, reflect) in queued {
        if q.added {
            if let Err(e) = reflect.insert(world, handle.id(), q.value.as_partial_reflect()) {
                warn!("bevy-ts: a script-made asset was not added: {e}");
            }
        } else if let Some(asset) = reflect.get_mut(world, handle.id()) {
            asset.apply(q.value.as_partial_reflect());
        }
    }
}

/// Empties the table when the script reloads or stops: refs given before are stale from now on.
/// A restart or stop (`remove`) also removes every script-made asset; a hot reload keeps them for
/// the entities it keeps, until the next restart.
pub fn clear(world: &mut World, remove: bool) {
    let Some(assets) = world.get_resource::<ScriptAssets>() else { return };
    let gone: Vec<Made> = {
        let mut t = assets.table();
        t.by_id.clear();
        t.values.clear();
        t.queued.clear();
        t.sources.clear();
        t.warned = false;
        let made: Vec<Made> = t.refs.drain().map(|(_, m)| m).collect();
        t.retired.extend(made);
        if remove { std::mem::take(&mut t.retired) } else { Vec::new() }
    };
    for m in gone {
        m.reflect.remove(world, m.handle.id());
    }
}

/// A JSON host call from a script: `assets.add`, `assets.get` or `assets.set`, `args` a JSON array.
pub fn host_call(world: &World, name: &str, args: &str) -> Result<String, String> {
    let args: Vec<Value> = serde_json::from_str(args).map_err(|e| format!("{name}: {e}"))?;
    let arg = |i: usize| args.get(i).unwrap_or(&Value::Null);
    let assets = world.get_resource::<ScriptAssets>().ok_or_else(|| "assets need BevyTsPlugin".to_string())?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let slots = Slots { registry: &registry, server: world.get_resource::<AssetServer>(), assets, check: false };
    let out = match name {
        "assets.add" => {
            let path = arg(0).as_str().ok_or_else(|| format!("assets.add: the type is a type path, not {}", arg(0)))?;
            add(&slots, path, arg(1))?
        }
        "assets.get" => get(world, &slots, arg(0))?,
        "assets.set" => {
            set(world, &slots, arg(0), arg(1))?;
            Value::Null
        }
        _ => return Err(format!("no host service {name}")),
    };
    Ok(out.to_string())
}

/// What writing and reading assets reach, and the `Handle<T>` hook `reflect_value` calls for refs
/// and asset values. Borrows no `World`, so TSX can hold one while it writes a component in place.
/// In `check` mode nothing is made: a ref the table does not know is checked by its form and named
/// type, values are built and dropped.
pub struct Slots<'a> {
    pub registry: &'a TypeRegistry,
    pub server: Option<&'a AssetServer>,
    pub assets: &'a ScriptAssets,
    pub check: bool,
}

impl Slots<'_> {
    /// The writer's context with this hook.
    pub fn cx(&self) -> reflect_value::Cx<'_> {
        reflect_value::Cx { registry: self.registry, server: self.server, handles: Some(self) }
    }

    /// `json` written over `base` (the type's default when `None`), converted to the concrete
    /// type. A value that fails any step builds nothing.
    fn build(&self, ty: &AssetType, base: Option<&dyn PartialReflect>, json: &Value) -> Result<Box<dyn Reflect>, String> {
        if let Some(h) = hand_map(ty.id) {
            return (h.write)(self, base, json);
        }
        let mut value = match base {
            Some(b) => b.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect(),
            None => default_value(self.registry, ty)?,
        };
        reflect_value::write_with(self.cx(), value.as_mut(), None, json).map_err(|e| format!("{}: {e}", ty.path))?;
        let from = self.registry.get_type_data::<ReflectFromReflect>(ty.id).ok_or_else(|| format!("{} is not FromReflect", ty.path))?;
        from.from_reflect(value.as_ref()).ok_or_else(|| format!("{}: the written value is not a whole {}", ty.path, ty.path))
    }

    /// An asset as JSON: its hand-map's read (given the JSON a script last wrote it from), else
    /// the reflected reader with this hook.
    fn read(&self, ty: &AssetType, id: UntypedAssetId, value: &dyn PartialReflect) -> Result<Value, String> {
        match hand_map(ty.id) {
            Some(h) => (h.read)(self, self.assets.source(id).as_ref(), value),
            None => reflect_value::read_with(self.registry, Some(self), value),
        }
    }

    /// `T.of(props)` in a slot of asset type `asset`: the cached asset with these props, else a new
    /// one, cached.
    fn value(&self, asset: TypeId, type_path: &Value, props: &Value) -> Result<UntypedHandle, String> {
        let path = type_path.as_str().ok_or_else(|| format!("an asset value's $of is a type path, not {type_path}"))?;
        let ty = asset_type(self.registry, path)?;
        if ty.id != asset {
            return Err(format!("{}.of(...) is not a {}", ty.path, path_of(self.registry, asset)));
        }
        let key = canonical(props);
        if let Some(n) = self.assets.cached(ty.id, &key) {
            return self.assets.handle(n);
        }
        let value = self.build(&ty, None, props)?;
        if self.check {
            return Ok(UntypedHandle::Uuid { type_id: ty.id, uuid: Uuid::nil() });
        }
        let (n, handle) = self.assets.make(&ty, value);
        if hand_map(ty.id).is_some() {
            self.assets.set_source(handle.id(), props.clone());
        }
        self.assets.cache(ty.id, key, n);
        Ok(handle)
    }
}

impl reflect_value::HandleSlots for Slots<'_> {
    fn write(&self, asset: TypeId, json: &Value) -> Result<UntypedHandle, String> {
        let Some(o) = json.as_object() else {
            return Err(format!("expected an asset path, an asset ref or T.of(...), got {json}"));
        };
        if let Some(n) = parse_ref(json) {
            let n = n?;
            if self.check && self.assets.handle(n).is_err() {
                // A check before the run that makes it: the ref's form and its named type.
                let ty = o.get("type").and_then(Value::as_str).ok_or_else(|| format!("asset ref {n} names no type"))?;
                let ty = asset_type(self.registry, ty)?;
                if ty.id != asset {
                    return Err(format!("asset ref {n} is a {}, not a {}", ty.path, path_of(self.registry, asset)));
                }
                return Ok(UntypedHandle::Uuid { type_id: asset, uuid: Uuid::nil() });
            }
            return self.assets.handle_for(self.registry, asset, n);
        }
        if let Some(path) = o.get("$of") {
            return self.value(asset, path, o.get("props").unwrap_or(&Value::Null));
        }
        if o.contains_key("$load") {
            return Err(format!("{} has no loader settings", path_of(self.registry, asset)));
        }
        Err(format!("expected an asset path, an asset ref or T.of(...), got {json}"))
    }

    fn read(&self, handle: &UntypedHandle) -> Option<Value> {
        let n = self.assets.ref_of(handle.id())?;
        Some(ref_json(n, &path_of(self.registry, handle.type_id())))
    }
}

/// `assets.add(Type, props)`: `props` written over the type's default, converted, then made.
pub fn add(slots: &Slots, type_path: &str, props: &Value) -> Result<Value, String> {
    let ty = asset_type(slots.registry, type_path)?;
    let value = slots.build(&ty, None, props)?;
    let (n, handle) = slots.assets.make(&ty, value);
    if hand_map(ty.id).is_some() {
        slots.assets.set_source(handle.id(), props.clone());
    }
    Ok(ref_json(n, ty.path))
}

/// `assets.get(ref)`: the asset as JSON, its queued value if it has one; `null` when it is gone.
pub fn get(world: &World, slots: &Slots, r: &Value) -> Result<Value, String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.get takes an asset ref, not {r}")))?;
    let handle = slots.assets.handle(n)?;
    let ty = asset_type_of(slots.registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    if let Some(v) = slots.assets.queued_value(n) {
        return slots.read(&ty, handle.id(), v.as_ref());
    }
    match ty.reflect.get(world, handle.id()) {
        Some(v) => slots.read(&ty, handle.id(), v.as_partial_reflect()),
        None => Ok(Value::Null),
    }
}

/// `assets.set(ref, patch)`: `patch` written over the asset's current value; queued only when the
/// result differs, so a no-op set marks nothing changed.
pub fn set(world: &World, slots: &Slots, r: &Value, patch: &Value) -> Result<(), String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.set takes an asset ref, not {r}")))?;
    let handle = slots.assets.handle(n)?;
    let ty = asset_type_of(slots.registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    let current = match slots.assets.queued_value(n) {
        Some(v) => v,
        None => {
            let v = ty.reflect.get(world, handle.id()).ok_or_else(|| format!("asset ref {n}: the asset is gone"))?;
            v.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect()
        }
    };
    let next = match hand_map(ty.id) {
        // A hand-mapped type: the patch replaces the keys it gives in the current read, then the
        // whole is written again; unchanged when the read is.
        Some(h) => {
            let source = slots.assets.source(handle.id());
            let read = (h.read)(slots, source.as_ref(), current.as_ref())?;
            let mut merged = read.clone();
            match (merged.as_object_mut(), patch) {
                (Some(m), Value::Object(p)) => m.extend(p.iter().map(|(k, v)| (k.clone(), v.clone()))),
                _ => return Err(format!("assets.set on a {} takes an object, not {patch}", ty.path)),
            }
            let next = (h.write)(slots, Some(current.as_ref()), &merged)?;
            if (h.read)(slots, Some(&merged), next.as_partial_reflect())? == read {
                return Ok(());
            }
            slots.assets.set_source(handle.id(), merged);
            next
        }
        None => {
            let next = slots.build(&ty, Some(current.as_ref()), patch)?;
            if current.reflect_partial_eq(next.as_partial_reflect()) == Some(true) {
                return Ok(());
            }
            next
        }
    };
    slots.assets.queue_set(n, next);
    Ok(())
}

/// An asset type reflection cannot reach whole (`Image` is opaque; `AnimationGraph`'s graph has no
/// reflected form), read and written by typed code instead of `reflect_value`.
pub struct HandMap {
    /// The asset's own type.
    pub asset: fn() -> TypeId,
    /// The asset as JSON; the second argument is the JSON a script last wrote it from, `None` for
    /// an asset no script wrote (a loaded file).
    pub read: fn(&Slots, Option<&Value>, &dyn PartialReflect) -> Result<Value, String>,
    /// The asset from JSON: over `base` when given, else whole (a type with no default builds
    /// from the JSON alone). The asset server loads; nothing else changes.
    pub write: fn(&Slots, Option<&dyn PartialReflect>, &Value) -> Result<Box<dyn Reflect>, String>,
}

/// The hand-mapped asset types. `assets.set` on one merges its patch over the current read at the
/// top level: a key given replaces that key whole.
pub const HAND_MAPS: &[HandMap] = &[];

fn hand_map(asset: TypeId) -> Option<&'static HandMap> {
    HAND_MAPS.iter().find(|h| (h.asset)() == asset)
}

fn default_value(registry: &TypeRegistry, ty: &AssetType) -> Result<Box<dyn PartialReflect>, String> {
    registry
        .get_type_data::<ReflectDefault>(ty.id)
        .map(|d| d.default().into_partial_reflect())
        .ok_or_else(|| format!("{} has no default: assets.add cannot build it", ty.path))
}

/// A ref as JSON: `{ "$asset": n, "type": path }`.
pub fn ref_json(n: u64, type_path: &str) -> Value {
    serde_json::json!({ "$asset": n, "type": type_path })
}

/// The ref number of `{ "$asset": n, ... }`; `None` for any other JSON.
pub fn parse_ref(json: &Value) -> Option<Result<u64, String>> {
    let n = json.as_object()?.get("$asset")?;
    Some(n.as_u64().filter(|&n| n >= 1).ok_or_else(|| format!("an asset ref's $asset is a whole number from 1, not {n}")))
}

/// `json` as text with every object's keys sorted, so equal props give equal keys whatever order
/// they were written in.
pub fn canonical(json: &Value) -> String {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&o[k]))).collect::<Map<_, _>>())
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            v => v.clone(),
        }
    }
    sorted(json).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::ecs::message::Messages;
    use bevy::image::Image;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>().register_asset_reflect::<Image>();
        app.init_asset::<Mesh>().register_asset_reflect::<Mesh>();
        app.register_type::<Name>();
        app.init_resource::<ScriptAssets>();
        app
    }

    fn image(app: &App) -> AssetType {
        asset_type(&app.world().resource::<AppTypeRegistry>().read(), "bevy_image::image::Image").expect("Image is an asset")
    }

    #[test]
    fn finds_an_asset_type_by_path() {
        let app = app();
        assert_eq!(image(&app).id, TypeId::of::<Image>());
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(asset_type(&registry, "bevy_ts::Nope").map(|_| ()).unwrap_err().contains("not a registered type"));
        let not_asset = asset_type(&registry, "bevy_ecs::name::Name").map(|_| ()).unwrap_err();
        assert!(not_asset.contains("not a reflected asset"), "{not_asset}");
    }

    #[test]
    fn made_assets_enter_assets_on_flush_and_leave_on_clear() {
        let mut app = app();
        let ty = image(&app);
        let (n, h) = app.world().resource::<ScriptAssets>().make(&ty, Box::new(Image::default()));
        assert_eq!(n, 1);
        let id = h.id().typed::<Image>();
        assert!(app.world().resource::<Assets<Image>>().get(id).is_none(), "queued until the run ends");
        flush(app.world_mut());
        assert!(app.world().resource::<Assets<Image>>().get(id).is_some());
        assert_eq!(app.world().resource::<ScriptAssets>().ref_of(h.id()), Some(1));
        clear(app.world_mut(), false);
        assert!(app.world().resource::<Assets<Image>>().get(id).is_some(), "a hot reload keeps it for its entities");
        clear(app.world_mut(), true);
        assert!(app.world().resource::<Assets<Image>>().get(id).is_none(), "a restart removes it");
        let t = app.world().resource::<ScriptAssets>();
        assert!(t.handle(1).unwrap_err().starts_with("stale asset ref"));
        assert!(t.handle(9).unwrap_err().starts_with("unknown asset ref"));
        assert_eq!(t.make(&ty, Box::new(Image::default())).0, 2, "never reused in a session");
    }

    #[test]
    fn hot_reloads_keep_assets_until_a_restart() {
        // Each hot reload re-runs the script's adds, so kept assets pile up per reload: a dev-only
        // limit (only `bevy-ts dev` hot reloads), bounded by the next restart or Stop.
        let mut app = app();
        let ty = image(&app);
        let mut ids = Vec::new();
        for _ in 0..2 {
            let (_, h) = app.world().resource::<ScriptAssets>().make(&ty, Box::new(Image::default()));
            ids.push(h.id().typed::<Image>());
            flush(app.world_mut());
            clear(app.world_mut(), false);
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 2, "both generations' assets kept");
        clear(app.world_mut(), true);
        assert!(ids.iter().all(|&id| app.world().resource::<Assets<Image>>().get(id).is_none()));
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn a_ref_of_another_type_names_both() {
        let app = app();
        let (n, _) = app.world().resource::<ScriptAssets>().make(&image(&app), Box::new(Image::default()));
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let t = app.world().resource::<ScriptAssets>();
        assert!(t.handle_for(&registry, TypeId::of::<Image>(), n).is_ok());
        let e = t.handle_for(&registry, TypeId::of::<Mesh>(), n).unwrap_err();
        assert!(e.contains("Image") && e.contains("Mesh"), "{e}");
    }

    fn call(app: &App, name: &str, args: Value) -> Result<Value, String> {
        host_call(app.world(), name, &args.to_string()).map(|s| serde_json::from_str(&s).expect("json"))
    }

    const MAT: &str = "bevy_pbr::pbr_material::StandardMaterial";
    const IMG: &str = "bevy_image::image::Image";

    fn pbr_app() -> App {
        let mut app = app();
        app.init_asset::<StandardMaterial>().register_asset_reflect::<StandardMaterial>();
        app
    }

    fn material(app: &App, r: &Value) -> StandardMaterial {
        let h = app.world().resource::<ScriptAssets>().handle(parse_ref(r).unwrap().unwrap()).unwrap();
        app.world().resource::<Assets<StandardMaterial>>().get(h.id().typed::<StandardMaterial>()).expect("added").clone()
    }

    fn modified(app: &App) -> usize {
        let msgs = app.world().resource::<Messages<AssetEvent<StandardMaterial>>>();
        msgs.iter_current_update_messages().filter(|e| matches!(e, AssetEvent::Modified { .. })).count()
    }

    #[test]
    fn mesh_has_no_default() {
        let app = app();
        let e = call(&app, "assets.add", serde_json::json!(["bevy_mesh::mesh::Mesh", {}])).unwrap_err();
        assert!(e.contains("Mesh") && e.contains("no default"), "{e}");
    }

    #[test]
    fn add_builds_from_the_default_and_is_seen_at_once() {
        let mut app = pbr_app();
        let r = call(&app, "assets.add", serde_json::json!([MAT, { "base_color": "#ff8800", "unlit": true }])).unwrap();
        assert_eq!(r["$asset"], 1);
        assert_eq!(r["type"], MAT);
        let got = call(&app, "assets.get", serde_json::json!([r])).unwrap();
        assert_eq!(got["unlit"], true, "get reads the queued value");
        flush(app.world_mut());
        let m = material(&app, &r);
        assert!(m.unlit);
        assert_eq!(m.base_color, Color::srgb_u8(0xff, 0x88, 0x00));
        assert_eq!(m.perceptual_roughness, StandardMaterial::default().perceptual_roughness);
    }

    #[test]
    fn a_bad_field_adds_nothing() {
        let app = pbr_app();
        let e = call(&app, "assets.add", serde_json::json!([MAT, { "roughnes": 0.5 }])).unwrap_err();
        assert!(e.contains("StandardMaterial") && e.contains("roughnes"), "{e}");
        assert!(app.world().resource::<ScriptAssets>().is_empty());
    }

    #[test]
    fn set_changes_one_field_and_a_no_op_set_sends_nothing() {
        let mut app = pbr_app();
        let r = call(&app, "assets.add", serde_json::json!([MAT, { "unlit": true }])).unwrap();
        flush(app.world_mut());
        app.update();
        call(&app, "assets.set", serde_json::json!([r, { "unlit": true }])).unwrap();
        assert!(app.world().resource::<ScriptAssets>().table().queued.is_empty(), "a no-op set queues nothing");
        flush(app.world_mut());
        app.update();
        assert_eq!(modified(&app), 0, "a no-op set sends no Modified");
        call(&app, "assets.set", serde_json::json!([r, { "perceptual_roughness": 0.2 }])).unwrap();
        flush(app.world_mut());
        // Bevy sends asset events from its own system, in the next update.
        app.update();
        assert_eq!(modified(&app), 1);
        let m = material(&app, &r);
        assert_eq!(m.perceptual_roughness, 0.2);
        assert!(m.unlit, "other fields keep their value");
        let got = call(&app, "assets.get", serde_json::json!([r])).unwrap();
        assert!((got["perceptual_roughness"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_texture_ref_goes_in_a_material_and_reads_back_as_its_ref() {
        let mut app = pbr_app();
        let img = {
            let ty = image(&app);
            let (n, _) = app.world().resource::<ScriptAssets>().make(&ty, Box::new(Image::default()));
            ref_json(n, IMG)
        };
        let r = call(&app, "assets.add", serde_json::json!([MAT, { "base_color_texture": img }])).unwrap();
        flush(app.world_mut());
        let tex = material(&app, &r).base_color_texture.expect("a texture");
        let want = app.world().resource::<ScriptAssets>().handle(1).unwrap();
        assert_eq!(tex.id().untyped(), want.id());
        let got = call(&app, "assets.get", serde_json::json!([r])).unwrap();
        assert_eq!(got["base_color_texture"]["$asset"], 1);
    }

    #[test]
    fn a_ref_of_the_wrong_type_names_both() {
        let app = pbr_app();
        let r = call(&app, "assets.add", serde_json::json!([MAT, {}])).unwrap();
        let e = call(&app, "assets.add", serde_json::json!([MAT, { "base_color_texture": r }])).unwrap_err();
        assert!(e.contains("StandardMaterial") && e.contains("Image"), "{e}");
    }

    #[test]
    fn a_stale_ref_fails() {
        let mut app = pbr_app();
        let r = call(&app, "assets.add", serde_json::json!([MAT, {}])).unwrap();
        clear(app.world_mut(), true);
        let e = call(&app, "assets.get", serde_json::json!([r])).unwrap_err();
        assert!(e.contains("stale asset ref"), "{e}");
    }

    #[test]
    fn equal_values_share_one_asset() {
        let app = pbr_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let slots = Slots { registry: &registry, server: None, assets, check: false };
        use reflect_value::HandleSlots;
        let id = TypeId::of::<StandardMaterial>();
        let a = slots.write(id, &serde_json::json!({ "$of": MAT, "props": { "unlit": true, "base_color": "#fff" } })).unwrap();
        let b = slots.write(id, &serde_json::json!({ "$of": MAT, "props": { "base_color": "#fff", "unlit": true } })).unwrap();
        assert_eq!(a.id(), b.id());
        assert_eq!(assets.len(), 1, "one material built");
        let c = slots.write(id, &serde_json::json!({ "$of": MAT, "props": { "base_color": "#000" } })).unwrap();
        assert_ne!(a.id(), c.id());
        let e = slots.write(TypeId::of::<Image>(), &serde_json::json!({ "$of": MAT, "props": {} })).unwrap_err();
        assert!(e.contains("StandardMaterial") && e.contains("Image"), "{e}");
    }

    #[test]
    fn check_mode_builds_nothing() {
        let app = pbr_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let slots = Slots { registry: &registry, server: None, assets, check: true };
        use reflect_value::HandleSlots;
        slots.write(TypeId::of::<StandardMaterial>(), &serde_json::json!({ "$of": MAT, "props": { "unlit": true } })).unwrap();
        assert!(assets.is_empty());
        let e = slots.write(TypeId::of::<StandardMaterial>(), &serde_json::json!({ "$of": MAT, "props": { "nope": 1 } })).unwrap_err();
        assert!(e.contains("nope"), "{e}");
    }

    #[test]
    fn canonical_sorts_keys() {
        let a = serde_json::json!({ "unlit": true, "base_color": "#fff", "n": { "b": 1, "a": [{ "y": 1, "x": 2 }] } });
        let b = serde_json::json!({ "n": { "a": [{ "x": 2, "y": 1 }], "b": 1 }, "base_color": "#fff", "unlit": true });
        assert_eq!(canonical(&a), canonical(&b));
    }

    #[test]
    fn parses_refs() {
        assert_eq!(parse_ref(&ref_json(3, "x")), Some(Ok(3)));
        assert!(parse_ref(&serde_json::json!({ "$asset": 0 })).unwrap().is_err());
        assert_eq!(parse_ref(&serde_json::json!("textures/a.png")), None);
    }
}
