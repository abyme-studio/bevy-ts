//! Owned instances: `Kind.add(props)` makes a `ScriptMaterial` outside the keyed cache, through
//! reflect-assets' hand map, and `assets.set` writes it in place at the same `AssetId` (design
//! decision 3). Its JSON is `{ kind, ...params, ...textures }`: params decoded from the bytes by
//! the kind's layout, textures as their asset paths (`null` unset).

use crate::kinds::{self, MaterialKinds, ParamDecl};
use crate::layout::Bindings;
use crate::render::{Kind, ScriptMaterial};
use bevy::prelude::*;
use bevy_ts::reflect_asset::Slots;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::Arc;

/// The hand map's read.
pub fn read(_: &Slots, _: Option<&Value>, m: &ScriptMaterial) -> Result<Value, String> {
    let mut out = Map::new();
    out.insert("kind".into(), m.kind.name.clone().into());
    let params = m.kind.bindings.params();
    for f in &params.fields {
        out.insert(f.name.clone(), params.decode(&f.name, &m.data).expect("a field of the layout"));
    }
    for (t, h) in m.kind.bindings.textures().iter().zip(&m.textures) {
        out.insert(t.clone(), h.as_ref().and_then(|h| h.path()).map_or(Value::Null, |p| p.to_string().into()));
    }
    Ok(Value::Object(out))
}

/// The hand map's write: the whole asset from `json`. `kind` is required and, on `assets.set`,
/// the base's; a name the kind lacks fails with the kind's names.
pub fn write(slots: &Slots, base: Option<&ScriptMaterial>, json: &Value) -> Result<ScriptMaterial, String> {
    let o = json.as_object().ok_or_else(|| format!("a ScriptMaterial is an object, not {json}"))?;
    let name = match o.get("kind") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Object(k)) if k.get("name").is_some_and(Value::is_string) => k["name"].as_str().expect("a string"),
        _ => return Err("a ScriptMaterial needs its kind: Kind.add(props) from material() in bevy-ts/material".into()),
    };
    if let Some(b) = base
        && b.kind.name != name
    {
        return Err(format!("a {} cannot become a {name}: its kind is fixed when it is added", b.kind.name));
    }
    let Some(kind) = kind_of(slots, base, name, o.get("$schema"))? else {
        // A check has no world: it sees the form only.
        return Ok(ScriptMaterial { kind: Arc::new(Kind::new(name, Bindings::new(name, [], &[])?, None, None, AlphaMode::Opaque)), data: Vec::new(), textures: Vec::new() });
    };
    let b = &kind.bindings;
    let mut data = b.params().defaults();
    let mut textures = vec![None; b.textures().len()];
    for (p, v) in o.iter().filter(|(p, _)| !["kind", "$schema"].contains(&p.as_str())) {
        if b.params().field(p).is_some() {
            b.params().encode(p, v, &mut data).map_err(|e| format!("{name}: {e}"))?;
        } else if let Some(i) = b.textures().iter().position(|t| t == p) {
            textures[i] = match v {
                Value::Null => None,
                Value::String(path) => match slots.server {
                    Some(s) => Some(s.load(path.clone())),
                    None if slots.check => None,
                    None => return Err(format!("{name}: texture {p} needs an AssetServer")),
                },
                _ => return Err(format!("{name}: texture {p} takes an asset path")),
            };
        } else if b.instance().field(p).is_some() {
            return Err(format!("{name}: {p} is an instance param, held by each entity: give it on <material kind={{{name}}}>, not on an owned instance"));
        } else {
            return Err(kinds::no_such_prop(name, b, p));
        }
    }
    Ok(ScriptMaterial { kind, data, textures })
}

/// What `$schema` holds: the kind's params, textures and instance params as `material()` declared
/// them.
#[derive(Deserialize)]
struct Schema {
    params: Vec<ParamDecl>,
    #[serde(default)]
    textures: Vec<String>,
    #[serde(default)]
    instance: Vec<ParamDecl>,
}

/// The kind an instance is written against. On `assets.set`, the live kind, else the base's: the
/// kind that draws, also while a newer declaration waits on its shaders (`KindState::wanted`), so a
/// patch is checked against the names that draw now. On
/// `assets.add`, the live kind when its bindings equal `schema`'s, else a kind made from `schema`
/// that draws nothing until the generation's kinds apply and re-encode it (`kinds::apply`): an
/// instance added while the script loads comes before its kind. `None` in a check with no world.
fn kind_of(slots: &Slots, base: Option<&ScriptMaterial>, name: &str, schema: Option<&Value>) -> Result<Option<Arc<Kind>>, String> {
    let live = match slots.world {
        Some(world) => world.get_resource::<MaterialKinds>().ok_or("script materials need ScriptMaterialPlugin")?.live.get(name).map(|k| k.kind.clone()),
        None => None,
    };
    if let Some(b) = base {
        return Ok(Some(live.unwrap_or_else(|| b.kind.clone())));
    }
    if slots.world.is_none() && !slots.check {
        return Err(format!("{name}.add needs the world: call it from a system, a render or while the script loads"));
    }
    let Some(schema) = schema else {
        return match live {
            Some(k) => Ok(Some(k)),
            None if slots.check => Ok(None),
            None => Err(format!("no material {name}; declare it with material({name:?}, ...) from bevy-ts/material")),
        };
    };
    let schema: Schema = serde_json::from_value(schema.clone()).map_err(|e| format!("{name}: its schema: {e}"))?;
    let bindings = Bindings::new(name, schema.params.iter().map(|p| (p.name.as_str(), &p.ty)), &schema.textures)
        .and_then(|b| b.with_instance(schema.instance.iter().map(|p| (p.name.as_str(), &p.ty))))
        .map_err(|e| format!("material {name}: {e}"))?;
    Ok(Some(match live {
        Some(k) if k.bindings == bindings => k,
        _ => Arc::new(Kind::new(name, bindings, None, None, AlphaMode::Opaque)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Base, ParamType};
    use bevy_ts::reflect_asset::ScriptAssets;
    use bevy::reflect::TypeRegistry;
    use serde_json::json;

    fn glow() -> Arc<Kind> {
        let p = [("color".to_string(), ParamType::Base(Base::Color)), ("pulse".to_string(), ParamType::Base(Base::F32))];
        let b = Bindings::new("game::Glow", p.iter().map(|(n, t)| (n.as_str(), t)), &["noise".into()]).unwrap();
        Arc::new(Kind::new("game::Glow", b, None, None, AlphaMode::Opaque))
    }

    fn with_slots<R>(world: Option<&World>, f: impl FnOnce(&Slots) -> R) -> R {
        let registry = TypeRegistry::default();
        let assets = ScriptAssets::default();
        f(&Slots { registry: &registry, server: None, assets: &assets, world, check: false })
    }

    fn base() -> ScriptMaterial {
        let k = glow();
        ScriptMaterial { data: k.bindings.params().defaults(), textures: vec![None], kind: k }
    }

    #[test]
    fn reads_back_what_it_writes() {
        let b = base();
        with_slots(None, |s| {
            let m = write(s, Some(&b), &json!({ "kind": "game::Glow", "pulse": 3, "color": "#ff0000", "noise": null })).unwrap();
            let r = read(s, None, &m).unwrap();
            assert_eq!(r["kind"], "game::Glow");
            assert_eq!(r["pulse"], 3.0);
            assert_eq!(r["noise"], Value::Null);
            assert_eq!(write(s, Some(&b), &r).unwrap().data, m.data, "its read writes the same bytes");
        });
    }

    #[test]
    fn unknown_names_and_another_kind_fail() {
        let b = base();
        with_slots(None, |s| {
            let e = write(s, Some(&b), &json!({ "kind": "game::Glow", "speed": 1 })).map(|_| ()).unwrap_err();
            assert_eq!(e, "game::Glow has no param or texture speed; it has color, pulse, noise");
            let e = write(s, Some(&b), &json!({ "kind": "game::Other" })).map(|_| ()).unwrap_err();
            assert!(e.contains("cannot become a game::Other"), "{e}");
            assert!(write(s, Some(&b), &json!({ "pulse": 1 })).map(|_| ()).unwrap_err().contains("needs its kind"));
        });
    }

    #[test]
    fn add_with_no_world_fails_saying_so() {
        with_slots(None, |s| {
            let e = write(s, None, &json!({ "kind": "game::Glow" })).map(|_| ()).unwrap_err();
            assert!(e.contains("needs the world"), "{e}");
        });
    }

    #[test]
    fn add_before_its_kind_is_live_encodes_by_the_schema_it_carries() {
        let mut world = World::new();
        world.init_resource::<MaterialKinds>();
        with_slots(Some(&world), |s| {
            let e = write(s, None, &json!({ "kind": "game::Glow" })).map(|_| ()).unwrap_err();
            assert!(e.contains("no material game::Glow"), "{e}");
            let schema = json!({ "params": [{ "name": "color", "type": "color" }, { "name": "pulse", "type": "f32" }], "textures": ["noise"] });
            let m = write(s, None, &json!({ "kind": "game::Glow", "pulse": 2, "$schema": schema })).unwrap();
            assert_eq!(m.kind.bindings, glow().bindings);
            assert!(m.kind.fragment.is_none(), "drawn once the kinds apply");
            assert_eq!(read(s, None, &m).unwrap()["pulse"], 2.0);
            assert!(read(s, None, &m).unwrap().get("$schema").is_none());
            let e = write(s, None, &json!({ "kind": "game::Glow", "speed": 2, "$schema": schema })).map(|_| ()).unwrap_err();
            assert!(e.contains("has no param or texture speed"), "{e}");
        });
    }
}
