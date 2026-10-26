//! TS relationships (custom-relationships): a source is a layout-tier TS component whose first
//! field is `target: Entity`; a target is a Rust-owned `Vec<Entity>` registered by id. Their
//! hooks are Bevy 0.20's (`relationship/mod.rs`), ported to work by `ComponentId`, and every TS
//! component's hook slots go through one dispatcher per slot that reads the id's role here.

use crate::components;
use bevy::ecs::component::ComponentId;
use bevy::ecs::entity::{ComponentCloneCtx, SourceComponent};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::relationship::{RelationshipAccessor, RelationshipAccessorInitializer, RelationshipHookMode};
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::ptr::{OwningPtr, Ptr};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A TS component id's half of a relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `target` is the target's id, set once it is registered; `offset` is where the `target`
    /// field sits (0, or 8 behind a heap header).
    Source { target: Option<ComponentId>, offset: usize, linked_spawn: bool, allow_self_referential: bool },
    Target { source: ComponentId, linked_spawn: bool },
}

/// Every TS component id's role, and the pairs whose hooks are off while a reload rebuilds them.
#[derive(Resource, Default, Debug)]
pub struct TsHooks {
    roles: HashMap<ComponentId, Role>,
    suspended: HashSet<ComponentId>,
}

impl TsHooks {
    pub fn role(&self, id: ComponentId) -> Option<Role> {
        self.roles.get(&id).copied()
    }

    pub(crate) fn set_role(&mut self, id: ComponentId, role: Role) {
        self.roles.insert(id, role);
    }

    pub(crate) fn suspend(&mut self, ids: impl IntoIterator<Item = ComponentId>) {
        self.suspended.extend(ids);
    }

    pub(crate) fn resume(&mut self) {
        self.suspended.clear();
    }

    fn live(&self, id: ComponentId) -> Option<Role> {
        if self.suspended.contains(&id) {
            return None;
        }
        self.role(id)
    }
}

fn role_of(world: &DeferredWorld, id: ComponentId) -> Option<Role> {
    world.get_resource::<TsHooks>()?.live(id)
}

fn suspended(world: &DeferredWorld, id: ComponentId) -> bool {
    world.get_resource::<TsHooks>().is_some_and(|h| h.suspended.contains(&id))
}

/// Bevy's rule for a source's hooks: `Skip` never runs them, `RunIfNotLinked` skips a linked pair.
fn source_runs(mode: RelationshipHookMode, linked_spawn: bool) -> bool {
    match mode {
        RelationshipHookMode::Run => true,
        RelationshipHookMode::Skip => false,
        RelationshipHookMode::RunIfNotLinked => !linked_spawn,
    }
}

/// The entity a source's bytes point at.
///
/// # Safety
/// `p` points at a live value of a source whose `target` field is at `offset`.
unsafe fn target_at(p: *const u8, offset: usize) -> Entity {
    // SAFETY: the caller's contract; an `Entity` field holds `Entity::to_bits`, which is the
    // struct's own layout (`repr(C, align(8))`, index then generation).
    unsafe { std::ptr::read_unaligned(p.add(offset) as *const Entity) }
}

/// The entity a source points at on `e`, read under its id.
fn source_target(world: &DeferredWorld, e: Entity, id: ComponentId, offset: usize) -> Option<Entity> {
    let p = world.get_entity(e).ok()?.get_by_id(id).ok()?;
    // SAFETY: `id` is a source with its `target` at `offset`.
    Some(unsafe { target_at(p.as_ptr(), offset) })
}

/// The `on_add` slot: ecs-queries' required components ([`components::on_add`]).
pub fn dispatch_on_add(world: DeferredWorld, ctx: HookContext) {
    if suspended(&world, ctx.component_id) {
        return;
    }
    components::on_add(world, ctx);
}

/// The `on_insert` slot: a source joins its target's list (Bevy's `Relationship::on_insert`).
pub fn dispatch_on_insert(mut world: DeferredWorld, ctx: HookContext) {
    let Some(Role::Source { target: Some(target_id), offset, linked_spawn, allow_self_referential }) = role_of(&world, ctx.component_id) else { return };
    if !source_runs(ctx.relationship_hook_mode, linked_spawn) {
        return;
    }
    let (entity, source_id) = (ctx.entity, ctx.component_id);
    let Some(target) = source_target(&world, entity, source_id, offset) else { return };
    let name = || world.components().get_name(source_id).map(|n| n.to_string()).unwrap_or_default();
    let at = ctx.caller.map(|l| format!("{l}: ")).unwrap_or_default();
    if !allow_self_referential && target == entity {
        warn!(
            "{at}The {}({target:?}) relationship on entity {entity:?} points to itself. The invalid {} relationship has been removed.\nIf this is intended, declare static allowSelfReferential = true on the source.",
            name(),
            name()
        );
        world.commands().entity(entity).remove_by_id(source_id);
        return;
    }
    if world.get_entity(target).is_ok() {
        // Deferred, as Bevy's `entry` is, for batch inserts.
        world.commands().queue(move |world: &mut World| {
            if let Ok(mut t) = world.get_entity_mut(target) {
                push_source(&mut t, target_id, entity);
            }
        });
    } else {
        warn!(
            "{at}The {}({target:?}) relationship on entity {entity:?} relates to an entity that does not exist. The invalid {} relationship has been removed.",
            name(),
            name()
        );
        world.commands().entity(entity).remove_by_id(source_id);
    }
}

/// Adds `source` to `t`'s list under `target_id`, inserting the list when missing.
pub(crate) fn push_source(t: &mut EntityWorldMut, target_id: ComponentId, source: Entity) {
    if let Ok(mut list) = t.get_mut_by_id(target_id) {
        // SAFETY: a target's value is a `Vec<Entity>`.
        unsafe { list.as_mut().deref_mut::<Vec<Entity>>() }.push(source);
        return;
    }
    // SAFETY: a target is registered with `Vec<Entity>`'s layout and drop; the value is moved in.
    OwningPtr::make(vec![source], |p| unsafe {
        t.insert_by_id(target_id, p);
    });
}

/// The `on_discard` slot: a source leaves its target's list (Bevy's `Relationship::on_discard`),
/// or a target's sources lose their relationship (Bevy's `RelationshipTarget::on_discard`).
pub fn dispatch_on_discard(mut world: DeferredWorld, ctx: HookContext) {
    let (entity, id) = (ctx.entity, ctx.component_id);
    match role_of(&world, id) {
        Some(Role::Source { target: Some(target_id), offset, linked_spawn, .. }) => {
            if !source_runs(ctx.relationship_hook_mode, linked_spawn) {
                return;
            }
            let Some(target) = source_target(&world, entity, id, offset) else { return };
            let Ok(mut t) = world.get_entity_mut(target) else { return };
            let Ok(mut list) = t.get_mut_by_id(target_id) else { return };
            // SAFETY: a target's value is a `Vec<Entity>`.
            let list = unsafe { list.as_mut().deref_mut::<Vec<Entity>>() };
            // Bevy's `Vec` collection: scan from the back, where recent sources live.
            if let Some(i) = list.iter().rposition(|e| *e == entity) {
                list.remove(i);
            }
            if list.is_empty() {
                // Checked again when it runs: an identical source inserted on top keeps it.
                world.commands().queue_silenced(move |world: &mut World| {
                    let Ok(mut t) = world.get_entity_mut(target) else { return };
                    // SAFETY: as above.
                    let empty = t.get_by_id(target_id).is_ok_and(|p| unsafe { p.deref::<Vec<Entity>>() }.is_empty());
                    if empty {
                        t.remove_by_id(target_id);
                    }
                });
            }
        }
        Some(Role::Target { source, .. }) => {
            // A target's `RunIfNotLinked` is a skip too, as Bevy's is.
            if !matches!(ctx.relationship_hook_mode, RelationshipHookMode::Run) {
                return;
            }
            for s in listed(&mut world, entity, id) {
                world.commands().queue(move |world: &mut World| {
                    if let Ok(mut em) = world.get_entity_mut(s) {
                        em.remove_by_id(source);
                    }
                });
            }
        }
        _ => {}
    }
}

/// The `on_despawn` slot: a linked target despawns its sources (Bevy's
/// `RelationshipTarget::on_despawn`).
pub fn dispatch_on_despawn(mut world: DeferredWorld, ctx: HookContext) {
    let Some(Role::Target { linked_spawn: true, .. }) = role_of(&world, ctx.component_id) else { return };
    for s in listed(&mut world, ctx.entity, ctx.component_id) {
        world.commands().entity(s).try_despawn();
    }
}

/// A target's entities on `e`.
fn listed(world: &mut DeferredWorld, e: Entity, id: ComponentId) -> Vec<Entity> {
    let Ok(em) = world.get_entity(e) else { return Vec::new() };
    let Ok(p) = em.get_by_id(id) else { return Vec::new() };
    // SAFETY: a target's value is a `Vec<Entity>`.
    unsafe { p.deref::<Vec<Entity>>() }.clone()
}

/// A target's `RelationshipAccessor` `iter`: one fn for every TS target.
///
/// # Safety
/// `p` points at a target's value.
unsafe fn iter_target<'a>(p: Ptr<'a>) -> Box<dyn Iterator<Item = Entity> + 'a> {
    // SAFETY: the caller's contract.
    Box::new(unsafe { p.deref::<Vec<Entity>>() }.iter().copied())
}

/// A source's accessor. Its target's id is filled in when the target registers.
pub(crate) fn source_accessor(offset: usize, linked_spawn: bool, allow_self_referential: bool) -> RelationshipAccessorInitializer {
    RelationshipAccessorInitializer::Relationship { entity_field_offset: offset, linked_spawn, allow_self_referential, relationship_target_getter: Arc::new(|_| None) }
}

/// A target's accessor, naming its registered source.
pub(crate) fn target_accessor(source: ComponentId, linked_spawn: bool, allow_self_referential: bool) -> RelationshipAccessorInitializer {
    RelationshipAccessorInitializer::RelationshipTarget { iter: iter_target, linked_spawn, allow_self_referential, relationship_getter: Arc::new(move |_| Some(source)) }
}

/// Where the clone's source points: its `target` mapped through the clone's entity mapper.
fn map_target(p: *mut u8, ctx: &mut ComponentCloneCtx) {
    let Some(RelationshipAccessor::Relationship { entity_field_offset: off, .. }) = ctx.component_info().relationship_accessor().copied() else { return };
    // SAFETY: `p` holds the clone's value, its `target` at `off`.
    let at = unsafe { p.add(off) } as *mut Entity;
    // SAFETY: as above.
    let mapped = ctx.entity_mapper().get_mapped(unsafe { std::ptr::read_unaligned(at) });
    // SAFETY: as above.
    unsafe { std::ptr::write_unaligned(at, mapped) };
}

/// A plain source's clone fn: its bytes, then its `target` mapped.
pub fn clone_source(source: &SourceComponent, ctx: &mut ComponentCloneCtx) {
    let size = ctx.component_info().layout().size();
    // SAFETY: the source is a live value of `size` bytes, plain data.
    let bytes = unsafe { std::slice::from_raw_parts(source.ptr().as_ptr(), size) };
    let mut buf = crate::heap::Aligned::new(bytes);
    map_target(buf.ptr().as_ptr(), ctx);
    // SAFETY: plain bytes own nothing, so a copy owns nothing twice.
    unsafe { ctx.write_target_component_ptr(Ptr::new(buf.ptr())) };
}

/// A source with a heap field's clone fn: a deep copy, then its `target` mapped.
pub fn clone_source_heap(source: &SourceComponent, ctx: &mut ComponentCloneCtx) {
    crate::heap::clone_component_with(source, ctx, map_target);
}

/// A target's clone fn (Bevy's `clone_relationship_target`): the clone's list starts empty and
/// fills as its sources are inserted; a linked clone clones the sources too, and a move
/// retargets them with no hook.
pub fn clone_target(source: &SourceComponent, ctx: &mut ComponentCloneCtx) {
    let Some(RelationshipAccessor::RelationshipTarget { linked_spawn, relationship, .. }) = ctx.component_info().relationship_accessor().copied() else { return };
    // SAFETY: a target's value is a `Vec<Entity>`.
    let list = unsafe { source.ptr().deref::<Vec<Entity>>() };
    let mut cloned = Vec::with_capacity(list.len());
    if ctx.linked_cloning() && linked_spawn {
        for e in list {
            ctx.queue_entity_clone(*e);
            cloned.push(ctx.entity_mapper().get_mapped(*e));
        }
    } else if ctx.moving() {
        let to = ctx.target();
        for &e in list {
            cloned.push(e);
            ctx.queue_deferred(move |world: &mut World, _| retarget(world, e, relationship, to));
        }
    }
    let cloned = std::mem::ManuallyDrop::new(cloned);
    // SAFETY: the clone owns the new `Vec`, which is never dropped here.
    unsafe { ctx.write_target_component_ptr(Ptr::from(&*cloned)) };
}

/// Points `e`'s source at `to` in place, with no hook (Bevy's `set_risky` under
/// `RelationshipHookMode::Skip`): the list was built by hand.
fn retarget(world: &mut World, e: Entity, source: ComponentId, to: Entity) {
    let offset = match world.resource::<TsHooks>().role(source) {
        Some(Role::Source { offset, .. }) => offset,
        _ => return,
    };
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    // SAFETY: a source is immutable only so that writes go through its hooks; this write is the
    // move Bevy makes with its own crate-private call, the list already holding `e`.
    let Ok(mut v) = (unsafe { em.get_mut_assume_mutable_by_id(source) }) else { return };
    // SAFETY: the value is a source with its `target` at `offset`.
    unsafe { std::ptr::write_unaligned(v.as_mut().as_ptr().add(offset) as *mut Entity, to) };
}

/// Fills a target's lists from its sources, under `target_id`, with no hook running: a reload's
/// rebuild. A source pointing at a missing entity, or at itself when not allowed, is removed.
pub(crate) fn rebuild(world: &mut World, source_id: ComponentId, target_id: ComponentId) {
    let Some(Role::Source { offset, allow_self_referential, .. }) = world.resource::<TsHooks>().role(source_id) else { return };
    let mut q = QueryBuilder::<Entity>::new(world).with_id(source_id).build();
    let sources: Vec<Entity> = q.iter(world).collect();
    for s in sources {
        let p = world.entity(s).get_by_id(source_id).expect("queried entity has the source");
        // SAFETY: `source_id` is a source with its `target` at `offset`.
        let t = unsafe { target_at(p.as_ptr(), offset) };
        if (t == s && !allow_self_referential) || world.get_entity(t).is_err() {
            world.entity_mut(s).remove_by_id(source_id);
            continue;
        }
        push_source(&mut world.entity_mut(t), target_id, s);
    }
}
