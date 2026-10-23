//! Reflected assets for scripts and TSX: every type registered with `ReflectAsset`, built, read
//! and edited through the one reflected-value writer and reader (`reflect_value`).
//!
//! An asset a script makes is held by [`ScriptAssets`], the ref table: a ref is
//! `{ "$asset": n, "type": path }` in JSON, `n` counting up from 1 and never reused in a session.
//! The table holds a strong handle per ref, so the asset lives until the script reloads or stops
//! (`generation::apply_set` clears it); a ref used after that is a "stale asset ref".
//!
//! Asset values (`T.of(props)`, `{ "$of": path, "props": ... }`) are cached by their type and the
//! props' [`canonical`] JSON, so every slot given equal props shares one asset.

use bevy::asset::{ReflectAsset, UntypedAssetId, UntypedHandle};
use bevy::prelude::*;
use bevy::reflect::TypeRegistry;
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::HashMap;

/// One session's refs past this many log a warning: `assets.add` in a per-frame system grows
/// memory until reload.
const REFS_WARN: usize = 10_000;

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
    let reflect = reg.data::<ReflectAsset>().ok_or_else(|| format!("{type_path} is not a reflected asset"))?.clone();
    Ok(AssetType { id: reg.type_id(), path: reg.type_info().type_path(), reflect })
}

/// The type path of an asset type, for messages.
fn path_of(registry: &TypeRegistry, id: TypeId) -> String {
    registry.get(id).map_or_else(|| format!("{id:?}"), |r| r.type_info().type_path().to_string())
}

/// The ref table: strong handles to the assets scripts made, by ref number and by asset id, and
/// the asset-value cache.
#[derive(Resource, Default)]
pub struct ScriptAssets {
    /// The last ref given; refs below it that are not in `refs` are stale.
    last: u64,
    refs: HashMap<u64, UntypedHandle>,
    by_id: HashMap<UntypedAssetId, u64>,
    values: HashMap<(TypeId, String), u64>,
    warned: bool,
}

impl ScriptAssets {
    /// The ref of `handle`: its existing one, else a new one holding it strong.
    pub fn insert(&mut self, handle: UntypedHandle) -> u64 {
        if let Some(&n) = self.by_id.get(&handle.id()) {
            return n;
        }
        self.last += 1;
        let n = self.last;
        self.by_id.insert(handle.id(), n);
        self.refs.insert(n, handle);
        if self.refs.len() > REFS_WARN && !self.warned {
            self.warned = true;
            warn!("bevy-ts: scripts hold {} asset refs this session; add an asset once and assets.set it after", self.refs.len());
        }
        n
    }

    /// The handle of ref `n`.
    pub fn handle(&self, n: u64) -> Result<&UntypedHandle, String> {
        match self.refs.get(&n) {
            Some(h) => Ok(h),
            None if n >= 1 && n <= self.last => Err(format!("stale asset ref {n}: assets live until the script reloads or stops")),
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
        Ok(h.clone())
    }

    /// The ref of a handle a script made, if any.
    pub fn ref_of(&self, id: UntypedAssetId) -> Option<u64> {
        self.by_id.get(&id).copied()
    }

    /// The ref of the cached value of `asset` with these canonical props.
    pub fn cached(&self, asset: TypeId, key: &str) -> Option<u64> {
        self.values.get(&(asset, key.to_string())).copied()
    }

    /// Caches ref `n` as the value of `asset` with these canonical props.
    pub fn cache(&mut self, asset: TypeId, key: String, n: u64) {
        self.values.insert((asset, key), n);
    }

    /// Drops every handle and cached value; refs given before are stale from now on.
    pub fn clear(&mut self) {
        self.refs.clear();
        self.by_id.clear();
        self.values.clear();
        self.warned = false;
    }

    /// How many refs the table holds.
    pub fn len(&self) -> usize {
        self.refs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }
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
        app.register_type::<Name>();
        app
    }

    #[test]
    fn finds_an_asset_type_by_path() {
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let t = asset_type(&registry, "bevy_image::image::Image").expect("Image is an asset");
        assert_eq!(t.id, TypeId::of::<Image>());
        assert!(asset_type(&registry, "bevy_ts::Nope").map(|_| ()).unwrap_err().contains("not a registered type"));
        let not_asset = asset_type(&registry, "bevy_ecs::name::Name").map(|_| ()).unwrap_err();
        assert!(not_asset.contains("not a reflected asset"), "{not_asset}");
    }

    #[test]
    fn refs_count_up_and_go_stale_on_clear() {
        let mut app = app();
        let h = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default()).untyped();
        let mut table = ScriptAssets::default();
        let n = table.insert(h.clone());
        assert_eq!(n, 1);
        assert_eq!(table.insert(h.clone()), 1, "one asset, one ref");
        assert_eq!(table.ref_of(h.id()), Some(1));
        table.clear();
        assert!(table.handle(1).unwrap_err().starts_with("stale asset ref"));
        assert!(table.handle(9).unwrap_err().starts_with("unknown asset ref"));
        assert_eq!(table.insert(h), 2, "never reused in a session");
    }

    #[test]
    fn a_ref_of_another_type_names_both() {
        let mut app = app();
        app.init_asset::<Mesh>().register_asset_reflect::<Mesh>();
        let h = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default()).untyped();
        let mut table = ScriptAssets::default();
        let n = table.insert(h);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(table.handle_for(&registry, TypeId::of::<Image>(), n).is_ok());
        let e = table.handle_for(&registry, TypeId::of::<Mesh>(), n).unwrap_err();
        assert!(e.contains("Image") && e.contains("Mesh"), "{e}");
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
