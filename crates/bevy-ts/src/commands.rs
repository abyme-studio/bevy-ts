//! The binary command buffer: structural changes JS queues during a call, applied by Rust right
//! after it.
//!
//! Format (little-endian, unaligned byte stream, one op after another):
//!
//! | op | byte | body |
//! |---|---|---|
//! | spawn | 1 | entity u64, n u16, n × (component u32, len u32, bytes) |
//! | insert | 2 | entity u64, n u16, n × (component u32, len u32, bytes) |
//! | despawn | 3 | entity u64 |
//! | remove | 4 | entity u64, n u16, n × component u32 |
//! | setParent | 5 | child u64, parent u64 (`u64::MAX` removes the parent) |
//! | spawnNative | 6 | entity u64, name len u16, name, params len u32, params JSON |
//! | plugin | 7 | opcode u16, len u32, bytes |
//! | writeMessage | 8 | message index u32, len u32, bytes |
//! | trigger | 9 | event index u32, target u64 (`u64::MAX` for a global event), len u32, bytes |
//! | setState | 10 | state index u32, value u32 |
//! | observe | 13 | entity u64, observer key index u32, len u32, its system declaration JSON |
//! | stateScoped | 14 | entity u64, kind u8 (bit 7: remove), machine index u32, value index u32 |
//! | saveWorld | 15 | name len u16, name, len u32, options JSON (`saves::SaveOptions`) |
//! | loadWorld | 16 | name len u16, name, parent u64 (`u64::MAX` for none) |
//! | insertReflect | 17 | entity u64, mode u8, type name len u16, name, JSON len u32, JSON (empty: the default) |
//! | spawnBatch | 18 | n u32, n × (entity u64, components as in spawn) |
//! | insertBatch | 19 | n u32, n × (entity u64, components as in insert) |
//! | insertIfNew | 20 | as insert |
//! | tryInsert | 21 | as insert |
//! | retain | 22 | as remove |
//! | clear | 23 | entity u64 |
//! | removeWithRequires | 24 | as remove |
//! | cloneAndSpawn | 25 | source u64, clone u64 |
//! | insertRecursive | 26 | as insert |
//! | removeRecursive | 27 | as remove |
//! | addChildren | 28 | entity u64, n u32, n × child u64 |
//! | detachChildren | 29 | entity u64 |
//! | despawnChildren | 30 | entity u64 |
//! | writeReflect | 31 | type path len u16, type path, len u32, the message's JSON |
//! | triggerReflect | 32 | type path len u16, type path, len u32, the event's JSON |
//!
//! Entities are `Entity::to_bits`. Component values are native layout bytes; a TS component with a
//! heap field (`string`) carries its values after them, in the same `len`: a `u16` count, then per
//! value its field's `u16` index in the layout and the value in the reflect codec's wire format.
//! Rust writes the header and every heap slot (`heap::HeapFill`); JS's bytes there are ignored.
//! Except
//! `insertReflect`'s: a reflected value's JSON, written by `reflect_value` (mode 0 inserts the
//! component from its default, mode 1 writes fields over the one just inserted, a layout value's
//! holes). A spawn or insert and the `insertReflect` ops right after it for the same entity are one
//! command: every reflected value is checked before any of it applies, and a bad one skips it all.

use crate::heap::{HeapBytes, HeapFill};
use bevy::ecs::component::ComponentId;
use bevy::ecs::ptr::OwningPtr;
use bevy::prelude::*;
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::Arc;

pub const OP_SPAWN: u8 = 1;
pub const OP_INSERT: u8 = 2;
pub const OP_DESPAWN: u8 = 3;
pub const OP_REMOVE: u8 = 4;
pub const OP_SET_PARENT: u8 = 5;
pub const OP_SPAWN_NATIVE: u8 = 6;
pub const OP_PLUGIN: u8 = 7;
pub const OP_WRITE_MESSAGE: u8 = 8;
pub const OP_TRIGGER: u8 = 9;
pub const OP_SET_STATE: u8 = 10;
pub const OP_RUN_SYSTEM: u8 = 11;
pub const OP_RUN_SCHEDULE: u8 = 12;
pub const OP_OBSERVE: u8 = 13;
/// A state-scoped component (`DespawnOnExit.of(Game.Playing)`): entity, kind (bit 7: remove),
/// machine index and value index in the generation.
pub const OP_STATE_SCOPED: u8 = 14;
pub const OP_SAVE_WORLD: u8 = 15;
pub const OP_LOAD_WORLD: u8 = 16;
pub const OP_INSERT_REFLECT: u8 = 17;
pub const OP_SPAWN_BATCH: u8 = 18;
pub const OP_INSERT_BATCH: u8 = 19;
pub const OP_INSERT_IF_NEW: u8 = 20;
pub const OP_TRY_INSERT: u8 = 21;
pub const OP_RETAIN: u8 = 22;
pub const OP_CLEAR: u8 = 23;
pub const OP_REMOVE_WITH_REQUIRES: u8 = 24;
pub const OP_CLONE_AND_SPAWN: u8 = 25;
pub const OP_INSERT_RECURSIVE: u8 = 26;
pub const OP_REMOVE_RECURSIVE: u8 = 27;
pub const OP_ADD_CHILDREN: u8 = 28;
pub const OP_DETACH_CHILDREN: u8 = 29;
pub const OP_DESPAWN_CHILDREN: u8 = 30;
pub const OP_WRITE_REFLECT: u8 = 31;
pub const OP_TRIGGER_REFLECT: u8 = 32;

/// `OP_INSERT_REFLECT`'s modes.
pub const REFLECT_INSERT: u8 = 0;
pub const REFLECT_PATCH: u8 = 1;

/// The built-in recipes reflect-coverage removed, each with what replaces it.
const REMOVED_RECIPES: [(&str, &str); 3] = [
    ("camera3d", "commands.spawn(Camera3d, Transform.lookingAt(position, lookAt), Projection.of(projection))"),
    ("directionalLight", "commands.spawn(DirectionalLight.of({ illuminance, shadow_maps_enabled }), Transform.lookingAt(position, lookAt))"),
    ("cuboid", "commands.spawn(Transform.of({ translation }), <entity><mesh shape=\"cuboid\" size={size} /><material color={color} /></entity>)"),
];

/// Size of the Rust-owned arena handed to each call. Larger buffers overflow into JS.
pub const ARENA_BYTES: usize = 64 * 1024;
/// Entity ids pre-allocated for `spawn` per call; JS asks for more with one host call each time
/// it runs out ([`RESERVE`]).
pub const POOL_SIZE: usize = 256;
/// The host function id of `reserve(n)`, which allocates `n` more pool ids during a call (a V8
/// small integer, above any exported method's id).
pub const RESERVE: u32 = 0x3FFF_FFFF;

/// Marks entities spawned by TS, so a restart can despawn them.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component, Default)]
pub struct TsSpawned;

/// bevy-ts's own bookkeeping components (`TsSpawned`, `ScriptHidden`, tsx's `TsxNode`): `clear`
/// and `retain` keep them, and `cloneAndSpawn` does not copy them, so a restart and the tsx tree
/// still see what made each entity.
#[derive(Resource, Default, Clone, Debug)]
pub struct Bookkeeping(pub Vec<ComponentId>);

impl Bookkeeping {
    /// Adds `C` to the set.
    pub fn add<C: Component>(world: &mut World) {
        let id = world.register_component::<C>();
        let mut b = world.get_resource_or_init::<Bookkeeping>();
        if !b.0.contains(&id) {
            b.0.push(id);
        }
    }
}

/// A component TS may write: its size, and for one with a heap field what builds its values.
#[derive(Clone, Debug)]
pub struct Writable {
    pub size: usize,
    pub heap: Option<Arc<HeapFill>>,
}

/// A decoded op.
#[derive(Debug, PartialEq)]
pub enum Op {
    Spawn { entity: Entity, components: Vec<(ComponentId, HeapBytes)> },
    Insert { entity: Entity, components: Vec<(ComponentId, HeapBytes)>, how: Insert },
    Despawn { entity: Entity },
    Remove { entity: Entity, components: Vec<ComponentId>, how: Remove },
    Clear { entity: Entity },
    CloneAndSpawn { source: Entity, clone: Entity },
    AddChildren { entity: Entity, children: Vec<Entity> },
    DetachChildren { entity: Entity },
    DespawnChildren { entity: Entity },
    SetParent { child: Entity, parent: Option<Entity> },
    SpawnNative { entity: Entity, recipe: String, params: serde_json::Value },
    Plugin { opcode: u16, bytes: Vec<u8> },
    WriteMessage { index: u32, bytes: Vec<u8> },
    Trigger { index: u32, target: Option<Entity>, bytes: Vec<u8> },
    /// `next.set(v)`, or `next.setIfNeq(v)` with `if_neq` (bit 31 of the value index).
    SetState { index: u32, value: u32, if_neq: bool },
    /// `commands.runSystem(id)` / `runSystemWith(id, input)`.
    RunSystem { identity: String, input: Option<Vec<u8>> },
    /// `commands.runSchedule(label)`.
    RunSchedule { label: String },
    /// `commands.entity(e).observe(fn)`: the observer's key index in the live generation and its
    /// declaration.
    Observe { entity: Entity, index: u32, decl: Box<crate::manifest::SystemDecl> },
    StateScoped { entity: Entity, kind: u8, remove: bool, machine: u32, value: u32 },
    /// `commands.saveWorld(name, options)`.
    SaveWorld { name: String, options: crate::saves::SaveOptions },
    /// `commands.loadWorld(name, { parent })`.
    LoadWorld { name: String, parent: Option<Entity> },
    InsertReflect { entity: Entity, mode: u8, type_name: String, json: Option<serde_json::Value> },
    /// A Rust message by type path, from its JSON.
    WriteReflect { path: String, json: serde_json::Value },
    /// A Rust event by type path, from its JSON (an entity event's target is in it).
    TriggerReflect { path: String, json: serde_json::Value },
}

/// How an insert treats what is there, as Bevy's `EntityCommands` methods of the same names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Insert {
    Insert,
    IfNew,
    Try,
    Recursive,
}

/// What a remove takes, as Bevy's `EntityCommands` methods of the same names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Remove {
    Remove,
    WithRequires,
    Retain,
    Recursive,
}

/// A plugin's command decoder: gets the op's bytes and `&mut World`.
pub type CommandDecoder = Arc<dyn Fn(&mut World, &[u8]) -> Result<(), String> + Send + Sync>;

/// A native spawn recipe: builds what TS cannot yet (meshes, materials, cameras) on an entity
/// already spawned.
pub type NativeRecipe = Arc<dyn Fn(&mut World, Entity, &serde_json::Value) -> Result<(), String> + Send + Sync>;

/// Plugin command kinds, by name and opcode.
#[derive(Resource, Default, Clone)]
pub struct CommandKinds {
    by_name: HashMap<String, u16>,
    decoders: Vec<(String, CommandDecoder)>,
}

impl CommandKinds {
    pub fn register(&mut self, name: impl Into<String>, decoder: CommandDecoder) -> u16 {
        let name = name.into();
        assert!(!self.by_name.contains_key(&name), "command kind {name} registered twice");
        let opcode = self.decoders.len() as u16;
        self.by_name.insert(name.clone(), opcode);
        self.decoders.push((name, decoder));
        opcode
    }

    pub fn opcode(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    pub fn all(&self) -> impl Iterator<Item = (&str, u16)> {
        self.by_name.iter().map(|(n, o)| (n.as_str(), *o))
    }

    fn get(&self, opcode: u16) -> Option<&(String, CommandDecoder)> {
        self.decoders.get(opcode as usize)
    }
}

/// Native spawn recipes, by name.
#[derive(Resource, Default, Clone)]
pub struct NativeRecipes {
    recipes: HashMap<String, NativeRecipe>,
}

impl NativeRecipes {
    pub fn register(&mut self, name: impl Into<String>, recipe: NativeRecipe) {
        self.recipes.insert(name.into(), recipe);
    }

    pub fn get(&self, name: &str) -> Option<NativeRecipe> {
        self.recipes.get(name).cloned()
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.at..self.at + n).ok_or_else(|| format!("command buffer truncated at byte {}", self.at))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn text(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
    fn entity(&mut self) -> Result<Entity, String> {
        let bits = self.u64()?;
        Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))
    }
    fn components(&mut self, sizes: &HashMap<ComponentId, Writable>) -> Result<Vec<(ComponentId, HeapBytes)>, String> {
        let n = self.u16()?;
        (0..n)
            .map(|_| {
                let id = ComponentId::new(self.u32()? as usize);
                let len = self.u32()? as usize;
                let w = sizes.get(&id).ok_or_else(|| format!("component id {} cannot be written from TS", id.index()))?;
                match &w.heap {
                    None if w.size != len => Err(format!("component id {} takes {} bytes, got {len}", id.index(), w.size)),
                    None => Ok((id, HeapBytes::plain(self.take(len)?.to_vec()))),
                    Some(_) if len < w.size => Err(format!("component id {} takes at least {} bytes, got {len}", id.index(), w.size)),
                    Some(fill) => {
                        let b = self.take(len)?;
                        Ok((id, fill.build(&b[..w.size], &b[w.size..])?))
                    }
                }
            })
            .collect()
    }
    fn ids(&mut self) -> Result<Vec<ComponentId>, String> {
        let n = self.u16()?;
        (0..n).map(|_| Ok(ComponentId::new(self.u32()? as usize))).collect()
    }
}

/// Decodes a whole buffer. Any malformed op or unknown opcode fails the whole buffer, so a
/// run's commands apply all or nothing.
pub fn decode(bytes: &[u8], sizes: &HashMap<ComponentId, Writable>, kinds: &CommandKinds) -> Result<Vec<Op>, String> {
    let mut r = Reader { b: bytes, at: 0 };
    let mut ops = Vec::new();
    while r.at < bytes.len() {
        let op = match r.u8()? {
            OP_SPAWN => Op::Spawn { entity: r.entity()?, components: r.components(sizes)? },
            op @ (OP_INSERT | OP_INSERT_IF_NEW | OP_TRY_INSERT | OP_INSERT_RECURSIVE) => {
                let how = match op {
                    OP_INSERT => Insert::Insert,
                    OP_INSERT_IF_NEW => Insert::IfNew,
                    OP_TRY_INSERT => Insert::Try,
                    _ => Insert::Recursive,
                };
                Op::Insert { entity: r.entity()?, components: r.components(sizes)?, how }
            }
            OP_DESPAWN => Op::Despawn { entity: r.entity()? },
            op @ (OP_REMOVE | OP_REMOVE_WITH_REQUIRES | OP_RETAIN | OP_REMOVE_RECURSIVE) => {
                let how = match op {
                    OP_REMOVE => Remove::Remove,
                    OP_REMOVE_WITH_REQUIRES => Remove::WithRequires,
                    OP_RETAIN => Remove::Retain,
                    _ => Remove::Recursive,
                };
                Op::Remove { entity: r.entity()?, components: r.ids()?, how }
            }
            op @ (OP_SPAWN_BATCH | OP_INSERT_BATCH) => {
                let spawn = op == OP_SPAWN_BATCH;
                let n = r.u32()?;
                for _ in 0..n {
                    let (entity, components) = (r.entity()?, r.components(sizes)?);
                    ops.push(if spawn { Op::Spawn { entity, components } } else { Op::Insert { entity, components, how: Insert::Insert } });
                }
                continue;
            }
            OP_CLEAR => Op::Clear { entity: r.entity()? },
            OP_CLONE_AND_SPAWN => Op::CloneAndSpawn { source: r.entity()?, clone: r.entity()? },
            OP_ADD_CHILDREN => {
                let entity = r.entity()?;
                let n = r.u32()?;
                Op::AddChildren { entity, children: (0..n).map(|_| r.entity()).collect::<Result<_, _>>()? }
            }
            OP_DETACH_CHILDREN => Op::DetachChildren { entity: r.entity()? },
            OP_DESPAWN_CHILDREN => Op::DespawnChildren { entity: r.entity()? },
            OP_SET_PARENT => {
                let child = r.entity()?;
                let bits = r.u64()?;
                let parent = if bits == u64::MAX {
                    None
                } else {
                    Some(Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))?)
                };
                Op::SetParent { child, parent }
            }
            OP_SPAWN_NATIVE => {
                let entity = r.entity()?;
                let n = r.u16()? as usize;
                let recipe = String::from_utf8(r.take(n)?.to_vec()).map_err(|e| e.to_string())?;
                let n = r.u32()? as usize;
                let params = serde_json::from_slice(r.take(n)?).map_err(|e| format!("spawnNative({recipe}) params: {e}"))?;
                Op::SpawnNative { entity, recipe, params }
            }
            OP_PLUGIN => {
                let opcode = r.u16()?;
                if kinds.get(opcode).is_none() {
                    return Err(format!("unregistered command opcode {opcode}"));
                }
                let n = r.u32()? as usize;
                Op::Plugin { opcode, bytes: r.take(n)?.to_vec() }
            }
            OP_WRITE_MESSAGE => {
                let index = r.u32()?;
                let n = r.u32()? as usize;
                Op::WriteMessage { index, bytes: r.take(n)?.to_vec() }
            }
            OP_TRIGGER => {
                let index = r.u32()?;
                let bits = r.u64()?;
                let target = if bits == u64::MAX {
                    None
                } else {
                    Some(Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))?)
                };
                let n = r.u32()? as usize;
                Op::Trigger { index, target, bytes: r.take(n)?.to_vec() }
            }
            OP_SET_STATE => {
                let index = r.u32()?;
                let v = r.u32()?;
                Op::SetState { index, value: v & 0x7fff_ffff, if_neq: v & 0x8000_0000 != 0 }
            }
            OP_RUN_SYSTEM => {
                let identity = r.text()?;
                let input = match r.u8()? {
                    0 => None,
                    _ => {
                        let n = r.u32()? as usize;
                        Some(r.take(n)?.to_vec())
                    }
                };
                Op::RunSystem { identity, input }
            }
            OP_RUN_SCHEDULE => Op::RunSchedule { label: r.text()? },
            OP_OBSERVE => {
                let entity = r.entity()?;
                let index = r.u32()?;
                let n = r.u32()? as usize;
                let decl = serde_json::from_slice(r.take(n)?).map_err(|e| format!("observe: {e}"))?;
                Op::Observe { entity, index, decl: Box::new(decl) }
            }
            OP_STATE_SCOPED => {
                let entity = r.entity()?;
                let kind = r.u8()?;
                Op::StateScoped { entity, kind: kind & 0x7f, remove: kind & 0x80 != 0, machine: r.u32()?, value: r.u32()? }
            }
            OP_SAVE_WORLD => {
                let name = r.text()?;
                let n = r.u32()? as usize;
                let options = serde_json::from_slice(r.take(n)?).map_err(|e| format!("saveWorld({name}) options: {e}"))?;
                Op::SaveWorld { name, options }
            }
            OP_LOAD_WORLD => {
                let name = r.text()?;
                let bits = r.u64()?;
                let parent = if bits == u64::MAX {
                    None
                } else {
                    Some(Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))?)
                };
                Op::LoadWorld { name, parent }
            }
            OP_INSERT_REFLECT => {
                let entity = r.entity()?;
                let mode = r.u8()?;
                let n = r.u16()? as usize;
                let type_name = String::from_utf8(r.take(n)?.to_vec()).map_err(|e| e.to_string())?;
                let n = r.u32()? as usize;
                let body = r.take(n)?;
                let json = if body.is_empty() { None } else { Some(serde_json::from_slice(body).map_err(|e| format!("{type_name}: {e}"))?) };
                Op::InsertReflect { entity, mode, type_name, json }
            }
            code @ (OP_WRITE_REFLECT | OP_TRIGGER_REFLECT) => {
                let path = r.text()?;
                let n = r.u32()? as usize;
                let json = serde_json::from_slice(r.take(n)?).map_err(|e| format!("{path}: {e}"))?;
                if code == OP_WRITE_REFLECT { Op::WriteReflect { path, json } } else { Op::TriggerReflect { path, json } }
            }
            other => return Err(format!("unknown command op {other} at byte {}", r.at - 1)),
        };
        ops.push(op);
    }
    Ok(ops)
}

/// `e` and its descendants through `Children`, parents first.
fn tree(world: &World, e: Entity) -> Vec<Entity> {
    let mut out = vec![e];
    let mut i = 0;
    while i < out.len() {
        if let Some(c) = world.get::<Children>(out[i]) {
            out.extend(c.iter());
        }
        i += 1;
    }
    out
}

/// `ids` and every component they require, transitively, as Bevy's `remove_with_requires`.
fn with_requires(world: &World, ids: &[ComponentId]) -> Vec<ComponentId> {
    let mut out = ids.to_vec();
    let mut i = 0;
    while i < out.len() {
        if let Some(info) = world.components().get_info(out[i]) {
            for r in info.required_components().iter_ids() {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
        i += 1;
    }
    out
}

/// Inserts raw native bytes as a component.
///
/// # Safety
/// `id` must be a component whose layout is exactly `bytes.len()` bytes of plain data with no
/// drop glue and no invalid bit patterns for the bytes given (TS components with no heap field, and
/// native layouts).
pub unsafe fn insert_bytes(entity: &mut EntityWorldMut, id: ComponentId, bytes: &[u8]) {
    // Copy into 16-aligned storage: arena bytes are unaligned.
    let mut buf = vec![0u128; bytes.len().div_ceil(16).max(1)];
    let dst = buf.as_mut_ptr() as *mut u8;
    // SAFETY: `buf` holds at least `bytes.len()` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
    // SAFETY: the pointer is valid and aligned to 16; the caller vouches for the type.
    unsafe { entity.insert_by_id(id, OwningPtr::new(NonNull::new_unchecked(dst))) };
    // The component took ownership of the bytes by copy; `buf` has no drop glue to run twice.
}

/// Inserts a whole value as a component, its heap values moved into the column.
///
/// # Safety
/// `id` must be a component whose values `bytes` is one of: a TS component, or a native layout
/// (as [`insert_bytes`] requires).
pub unsafe fn insert_owned(entity: &mut EntityWorldMut, id: ComponentId, bytes: HeapBytes) {
    let mut buf = crate::heap::Aligned::new(bytes.bytes());
    // SAFETY: the pointer is valid and aligned to 16; the caller vouches for the type.
    unsafe { entity.insert_by_id(id, OwningPtr::new(buf.ptr())) };
    // The column owns the heap values now.
    bytes.into_moved();
}

/// Inserts `typed` (values already owned by pointer, with their ids) and whole values as
/// components, all in one archetype move; their heap values move into the columns.
///
/// # Safety
/// Each `typed` pointer must hold a value of its id's type, and each id in `parts` must be as
/// [`insert_owned`] requires for its value. No id may appear twice.
pub unsafe fn insert_bytes_many<'a>(entity: &mut EntityWorldMut, typed: Vec<(ComponentId, OwningPtr<'a>)>, parts: Vec<(ComponentId, HeapBytes)>) {
    // Each copied into 16-aligned storage, as `insert_owned` does.
    let mut bufs: Vec<_> = parts.iter().map(|(_, b)| crate::heap::Aligned::new(b.bytes())).collect();
    let ids: Vec<ComponentId> = typed.iter().map(|(id, _)| *id).chain(parts.iter().map(|(id, _)| *id)).collect();
    // SAFETY: each buffer pointer is valid and aligned to 16.
    let raw = bufs.iter_mut().map(|b| unsafe { OwningPtr::new(b.ptr()) });
    // SAFETY: the caller vouches for every pointer's type.
    unsafe { entity.insert_by_ids(&ids, typed.into_iter().map(|(_, p)| p).chain(raw)) };
    // The columns own the heap values now.
    for (_, b) in parts {
        b.into_moved();
    }
}

/// Applies decoded ops in order. Errors in single ops (a missing entity) are collected as
/// warnings and the rest still apply.
pub fn apply(world: &mut World, ops: Vec<Op>, kinds: &CommandKinds, recipes: &NativeRecipes) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut ops = ops.into_iter().peekable();
    while let Some(op) = ops.next() {
        // A spawn or insert and the reflected values right after it for the same entity are one
        // command: every reflected value is checked first, and one that fails skips them all.
        if let Op::Spawn { entity, .. } | Op::Insert { entity, .. } = &op {
            let entity = *entity;
            let mut reflected = Vec::new();
            while let Some(Op::InsertReflect { entity: e, .. }) = ops.peek() {
                if *e != entity {
                    break;
                }
                reflected.push(ops.next().expect("peeked"));
            }
            let checked = reflected.iter().try_for_each(|r| match r {
                Op::InsertReflect { mode, type_name, json, .. } => check_reflect(world, *mode, type_name, json.as_ref()).map_err(|e| format!("{type_name}: {e}")),
                _ => unreachable!("reflected ops"),
            });
            if let Err(err) = checked {
                let what = if matches!(op, Op::Spawn { .. }) { "spawn" } else { "insert" };
                warnings.push(format!("{what} {entity}: {err}; the command was skipped"));
                continue;
            }
            apply_one(world, op, kinds, recipes, &mut warnings);
            for r in reflected {
                apply_one(world, r, kinds, recipes, &mut warnings);
            }
            continue;
        }
        apply_one(world, op, kinds, recipes, &mut warnings);
    }
    warnings
}

/// The [`Bookkeeping`] components' ids, for `clear`, `retain` and `cloneAndSpawn`.
fn bookkeeping(world: &World) -> Vec<ComponentId> {
    world.get_resource::<Bookkeeping>().map(|b| b.0.clone()).unwrap_or_default()
}

/// Applies one op, pushing what fails to `warnings`.
fn apply_one(world: &mut World, op: Op, kinds: &CommandKinds, recipes: &NativeRecipes, warnings: &mut Vec<String>) {
    {
        match op {
            Op::Spawn { entity, components } => match world.spawn_empty_at(entity) {
                Ok(mut e) => {
                    e.insert(TsSpawned);
                    for (id, bytes) in components {
                        // SAFETY: decode checked the id is TS-writable and built its value.
                        unsafe { insert_owned(&mut e, id, bytes) };
                    }
                }
                Err(err) => warnings.push(format!("spawn {entity}: {err}")),
            },
            Op::Insert { entity, components, how } => {
                if world.get_entity(entity).is_err() {
                    let method = match how {
                        Insert::Try => return,
                        Insert::Insert => "insert",
                        Insert::IfNew => "insertIfNew",
                        Insert::Recursive => "insertRecursive",
                    };
                    warnings.push(format!("{method}: entity {entity} does not exist"));
                    return;
                }
                let targets = if how == Insert::Recursive { tree(world, entity) } else { vec![entity] };
                for t in targets {
                    let mut e = world.entity_mut(t);
                    for (id, bytes) in &components {
                        if how == Insert::IfNew && e.contains_id(*id) {
                            continue;
                        }
                        // SAFETY: as above; each target gets its own copy of the heap values.
                        match bytes.shape() {
                            None => unsafe { insert_bytes(&mut e, *id, bytes.bytes()) },
                            Some(_) => unsafe { insert_owned(&mut e, *id, bytes.duplicate()) },
                        }
                    }
                }
            }
            Op::Despawn { entity } => {
                if !world.despawn(entity) {
                    warnings.push(format!("despawn: entity {entity} does not exist"));
                }
            }
            Op::Remove { entity, components, how } => {
                if world.get_entity(entity).is_err() {
                    let method = match how {
                        Remove::Remove => "remove",
                        Remove::WithRequires => "removeWithRequires",
                        Remove::Retain => "retain",
                        Remove::Recursive => "removeRecursive",
                    };
                    warnings.push(format!("{method}: entity {entity} does not exist"));
                    return;
                }
                match how {
                    Remove::Remove => {
                        world.entity_mut(entity).remove_by_ids(&components);
                    }
                    Remove::WithRequires => {
                        let all = with_requires(world, &components);
                        world.entity_mut(entity).remove_by_ids(&all);
                    }
                    Remove::Retain => {
                        // Bevy keeps the retained components' required components too; bevy-ts keeps
                        // its bookkeeping.
                        let mut keep = with_requires(world, &components);
                        keep.extend(bookkeeping(world));
                        let drop: Vec<ComponentId> = world.entity(entity).archetype().components().iter().copied().filter(|c| !keep.contains(c)).collect();
                        world.entity_mut(entity).remove_by_ids(&drop);
                    }
                    Remove::Recursive => {
                        for t in tree(world, entity) {
                            world.entity_mut(t).remove_by_ids(&components);
                        }
                    }
                }
            }
            Op::Clear { entity } => {
                // Every component but bevy-ts's bookkeeping, which is not the game's.
                let ours = bookkeeping(world);
                let drop: Option<Vec<ComponentId>> =
                    world.get_entity(entity).ok().map(|e| e.archetype().components().iter().copied().filter(|c| !ours.contains(c)).collect());
                match drop {
                    Some(drop) => {
                        world.entity_mut(entity).remove_by_ids(&drop);
                    }
                    None => warnings.push(format!("clear: entity {entity} does not exist")),
                }
            }
            Op::CloneAndSpawn { source, clone } => {
                if world.get_entity(source).is_err() {
                    warnings.push(format!("cloneAndSpawn: entity {source} does not exist"));
                    return;
                }
                if let Err(err) = world.spawn_empty_at(clone) {
                    warnings.push(format!("cloneAndSpawn {clone}: {err}"));
                    return;
                }
                // The clone is a TS spawn: bevy-ts's own markers are not copied, and it is marked
                // as one.
                let ours = bookkeeping(world);
                bevy::ecs::entity::EntityCloner::build_opt_out(world).deny_by_ids(ours).clone_entity(source, clone);
                world.entity_mut(clone).insert(TsSpawned);
            }
            Op::AddChildren { entity, children } => {
                if world.get_entity(entity).is_err() {
                    warnings.push(format!("addChildren: entity {entity} does not exist"));
                    return;
                }
                let (live, gone): (Vec<Entity>, Vec<Entity>) = children.into_iter().partition(|c| world.get_entity(*c).is_ok());
                for c in gone {
                    warnings.push(format!("addChildren: child {c} does not exist"));
                }
                world.entity_mut(entity).add_children(&live);
            }
            Op::DetachChildren { entity } => match world.get_entity_mut(entity) {
                Ok(mut e) => {
                    e.detach_all_children();
                }
                Err(_) => warnings.push(format!("detachChildren: entity {entity} does not exist")),
            },
            Op::DespawnChildren { entity } => match world.get_entity_mut(entity) {
                Ok(mut e) => {
                    e.despawn_children();
                }
                Err(_) => warnings.push(format!("despawnChildren: entity {entity} does not exist")),
            },
            Op::SetParent { child, parent } => match (world.get_entity(child).is_ok(), parent) {
                (false, _) => warnings.push(format!("setParent: entity {child} does not exist")),
                (true, Some(p)) if world.get_entity(p).is_err() => {
                    warnings.push(format!("setParent: parent {p} does not exist"))
                }
                (true, Some(p)) => {
                    world.entity_mut(child).insert(ChildOf(p));
                }
                (true, None) => {
                    world.entity_mut(child).remove::<ChildOf>();
                }
            },
            Op::SpawnNative { entity, recipe, params } => {
                let Some(f) = recipes.get(&recipe) else {
                    match REMOVED_RECIPES.iter().find(|(name, _)| *name == recipe) {
                        Some((_, now)) => warnings.push(format!("spawnNative(\"{recipe}\"): the built-in recipe was removed; spawn its components instead: {now}")),
                        None => warnings.push(format!("spawnNative: no recipe named {recipe}")),
                    }
                    return;
                };
                match world.spawn_empty_at(entity) {
                    Ok(mut e) => {
                        e.insert(TsSpawned);
                        if let Err(err) = f(world, entity, &params) {
                            warnings.push(format!("spawnNative({recipe}): {err}"));
                        }
                    }
                    Err(err) => warnings.push(format!("spawnNative {entity}: {err}")),
                }
            }
            Op::WriteMessage { index, bytes } => {
                let pushed = world
                    .get_resource_mut::<crate::messages::TsPayloads>()
                    .and_then(|mut p| p.messages.get_mut(index as usize).map(|q| q.push(&bytes)));
                if pushed.is_none() {
                    warnings.push(format!("write: no message type {index}"));
                }
            }
            Op::Trigger { index, target, bytes } => {
                let Some(t) = world.get_resource::<crate::messages::TsPayloads>().and_then(|p| p.events.get(index as usize)).cloned() else {
                    warnings.push(format!("trigger: no event type {index}"));
                    return;
                };
                let path = match target {
                    Some(e) if world.get_entity(e).is_ok() => crate::observers::propagation_path(world, e, t.propagate),
                    Some(e) => {
                        warnings.push(format!("trigger {}: entity {e} does not exist", t.name));
                        return;
                    }
                    None => vec![],
                };
                let attached = crate::observers::attached_path(world, &path);
                world.get_resource_or_init::<crate::observers::ObserverQueue>().0.push_back(crate::observers::Pending {
                    trigger: crate::observers::Trigger::Event(index as usize),
                    bytes,
                    hops: vec![],
                    original: path.first().copied(),
                    attached,
                    path,
                });
            }
            Op::SetState { index, value, if_neq } => {
                let target = world
                    .get_resource::<crate::messages::TsPayloads>()
                    .and_then(|p| p.states.get(index as usize))
                    .and_then(|m| m.syms.get(value as usize).map(|s| (m.slot, *s, m.name.clone(), m.derived(), !m.computed.is_empty())));
                match target {
                    // A computed state follows its sources only, as in Bevy.
                    Some((_, _, name, _, true)) => warnings.push(format!("setState: {name} is a computed state; its value follows its sources")),
                    Some((slot, sym, name, derived, false)) => {
                        // A sub state's NextState is ignored while it does not exist, as in Bevy.
                        if !crate::states::set_next_of(world, slot, sym, if_neq) && !derived {
                            warnings.push(format!("setState: {name} has no value yet; give it one with app.initState"));
                        }
                    }
                    None => warnings.push(format!("setState: no state {index} value {value}")),
                }
            }
            Op::RunSystem { identity, input } => {
                use crate::schedules::{PendingRun, PendingRuns, RunTarget};
                world.get_resource_or_init::<PendingRuns>().0.push_back(PendingRun { target: RunTarget::System { identity, input }, caller: None });
            }
            Op::StateScoped { entity, kind, remove, machine, value } => {
                if world.get_entity(entity).is_err() {
                    warnings.push(format!("{}: entity {entity} does not exist", crate::states::SCOPED[(kind as usize).min(5)]));
                } else if remove {
                    crate::states::remove_scoped(world, entity, kind);
                } else {
                    let target = world.resource::<crate::messages::TsPayloads>().states.get(machine as usize).and_then(|m| m.syms.get(value as usize).map(|s| (m.slot, *s)));
                    match target {
                        Some((slot, sym)) => crate::states::insert_scoped_at(world, entity, kind, slot, sym),
                        None => warnings.push(format!("{}: no state {machine} value {value}", crate::states::SCOPED[(kind as usize).min(5)])),
                    }
                }
            }
            Op::RunSchedule { label } => {
                use crate::schedules::{PendingRun, PendingRuns, RunTarget};
                world.get_resource_or_init::<PendingRuns>().0.push_back(PendingRun { target: RunTarget::Schedule(label), caller: None });
            }
            Op::Observe { entity, index, decl } => {
                if world.get_entity(entity).is_err() {
                    warnings.push(format!("observe {}: entity {entity} does not exist", decl.name));
                    return;
                }
                crate::observers::observe(world, entity, index, *decl);
            }
            Op::SaveWorld { name, options } => crate::saves::save(world, &name, options),
            Op::LoadWorld { name, parent } => crate::saves::load(world, &name, parent),
            Op::InsertReflect { entity, mode, type_name, json } => {
                if let Err(err) = insert_reflect(world, entity, mode, &type_name, json.as_ref()) {
                    warnings.push(format!("insert {type_name}: {err}"));
                }
            }
            Op::WriteReflect { path, json } => {
                if let Err(e) = crate::rust_events::write_message(world, &path, &json) {
                    warnings.push(format!("write: {e}"));
                }
            }
            Op::TriggerReflect { path, json } => {
                if let Err(e) = crate::rust_events::trigger_event(world, &path, &json) {
                    warnings.push(format!("trigger: {e}"));
                }
            }
            Op::Plugin { opcode, bytes } => {
                let (name, decoder) = kinds.get(opcode).expect("decode checked the opcode").clone();
                if let Err(err) = decoder(world, &bytes) {
                    warnings.push(format!("command {name}: {err}"));
                }
            }
        }
    }
}

/// `OP_INSERT_REFLECT`: inserts a reflected component from its `ReflectDefault` (or
/// `ReflectFromWorld`) with `json` written over it, or writes `json`'s fields over the entity's
/// component (`REFLECT_PATCH`, a layout value's holes). A failure leaves the entity as it was.
#[cfg(feature = "reflect")]
fn insert_reflect(world: &mut World, entity: Entity, mode: u8, type_name: &str, json: Option<&serde_json::Value>) -> Result<(), String> {
    use crate::reflect_value;
    use bevy::ecs::reflect::{ReflectComponent, ReflectFromWorld};
    use bevy::reflect::{ParsedPath, ReflectPath};
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.get_resource::<AssetServer>().cloned();
    let registry = registry.read();
    let reg = registry
        .get_with_type_path(type_name)
        .or_else(|| registry.get_with_short_type_path(type_name))
        .ok_or_else(|| format!("{type_name} is not a registered type"))?;
    let path = reg.type_info().type_path();
    reflect_value::refuse_computed(path)?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{path} does not reflect Component"))?.clone();
    if world.get_entity(entity).is_err() {
        return Err(format!("entity {entity} does not exist"));
    }
    let default = match reg.data::<ReflectDefault>() {
        Some(d) => Some(d.default()),
        None => match reg.data::<ReflectFromWorld>() {
            // A `FromWorld` that panics fails this command, not the frame.
            Some(f) => Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.from_world(world))).map_err(|_| format!("{path}'s FromWorld panicked"))?),
            None => None,
        },
    };
    match mode {
        REFLECT_INSERT => {
            let mut value = default.ok_or_else(|| format!("{path} has no reflected Default or FromWorld, so a script cannot insert it"))?;
            if let Some(json) = json {
                reflect_value::write_component(&registry, server.as_ref(), value.as_partial_reflect_mut(), None, json)?;
            }
            rc.insert(&mut world.entity_mut(entity), value.as_partial_reflect(), &registry);
            Ok(())
        }
        REFLECT_PATCH => {
            let Some(serde_json::Value::Object(fields)) = json else { return Err(format!("{path}: a patch is an object of fields")) };
            let mut e = world.entity_mut(entity);
            let mut current = rc.reflect_mut(&mut e).ok_or_else(|| format!("entity {entity} has no {path} to patch"))?;
            // Written into a copy first, so a bad field changes nothing.
            let mut copy = current.reflect_clone().map_err(|e| format!("{path}: {e}"))?;
            for (field, v) in fields {
                let at = ParsedPath::parse(&format!(".{field}")).map_err(|_| format!("{path} has no field {field}"))?;
                let slot = at.reflect_element_mut(copy.as_partial_reflect_mut()).map_err(|_| format!("{path} has no field {field}"))?;
                let base = default.as_deref().and_then(|d| at.reflect_element(d.as_partial_reflect()).ok());
                reflect_value::write(&registry, server.as_ref(), slot, base, v).map_err(|e| format!("{path}.{field}: {e}"))?;
            }
            current.apply(copy.as_partial_reflect());
            Ok(())
        }
        other => Err(format!("unknown reflect insert mode {other}")),
    }
}

/// Checks an `OP_INSERT_REFLECT` before any of its command applies: the type is a writable
/// reflected component and `json` writes over a fresh default of it (a patch's fields too). A type
/// whose only default is `FromWorld` is built from the live world for the check, as for its insert.
#[cfg(feature = "reflect")]
fn check_reflect(world: &mut World, mode: u8, type_name: &str, json: Option<&serde_json::Value>) -> Result<(), String> {
    use crate::reflect_value;
    use bevy::ecs::reflect::{ReflectComponent, ReflectFromWorld};
    use bevy::reflect::{ParsedPath, ReflectPath};
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let reg = registry
        .get_with_type_path(type_name)
        .or_else(|| registry.get_with_short_type_path(type_name))
        .ok_or_else(|| format!("{type_name} is not a registered type"))?;
    let path = reg.type_info().type_path();
    reflect_value::refuse_computed(path)?;
    reg.data::<ReflectComponent>().ok_or_else(|| format!("{path} does not reflect Component"))?;
    // A type whose only default needs the live world is built from it, as its insert will be.
    let value = match reflect_value::default_of(&registry, reg.type_id()) {
        Some(v) => Some(v),
        None => match reg.data::<ReflectFromWorld>() {
            Some(f) => Some(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.from_world(world)))
                    .map_err(|_| format!("{path}'s FromWorld panicked"))?
                    .into_partial_reflect(),
            ),
            None => None,
        },
    };
    let Some(mut value) = value else { return Ok(()) };
    match (mode, json) {
        (REFLECT_INSERT, None) => Ok(()),
        (REFLECT_INSERT, Some(json)) => reflect_value::write_component(&registry, None, value.as_mut(), None, json),
        (REFLECT_PATCH, Some(serde_json::Value::Object(fields))) => fields.iter().try_for_each(|(field, v)| {
            let at = ParsedPath::parse(&format!(".{field}")).map_err(|_| format!("{path} has no field {field}"))?;
            let slot = at.reflect_element_mut(value.as_mut()).map_err(|_| format!("{path} has no field {field}"))?;
            reflect_value::write(&registry, None, slot, None, v).map_err(|e| format!("{path}.{field}: {e}"))
        }),
        (REFLECT_PATCH, _) => Err(format!("{path}: a patch is an object of fields")),
        (other, _) => Err(format!("unknown reflect insert mode {other}")),
    }
}

#[cfg(not(feature = "reflect"))]
fn check_reflect(_: &mut World, _: u8, type_name: &str, _: Option<&serde_json::Value>) -> Result<(), String> {
    Err(format!("{type_name}: bevy-ts is built without the reflect feature"))
}

#[cfg(not(feature = "reflect"))]
fn insert_reflect(_: &mut World, _: Entity, _: u8, type_name: &str, _: Option<&serde_json::Value>) -> Result<(), String> {
    Err(format!("{type_name}: bevy-ts is built without the reflect feature"))
}

/// Encodes ops the way the JS runtime does; used by the fake backend and tests.
#[derive(Default, Clone, Debug)]
pub struct Encoder {
    pub bytes: Vec<u8>,
}

impl Encoder {
    fn components(&mut self, components: &[(ComponentId, &[u8])]) {
        self.bytes.extend((components.len() as u16).to_le_bytes());
        for (id, b) in components {
            self.bytes.extend((id.index() as u32).to_le_bytes());
            self.bytes.extend((b.len() as u32).to_le_bytes());
            self.bytes.extend_from_slice(b);
        }
    }
    pub fn spawn(&mut self, entity: Entity, components: &[(ComponentId, &[u8])]) {
        self.bytes.push(OP_SPAWN);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.components(components);
    }
    pub fn insert(&mut self, entity: Entity, components: &[(ComponentId, &[u8])]) {
        self.bytes.push(OP_INSERT);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.components(components);
    }
    pub fn despawn(&mut self, entity: Entity) {
        self.bytes.push(OP_DESPAWN);
        self.bytes.extend(entity.to_bits().to_le_bytes());
    }
    pub fn remove(&mut self, entity: Entity, components: &[ComponentId]) {
        self.bytes.push(OP_REMOVE);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.bytes.extend((components.len() as u16).to_le_bytes());
        for id in components {
            self.bytes.extend((id.index() as u32).to_le_bytes());
        }
    }
    pub fn set_parent(&mut self, child: Entity, parent: Option<Entity>) {
        self.bytes.push(OP_SET_PARENT);
        self.bytes.extend(child.to_bits().to_le_bytes());
        self.bytes.extend(parent.map_or(u64::MAX, Entity::to_bits).to_le_bytes());
    }
    pub fn spawn_native(&mut self, entity: Entity, recipe: &str, params: &serde_json::Value) {
        self.bytes.push(OP_SPAWN_NATIVE);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.bytes.extend((recipe.len() as u16).to_le_bytes());
        self.bytes.extend(recipe.as_bytes());
        let p = params.to_string();
        self.bytes.extend((p.len() as u32).to_le_bytes());
        self.bytes.extend(p.as_bytes());
    }
    pub fn observe(&mut self, entity: Entity, index: u32, decl: &crate::manifest::SystemDecl) {
        self.bytes.push(OP_OBSERVE);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.bytes.extend(index.to_le_bytes());
        let d = serde_json::to_vec(decl).expect("a declaration serializes");
        self.bytes.extend((d.len() as u32).to_le_bytes());
        self.bytes.extend(d);
    }
    pub fn save_world(&mut self, name: &str, options: &serde_json::Value) {
        self.bytes.push(OP_SAVE_WORLD);
        self.bytes.extend((name.len() as u16).to_le_bytes());
        self.bytes.extend(name.as_bytes());
        let o = options.to_string();
        self.bytes.extend((o.len() as u32).to_le_bytes());
        self.bytes.extend(o.as_bytes());
    }
    pub fn load_world(&mut self, name: &str, parent: Option<Entity>) {
        self.bytes.push(OP_LOAD_WORLD);
        self.bytes.extend((name.len() as u16).to_le_bytes());
        self.bytes.extend(name.as_bytes());
        self.bytes.extend(parent.map_or(u64::MAX, Entity::to_bits).to_le_bytes());
    }

    pub fn insert_reflect(&mut self, entity: Entity, mode: u8, type_name: &str, json: Option<&serde_json::Value>) {
        self.bytes.push(OP_INSERT_REFLECT);
        self.bytes.extend(entity.to_bits().to_le_bytes());
        self.bytes.push(mode);
        self.bytes.extend((type_name.len() as u16).to_le_bytes());
        self.bytes.extend(type_name.as_bytes());
        let j = json.map(|j| j.to_string()).unwrap_or_default();
        self.bytes.extend((j.len() as u32).to_le_bytes());
        self.bytes.extend(j.as_bytes());
    }

    pub fn write_reflect(&mut self, path: &str, json: &serde_json::Value) {
        self.reflect(OP_WRITE_REFLECT, path, json);
    }
    pub fn trigger_reflect(&mut self, path: &str, json: &serde_json::Value) {
        self.reflect(OP_TRIGGER_REFLECT, path, json);
    }
    fn reflect(&mut self, op: u8, path: &str, json: &serde_json::Value) {
        self.bytes.push(op);
        self.bytes.extend((path.len() as u16).to_le_bytes());
        self.bytes.extend(path.as_bytes());
        let j = json.to_string();
        self.bytes.extend((j.len() as u32).to_le_bytes());
        self.bytes.extend(j.as_bytes());
    }
    pub fn plugin(&mut self, opcode: u16, bytes: &[u8]) {
        self.bytes.push(OP_PLUGIN);
        self.bytes.extend(opcode.to_le_bytes());
        self.bytes.extend((bytes.len() as u32).to_le_bytes());
        self.bytes.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_then_parent_in_one_buffer() {
        let mut world = World::new();
        let a = world.entity_allocator().alloc();
        let b = world.entity_allocator().alloc();
        let mut enc = Encoder::default();
        enc.spawn(a, &[]);
        enc.spawn(b, &[]);
        enc.set_parent(b, Some(a));
        let ops = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap();
        let warnings = apply(&mut world, ops, &CommandKinds::default(), &NativeRecipes::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(world.get::<ChildOf>(b).unwrap().parent(), a);
        assert!(world.get::<TsSpawned>(a).is_some());
    }

    #[cfg(feature = "pbr")]
    #[test]
    fn reflect_values_insert_over_their_default_and_patch_holes() {
        use bevy::camera::{Projection, ScalingMode};
        use bevy::light::DirectionalLight;
        use serde_json::json;
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        registry.write().register::<Projection>();
        registry.write().register::<DirectionalLight>();
        world.insert_resource(registry);
        let a = world.entity_allocator().alloc();
        let mut enc = Encoder::default();
        enc.spawn(a, &[]);
        enc.insert_reflect(a, REFLECT_INSERT, "Projection", Some(&json!({ "Orthographic": { "scaling_mode": { "FixedVertical": { "viewport_height": 12 } } } })));
        enc.insert_reflect(a, REFLECT_INSERT, "bevy_light::directional_light::DirectionalLight", None);
        enc.insert_reflect(a, REFLECT_PATCH, "DirectionalLight", Some(&json!({ "color": "#ff0000" })));
        // A bad value skips its whole command; the next command still applies.
        let b = world.entity_allocator().alloc();
        enc.spawn(b, &[]);
        enc.insert_reflect(b, REFLECT_INSERT, "DirectionalLight", None);
        enc.insert_reflect(b, REFLECT_PATCH, "DirectionalLight", Some(&json!({ "colour": "#ff0000" })));
        let c = world.entity_allocator().alloc();
        enc.spawn(c, &[]);
        let ops = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap();
        let warnings = apply(&mut world, ops, &CommandKinds::default(), &NativeRecipes::default());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("DirectionalLight") && warnings[0].contains("colour") && warnings[0].contains("skipped"), "{warnings:?}");
        assert!(world.get_entity(b).is_err(), "the failing spawn left nothing");
        assert!(world.get::<TsSpawned>(c).is_some(), "the next command applied");
        match world.get::<Projection>(a).unwrap() {
            Projection::Orthographic(o) => assert!(matches!(o.scaling_mode, ScalingMode::FixedVertical { viewport_height: 12.0 })),
            other => panic!("{other:?}"),
        }
        assert_eq!(world.get::<DirectionalLight>(a).unwrap().color, Color::srgb(1.0, 0.0, 0.0));
    }

    /// A component whose only default is `FromWorld`.
    #[derive(Component, Reflect, Debug)]
    #[reflect(Component, FromWorld)]
    struct Seeded {
        seed: u32,
    }

    impl FromWorld for Seeded {
        fn from_world(_: &mut World) -> Self {
            Seeded { seed: 7 }
        }
    }

    #[cfg(feature = "reflect")]
    #[test]
    fn a_from_world_type_with_a_bad_value_skips_its_command() {
        use serde_json::json;
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        registry.write().register::<Seeded>();
        world.insert_resource(registry);
        let (a, b) = (world.entity_allocator().alloc(), world.entity_allocator().alloc());
        let mut enc = Encoder::default();
        enc.spawn(a, &[]);
        enc.insert_reflect(a, REFLECT_INSERT, "Seeded", Some(&json!({ "seed": "many" })));
        enc.spawn(b, &[]);
        enc.insert_reflect(b, REFLECT_INSERT, "Seeded", Some(&json!({ "seed": 3 })));
        let ops = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap();
        let warnings = apply(&mut world, ops, &CommandKinds::default(), &NativeRecipes::default());
        assert!(warnings.len() == 1 && warnings[0].contains("Seeded") && warnings[0].contains("skipped"), "{warnings:?}");
        assert!(world.get_entity(a).is_err(), "the failing spawn left nothing");
        assert_eq!(world.get::<Seeded>(b).unwrap().seed, 3);
    }

    #[test]
    fn a_removed_recipe_names_its_replacement() {
        let mut world = World::new();
        let a = world.entity_allocator().alloc();
        let mut enc = Encoder::default();
        enc.spawn_native(a, "camera3d", &serde_json::json!({}));
        let ops = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap();
        let warnings = apply(&mut world, ops, &CommandKinds::default(), &NativeRecipes::default());
        assert!(warnings[0].contains("removed") && warnings[0].contains("Transform.lookingAt"), "{warnings:?}");
    }

    #[test]
    fn unknown_opcode_fails_the_buffer() {
        let mut enc = Encoder::default();
        enc.despawn(Entity::from_raw_u32(1).unwrap());
        enc.plugin(9, b"x");
        let e = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap_err();
        assert!(e.contains("opcode 9"), "{e}");
    }

    #[test]
    fn save_and_load_ops_round_trip() {
        let parent = Entity::from_raw_u32(7).unwrap();
        let mut enc = Encoder::default();
        enc.save_world("slot1", &serde_json::json!({ "entities": [5], "allow": ["game::Health"], "resources": ["game::Score"] }));
        enc.save_world("slot2", &serde_json::json!({}));
        enc.load_world("slot1", Some(parent));
        enc.load_world("slot2", None);
        let ops = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap();
        let options = crate::saves::SaveOptions {
            entities: Some(vec![5]),
            allow: Some(vec!["game::Health".into()]),
            deny: vec![],
            resources: vec!["game::Score".into()],
        };
        assert_eq!(
            ops,
            vec![
                Op::SaveWorld { name: "slot1".into(), options },
                Op::SaveWorld { name: "slot2".into(), options: Default::default() },
                Op::LoadWorld { name: "slot1".into(), parent: Some(parent) },
                Op::LoadWorld { name: "slot2".into(), parent: None },
            ]
        );
        let mut bad = Encoder::default();
        bad.save_world("slot1", &serde_json::json!({ "allow": 3 }));
        let e = decode(&bad.bytes, &HashMap::new(), &CommandKinds::default()).unwrap_err();
        assert!(e.contains("saveWorld(slot1) options"), "{e}");
    }

    #[test]
    fn plugin_ops_keep_order() {
        let mut world = World::new();
        let mut kinds = CommandKinds::default();
        let op = kinds.register(
            "test/name",
            Arc::new(|world: &mut World, bytes: &[u8]| {
                let e = Entity::from_bits(u64::from_le_bytes(bytes.try_into().unwrap()));
                world.get_entity_mut(e).map_err(|e| e.to_string())?.insert(Name::new("named"));
                Ok(())
            }),
        );
        let other = world.spawn_empty().id();
        let a = world.entity_allocator().alloc();
        let mut enc = Encoder::default();
        enc.spawn(a, &[]);
        enc.plugin(op, &a.to_bits().to_le_bytes());
        enc.despawn(other);
        let ops = decode(&enc.bytes, &HashMap::new(), &kinds).unwrap();
        assert!(apply(&mut world, ops, &kinds, &NativeRecipes::default()).is_empty());
        assert_eq!(world.get::<Name>(a).unwrap().as_str(), "named");
        assert!(world.get_entity(other).is_err());
    }
}
