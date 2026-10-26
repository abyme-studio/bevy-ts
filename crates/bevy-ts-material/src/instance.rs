//! Per-entity `instance` params (design decision 4): one global component, [`ScriptInstance`],
//! packed into one storage buffer that every kind with instance params binds after its textures;
//! each entity's row is its `MeshTag`. Opt-in: only an entity of a kind that declares `instance`
//! gets one. A `MeshTag` bevy-ts did not set is kept, and the entity gets no `ScriptInstance`.
//!
//! bevy-ts packs the rows itself, not through Bevy's `GpuComponentArrayBuffer`, whose packing
//! system cannot be ordered: an entity that got its values and lost them before it ran panicked
//! Bevy's removal. Here a row is written when [`put`] writes the component, taken by the
//! component's remove hook (a despawn too), and the buffer is uploaded once, in `Last`.

use crate::layout::{Bindings, INSTANCE_BYTES};
use crate::render::{ScriptMaterial, ScriptMaterial3d};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::BufferUsages;
use bevy::render::storage::{ShaderBuffer, ShaderBufferData};
use bevy_ts::backend::SourceLocation;
use bevy_ts::log::{LogLevel, ScriptLog};
use serde_json::{Map, Value};

/// Words of one entity's instance params: [`INSTANCE_BYTES`] bytes.
pub const WORDS: usize = INSTANCE_BYTES / 4;

/// One entity's instance params, laid out by its kind's instance struct. Given and taken only
/// through [`put`] and [`remove`] (a despawn is fine): a `ScriptInstance` removed another way
/// leaves its `MeshTag`, which the next [`put`] would take for a user's.
#[derive(Component, Clone, Debug, PartialEq)]
#[component(on_remove = take_row)]
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

/// Every `ScriptInstance`'s row: row `i` is `entities[i]`'s values, and its `MeshTag` is `i`.
#[derive(Resource, Default)]
pub struct Instances {
    /// The storage buffer, with an app that has a renderer's `ShaderBuffer` assets.
    buffer: Option<Handle<ShaderBuffer>>,
    rows: EntityHashMap<u32>,
    entities: Vec<Entity>,
    data: Vec<[u32; WORDS]>,
    dirty: bool,
}

impl Instances {
    fn write(&mut self, e: Entity, data: [u32; WORDS]) -> u32 {
        let row = match self.rows.get(&e) {
            Some(r) => *r,
            None => {
                let r = self.entities.len() as u32;
                self.rows.insert(e, r);
                self.entities.push(e);
                self.data.push(data);
                r
            }
        };
        self.data[row as usize] = data;
        self.dirty = true;
        row
    }
}

pub(crate) fn plugin(app: &mut App) {
    let buffer = app.world_mut().get_resource_mut::<Assets<ShaderBuffer>>().map(|mut b| {
        b.add(ShaderBuffer {
            data: ShaderBufferData::Initialized { data: vec![0u8; INSTANCE_BYTES].into(), buffer_size: INSTANCE_BYTES as u64 },
            label: "script_material instances".into(),
            buffer_usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            asset_usage: RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            copy_on_resize: true,
        })
    });
    app.insert_resource(Instances { buffer, ..default() }).add_systems(Last, upload);
}

/// The storage buffer every kind with instance params binds.
pub fn buffer(world: &World) -> Option<Handle<ShaderBuffer>> {
    world.get_resource::<Instances>().and_then(|i| i.buffer.clone())
}

/// The values `e`'s `MeshTag` indexes in the buffer, when that row is `e`'s.
pub fn packed(world: &World, e: Entity) -> Option<[u32; WORDS]> {
    let inst = world.get_resource::<Instances>()?;
    let row = world.get::<MeshTag>(e)?.value as usize;
    (inst.entities.get(row) == Some(&e)).then(|| inst.data[row])
}

/// Rows packed: one per `ScriptInstance`.
pub fn rows(world: &World) -> usize {
    world.get_resource::<Instances>().map_or(0, |i| i.entities.len())
}

/// The remove hook: the last row moves into the gap, and its entity's `MeshTag` follows it.
fn take_row(mut world: DeferredWorld, ctx: HookContext) {
    let Some(mut inst) = world.get_resource_mut::<Instances>() else { return };
    let Some(row) = inst.rows.remove(&ctx.entity) else { return };
    inst.entities.swap_remove(row as usize);
    inst.data.swap_remove(row as usize);
    inst.dirty = true;
    let Some(&moved) = inst.entities.get(row as usize) else { return };
    inst.rows.insert(moved, row);
    if let Some(mut tag) = world.get_mut::<MeshTag>(moved) {
        *tag = MeshTag::new(row);
    }
}

/// The rows to the GPU, once a frame when any changed. The buffer grows by half again when full
/// and never shrinks, so a steady count allocates nothing.
fn upload(mut inst: ResMut<Instances>, buffers: Option<ResMut<Assets<ShaderBuffer>>>) {
    if !inst.dirty {
        return;
    }
    inst.dirty = false;
    let Some(mut buffers) = buffers else { return };
    let Some(mut b) = inst.buffer.as_ref().and_then(|h| buffers.get_mut(h)) else { return };
    let ShaderBufferData::Initialized { data, buffer_size } = &mut b.data else { return };
    let needed = inst.data.len().max(1) * INSTANCE_BYTES;
    if data.len() < needed {
        let grown = (data.len() * 3 / 2).max(needed).next_multiple_of(INSTANCE_BYTES);
        data.resize(grown, 0);
        *buffer_size = grown as u64;
    }
    let out: &mut [u8] = data.as_mut();
    let (words, _) = out.as_chunks_mut::<4>();
    for (w, v) in words.iter_mut().zip(inst.data.iter().flatten()) {
        *w = v.to_le_bytes();
    }
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

/// Gives `e` instance params `si`: written in place when it has them, inserted with its row's
/// `MeshTag` when it has no `MeshTag`, refused (and reported once) when it has one bevy-ts did not
/// set. The row is written at once, so the frame's upload carries it.
pub fn put(world: &mut World, e: Entity, si: ScriptInstance) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    if let Some(mut old) = em.get_mut::<ScriptInstance>() {
        if old.data != si.data || old.kind != si.kind {
            let data = si.data;
            *old = si;
            world.resource_mut::<Instances>().write(e, data);
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
    let row = world.resource_mut::<Instances>().write(e, si.data);
    world.entity_mut(e).insert((si, MeshTag::new(row)));
}

/// Takes `e`'s instance params, and the `MeshTag` that indexed them, away: the one way out besides
/// a despawn. The remove hook drops its row; a tag bevy-ts did not set is never touched.
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
/// instance props on the entity. Fails for a kind that is not live, or a texture ref that is stale
/// or of another type.
pub fn apply(world: &mut World, e: Entity, kind: &str, props: &Map<String, Value>, at: Option<SourceLocation>) -> Result<Handle<ScriptMaterial>, String> {
    let (key, instance, bindings) = crate::kinds::split_props(world, kind, props).ok_or_else(|| format!("no material {kind}"))?;
    let h = crate::kinds::instance(world, kind, &key)?;
    set(world, e, kind, &bindings, &instance, at);
    Ok(h)
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
