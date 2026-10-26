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
//! session. Refs go stale when the script reloads or stops ([`reloading`], [`applied`]); a ref
//! used after that is a "stale asset ref". The assets stay for the entities a hot reload keeps,
//! until a restart or stop removes them. Asset values (`T.of(props)`,
//! `{ "$of": path, "props": ... }`) are cached by their type and the props' [`canonical`] JSON, so
//! every slot given equal props shares one asset. `Image` is hand-mapped by its sampler alone, and
//! `Image.load(path, settings)` (`{ "$load": path, "settings": ... }`) loads with loader settings.

use bevy::asset::uuid::Uuid;
use crate::reflect_value;
use bevy::asset::{AssetServer, ReflectAsset, UntypedAssetId, UntypedHandle};
use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::reflect::{PartialReflect, ReflectFromReflect, TypeRegistry};
use serde_json::{Map, Value};
use std::any::{Any, TypeId};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

/// One session's assets past this many log a warning: `assets.add` in a per-frame system grows
/// memory until a restart or stop.
const REFS_WARN: usize = 10_000;

/// The high half of every script-made asset's UUID; the low half is its ref number.
const UUID_TAG: u64 = 0x6265_7679_2d74_7341; // "bevy-tsA"

/// An asset type found by its type path: hand-mapped, else reflected.
#[derive(Clone)]
pub struct AssetType {
    /// The asset's own type (`StandardMaterial`), not its handle's.
    pub id: TypeId,
    pub path: &'static str,
    kind: Kind,
}

/// How an asset type is built, read and stored.
#[derive(Clone)]
enum Kind {
    Reflected(ReflectAsset),
    Hand(Arc<HandMap>),
}

impl Kind {
    fn remove(&self, world: &mut World, id: UntypedAssetId) {
        match self {
            Kind::Reflected(r) => {
                r.remove(world, id);
            }
            Kind::Hand(h) => (h.remove)(world, id),
        }
    }
}

/// The asset type `type_path`: a hand-mapped one, else one registered with `ReflectAsset`;
/// refused when it is neither.
pub fn asset_type(registry: &TypeRegistry, type_path: &str) -> Result<AssetType, String> {
    if let Some(h) = hands().iter().find(|h| h.path == type_path) {
        return Ok(AssetType { id: h.id, path: h.path, kind: Kind::Hand(h.clone()) });
    }
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not a registered type"))?;
    asset_type_of(registry, reg.type_id()).ok_or_else(|| format!("{type_path} is not a reflected asset"))
}

fn asset_type_of(registry: &TypeRegistry, id: TypeId) -> Option<AssetType> {
    if let Some(h) = hand_map(id) {
        return Some(AssetType { id, path: h.path, kind: Kind::Hand(h) });
    }
    let reg = registry.get(id)?;
    let reflect = reg.data::<ReflectAsset>()?.clone();
    Some(AssetType { id, path: reg.type_info().type_path(), kind: Kind::Reflected(reflect) })
}

/// The type path of an asset type, for messages.
fn path_of(registry: &TypeRegistry, id: TypeId) -> String {
    if let Some(h) = hand_map(id) {
        return h.path.to_string();
    }
    registry.get(id).map_or_else(|| format!("{id:?}"), |r| r.type_info().type_path().to_string())
}

/// One ref's entry.
struct Made {
    handle: UntypedHandle,
    hold: Hold,
}

/// How an entry holds its asset.
enum Hold {
    /// A script-made asset at a UUID handle, kept until restart or stop; its asset type, to
    /// remove it then.
    Uuid(Kind),
    /// A strong handle (`server.load`, `meshes.add`): dropping the entry drops the handle, and Bevy
    /// frees the asset once nothing else holds it. Never queued.
    Strong,
}

/// A built asset value: reflected, or a hand-mapped type's own value.
enum Built {
    Reflected(Box<dyn Reflect>),
    Hand(Hand),
}

impl Built {
    fn try_clone(&self) -> Option<Built> {
        match self {
            Built::Reflected(v) => v.reflect_clone().ok().map(Built::Reflected),
            Built::Hand(v) => Some(Built::Hand(v.clone())),
        }
    }
}

/// A value waiting for [`flush`]: a new asset, or a changed one.
struct Queued {
    added: bool,
    value: Built,
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
    /// Each path loaded with settings, and the settings it was first loaded with: the asset server
    /// ignores the settings of a later load of the same path.
    loads: HashMap<String, String>,
    /// UUID assets of refs a reload or `drop` made stale, kept for the entities that still use
    /// them until the next restart or stop.
    retired: Vec<Made>,
    /// Handles `assets.remove` took, removed from their `Assets<T>` at [`flush`].
    removing: Vec<UntypedHandle>,
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
    fn make(&self, ty: &AssetType, value: Built) -> (u64, UntypedHandle) {
        let (n, handle) = self.make_unqueued(ty);
        self.table().queued.insert(n, Queued { added: true, value });
        (n, handle)
    }

    /// A new ref for an asset of `ty`, with its UUID handle; its caller inserts the value now, with
    /// `&mut World`.
    fn make_unqueued(&self, ty: &AssetType) -> (u64, UntypedHandle) {
        let mut t = self.table();
        t.last += 1;
        let n = t.last;
        let handle = UntypedHandle::Uuid { type_id: ty.id, uuid: Uuid::from_u64_pair(UUID_TAG, n) };
        t.by_id.insert(handle.id(), n);
        t.refs.insert(n, Made { handle: handle.clone(), hold: Hold::Uuid(ty.kind.clone()) });
        if t.refs.len() > REFS_WARN && !t.warned {
            t.warned = true;
            warn!("bevy-ts: scripts hold {} assets this session; add an asset once and assets.set it after", t.refs.len());
        }
        (n, handle)
    }

    /// A new ref holding the strong `handle`. The same handle twice gives the same ref.
    pub fn insert_strong(&self, handle: UntypedHandle) -> u64 {
        let mut t = self.table();
        if let Some(&n) = t.by_id.get(&handle.id()) {
            return n;
        }
        t.last += 1;
        let n = t.last;
        t.by_id.insert(handle.id(), n);
        t.refs.insert(n, Made { handle, hold: Hold::Strong });
        if t.refs.len() > REFS_WARN && !t.warned {
            t.warned = true;
            warn!("bevy-ts: scripts hold {} asset refs this session; assets.drop the ones you replace", t.refs.len());
        }
        n
    }

    /// `assets.drop(ref)`: forgets ref `n`. A strong handle drops with it, so Bevy frees the asset
    /// once nothing else holds it; a UUID asset stays, as Bevy's UUID handles keep theirs, until
    /// restart or stop.
    pub fn drop(&self, n: u64) -> Result<(), String> {
        let mut t = self.table();
        let m = t.refs.remove(&n).ok_or_else(|| Self::missing(&t, n))?;
        t.by_id.remove(&m.handle.id());
        // A queued add or set of a UUID asset still goes in: a slot may hold its handle.
        if matches!(m.hold, Hold::Uuid(_)) {
            t.retired.push(m);
        }
        Ok(())
    }

    /// `assets.remove(ref)`: forgets ref `n` and removes its asset (Bevy's `Assets::remove`) when
    /// the run ends, at [`flush`]. The asset goes even while a component holds it.
    pub fn remove(&self, n: u64) -> Result<(), String> {
        let mut t = self.table();
        let m = t.refs.remove(&n).ok_or_else(|| Self::missing(&t, n))?;
        t.by_id.remove(&m.handle.id());
        t.queued.remove(&n);
        t.sources.remove(&m.handle.id());
        t.removing.push(m.handle);
        Ok(())
    }

    /// The handle of ref `n`.
    pub fn handle(&self, n: u64) -> Result<UntypedHandle, String> {
        let t = self.table();
        match t.refs.get(&n) {
            Some(m) => Ok(m.handle.clone()),
            None => Err(Self::missing(&t, n)),
        }
    }

    fn missing(t: &Table, n: u64) -> String {
        if n >= 1 && n <= t.last {
            format!("stale asset ref {n}: dropped, removed, or made before a reload applied")
        } else {
            format!("unknown asset ref {n}")
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
    fn queued_value(&self, n: u64) -> Option<Built> {
        self.table().queued.get(&n).and_then(|q| q.value.try_clone())
    }

    /// Queues `value` as ref `n`'s new value; a queued add stays an add.
    fn queue_set(&self, n: u64, value: Built) {
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

/// A queued value, its handle, and its kind when the table knows it.
type FlushItem = (Queued, UntypedHandle, Option<Kind>);

/// Puts every queued value in its `Assets<T>`: an add inserted at its UUID, a set applied once.
/// A hand-mapped value is inserted whole at the ref's id, for an add and a set alike.
/// Runs after each system run and entry call, before its commands apply.
pub fn flush(world: &mut World) {
    let Some(assets) = world.get_resource::<ScriptAssets>() else { return };
    let (queued, removing): (Vec<FlushItem>, Vec<UntypedHandle>) = {
        let mut t = assets.table();
        let queued = std::mem::take(&mut t.queued);
        let queued = queued
            .into_iter()
            .filter_map(|(n, q)| {
                let made = t.refs.get(&n).or_else(|| {
                    let uuid = Uuid::from_u64_pair(UUID_TAG, n);
                    t.retired.iter().find(|m| matches!(m.handle, UntypedHandle::Uuid { uuid: u, .. } if u == uuid))
                });
                match made {
                    Some(Made { handle, hold: Hold::Uuid(k) }) => Some((q, handle.clone(), Some(k.clone()))),
                    // A strong ref's set (`meshes.add`, `server.load`): its type's kind is looked up below.
                    Some(Made { handle, hold: Hold::Strong }) => Some((q, handle.clone(), None)),
                    None => None,
                }
            })
            .collect();
        (queued, std::mem::take(&mut t.removing))
    };
    let kind_of = |world: &World, id: TypeId| asset_type_of(&world.resource::<AppTypeRegistry>().read(), id).map(|t| t.kind);
    for handle in removing {
        match kind_of(world, handle.type_id()) {
            Some(k) => k.remove(world, handle.id()),
            None => warn!("bevy-ts: assets.remove: {:?} is not an asset type scripts reach; the asset stays", handle.type_id()),
        }
    }
    for (q, handle, kind) in queued {
        let Some(kind) = kind.or_else(|| kind_of(world, handle.type_id())) else {
            continue;
        };
        let failed = match (kind, q.value) {
            (Kind::Hand(h), Built::Hand(v)) => (h.put)(world, handle.id(), v).err(),
            (Kind::Reflected(reflect), Built::Reflected(v)) if q.added => reflect.insert(world, handle.id(), v.as_partial_reflect()).err().map(|e| e.to_string()),
            (Kind::Reflected(reflect), Built::Reflected(v)) => {
                if let Some(asset) = reflect.get_mut(world, handle.id())
                    && let Err(v) = asset.set(v)
                {
                    asset.apply(v.as_partial_reflect());
                }
                None
            }
            _ => Some("its value is not of its type".to_string()),
        };
        if let Some(e) = failed {
            warn!("bevy-ts: a script-made {} was not {}: {e}", path_of(&world.resource::<AppTypeRegistry>().read(), handle.type_id()), if q.added { "added" } else { "set" });
        }
    }
}

/// A reload under way: the last ref the live generation gave, and its asset-value cache, set
/// aside so the loading generation builds its own.
pub struct Reloading {
    last: u64,
    values: HashMap<(TypeId, String), u64>,
}

/// Starts a reload. A restart or Stop ends the live generation now: its refs go stale, its strong
/// handles drop and every script-made asset is removed. A hot reload keeps the live generation's
/// refs working until the new one applies ([`applied`]); if it fails ([`failed`]) the live one
/// carries on.
pub fn reloading(world: &mut World, restart: bool) -> Reloading {
    let Some(assets) = world.get_resource::<ScriptAssets>().cloned() else { return Reloading { last: 0, values: HashMap::new() } };
    if restart {
        let last = assets.table().last;
        retire(world, &assets, last, true);
    }
    let mut t = assets.table();
    Reloading { last: t.last, values: std::mem::take(&mut t.values) }
}

/// The new generation applied: the refs the old one gave are stale from now on, and their strong
/// handles drop. Their UUID assets stay for the entities the reload kept, until the next restart
/// or stop.
pub fn applied(world: &mut World, r: Reloading) {
    let Some(assets) = world.get_resource::<ScriptAssets>().cloned() else { return };
    retire(world, &assets, r.last, false);
}

/// The new generation failed: what it made while loading is dropped, from the queue and from
/// `Assets`, and the live generation's refs and value cache are as they were.
pub fn failed(world: &mut World, r: Reloading) {
    let Some(assets) = world.get_resource::<ScriptAssets>().cloned() else { return };
    let gone: Vec<Made> = {
        let mut guard = assets.table();
        let t = &mut *guard;
        let new: Vec<u64> = t.refs.keys().copied().filter(|n| *n > r.last).collect();
        let mut gone = Vec::new();
        for n in new {
            t.queued.remove(&n);
            if let Some(m) = t.refs.remove(&n) {
                t.by_id.remove(&m.handle.id());
                t.sources.remove(&m.handle.id());
                gone.push(m);
            }
        }
        t.values = r.values;
        gone
    };
    // Strong entries drop here, outside the lock; Bevy frees what nothing else holds.
    for m in gone {
        if let Hold::Uuid(k) = m.hold {
            k.remove(world, m.handle.id());
        }
    }
}

/// Makes refs up to `last` stale: strong handles drop, UUID assets are kept for the entities that
/// use them. `remove` also removes every script-made asset, those of refs made stale before
/// included.
fn retire(world: &mut World, assets: &ScriptAssets, last: u64, remove: bool) {
    let (gone, _strong): (Vec<Made>, Vec<Made>) = {
        let mut guard = assets.table();
        let t = &mut *guard;
        let old: Vec<u64> = t.refs.keys().copied().filter(|n| *n <= last).collect();
        let mut strong = Vec::new();
        for n in old {
            t.queued.remove(&n);
            if let Some(m) = t.refs.remove(&n) {
                t.by_id.remove(&m.handle.id());
                t.sources.remove(&m.handle.id());
                match m.hold {
                    Hold::Uuid(_) => t.retired.push(m),
                    Hold::Strong => strong.push(m),
                }
            }
        }
        let refs = &t.refs;
        t.values.retain(|_, n| refs.contains_key(n));
        if remove {
            t.removing.clear();
        }
        t.warned = false;
        (if remove { std::mem::take(&mut t.retired) } else { Vec::new() }, strong)
    };
    for m in gone {
        if let Hold::Uuid(k) = m.hold {
            k.remove(world, m.handle.id());
        }
    }
}

/// A JSON host call from a script: `assets.add`, `get`, `set`, `drop`, `remove` or `id`, `args` a JSON array.
pub fn host_call(world: &World, name: &str, args: &str) -> Result<String, String> {
    let args: Vec<Value> = serde_json::from_str(args).map_err(|e| format!("{name}: {e}"))?;
    let arg = |i: usize| args.get(i).unwrap_or(&Value::Null);
    let assets = world.get_resource::<ScriptAssets>().ok_or_else(|| "assets need BevyTsPlugin".to_string())?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let slots = Slots { registry: &registry, server: world.get_resource::<AssetServer>(), assets, world: Some(world), check: false };
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
        "assets.drop" | "assets.remove" => {
            let n = parse_ref(arg(0)).unwrap_or_else(|| Err(format!("{name} takes an asset ref, not {}", arg(0))))?;
            if name == "assets.drop" { assets.drop(n)? } else { assets.remove(n)? }
            Value::Null
        }
        "assets.id" => {
            let n = parse_ref(arg(0)).unwrap_or_else(|| Err(format!("ref.id: not an asset ref: {}", arg(0))))?;
            id_json(&registry, assets.handle(n)?.id())?
        }
        _ => return Err(format!("no host service {name}")),
    };
    Ok(out.to_string())
}

/// What writing and reading assets reach, and the `Handle<T>` hook `reflect_value` calls for refs
/// and asset values. `world` is the app's world for a hand map that reads its resources; it is
/// `None` in `check` mode and where the world is borrowed mutably meanwhile (TSX and the remote
/// writing a component in place). In `check` mode nothing is made: a ref the table does not know
/// is checked by its form and named type, values are built and dropped.
pub struct Slots<'a> {
    pub registry: &'a TypeRegistry,
    pub server: Option<&'a AssetServer>,
    pub assets: &'a ScriptAssets,
    pub world: Option<&'a World>,
    pub check: bool,
}

impl Slots<'_> {
    /// The writer's context with this hook.
    pub fn cx(&self) -> reflect_value::Cx<'_> {
        reflect_value::Cx { registry: self.registry, server: self.server, handles: Some(self) }
    }

    /// A new asset of `ty` from `json`: its hand map's write, else `json` written over the type's
    /// default. A value that fails any step builds nothing.
    fn build(&self, ty: &AssetType, json: &Value) -> Result<Built, String> {
        match &ty.kind {
            Kind::Hand(h) => (h.write)(self, None, json).map(Built::Hand),
            Kind::Reflected(_) => self.write_over(ty, None, json).map(Built::Reflected),
        }
    }

    /// `json` written over `base` (the type's default when `None`), converted to the concrete
    /// type.
    fn write_over(&self, ty: &AssetType, base: Option<&dyn PartialReflect>, json: &Value) -> Result<Box<dyn Reflect>, String> {
        let mut value = match base {
            Some(b) => b.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect(),
            None => default_value(self.registry, ty)?,
        };
        reflect_value::write_with(self.cx(), value.as_mut(), None, json).map_err(|e| format!("{}: {e}", ty.path))?;
        let from = self.registry.get_type_data::<ReflectFromReflect>(ty.id).ok_or_else(|| format!("{} is not FromReflect", ty.path))?;
        from.from_reflect(value.as_ref()).ok_or_else(|| format!("{}: the written value is not a whole {}", ty.path, ty.path))
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
        let value = self.build(&ty, props)?;
        if self.check {
            return Ok(UntypedHandle::Uuid { type_id: ty.id, uuid: Uuid::nil() });
        }
        let (n, handle) = self.assets.make(&ty, value);
        if matches!(ty.kind, Kind::Hand(_)) {
            self.assets.set_source(handle.id(), props.clone());
        }
        self.assets.cache(ty.id, key, n);
        Ok(handle)
    }

    /// `T.of(props)` of a reflected `T` from Rust, with a step after the write for fields reflection
    /// cannot reach (`StandardMaterial::cull_mode`): `finish` edits the built value, and `extra`,
    /// what it sets as JSON, joins the cache key. With `extra` null it is the script's `T.of(props)`
    /// and shares its asset. Returns the handle, and the value when it was built now: the caller
    /// inserts it at the handle, whole, since a reflected insert would drop what `finish` set.
    pub fn value_with<T: Asset + Reflect>(&self, props: &Value, extra: &Value, finish: &mut dyn FnMut(&mut T)) -> Result<(Handle<T>, Option<T>), String> {
        let ty = asset_type(self.registry, T::type_path())?;
        if matches!(ty.kind, Kind::Hand(_)) {
            return Err(format!("{} is hand-mapped, so it has no reflected fields to finish", ty.path));
        }
        // Not JSON, so never a script value's key.
        let key = if extra.is_null() { canonical(props) } else { format!("{}|{}", canonical(props), canonical(extra)) };
        if let Some(n) = self.assets.cached(ty.id, &key) {
            return Ok((self.assets.handle(n)?.typed::<T>(), None));
        }
        let built = self.write_over(&ty, None, props)?;
        let mut value = *built.downcast::<T>().map_err(|_| format!("{}: the written value is not a whole {}", ty.path, ty.path))?;
        finish(&mut value);
        let (n, handle) = self.assets.make_unqueued(&ty);
        self.assets.cache(ty.id, key, n);
        Ok((handle.typed::<T>(), Some(value)))
    }
}

impl Slots<'_> {
    /// `T.load(path, settings)` in a slot of asset type `asset`, by its type's [`LOADERS`] entry. A
    /// path loaded again with other settings keeps the first (the asset server's rule), and warns.
    fn load(&self, asset: TypeId, path: &Value, settings: &Value) -> Result<UntypedHandle, String> {
        let type_path = path_of(self.registry, asset);
        let path = path.as_str().filter(|p| !p.is_empty()).ok_or_else(|| format!("{type_path}.load takes an asset path, not {path}"))?;
        let load = LOADERS.iter().find(|(p, _)| *p == type_path).map(|(_, l)| *l).ok_or_else(|| format!("{type_path} has no loader settings: only {} loads with them", LOADERS.iter().map(|(p, _)| *p).collect::<Vec<_>>().join(", ")))?;
        let empty = Map::new();
        let settings_map = match settings {
            Value::Null => &empty,
            Value::Object(o) => o,
            v => return Err(format!("{type_path}.load's settings are an object, not {v}")),
        };
        let handle = load(self, path, settings_map)?;
        if !self.check {
            let key = canonical(settings);
            let mut t = self.assets.table();
            match t.loads.get(path) {
                Some(first) if *first != key => warn!("bevy-ts: {path} is loaded with settings {key} after {first}; the asset server keeps the first"),
                Some(_) => {}
                None => _ = t.loads.insert(path.to_string(), key),
            }
        }
        Ok(handle)
    }
}

/// A loader-settings entry: loads `path` with the settings given, read by hand (loader settings are
/// serde, not `Reflect`). In check mode it checks the settings and loads nothing.
type Load = fn(&Slots, &str, &Map<String, Value>) -> Result<UntypedHandle, String>;

/// The asset types `T.load(path, settings)` takes, by type path. Adding a settings key is one line
/// in its entry.
const LOADERS: &[(&str, Load)] = &[("bevy_image::image::Image", load_image)];

/// Whether `T.load` takes asset type `type_path`.
pub fn loadable(type_path: &str) -> bool {
    LOADERS.iter().any(|(p, _)| *p == type_path)
}

/// `Image.load(path, { sampler, is_srgb })`, by `ImageLoaderSettings`.
fn load_image(slots: &Slots, path: &str, settings: &Map<String, Value>) -> Result<UntypedHandle, String> {
    let mut sampler = None;
    let mut is_srgb = None;
    for (k, v) in settings {
        match k.as_str() {
            "sampler" => sampler = Some(image_sampler(slots, None, v)?),
            "is_srgb" => is_srgb = Some(v.as_bool().ok_or_else(|| format!("Image.load's is_srgb is true or false, not {v}"))?),
            _ => return Err(format!("Image.load takes the settings sampler and is_srgb, not {k}")),
        }
    }
    if slots.check {
        return Ok(UntypedHandle::Uuid { type_id: TypeId::of::<Image>(), uuid: Uuid::nil() });
    }
    let server = slots.server.ok_or_else(|| format!("Image.load({path:?}) needs an AssetServer"))?;
    let h: Handle<Image> = server
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            if let Some(x) = &sampler {
                s.sampler = x.clone();
            }
            if let Some(b) = is_srgb {
                s.is_srgb = b;
            }
        })
        .load(path.to_string());
    Ok(h.untyped())
}

/// An `ImageSampler` from the writer's JSON: `"Default"`, or `{ Descriptor: { ... } }` with the
/// fields given written over `base`'s descriptor, else over `ImageSamplerDescriptor::default()`.
fn image_sampler(slots: &Slots, base: Option<&ImageSampler>, json: &Value) -> Result<ImageSampler, String> {
    let mut s = match (json.as_object(), base) {
        (Some(o), Some(b @ ImageSampler::Descriptor(_))) if o.contains_key("Descriptor") => b.clone(),
        // Unset fields keep `ImagePlugin::default()`'s sampler, not the descriptor's own default,
        // whose filters are Nearest: a repeating photo stays smooth.
        (Some(o), _) if o.contains_key("Descriptor") => ImageSampler::Descriptor(ImageSamplerDescriptor::linear()),
        _ => ImageSampler::Default,
    };
    reflect_value::write_with(slots.cx(), &mut s, None, json).map_err(|e| format!("sampler: {e}"))?;
    Ok(s)
}

/// `Image`'s hand-mapped read: `{ sampler }` alone. Its pixels and texture descriptors are opaque
/// to reflection, so a script reaches the sampler only.
fn image_read(slots: &Slots, _: Option<&Value>, image: &Image) -> Result<Value, String> {
    Ok(serde_json::json!({ "sampler": reflect_value::read_with(slots.registry, Some(slots), &image.sampler)? }))
}

/// `Image`'s hand-mapped write: the base `Image` (its default for an add) with the sampler given
/// written over its own.
fn image_write(slots: &Slots, base: Option<&Image>, json: &Value) -> Result<Image, String> {
    let o = json.as_object().ok_or_else(|| format!("an Image takes {{ sampler }}, not {json}"))?;
    let mut image = base.cloned().unwrap_or_default();
    for (k, v) in o {
        match k.as_str() {
            "sampler" => image.sampler = image_sampler(slots, base.map(|b| &b.sampler), v)?,
            _ => return Err(format!("Image has no field {k} for scripts: they set its sampler alone")),
        }
    }
    Ok(image)
}

/// Hand-maps `Image` by its sampler ([`image_read`], [`image_write`]), unless the app mapped it.
pub fn register_image() {
    own(entry::<Image>(image_read, image_write, None));
}

impl reflect_value::HandleSlots for Slots<'_> {
    fn write(&self, asset: TypeId, json: &Value) -> Result<UntypedHandle, String> {
        let Some(o) = json.as_object() else {
            return Err(format!("expected an asset path, an asset ref, T.of(...) or T.load(...), got {json}"));
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
        if let Some(path) = o.get("$load") {
            return self.load(asset, path, o.get("settings").unwrap_or(&Value::Null));
        }
        Err(format!("expected an asset path, an asset ref, T.of(...) or T.load(...), got {json}"))
    }

    fn read(&self, handle: &UntypedHandle) -> Option<Value> {
        let n = self.assets.ref_of(handle.id())?;
        Some(ref_json(n, &path_of(self.registry, handle.type_id())))
    }
}

/// `assets.add(Type, props)`: `props` written over the type's default, converted, then made.
pub fn add(slots: &Slots, type_path: &str, props: &Value) -> Result<Value, String> {
    let ty = asset_type(slots.registry, type_path)?;
    let value = slots.build(&ty, props)?;
    let (n, handle) = slots.assets.make(&ty, value);
    if matches!(ty.kind, Kind::Hand(_)) {
        slots.assets.set_source(handle.id(), props.clone());
    }
    Ok(ref_json(n, ty.path))
}

/// `assets.get(ref)`: the asset as JSON, its queued value if it has one; `null` when it is gone.
pub fn get(world: &World, slots: &Slots, r: &Value) -> Result<Value, String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.get takes an asset ref, not {r}")))?;
    let handle = slots.assets.handle(n)?;
    let ty = asset_type_of(slots.registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    let queued = slots.assets.queued_value(n);
    match (&ty.kind, &queued) {
        // A hand map's read is given the JSON a script last wrote the asset from.
        (Kind::Hand(h), Some(Built::Hand(v))) => (h.read)(slots, slots.assets.source(handle.id()).as_ref(), v.as_ref()),
        (Kind::Hand(h), _) => match (h.get)(world, handle.id()) {
            Some(v) => (h.read)(slots, slots.assets.source(handle.id()).as_ref(), v),
            None => Ok(Value::Null),
        },
        (Kind::Reflected(_), Some(Built::Reflected(v))) => reflect_value::read_with(slots.registry, Some(slots), v.as_partial_reflect()),
        (Kind::Reflected(reflect), _) => match reflect.get(world, handle.id()) {
            Some(v) => reflect_value::read_with(slots.registry, Some(slots), v.as_partial_reflect()),
            None => Ok(Value::Null),
        },
    }
}

/// `assets.set(ref, patch)`: `patch` written over the asset's current value; queued only when the
/// result differs, so a no-op set marks nothing changed.
pub fn set(world: &World, slots: &Slots, r: &Value, patch: &Value) -> Result<(), String> {
    let n = parse_ref(r).unwrap_or_else(|| Err(format!("assets.set takes an asset ref, not {r}")))?;
    let handle = slots.assets.handle(n)?;
    let ty = asset_type_of(slots.registry, handle.type_id()).ok_or_else(|| format!("asset ref {n} is not a reflected asset"))?;
    let gone = || format!("asset ref {n}: the asset is gone");
    let queued = slots.assets.queued_value(n);
    let next = match &ty.kind {
        // A hand-mapped type: the patch replaces the keys it gives in the current read, then the
        // whole is written again; unchanged when the read is.
        Kind::Hand(h) => {
            let current: &dyn Any = match &queued {
                Some(Built::Hand(v)) => v.as_ref(),
                _ => (h.get)(world, handle.id()).ok_or_else(gone)?,
            };
            let source = slots.assets.source(handle.id());
            let read = (h.read)(slots, source.as_ref(), current)?;
            let mut merged = read.clone();
            match (merged.as_object_mut(), patch) {
                (Some(m), Value::Object(p)) => m.extend(p.iter().map(|(k, v)| (k.clone(), v.clone()))),
                _ => return Err(format!("assets.set on a {} takes an object, not {patch}", ty.path)),
            }
            let next = (h.write)(slots, Some(current), &merged)?;
            if (h.read)(slots, Some(&merged), next.as_ref())? == read {
                return Ok(());
            }
            slots.assets.set_source(handle.id(), merged);
            Built::Hand(next)
        }
        Kind::Reflected(reflect) => {
            let current = match queued {
                Some(Built::Reflected(v)) => v.into_partial_reflect(),
                _ => {
                    let v = reflect.get(world, handle.id()).ok_or_else(gone)?;
                    v.reflect_clone().map_err(|e| format!("{}: {e}", ty.path))?.into_partial_reflect()
                }
            };
            let next = slots.write_over(&ty, Some(current.as_ref()), patch)?;
            if current.reflect_partial_eq(next.as_partial_reflect()) == Some(true) {
                return Ok(());
            }
            Built::Reflected(next)
        }
    };
    slots.assets.queue_set(n, next);
    Ok(())
}

/// [`register`]'s read: the asset as JSON. Given the slots, the JSON a script last wrote the asset
/// from (`None` for an asset no script wrote, a loaded file) and the asset.
pub type HandRead<T> = fn(&Slots, Option<&Value>, &T) -> Result<Value, String>;
/// [`register`]'s write: the asset from JSON. Given the slots, the base (the current asset for
/// `assets.set`, `None` for `assets.add` and `T.of`, which build whole from the JSON) and the JSON.
/// The asset server loads; nothing else changes.
pub type HandWrite<T> = fn(&Slots, Option<&T>, &Value) -> Result<T, String>;

/// A hand-mapped value, shared while it is queued: `assets.get` reads it without a clone.
type Hand = Arc<dyn Any + Send + Sync>;
/// [`HandRead`] over an erased `T`.
type ErasedRead = Box<dyn Fn(&Slots, Option<&Value>, &dyn Any) -> Result<Value, String> + Send + Sync>;
/// [`HandWrite`] over an erased `T`.
type ErasedWrite = Box<dyn Fn(&Slots, Option<&dyn Any>, &Value) -> Result<Hand, String> + Send + Sync>;

/// An asset type reflection cannot reach whole (`Image` is opaque; `AnimationGraph`'s graph has no
/// reflected form; `GizmoAsset` is not `Reflect` at all), read and written by typed code instead of
/// `reflect_value`, and stored in its `Assets<T>` by `T` itself.
struct HandMap {
    id: TypeId,
    path: &'static str,
    read: ErasedRead,
    write: ErasedWrite,
    get: fn(&World, UntypedAssetId) -> Option<&dyn Any>,
    put: fn(&mut World, UntypedAssetId, Hand) -> Result<(), String>,
    remove: fn(&mut World, UntypedAssetId),
    /// Where the app mapped it; `None` for bevy-ts' own (`Mesh`, `Image`), which an app's replaces.
    at: Option<&'static std::panic::Location<'static>>,
}

/// The hand-mapped asset types. Process-wide, like a type registration, so a check with only the
/// type registry (TSX's load check) sees them too.
static HANDS: RwLock<Vec<Arc<HandMap>>> = RwLock::new(Vec::new());

fn hands() -> Vec<Arc<HandMap>> {
    builtins();
    HANDS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// bevy-ts' own hand maps, registered before the first lookup: `Mesh` under `pbr`.
fn builtins() {
    #[cfg(feature = "pbr")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| own(entry::<Mesh>(crate::mesh_data::read, crate::mesh_data::write, None)));
    }
}

/// Adds one of bevy-ts' own maps, unless the app mapped its type.
fn own(entry: HandMap) {
    insert(&mut HANDS.write().unwrap_or_else(|e| e.into_inner()), entry).expect("bevy-ts' own map is never refused");
}

/// Whether asset type `asset` is hand-mapped: a check that walks reflected fields round-trips
/// it through `assets.add` and `assets.get` instead.
pub fn is_hand_mapped(asset: TypeId) -> bool {
    hand_map(asset).is_some()
}

fn hand_map(asset: TypeId) -> Option<Arc<HandMap>> {
    builtins();
    HANDS.read().unwrap_or_else(|e| e.into_inner()).iter().find(|h| h.id == asset).cloned()
}

/// Hand-maps `T` (`BevyTsAppExt::ts_hand_map`): `assets.add`, `assets.get`, `assets.set`, `T.of`
/// and refs reach it through `read` and `write`, and [`flush`] inserts the built `T` whole into
/// `Assets<T>` at the ref's id. `T` needs no `Reflect`; it needs `app.init_asset::<T>()`. A type
/// hand-mapped and also reflected uses its hand map. `assets.set` merges its patch over the
/// current read at the top level: a key given replaces that key whole. The app's map of `T` wins
/// over bevy-ts' own, whichever comes first. Mapping `T` again from the same place replaces its
/// entry (one process builds many apps); from another place it panics naming both.
#[track_caller]
pub fn register<T: Asset>(read: HandRead<T>, write: HandWrite<T>) {
    let entry = entry::<T>(read, write, Some(std::panic::Location::caller()));
    if let Err(e) = insert(&mut HANDS.write().unwrap_or_else(|e| e.into_inner()), entry) {
        panic!("{e}");
    }
}

/// Adds `entry` to `hands`. bevy-ts' own never replaces the app's; the app's replaces bevy-ts'
/// own and its own from the same place, and is refused from another place.
fn insert(hands: &mut Vec<Arc<HandMap>>, entry: HandMap) -> Result<(), String> {
    if let Some(old) = hands.iter().find(|h| h.id == entry.id) {
        match (old.at, entry.at) {
            (Some(_), None) => return Ok(()),
            (Some(a), Some(b)) if a != b => return Err(format!("{} is hand-mapped twice: at {a} and at {b}", entry.path)),
            _ => {}
        }
    }
    hands.retain(|h| h.id != entry.id);
    hands.push(Arc::new(entry));
    Ok(())
}

fn entry<T: Asset>(read: HandRead<T>, write: HandWrite<T>, at: Option<&'static std::panic::Location<'static>>) -> HandMap {
    fn typed<T: Asset>(v: &dyn Any) -> Result<&T, String> {
        v.downcast_ref::<T>().ok_or_else(|| format!("not a {}", T::type_path()))
    }
    fn get<T: Asset>(world: &World, id: UntypedAssetId) -> Option<&dyn Any> {
        world.get_resource::<Assets<T>>()?.get(id.typed::<T>()).map(|a| a as &dyn Any)
    }
    fn put<T: Asset>(world: &mut World, id: UntypedAssetId, v: Hand) -> Result<(), String> {
        let v = v.downcast::<T>().map_err(|_| format!("not a {}", T::type_path()))?;
        let v = Arc::try_unwrap(v).map_err(|_| "its value is still borrowed".to_string())?;
        let mut assets = world.get_resource_mut::<Assets<T>>().ok_or_else(|| format!("no Assets<{}>: call app.init_asset", T::type_path()))?;
        assets.insert(id.typed::<T>(), v).map_err(|e| e.to_string())
    }
    fn remove<T: Asset>(world: &mut World, id: UntypedAssetId) {
        if let Some(mut assets) = world.get_resource_mut::<Assets<T>>() {
            assets.remove(id.typed::<T>());
        }
    }
    HandMap {
        id: TypeId::of::<T>(),
        path: T::type_path(),
        read: Box::new(move |slots, source, v| read(slots, source, typed::<T>(v)?)),
        write: Box::new(move |slots, base, json| {
            let base = base.map(typed::<T>).transpose()?;
            write(slots, base, json).map(|v| Arc::new(v) as Hand)
        }),
        get: get::<T>,
        put: put::<T>,
        remove: remove::<T>,
        at,
    }
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

/// `ref.id`: the asset's `AssetId<A>` read as `reflect_value` reads it in an `AssetEvent<A>`. Its
/// shape does not depend on `A`, so it is read through one fixed type.
pub fn id_json(registry: &TypeRegistry, id: UntypedAssetId) -> Result<Value, String> {
    let typed: bevy::asset::AssetId<bevy::asset::LoadedFolder> = id.typed_unchecked();
    crate::reflect_value::read(registry, &typed)
}

/// The inverse of [`id_json`]: `json`, an `AssetId` in its shape, as the id of an asset of type
/// `type_id` (an `AssetId<A>` from TS: `ref.id`, or an `AssetEvent<A>`'s).
pub fn id_of_json(registry: &TypeRegistry, type_id: TypeId, json: &Value) -> Result<UntypedAssetId, String> {
    let mut typed = bevy::asset::AssetId::<bevy::asset::LoadedFolder>::default();
    crate::reflect_value::write(registry, None, &mut typed, None, json).map_err(|e| format!("not an AssetId: {e}"))?;
    Ok(match typed {
        bevy::asset::AssetId::Index { index, .. } => UntypedAssetId::Index { type_id, index },
        bevy::asset::AssetId::Uuid { uuid } => UntypedAssetId::Uuid { type_id, uuid },
    })
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
        // As `BevyTsPlugin` does; the map is process-wide, so every test sees `Image` hand-mapped.
        register_image();
        app
    }

    /// A made `Image`, as a hand-mapped value.
    fn blank() -> Built {
        Built::Hand(Arc::new(Image::default()))
    }

    fn image(app: &App) -> AssetType {
        asset_type(&app.world().resource::<AppTypeRegistry>().read(), "bevy_image::image::Image").expect("Image is an asset")
    }

    /// An app's own `Mesh` map, told apart from bevy-ts' by what it reads.
    #[cfg(feature = "pbr")]
    fn app_mesh_read(_: &Slots, _: Option<&Value>, _: &Mesh) -> Result<Value, String> {
        Ok(Value::from("app's"))
    }

    #[cfg(feature = "pbr")]
    fn reads(h: &HandMap) -> Value {
        let world = World::new();
        let registry = TypeRegistry::new();
        let assets = ScriptAssets::default();
        let slots = Slots { registry: &registry, assets: &assets, server: None, world: Some(&world), check: false };
        (h.read)(&slots, None, &Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, Default::default())).unwrap()
    }

    #[cfg(feature = "pbr")]
    #[test]
    fn an_apps_own_hand_map_wins_over_bevy_ts_own() {
        let at = Some(std::panic::Location::caller());
        // The app maps `Mesh` before the first lookup, which adds bevy-ts' own after it.
        let mut hands = Vec::new();
        insert(&mut hands, entry::<Mesh>(app_mesh_read, crate::mesh_data::write, at)).unwrap();
        insert(&mut hands, entry::<Mesh>(crate::mesh_data::read, crate::mesh_data::write, None)).unwrap();
        assert_eq!(hands.len(), 1);
        assert_eq!(reads(&hands[0]), Value::from("app's"));
        // And after it: the app's replaces bevy-ts' own.
        let mut hands = Vec::new();
        insert(&mut hands, entry::<Mesh>(crate::mesh_data::read, crate::mesh_data::write, None)).unwrap();
        insert(&mut hands, entry::<Mesh>(app_mesh_read, crate::mesh_data::write, at)).unwrap();
        assert_eq!(hands.len(), 1);
        assert_eq!(reads(&hands[0]), Value::from("app's"));
    }

    #[cfg(feature = "pbr")]
    #[test]
    fn an_app_mapping_a_type_twice_is_refused_naming_both() {
        let mut hands = Vec::new();
        let first = std::panic::Location::caller();
        insert(&mut hands, entry::<Mesh>(app_mesh_read, crate::mesh_data::write, Some(first))).unwrap();
        // The same place again is the same registration in another app of the process.
        insert(&mut hands, entry::<Mesh>(app_mesh_read, crate::mesh_data::write, Some(first))).unwrap();
        let second = std::panic::Location::caller();
        let e = insert(&mut hands, entry::<Mesh>(crate::mesh_data::read, crate::mesh_data::write, Some(second))).unwrap_err();
        assert_eq!(e, format!("bevy_mesh::mesh::Mesh is hand-mapped twice: at {first} and at {second}"));
        assert_eq!(reads(&hands[0]), Value::from("app's"), "the first stays");
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
    fn made_assets_enter_assets_on_flush_and_leave_on_a_restart() {
        let mut app = app();
        let ty = image(&app);
        let (n, h) = app.world().resource::<ScriptAssets>().make(&ty, blank());
        assert_eq!(n, 1);
        let id = h.id().typed::<Image>();
        assert!(app.world().resource::<Assets<Image>>().get(id).is_none(), "queued until the run ends");
        flush(app.world_mut());
        assert!(app.world().resource::<Assets<Image>>().get(id).is_some());
        assert_eq!(app.world().resource::<ScriptAssets>().ref_of(h.id()), Some(1));
        reload(app.world_mut(), false);
        assert!(app.world().resource::<Assets<Image>>().get(id).is_some(), "a hot reload keeps it for its entities");
        reload(app.world_mut(), true);
        assert!(app.world().resource::<Assets<Image>>().get(id).is_none(), "a restart removes it");
        let t = app.world().resource::<ScriptAssets>();
        assert!(t.handle(1).unwrap_err().starts_with("stale asset ref"));
        assert!(t.handle(9).unwrap_err().starts_with("unknown asset ref"));
        assert_eq!(t.make(&ty, blank()).0, 2, "never reused in a session");
    }

    /// A reload that applies.
    fn reload(world: &mut World, restart: bool) {
        let r = reloading(world, restart);
        applied(world, r);
    }

    #[test]
    fn a_failed_hot_reload_keeps_the_live_refs_and_drops_its_own() {
        let mut app = app();
        let ty = image(&app);
        let (old, _) = app.world().resource::<ScriptAssets>().make(&ty, blank());
        flush(app.world_mut());
        let r = reloading(app.world_mut(), false);
        // The new generation adds while it loads, and its first render's assets go in.
        let (new, h) = app.world().resource::<ScriptAssets>().make(&ty, blank());
        flush(app.world_mut());
        failed(app.world_mut(), r);
        let t = app.world().resource::<ScriptAssets>();
        assert!(t.handle(old).is_ok(), "the live generation's ref still works");
        assert!(t.handle(new).unwrap_err().starts_with("stale asset ref"));
        assert!(app.world().resource::<Assets<Image>>().get(h.id().typed::<Image>()).is_none(), "the failed generation's asset is gone");
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
    }

    #[test]
    fn a_hot_reload_makes_the_old_refs_stale_once_it_applies() {
        let mut app = app();
        let ty = image(&app);
        let (old, _) = app.world().resource::<ScriptAssets>().make(&ty, blank());
        flush(app.world_mut());
        let r = reloading(app.world_mut(), false);
        assert!(app.world().resource::<ScriptAssets>().handle(old).is_ok(), "live while the new one loads");
        let (new, _) = app.world().resource::<ScriptAssets>().make(&ty, blank());
        applied(app.world_mut(), r);
        let t = app.world().resource::<ScriptAssets>();
        assert!(t.handle(old).unwrap_err().starts_with("stale asset ref"));
        assert!(t.handle(new).is_ok(), "what the new generation made while loading is its own");
    }

    #[test]
    fn hot_reloads_keep_assets_until_a_restart() {
        // Each hot reload re-runs the script's adds, so kept assets pile up per reload: a dev-only
        // limit (only `bevy-ts dev` hot reloads), bounded by the next restart or Stop.
        let mut app = app();
        let ty = image(&app);
        let mut ids = Vec::new();
        for _ in 0..2 {
            let (_, h) = app.world().resource::<ScriptAssets>().make(&ty, blank());
            ids.push(h.id().typed::<Image>());
            flush(app.world_mut());
            reload(app.world_mut(), false);
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 2, "both generations' assets kept");
        reload(app.world_mut(), true);
        assert!(ids.iter().all(|&id| app.world().resource::<Assets<Image>>().get(id).is_none()));
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn a_ref_of_another_type_names_both() {
        let app = app();
        let (n, _) = app.world().resource::<ScriptAssets>().make(&image(&app), blank());
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
    fn a_mesh_builds_by_its_hand_map_from_an_empty_triangle_list() {
        let app = app();
        let r = call(&app, "assets.add", serde_json::json!(["bevy_mesh::mesh::Mesh", {}])).unwrap();
        let read = call(&app, "assets.get", serde_json::json!([r])).unwrap();
        assert_eq!(read["primitive_topology"], "TriangleList");
        assert_eq!(read["attributes"], serde_json::json!({}));
        assert_eq!(read["indices"], serde_json::Value::Null);
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
            let (n, _) = app.world().resource::<ScriptAssets>().make(&ty, blank());
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
        reload(app.world_mut(), true);
        let e = call(&app, "assets.get", serde_json::json!([r])).unwrap_err();
        assert!(e.contains("stale asset ref"), "{e}");
    }

    #[test]
    fn equal_values_share_one_asset() {
        let app = pbr_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let slots = Slots { registry: &registry, server: None, assets, world: None, check: false };
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

    /// A fixture asset that is not `Reflect`, hand-mapped as `{ text, count }`.
    #[derive(Asset, TypePath, Default, Clone, PartialEq)]
    struct Note {
        text: String,
        count: u32,
    }

    fn note_read(_: &Slots, _: Option<&Value>, n: &Note) -> Result<Value, String> {
        Ok(serde_json::json!({ "text": n.text, "count": n.count }))
    }

    fn note_write(_: &Slots, base: Option<&Note>, json: &Value) -> Result<Note, String> {
        let mut n = base.cloned().unwrap_or_default();
        if let Some(t) = json.get("text") {
            n.text = t.as_str().ok_or("text is a string")?.to_string();
        }
        if let Some(c) = json.get("count") {
            n.count = c.as_u64().ok_or("count is a number")? as u32;
        }
        Ok(n)
    }

    const NOTE: &str = "bevy_ts::reflect_asset::tests::Note";

    fn note_app() -> App {
        use crate::plugin_api::BevyTsAppExt;
        let mut app = app();
        app.init_asset::<Note>().ts_hand_map::<Note>(note_read, note_write);
        app
    }

    fn note(app: &App, n: u64) -> Option<Note> {
        let h = app.world().resource::<ScriptAssets>().handle(n).unwrap();
        app.world().resource::<Assets<Note>>().get(h.id().typed::<Note>()).cloned()
    }

    #[test]
    fn a_hand_mapped_type_reads_and_writes_by_its_entry() {
        let mut app = note_app();
        let r = call(&app, "assets.add", serde_json::json!([NOTE, { "text": "hi", "count": 1 }])).unwrap();
        assert_eq!(call(&app, "assets.get", serde_json::json!([r])).unwrap(), serde_json::json!({ "text": "hi", "count": 1 }));
        flush(app.world_mut());
        call(&app, "assets.set", serde_json::json!([r, { "count": 1 }])).unwrap();
        assert!(app.world().resource::<ScriptAssets>().table().queued.is_empty(), "equal reads queue nothing");
        call(&app, "assets.set", serde_json::json!([r, { "count": 2 }])).unwrap();
        assert_eq!(call(&app, "assets.get", serde_json::json!([r])).unwrap()["count"], 2, "get reads the queued set");
        flush(app.world_mut());
        let n = note(&app, 1).expect("in Assets<Note>");
        assert_eq!((n.text.as_str(), n.count), ("hi", 2), "the patch merged over the current read");
        let h = app.world().resource::<ScriptAssets>().handle(1).unwrap();
        assert_eq!(app.world().resource::<ScriptAssets>().source(h.id()), Some(serde_json::json!({ "text": "hi", "count": 2 })));
    }

    #[test]
    fn a_hand_mapped_type_needs_no_reflect_and_goes_in_its_assets_at_the_refs_id() {
        let mut app = note_app();
        assert!(app.world().resource::<AppTypeRegistry>().read().get(TypeId::of::<Note>()).is_none(), "Note is not registered");
        assert!(is_hand_mapped(TypeId::of::<Note>()) && !is_hand_mapped(TypeId::of::<StandardMaterial>()));
        call(&app, "assets.add", serde_json::json!([NOTE, { "text": "a" }])).unwrap();
        assert!(note(&app, 1).is_none(), "queued until the run ends");
        flush(app.world_mut());
        assert_eq!(note(&app, 1).map(|n| n.text), Some("a".to_string()));
        reload(app.world_mut(), true);
        assert_eq!(app.world().resource::<Assets<Note>>().len(), 0, "a restart removes it");
    }

    #[test]
    fn a_hand_mapped_value_is_shared_and_reads_back_as_its_ref() {
        let app = note_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let slots = Slots { registry: &registry, server: None, assets, world: None, check: false };
        use reflect_value::HandleSlots;
        let id = TypeId::of::<Note>();
        let a = slots.write(id, &serde_json::json!({ "$of": NOTE, "props": { "text": "x", "count": 1 } })).unwrap();
        let b = slots.write(id, &serde_json::json!({ "$of": NOTE, "props": { "count": 1, "text": "x" } })).unwrap();
        assert_eq!(a.id(), b.id());
        assert_eq!(assets.len(), 1);
        assert_eq!(slots.read(&a), Some(ref_json(1, NOTE)), "a ref names the hand-mapped type's path");
        let e = slots.write(TypeId::of::<Image>(), &ref_json(1, NOTE)).unwrap_err();
        assert!(e.contains("Note") && e.contains("Image"), "{e}");
        let check = Slots { registry: &registry, server: None, assets: &ScriptAssets::default(), world: None, check: true };
        check.write(id, &ref_json(9, NOTE)).expect("a check knows the hand-mapped type by its path");
    }

    #[test]
    fn a_ref_passes_a_check_by_its_type_and_fails_with_no_hook() {
        let app = pbr_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = ScriptAssets::default();
        let slots = Slots { registry: &registry, server: None, assets: &assets, world: None, check: true };
        let mut m = StandardMaterial::default();
        let img = ref_json(7, IMG);
        reflect_value::write_with(slots.cx(), m.as_partial_reflect_mut(), None, &serde_json::json!({ "base_color_texture": img })).expect("a ref the table does not know passes a check by its type");
        let mat = ref_json(7, MAT);
        let e = reflect_value::write_with(slots.cx(), m.as_partial_reflect_mut(), None, &serde_json::json!({ "base_color_texture": mat })).unwrap_err();
        assert!(e.contains("StandardMaterial") && e.contains("Image"), "{e}");
        let e = reflect_value::write(&registry, None, m.as_partial_reflect_mut(), None, &serde_json::json!({ "base_color_texture": img })).unwrap_err();
        assert!(e.contains("Handle"), "with no hook a ref fails naming the handle type: {e}");
    }

    #[test]
    fn check_mode_builds_nothing() {
        let app = pbr_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let slots = Slots { registry: &registry, server: None, assets, world: None, check: true };
        use reflect_value::HandleSlots;
        slots.write(TypeId::of::<StandardMaterial>(), &serde_json::json!({ "$of": MAT, "props": { "unlit": true } })).unwrap();
        assert!(assets.is_empty());
        let e = slots.write(TypeId::of::<StandardMaterial>(), &serde_json::json!({ "$of": MAT, "props": { "nope": 1 } })).unwrap_err();
        assert!(e.contains("nope"), "{e}");
    }

    fn image_of(app: &App, r: &Value) -> Image {
        let h = app.world().resource::<ScriptAssets>().handle(parse_ref(r).unwrap().unwrap()).unwrap();
        app.world().resource::<Assets<Image>>().get(h.id().typed::<Image>()).expect("added").clone()
    }

    #[test]
    fn an_images_sampler_is_set_and_read_back() {
        let mut app = app();
        let r = call(&app, "assets.add", serde_json::json!([IMG, {}])).unwrap();
        assert_eq!(call(&app, "assets.get", serde_json::json!([r])).unwrap(), serde_json::json!({ "sampler": "Default" }));
        let repeat = serde_json::json!({ "Descriptor": { "address_mode_u": "Repeat", "address_mode_v": "Repeat" } });
        call(&app, "assets.set", serde_json::json!([r, { "sampler": repeat }])).unwrap();
        flush(app.world_mut());
        let ImageSampler::Descriptor(d) = image_of(&app, &r).sampler else { panic!("a descriptor sampler") };
        let want = ImageSamplerDescriptor { address_mode_u: bevy::image::ImageAddressMode::Repeat, address_mode_v: bevy::image::ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() };
        assert_eq!(d, want, "the fields not given keep the default plugin's linear sampler");
        let got = call(&app, "assets.get", serde_json::json!([r])).unwrap();
        assert_eq!(got["sampler"]["Descriptor"]["address_mode_u"], "Repeat");
        assert_eq!(got["sampler"]["Descriptor"]["anisotropy_clamp"], 1);
        assert_eq!(image_of(&app, &r).size(), Image::default().size(), "its pixels are the base's");
        let nearest = serde_json::json!({ "Descriptor": { "mag_filter": "Nearest" } });
        call(&app, "assets.set", serde_json::json!([r, { "sampler": nearest }])).unwrap();
        flush(app.world_mut());
        let ImageSampler::Descriptor(d) = image_of(&app, &r).sampler else { panic!("a descriptor sampler") };
        assert_eq!((d.address_mode_u, d.mag_filter), (bevy::image::ImageAddressMode::Repeat, bevy::image::ImageFilterMode::Nearest), "written over the current sampler");
        let e = call(&app, "assets.set", serde_json::json!([r, { "data": [] }])).unwrap_err();
        assert!(e.contains("data") && e.contains("sampler"), "{e}");
    }

    #[test]
    fn an_image_round_trips_from_nothing() {
        let app = app();
        let first = call(&app, "assets.add", serde_json::json!([IMG, {}])).unwrap();
        let read = call(&app, "assets.get", serde_json::json!([first])).unwrap();
        let again = call(&app, "assets.add", serde_json::json!([IMG, read])).unwrap();
        assert_eq!(call(&app, "assets.get", serde_json::json!([again])).unwrap(), read);
        let nearest = serde_json::json!({ "sampler": { "Descriptor": { "mag_filter": "Nearest" } } });
        let n = call(&app, "assets.add", serde_json::json!([IMG, nearest])).unwrap();
        let read = call(&app, "assets.get", serde_json::json!([n])).unwrap();
        let again = call(&app, "assets.add", serde_json::json!([IMG, read])).unwrap();
        assert_eq!(call(&app, "assets.get", serde_json::json!([again])).unwrap(), read);
    }

    #[test]
    fn image_load_takes_its_settings_and_refuses_others() {
        use reflect_value::HandleSlots;
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let assets = app.world().resource::<ScriptAssets>();
        let server = app.world().resource::<AssetServer>();
        let slots = Slots { registry: &registry, server: Some(server), assets, world: None, check: false };
        let id = TypeId::of::<Image>();
        let linear = serde_json::json!({ "$load": "a.png", "settings": { "is_srgb": false, "sampler": { "Descriptor": { "address_mode_u": "Repeat" } } } });
        let a = slots.write(id, &linear).unwrap();
        assert_eq!(slots.write(id, &linear).unwrap().id(), a.id(), "one path, one image");
        assert_eq!(assets.table().loads.get("a.png").map(String::as_str), Some(canonical(&linear["settings"]).as_str()));
        slots.write(id, &serde_json::json!({ "$load": "a.png" })).expect("other settings warn and keep the first");
        assert_eq!(assets.table().loads.get("a.png").map(String::as_str), Some(canonical(&linear["settings"]).as_str()));
        let e = slots.write(id, &serde_json::json!({ "$load": "b.png", "settings": { "srgb": false } })).unwrap_err();
        assert!(e.contains("srgb") && e.contains("is_srgb"), "{e}");
        let e = slots.write(id, &serde_json::json!({ "$load": "b.png", "settings": { "is_srgb": 0 } })).unwrap_err();
        assert!(e.contains("is_srgb"), "{e}");
        let e = slots.write(TypeId::of::<Mesh>(), &serde_json::json!({ "$load": "m.glb" })).unwrap_err();
        assert!(e.contains("Mesh") && e.contains("no loader settings"), "{e}");
        let check = Slots { registry: &registry, server: None, assets, world: None, check: true };
        check.write(id, &serde_json::json!({ "$load": "c.png", "settings": { "is_srgb": true } })).expect("a check loads nothing");
        assert!(check.write(id, &serde_json::json!({ "$load": "c.png", "settings": { "nope": 1 } })).is_err());
        assert!(!assets.table().loads.contains_key("c.png"));
        let none = Slots { registry: &registry, server: None, assets, world: None, check: false };
        assert!(none.write(id, &serde_json::json!({ "$load": "d.png" })).unwrap_err().contains("AssetServer"));
        assert!(loadable(IMG) && !loadable(MAT));
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
