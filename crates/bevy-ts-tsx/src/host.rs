//! The `tsx` host call. Each frame in `PostUpdate`, after the bevy-ts stages, Rust checks the
//! change ticks of every component a binding read (no JS). Only when one moved, or a generation
//! or mount brought new bindings, or JS asked (a signal set by a system), does it call JS once,
//! carrying the changed values; the call returns patches for bound props, slot mounts, and the
//! pairs to watch. A quiet frame makes no call.

use crate::desc::{NodeDesc, normalize_sources};
use crate::reconcile::{self, Prune, apply_root, log_error, resolve_tree};
use crate::resolve;
use crate::tree::{BindTarget, FieldState, Provenance, Subscription, TsxNode, TsxTree};
use crate::ui::UiKind;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::components::TsComponents;
use bevy_ts::layout::{ComponentLayout, NativeLayouts};
use bevy_ts::{plugin_api, value};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The entry every generation registers for tsx.
pub const ENTRY: &str = "tsx";
/// The command kind JS appends when a binding changed outside a host call.
pub const FLUSH: &str = "tsx/flush";

/// Host calls one frame may chain while JS asks for values it has not seen.
const MAX_ROUNDS: usize = 8;

#[derive(Serialize, Default)]
struct Payload {
    refs: Vec<(u32, u64)>,
    values: Vec<(u64, String, Value)>,
    drop: Vec<u32>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Response {
    patches: Vec<(u32, Value)>,
    mounts: Vec<(u32, String, Vec<NodeDesc>)>,
    unmounts: Vec<(u32, String)>,
    subscribe: Vec<(u64, String)>,
    unsubscribe: Vec<(u64, String)>,
    needs: Vec<(u64, String)>,
    errors: Vec<ScriptError>,
}

/// A component by name with a native layout: a live TS component or a registered Rust one.
fn component(world: &World, name: &str) -> Option<(ComponentId, ComponentLayout)> {
    if let Some(c) = world.resource::<TsComponents>().live(name) {
        return Some((c.id, c.layout.clone()));
    }
    world.resource::<NativeLayouts>().get(name).map(|n| (n.id, n.layout.clone()))
}

/// A component's current value as JSON, or null when the entity or component is gone.
fn read(world: &World, e: Entity, name: &str) -> Value {
    let Some((id, layout)) = component(world, name) else { return Value::Null };
    let Some(ptr) = world.get_entity(e).ok().and_then(|em| em.get_by_id(id).ok()) else { return Value::Null };
    // SAFETY: the component was registered with `layout`, so it holds `layout.size` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) };
    value::decode_component(&layout, bytes)
}

fn entity_of(world: &World, tree: &TsxTree, node: u64) -> Option<Entity> {
    let e = *tree.entities.get(&node)?;
    world.get::<TsxNode>(e).filter(|n| n.id == node).map(|_| e)
}

/// `PostUpdate`: the per-frame check, and the call when something moved.
pub fn frame(world: &mut World) {
    let reactive = &world.resource::<TsxTree>().reactive;
    if !reactive.due && reactive.subs.is_empty() {
        return;
    }
    let now = world.read_change_tick();
    let mut changed = Vec::new();
    for ((e, name), sub) in &reactive.subs {
        let moved = match world.get_entity(*e) {
            Ok(em) => em.get_change_ticks_by_id(sub.id).is_none_or(|t| t.is_changed(sub.last, now)),
            // Despawned: tell JS once (null), then stop watching.
            Err(_) => true,
        };
        if moved {
            changed.push((*e, name.clone()));
        }
    }
    if !reactive.due && changed.is_empty() {
        return;
    }
    run(world, changed);
}

fn run(world: &mut World, changed: Vec<(Entity, String)>) {
    let mut payload = Payload::default();
    {
        let mut tree = world.remove_resource::<TsxTree>().expect("tsx tree");
        let refs = std::mem::take(&mut tree.reactive.new_refs);
        for r in refs {
            if let Some(e) = tree.reactive.refs.get(&r).and_then(|n| entity_of(world, &tree, *n)) {
                payload.refs.push((r, e.to_bits()));
            }
        }
        let dead: Vec<u32> = tree
            .reactive
            .targets
            .iter()
            .filter(|(_, t)| entity_of(world, &tree, t.node).is_none() && tree.entities.contains_key(&t.node))
            .map(|(id, _)| *id)
            .collect();
        for id in &dead {
            tree.reactive.targets.remove(id);
        }
        payload.drop = dead;
        world.insert_resource(tree);
    }
    payload.values = deliver(world, &changed);

    for _ in 0..MAX_ROUNDS {
        let bytes = serde_json::to_vec(&payload).expect("serializable");
        world.resource_mut::<TsxTree>().reactive.calls += 1;
        let out = match plugin_api::call_entry(world, ENTRY, &bytes) {
            Ok(out) => out,
            Err(e) => {
                log_error(world, format!("tsx host call: {}", e.message), e.location);
                break;
            }
        };
        let response: Response = match serde_json::from_slice(&out) {
            Ok(r) => r,
            Err(e) => {
                log_error(world, format!("tsx host call returned bad output: {e}"), None);
                break;
            }
        };
        let needs = apply(world, response);
        let refs = std::mem::take(&mut world.resource_mut::<TsxTree>().reactive.new_refs);
        if needs.is_empty() && refs.is_empty() {
            break;
        }
        let tree = world.resource::<TsxTree>();
        payload = Payload {
            refs: refs
                .into_iter()
                .filter_map(|r| tree.reactive.refs.get(&r).and_then(|n| entity_of(world, tree, *n)).map(|e| (r, e.to_bits())))
                .collect(),
            values: Vec::new(),
            drop: Vec::new(),
        };
        payload.values = deliver(world, &needs);
    }

    let mut tree = world.remove_resource::<TsxTree>().expect("tsx tree");
    tree.reactive.due = false;
    let sweep = std::mem::take(&mut tree.reactive.sweep);
    let slots: Vec<_> = if sweep { tree.reactive.slots.values().cloned().collect() } else { vec![] };
    world.insert_resource(tree);
    // A fresh generation's slots have mounted what they show now; content an earlier
    // generation mounted that they did not mount again goes.
    for s in slots {
        reconcile::sweep(world, &s.root, &s.prefix, &s.groups);
    }
}

/// The current values of `pairs`, marking them delivered now.
fn deliver(world: &mut World, pairs: &[(Entity, String)]) -> Vec<(u64, String, Value)> {
    let now = world.read_change_tick();
    let values: Vec<_> = pairs.iter().map(|(e, name)| (e.to_bits(), name.clone(), read(world, *e, name))).collect();
    let mut tree = world.resource_mut::<TsxTree>();
    for ((e, name), (_, _, v)) in pairs.iter().zip(&values) {
        if v.is_null() {
            tree.reactive.subs.remove(&(*e, name.clone()));
        } else if let Some(s) = tree.reactive.subs.get_mut(&(*e, name.clone())) {
            s.last = now;
        }
    }
    values
}

/// Applies a host call's output; returns the pairs JS read but has no value for.
fn apply(world: &mut World, r: Response) -> Vec<(Entity, String)> {
    for e in r.errors {
        log_error(world, e.message, e.location);
    }
    let now = world.read_change_tick();
    for (bits, name) in &r.unsubscribe {
        if let Some(e) = Entity::try_from_bits(*bits) {
            world.resource_mut::<TsxTree>().reactive.subs.remove(&(e, name.clone()));
        }
    }
    for (bits, name) in &r.subscribe {
        let Some(e) = Entity::try_from_bits(*bits) else { continue };
        match component(world, name) {
            Some((id, _)) => {
                world.resource_mut::<TsxTree>().reactive.subs.insert((e, name.clone()), Subscription { id, last: now });
            }
            None => log_error(world, format!("ref.get({name}): tsx reads components with a native layout (TS components, Transform)"), None),
        }
    }
    for (id, v) in r.patches {
        patch(world, id, v);
    }
    for (slot, group) in r.unmounts {
        slot_op(world, slot, group, Vec::new(), false);
    }
    for (slot, group, nodes) in r.mounts {
        slot_op(world, slot, group, nodes, true);
    }
    r.needs.into_iter().filter_map(|(bits, name)| Entity::try_from_bits(bits).map(|e| (e, name))).collect()
}

/// Writes one bound prop, unless it already holds the value.
fn patch(world: &mut World, binding: u32, v: Value) {
    let tree = world.resource::<TsxTree>();
    let Some(target) = tree.reactive.targets.get(&binding).cloned() else { return };
    let Some(e) = entity_of(world, tree, target.node) else { return };
    let BindTarget { component: key, prop, .. } = target;
    let node = world.get::<TsxNode>(e).expect("alive");
    let Some(owned) = node.components.get(&key) else { return };
    if owned.fields.get(&prop).is_some_and(|s| s.applied == v) {
        return;
    }
    let source = owned.source.clone();
    let written = if let Some(kind) = UiKind::from_tag(&key) {
        let old: Map<String, Value> = owned.fields.iter().filter(|(_, s)| !s.applied.is_null()).map(|(k, s)| (k.clone(), s.applied.clone())).collect();
        let mut new = old.clone();
        if v.is_null() {
            new.remove(&prop);
        } else {
            new.insert(prop.clone(), v.clone());
        }
        crate::ui::update(world, e, kind, &old, &new).map(|_| ())
    } else if key == resolve::MESH || key == resolve::MATERIAL || key == resolve::NAME {
        Err("this prop cannot be bound".to_string())
    } else {
        match component(world, &key) {
            Some((id, layout)) => match layout.field(&prop) {
                Some(fl) => {
                    let mut scratch = vec![0u8; fl.ty.size()];
                    value::encode_field(&format!("{key}.{prop}"), &fl.ty, &v, &mut scratch)
                        .map_err(|e| e.to_string())
                        .map(|_| {
                            reconcile::write_field(world, e, id, &layout, &prop, &v);
                        })
                }
                None => Err(format!("{key} has no field {prop}")),
            },
            None => Err(format!("{key} is not a live component")),
        }
    };
    match written {
        Ok(()) => {
            let mut node = world.get_mut::<TsxNode>(e).expect("alive");
            if let Some(c) = node.components.get_mut(&key) {
                c.fields.insert(prop, FieldState { provenance: Provenance::Bound, applied: v });
            }
            world.resource_mut::<TsxTree>().reactive.patched += 1;
        }
        Err(m) => log_error(world, format!("binding <{key} {prop}>: {m}"), source),
    }
}

/// Mounts (or, with `mount` false, unmounts) one group of a slot's content.
fn slot_op(world: &mut World, slot: u32, group: String, mut nodes: Vec<NodeDesc>, mount: bool) {
    let Some(state) = world.resource::<TsxTree>().reactive.slots.get(&slot).cloned() else { return };
    let mut prefix = state.prefix.clone();
    prefix.push(group.clone());
    let mut types = state.types.clone();
    types.push("group".into());
    normalize_sources(&mut nodes);
    let mut errors = Vec::new();
    resolve::check_nodes(&nodes, world.resource::<TsComponents>(), world.resource::<NativeLayouts>(), &mut errors);
    if let Some(e) = errors.into_iter().next() {
        log_error(world, e.message, e.location);
        return;
    }
    let resolved = match resolve_tree(world, &state.root, &nodes, (&prefix, &types)) {
        Ok(r) => r,
        Err(e) => {
            log_error(world, e.message, e.location);
            return;
        }
    };
    apply_root(world, &state.root, None, &resolved, state.parent, Prune::Under(prefix));
    let mut tree = world.resource_mut::<TsxTree>();
    if let Some(s) = tree.reactive.slots.get_mut(&slot) {
        if mount {
            s.groups.insert(group);
        } else {
            s.groups.remove(&group);
        }
    }
}

/// `tsx/flush`: JS changed a binding outside a host call.
pub fn flush(world: &mut World, _bytes: &[u8]) -> Result<(), String> {
    world.resource_mut::<TsxTree>().reactive.due = true;
    Ok(())
}
