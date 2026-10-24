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
//! | spawnBatch | 13 | n u32, n × (entity u64, components as in spawn) |
//! | insertBatch | 14 | n u32, n × (entity u64, components as in insert) |
//! | insertIfNew | 15 | as insert |
//! | tryInsert | 16 | as insert |
//! | retain | 17 | as remove |
//! | clear | 18 | entity u64 |
//! | removeWithRequires | 19 | as remove |
//! | cloneAndSpawn | 20 | source u64, clone u64 |
//! | insertRecursive | 21 | as insert |
//! | removeRecursive | 22 | as remove |
//! | addChildren | 23 | entity u64, n u32, n × child u64 |
//! | detachChildren | 24 | entity u64 |
//! | despawnChildren | 25 | entity u64 |
//!
//! Entities are `Entity::to_bits`. Component values are native layout bytes.

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
pub const OP_SPAWN_BATCH: u8 = 13;
pub const OP_INSERT_BATCH: u8 = 14;
pub const OP_INSERT_IF_NEW: u8 = 15;
pub const OP_TRY_INSERT: u8 = 16;
pub const OP_RETAIN: u8 = 17;
pub const OP_CLEAR: u8 = 18;
pub const OP_REMOVE_WITH_REQUIRES: u8 = 19;
pub const OP_CLONE_AND_SPAWN: u8 = 20;
pub const OP_INSERT_RECURSIVE: u8 = 21;
pub const OP_REMOVE_RECURSIVE: u8 = 22;
pub const OP_ADD_CHILDREN: u8 = 23;
pub const OP_DETACH_CHILDREN: u8 = 24;
pub const OP_DESPAWN_CHILDREN: u8 = 25;

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

/// A decoded op.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Spawn { entity: Entity, components: Vec<(ComponentId, Vec<u8>)> },
    Insert { entity: Entity, components: Vec<(ComponentId, Vec<u8>)>, how: Insert },
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
    SetState { index: u32, value: u32 },
    /// `commands.runSystem(id)` / `runSystemWith(id, input)`.
    RunSystem { identity: String, input: Option<Vec<u8>> },
    /// `commands.runSchedule(label)`.
    RunSchedule { label: String },
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
    fn components(&mut self, sizes: &HashMap<ComponentId, usize>) -> Result<Vec<(ComponentId, Vec<u8>)>, String> {
        let n = self.u16()?;
        (0..n)
            .map(|_| {
                let id = ComponentId::new(self.u32()? as usize);
                let len = self.u32()? as usize;
                let size = *sizes.get(&id).ok_or_else(|| format!("component id {} cannot be written from TS", id.index()))?;
                if size != len {
                    return Err(format!("component id {} takes {size} bytes, got {len}", id.index()));
                }
                Ok((id, self.take(len)?.to_vec()))
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
pub fn decode(bytes: &[u8], sizes: &HashMap<ComponentId, usize>, kinds: &CommandKinds) -> Result<Vec<Op>, String> {
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
            OP_SET_STATE => Op::SetState { index: r.u32()?, value: r.u32()? },
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
/// drop glue and no invalid bit patterns for the bytes given (TS components and native layouts).
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

/// Inserts `typed` (values already owned by pointer, with their ids) and raw native bytes as
/// components, all in one archetype move.
///
/// # Safety
/// Each `typed` pointer must hold a value of its id's type, and each id in `parts` must be as
/// [`insert_bytes`] requires for its bytes. No id may appear twice.
pub unsafe fn insert_bytes_many<'a>(entity: &mut EntityWorldMut, typed: Vec<(ComponentId, OwningPtr<'a>)>, parts: &[(ComponentId, &[u8])]) {
    // Each copied into 16-aligned storage, as `insert_bytes` does.
    let mut bufs: Vec<Vec<u128>> = parts.iter().map(|(_, b)| vec![0u128; b.len().div_ceil(16).max(1)]).collect();
    for (buf, (_, bytes)) in bufs.iter_mut().zip(parts) {
        // SAFETY: `buf` holds at least `bytes.len()` bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr() as *mut u8, bytes.len()) };
    }
    let ids: Vec<ComponentId> = typed.iter().map(|(id, _)| *id).chain(parts.iter().map(|(id, _)| *id)).collect();
    // SAFETY: each buffer pointer is valid and aligned to 16.
    let raw = bufs.iter_mut().map(|b| unsafe { OwningPtr::new(NonNull::new_unchecked(b.as_mut_ptr() as *mut u8)) });
    // SAFETY: the caller vouches for every pointer's type.
    unsafe { entity.insert_by_ids(&ids, typed.into_iter().map(|(_, p)| p).chain(raw)) };
}

/// Applies decoded ops in order. Errors in single ops (a missing entity) are collected as
/// warnings and the rest still apply.
pub fn apply(world: &mut World, ops: Vec<Op>, kinds: &CommandKinds, recipes: &NativeRecipes) -> Vec<String> {
    let mut warnings = Vec::new();
    let bookkeeping = world.get_resource::<Bookkeeping>().map(|b| b.0.clone()).unwrap_or_default();
    for op in ops {
        match op {
            Op::Spawn { entity, components } => match world.spawn_empty_at(entity) {
                Ok(mut e) => {
                    e.insert(TsSpawned);
                    for (id, bytes) in components {
                        // SAFETY: decode checked the id is TS-writable and the size matches.
                        unsafe { insert_bytes(&mut e, id, &bytes) };
                    }
                }
                Err(err) => warnings.push(format!("spawn {entity}: {err}")),
            },
            Op::Insert { entity, components, how } => {
                if world.get_entity(entity).is_err() {
                    let method = match how {
                        Insert::Try => continue,
                        Insert::Insert => "insert",
                        Insert::IfNew => "insertIfNew",
                        Insert::Recursive => "insertRecursive",
                    };
                    warnings.push(format!("{method}: entity {entity} does not exist"));
                    continue;
                }
                let targets = if how == Insert::Recursive { tree(world, entity) } else { vec![entity] };
                for t in targets {
                    let mut e = world.entity_mut(t);
                    for (id, bytes) in &components {
                        if how == Insert::IfNew && e.contains_id(*id) {
                            continue;
                        }
                        // SAFETY: as above.
                        unsafe { insert_bytes(&mut e, *id, bytes) };
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
                    continue;
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
                        keep.extend(&bookkeeping);
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
                let drop: Option<Vec<ComponentId>> =
                    world.get_entity(entity).ok().map(|e| e.archetype().components().iter().copied().filter(|c| !bookkeeping.contains(c)).collect());
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
                    continue;
                }
                if let Err(err) = world.spawn_empty_at(clone) {
                    warnings.push(format!("cloneAndSpawn {clone}: {err}"));
                    continue;
                }
                // The clone is a TS spawn: bevy-ts's own markers are not copied, and it is marked
                // as one.
                bevy::ecs::entity::EntityCloner::build_opt_out(world).deny_by_ids(bookkeeping.clone()).clone_entity(source, clone);
                world.entity_mut(clone).insert(TsSpawned);
            }
            Op::AddChildren { entity, children } => {
                if world.get_entity(entity).is_err() {
                    warnings.push(format!("addChildren: entity {entity} does not exist"));
                    continue;
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
                    warnings.push(format!("spawnNative: no recipe named {recipe}"));
                    continue;
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
                    continue;
                };
                let path = match target {
                    Some(e) if world.get_entity(e).is_ok() => crate::observers::propagation_path(world, e, t.propagate),
                    Some(e) => {
                        warnings.push(format!("trigger {}: entity {e} does not exist", t.name));
                        continue;
                    }
                    None => vec![],
                };
                world.get_resource_or_init::<crate::observers::ObserverQueue>().0.push_back(crate::observers::Pending {
                    trigger: crate::observers::Trigger::Event(index as usize),
                    bytes,
                    path,
                });
            }
            Op::SetState { index, value } => {
                match world.get_resource_mut::<crate::messages::TsPayloads>().and_then(|mut p| {
                    p.states.get_mut(index as usize).filter(|s| (value as usize) < s.values.len()).map(|s| s.next = Some(value))
                }) {
                    Some(()) => {}
                    None => warnings.push(format!("setState: no state {index} value {value}")),
                }
            }
            Op::RunSystem { identity, input } => {
                use crate::schedules::{PendingRun, PendingRuns, RunTarget};
                world.get_resource_or_init::<PendingRuns>().0.push_back(PendingRun { target: RunTarget::System { identity, input }, caller: None });
            }
            Op::RunSchedule { label } => {
                use crate::schedules::{PendingRun, PendingRuns, RunTarget};
                world.get_resource_or_init::<PendingRuns>().0.push_back(PendingRun { target: RunTarget::Schedule(label), caller: None });
            }
            Op::Plugin { opcode, bytes } => {
                let (name, decoder) = kinds.get(opcode).expect("decode checked the opcode").clone();
                if let Err(err) = decoder(world, &bytes) {
                    warnings.push(format!("command {name}: {err}"));
                }
            }
        }
    }
    warnings
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

    #[test]
    fn unknown_opcode_fails_the_buffer() {
        let mut enc = Encoder::default();
        enc.despawn(Entity::from_raw_u32(1).unwrap());
        enc.plugin(9, b"x");
        let e = decode(&enc.bytes, &HashMap::new(), &CommandKinds::default()).unwrap_err();
        assert!(e.contains("opcode 9"), "{e}");
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
