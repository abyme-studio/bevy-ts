//! Per-entity `instance` params (design decision 4): one global component, [`ScriptInstance`],
//! packed by Bevy's `GpuComponentArrayBuffer` into one storage buffer that every kind with
//! instance params binds after its textures; each entity's index is its `MeshTag`. Opt-in: only
//! an entity of a kind that declares `instance` gets one. A `MeshTag` bevy-ts did not set is kept,
//! and the entity gets no `ScriptInstance`.

use crate::layout::{Bindings, INSTANCE_BYTES};
use crate::render::{ScriptMaterial, ScriptMaterial3d};
use bevy::ecs::query::QueryItem;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::gpu_component_array_buffer::{GpuComponentArray, GpuComponentArrayBuffer, GpuComponentArrayBufferPlugin};
use bevy_ts::backend::SourceLocation;
use bevy_ts::log::{LogLevel, ScriptLog};
use serde_json::{Map, Value};

/// Words of one entity's instance params: [`INSTANCE_BYTES`] bytes.
pub const WORDS: usize = INSTANCE_BYTES / 4;

/// One entity's instance params, laid out by its kind's instance struct.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ScriptInstance {
    pub data: [u32; WORDS],
    /// The kind it is laid out for, so a changed schema re-encodes it by name.
    pub kind: String,
    /// The element it came from, for errors on the meshes a glTF root gives it to.
    pub at: Option<SourceLocation>,
}

impl ScriptInstance {
    /// The value of one instance param, decoded by `bindings`.
    pub fn get(&self, bindings: &Bindings, name: &str) -> Option<Value> {
        bindings.instance().decode(name, &bytes(&self.data))
    }
}

/// On an entity whose own `MeshTag` kept it from taking instance params, once reported.
#[derive(Component, Clone, Debug)]
pub struct KeptMeshTag;

impl GpuComponentArrayBuffer for ScriptInstance {
    type QueryData = &'static ScriptInstance;
    // Only meshes draw it (a glTF root holds one for its meshes); a quiet frame packs nothing.
    type QueryFilter = (With<Mesh3d>, Or<(Changed<ScriptInstance>, Added<Mesh3d>)>);
    type Out = [u32; WORDS];

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        Some(item.data)
    }

    fn label() -> std::borrow::Cow<'static, str> {
        "script_material instances".into()
    }
}

/// The buffer needs Bevy's `ShaderBuffer` assets: an app with no renderer packs nothing, and its
/// entities still hold their `ScriptInstance`.
pub(crate) fn plugin(app: &mut App) {
    if app.world().contains_resource::<Assets<bevy::render::storage::ShaderBuffer>>() {
        app.add_plugins(GpuComponentArrayBufferPlugin::<ScriptInstance>::default());
    }
}

/// The storage buffer every kind with instance params binds.
pub fn buffer(world: &World) -> Option<Handle<bevy::render::storage::ShaderBuffer>> {
    world.get_resource::<GpuComponentArray<ScriptInstance>>().map(|a| a.buffer.clone())
}

fn bytes(data: &[u32; WORDS]) -> Vec<u8> {
    data.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn words(bytes: &[u8]) -> [u32; WORDS] {
    let mut out = [0u32; WORDS];
    for (w, c) in out.iter_mut().zip(bytes.chunks(4)) {
        let mut b = [0u8; 4];
        b[..c.len()].copy_from_slice(c);
        *w = u32::from_le_bytes(b);
    }
    out
}

/// `props` encoded by the kind's instance layout over its defaults (zeros, colours white). A
/// value that does not encode is reported and keeps its default.
fn encode(world: &mut World, kind: &str, bindings: &Bindings, props: &Map<String, Value>, at: &Option<SourceLocation>) -> [u32; WORDS] {
    let layout = bindings.instance();
    let mut data = layout.defaults();
    for (p, v) in props {
        if v.is_null() {
            continue;
        }
        if let Err(e) = layout.encode(p, v, &mut data) {
            log(world, format!("<material kind={{{kind}}}>: {e}"), at.clone());
        }
    }
    words(&data)
}

fn log(world: &mut World, message: String, at: Option<SourceLocation>) {
    let generation = bevy_ts::runtime::current_generation(world);
    if let Some(mut l) = world.get_resource_mut::<ScriptLog>() {
        l.push(LogLevel::Error, message, at, generation);
    }
}

/// Gives `e` instance params `si`: written in place when it has them, inserted when it has no
/// `MeshTag`, refused (and reported once) when it has a `MeshTag` bevy-ts did not set.
pub fn put(world: &mut World, e: Entity, si: ScriptInstance) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    if let Some(mut old) = em.get_mut::<ScriptInstance>() {
        if old.data != si.data || old.kind != si.kind {
            *old = si;
        }
        return;
    }
    if em.contains::<MeshTag>() {
        if !em.contains::<KeptMeshTag>() {
            em.insert(KeptMeshTag);
            let message = format!(
                "<material kind={{{}}}>: entity {e} has a MeshTag bevy-ts did not set; it keeps it and takes no instance params (one MeshTag per entity)",
                si.kind
            );
            log(world, message, si.at);
        }
        return;
    }
    em.insert(si);
}

/// Takes `e`'s instance params, and the `MeshTag` that indexed them, away. Bevy's buffer drops
/// its row; a tag it did not set is never touched.
pub fn remove(world: &mut World, e: Entity) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    em.remove::<KeptMeshTag>();
    if em.take::<ScriptInstance>().is_some() {
        em.remove::<MeshTag>();
    }
}

/// `<material kind>`'s instance props on `e`, by the live kind's bindings: inserted or written for
/// a kind with instance params, removed for one without.
pub fn set(world: &mut World, e: Entity, kind: &str, bindings: &Bindings, props: &Map<String, Value>, at: Option<SourceLocation>) {
    if !bindings.has_instance() {
        remove(world, e);
        return;
    }
    let data = encode(world, kind, bindings, props, &at);
    put(world, e, ScriptInstance { data, kind: kind.into(), at });
}

/// `<material kind={K} ...props />` on `e`: the shared asset for its params and textures, and its
/// instance props on the entity. `None` for a kind that is not live.
pub fn apply(world: &mut World, e: Entity, kind: &str, props: &Map<String, Value>, at: Option<SourceLocation>) -> Option<Handle<ScriptMaterial>> {
    let (key, instance, bindings) = crate::kinds::split_props(world, kind, props)?;
    let h = crate::kinds::instance(world, kind, &key)?;
    set(world, e, kind, &bindings, &instance, at);
    Some(h)
}

/// A bound prop of `<material kind>` arrived: `props` are the element's values now. Only an
/// instance param may be bound; it writes the entity's `ScriptInstance`, never the asset.
pub fn patch(world: &mut World, e: Entity, kind: &str, prop: &str, props: &Map<String, Value>, at: Option<SourceLocation>) -> Result<(), String> {
    let (_, instance, bindings) = crate::kinds::split_props(world, kind, props).ok_or_else(|| format!("no material {kind}"))?;
    if bindings.instance().field(prop).is_none() {
        return Err(crate::kinds::BOUND.into());
    }
    set(world, e, kind, &bindings, &instance, at);
    Ok(())
}

/// `<material asset={ref} />` on `e`: an owned instance's entity reads its kind's instance params
/// at their defaults, never another entity's row.
pub fn owned(world: &mut World, e: Entity, h: &Handle<ScriptMaterial>, at: Option<SourceLocation>) {
    let Some(kind) = world.resource::<Assets<ScriptMaterial>>().get(h).map(|m| m.kind.clone()) else { return };
    set(world, e, &kind.name, &kind.bindings, &Map::new(), at);
}

/// A kind's schema changed: each `ScriptInstance` of the kind is re-encoded by name (`old`'s
/// layout to `new`'s), or removed when `new` has no instance params; and when it has some, each
/// entity drawing the kind without one gets one at the defaults.
pub fn reencode(world: &mut World, kind: &str, old: Option<&Bindings>, new: &Bindings) {
    let mut q = world.query::<(Entity, &ScriptInstance)>();
    let had: Vec<(Entity, ScriptInstance)> = q.iter(world).filter(|(_, s)| s.kind == kind).map(|(e, s)| (e, s.clone())).collect();
    for (e, s) in &had {
        if !new.has_instance() {
            remove(world, *e);
            continue;
        }
        let props: Map<String, Value> = match old {
            Some(o) if o.instance() != new.instance() => o.instance().fields.iter().filter_map(|f| s.get(o, &f.name).map(|v| (f.name.clone(), v))).collect(),
            _ => continue,
        };
        let props = props.into_iter().filter(|(p, _)| new.instance().field(p).is_some()).collect();
        set(world, *e, kind, new, &props, s.at.clone());
    }
    if !new.has_instance() {
        return;
    }
    let mut q = world.query_filtered::<(Entity, &ScriptMaterial3d), (Without<ScriptInstance>, Without<KeptMeshTag>)>();
    let materials = world.resource::<Assets<ScriptMaterial>>();
    let missing: Vec<Entity> = q.iter(world).filter(|(_, m)| materials.get(&m.0).is_some_and(|m| m.kind.name == kind)).map(|(e, _)| e).collect();
    for e in missing {
        set(world, e, kind, new, &Map::new(), None);
    }
}
