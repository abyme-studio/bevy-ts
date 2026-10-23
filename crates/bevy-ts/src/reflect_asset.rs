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
use bevy::asset::{ReflectAsset, UntypedAssetId, UntypedHandle};
use bevy::prelude::*;
use bevy::reflect::{PartialReflect, ReflectFromReflect, TypeRegistry};
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard};

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
    /// Assets of refs a reload made stale, kept for the entities that still use them until the
    /// next restart or stop.
    retired: Vec<Made>,
    warned: bool,
}

/// The ref table: every asset scripts made this session, by ref number and by asset id, the
/// asset-value cache, and the values waiting for [`flush`]. Behind a lock: scripts reach it
/// through `&World` during a run.
#[derive(Resource, Default)]
pub struct ScriptAssets(Mutex<Table>);

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
    let out = match name {
        "assets.add" => {
            let path = arg(0).as_str().ok_or_else(|| format!("assets.add: the type is a type path, not {}", arg(0)))?;
            add(world, path, arg(1))?
        }
        "assets.get" => get(world, arg(0))?,
        "assets.set" => {
            set(world, arg(0), arg(1))?;
            Value::Null
        }
        _ => return Err(format!("no host service {name}")),
    };
    Ok(out.to_string())
}

fn script_assets(world: &World) -> Result<&ScriptAssets, String> {
    world.get_resource::<ScriptAssets>().ok_or_else(|| "assets need BevyTsPlugin".to_string())
}

/// `assets.add(Type, props)`: `props` written over the type's default, converted, then made.
pub fn add(world: &World, type_path: &str, props: &Value) -> Result<Value, String> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let ty = asset_type(&registry, type_path)?;
    let value = build(world, &registry, &ty, None, props)?;
    let (n, _) = script_assets(world)?.make(&ty, value);
    Ok(ref_json(n, ty.path))
}

/// `assets.get(ref)`: the asset as JSON, its queued value if it has one; `null` when it is gone.
pub fn get(world: &World, r: &Value) -> Result<Value, String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.get takes an asset ref, not {r}")))?;
    let assets = script_assets(world)?;
    let handle = assets.handle(n)?;
    let registry = world.resource::<AppTypeRegistry>().read();
    if let Some(v) = assets.queued_value(n) {
        return read_value(world, &registry, v.as_ref());
    }
    let ty = asset_type_of(&registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    match ty.reflect.get(world, handle.id()) {
        Some(v) => read_value(world, &registry, v.as_partial_reflect()),
        None => Ok(Value::Null),
    }
}

/// `assets.set(ref, patch)`: `patch` written over the asset's current value; queued only when the
/// result differs, so a no-op set marks nothing changed.
pub fn set(world: &World, r: &Value, patch: &Value) -> Result<(), String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.set takes an asset ref, not {r}")))?;
    let assets = script_assets(world)?;
    let handle = assets.handle(n)?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let ty = asset_type_of(&registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    let current = match assets.queued_value(n) {
        Some(v) => v,
        None => {
            let v = ty.reflect.get(world, handle.id()).ok_or_else(|| format!("asset ref {n}: the asset is gone"))?;
            v.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect()
        }
    };
    let next = build(world, &registry, &ty, Some(current.as_ref()), patch)?;
    if current.reflect_partial_eq(next.as_partial_reflect()) == Some(true) {
        return Ok(());
    }
    assets.queue_set(n, next);
    Ok(())
}

/// `json` written over `base` (the type's default when `None`), converted to the concrete type.
/// A value that fails any step builds nothing.
fn build(world: &World, registry: &TypeRegistry, ty: &AssetType, base: Option<&dyn PartialReflect>, json: &Value) -> Result<Box<dyn Reflect>, String> {
    let mut value = match base {
        Some(b) => b.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect(),
        None => default_value(registry, ty)?,
    };
    write_value(world, registry, value.as_mut(), json)?;
    let from = registry.get_type_data::<ReflectFromReflect>(ty.id).ok_or_else(|| format!("{} is not FromReflect", ty.path))?;
    from.from_reflect(value.as_ref()).ok_or_else(|| format!("{}: the written value is not a whole {}", ty.path, ty.path))
}

fn default_value(registry: &TypeRegistry, ty: &AssetType) -> Result<Box<dyn PartialReflect>, String> {
    registry
        .get_type_data::<ReflectDefault>(ty.id)
        .map(|d| d.default().into_partial_reflect())
        .ok_or_else(|| format!("{} has no default: assets.add cannot build it", ty.path))
}

/// Writes `json` into `slot` through `reflect_value`'s writer, with this module's handle hook.
fn write_value(_world: &World, _registry: &TypeRegistry, _slot: &mut dyn PartialReflect, _json: &Value) -> Result<(), String> {
    // reflect-coverage stage 1 brings `reflect_value::write_with` and the `HandleSlots` hook.
    Err("assets: the reflected writer is not on main yet".into())
}

/// Reads `value` as JSON through `reflect_value`'s reader, with this module's handle hook.
fn read_value(_world: &World, _registry: &TypeRegistry, _value: &dyn PartialReflect) -> Result<Value, String> {
    // reflect-coverage stage 1 brings `reflect_value::read_with`.
    Err("assets: the reflected reader is not on main yet".into())
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

    #[test]
    fn mesh_has_no_default() {
        let app = app();
        let e = add(app.world(), "bevy_mesh::mesh::Mesh", &serde_json::json!({})).unwrap_err();
        assert!(e.contains("Mesh") && e.contains("no default"), "{e}");
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
