//! The `tsx` host call. Each frame in `PostUpdate`, after the bevy-ts stages, Rust checks the
//! change ticks of every component a binding read (no JS). Only when one moved, or a generation
//! or mount brought new bindings, or JS asked (a signal set by a system), does it call JS once,
//! carrying the changed values; the call returns patches for bound props, slot mounts, and the
//! pairs to watch. A quiet frame makes no call.

use crate::desc::{Files, NodeDesc, normalize_sources, normalize_with};
use crate::reconcile::{self, Prune, RNode, apply_root, log_error, resolve_tree};
use crate::resolve;
use crate::tree::{BindTarget, FieldState, GroupRows, Provenance, Subscription, TsxNode, TsxTree};
use crate::ui::UiKind;
use bevy::ecs::component::ComponentId;
use bevy::ecs::query::QueryBuilder;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::*;
use bevy_ts::backend::{ScriptError, SourceLocation};
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
    /// Pair values; no entity for a resource (`res(R)`).
    values: Vec<(Option<u64>, String, Value)>,
    drop: Vec<u32>,
    /// Site instances and their entities, once mounted.
    instances: Vec<(u32, u64)>,
    /// Site rows whose own reads changed, as JSON (the binary rows travel after the header).
    rows: Vec<(u32, Vec<(String, Value)>)>,
    /// Picking events since the last call, in arrival order: handler id, event.
    events: Vec<(u32, Value)>,
    /// Scene component instances to expand (levels): name or `module#export`, props, location.
    #[serde(skip_serializing_if = "Option::is_none")]
    expand: Option<Vec<ExpandWire>>,
}

/// An `expand` item as it crosses: name or `module#export`, props, location.
type ExpandWire = (String, Map<String, Value>, SourceLocation);

#[derive(Deserialize, Default)]
#[serde(default)]
struct Response {
    patches: Vec<(u32, Value)>,
    mounts: Vec<(u32, String, Vec<NodeDesc>)>,
    unmounts: Vec<(u32, String)>,
    /// A slot's group keys in item order, when it changed other than by appending.
    orders: Vec<(u32, Vec<String>)>,
    subscribe: Vec<(Option<u64>, String)>,
    unsubscribe: Vec<(Option<u64>, String)>,
    needs: Vec<(Option<u64>, String)>,
    /// Sites whose reads were fixed by their first run: site, own components read.
    sites: Vec<(u32, Vec<String>)>,
    /// One description per `expand` item, in order; null where it failed.
    expanded: Option<Vec<Option<NodeDesc>>>,
    /// One entry per `expand` item, in order: the error its expansion raised, or null.
    failures: Option<Vec<Option<ScriptError>>>,
    errors: Vec<ScriptError>,
    /// Slot content this call mounted has bindings to start once it is mounted: call again.
    more: bool,
}

/// A component by name with a native layout: a live TS component or a registered Rust one.
fn component(world: &World, name: &str) -> Option<(ComponentId, ComponentLayout)> {
    if let Some(c) = world.resource::<TsComponents>().live(name) {
        return Some((c.id, c.layout.clone()));
    }
    world.resource::<NativeLayouts>().get(name).map(|n| (n.id, n.layout.clone()))
}

/// A component's current value as JSON, or null when the entity or component is gone.
/// The entity a pair reads: its own, or a resource's entity.
fn target(world: &World, e: Option<Entity>, id: ComponentId) -> Option<Entity> {
    match e {
        Some(e) => Some(e),
        None => world.resource_entities().get(id),
    }
}

/// A pair key from JS: an entity's bits, or null for a resource.
fn key(bits: Option<u64>) -> Option<Option<Entity>> {
    match bits {
        None => Some(None),
        Some(b) => Entity::try_from_bits(b).map(Some),
    }
}

/// A `has(C)` pair's component: a native-layout one, or any reflected component by type path or
/// short type path (markers such as `Pressed` have no layout). `None` while the world has not
/// registered it, which no entity can then carry.
fn presence_component(world: &World, name: &str) -> Option<ComponentId> {
    if let Some((id, _)) = component(world, name) {
        return Some(id);
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let reg = registry.get_with_type_path(name).or_else(|| registry.get_with_short_type_path(name))?;
    world.components().get_id(reg.type_id())
}

/// As [`presence_component`], registering a reflected component the world has not seen yet, so
/// the pair has an id to watch before any entity carries it.
fn presence_component_mut(world: &mut World, name: &str) -> Option<ComponentId> {
    if let Some(id) = presence_component(world, name) {
        return Some(id);
    }
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let reg = registry.get_with_type_path(name).or_else(|| registry.get_with_short_type_path(name))?;
    let rc = reg.data::<ReflectComponent>()?;
    Some(rc.register_component(world))
}

/// Whether the entity is alive and carries the component.
fn present(world: &World, e: Option<Entity>, id: Option<ComponentId>) -> bool {
    match (e.and_then(|e| world.get_entity(e).ok()), id) {
        (Some(em), Some(id)) => em.contains_id(id),
        _ => false,
    }
}

/// A component a ref may read: a layout's, or a reflect-tier Rust one read through reflection.
fn pair_component(world: &World, name: &str) -> Option<ComponentId> {
    component(world, name).map(|(id, _)| id).or_else(|| world.resource::<NativeLayouts>().reflect_tier(name).map(|t| t.id))
}

fn read_pair(world: &World, e: Option<Entity>, name: &str) -> Value {
    if let Some(c) = name.strip_prefix('?') {
        return Value::Bool(present(world, e, presence_component(world, c)));
    }
    let Some(id) = pair_component(world, name) else { return Value::Null };
    match target(world, e, id) {
        Some(t) => read(world, t, name),
        None => Value::Null,
    }
}

/// A component's value as `ref.get` sees it: a layout's fields with its holes read through
/// reflection, or a reflect-tier component's whole value in `reflect_value::read_component`'s shape.
fn read(world: &World, e: Entity, name: &str) -> Value {
    let Some(em) = world.get_entity(e).ok() else { return Value::Null };
    let Some((id, layout)) = component(world, name) else {
        let Some(id) = pair_component(world, name) else { return Value::Null };
        return reflected(world, em, id, bevy_ts::reflect_value::read_component).unwrap_or(Value::Null);
    };
    let Ok(ptr) = em.get_by_id(id) else { return Value::Null };
    // SAFETY: the component was registered with `layout`, so it holds `layout.size` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) };
    // SAFETY: a live column value of `layout`.
    let mut out = unsafe { value::decode_component(&layout, bytes) };
    if let Value::Object(o) = &mut out
        && world.resource::<NativeLayouts>().by_id(id).is_some_and(|n| !n.holes.is_empty())
    {
        let holes = reflected(world, em, id, |registry, v| {
            let bevy::reflect::ReflectRef::Struct(st) = v.reflect_ref() else { return Ok(Value::Null) };
            let mut m = Map::new();
            for i in 0..st.field_len() {
                let f = st.name_at(i).unwrap_or_default();
                if !layout.fields.iter().any(|l| l.name == f)
                    && let Ok(j) = bevy_ts::reflect_value::read(registry, st.field_at(i).expect("field"))
                {
                    m.insert(f.to_string(), j);
                }
            }
            Ok(Value::Object(m))
        });
        if let Some(Value::Object(h)) = holes {
            o.extend(h);
        }
    }
    out
}

/// `f` over component `id` of `em` by reflection.
fn reflected(
    world: &World,
    em: bevy::ecs::world::EntityRef,
    id: ComponentId,
    f: impl FnOnce(&bevy::reflect::TypeRegistry, &dyn PartialReflect) -> Result<Value, String>,
) -> Option<Value> {
    let type_id = world.components().get_info(id)?.type_id()?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let rc = registry.get_type_data::<ReflectComponent>(type_id)?;
    let v = rc.reflect(em)?;
    f(&registry, v.as_partial_reflect()).ok()
}

fn entity_of(world: &World, tree: &TsxTree, node: u64) -> Option<Entity> {
    let e = *tree.entities.get(&node)?;
    world.get::<TsxNode>(e).filter(|n| n.id == node).map(|_| e)
}

/// `PostUpdate`: the per-frame check, and the call when something moved.
pub fn frame(world: &mut World) {
    let started = std::time::Instant::now();
    world.resource_mut::<TsxTree>().reactive.last_frame_us = 0.0;
    frame_inner(world);
    let us = started.elapsed().as_secs_f64() * 1e6;
    world.resource_mut::<TsxTree>().reactive.last_frame_us = us;
}

fn frame_inner(world: &mut World) {
    let reactive = &world.resource::<TsxTree>().reactive;
    if !reactive.due && reactive.events.is_empty() && reactive.subs.is_empty() && reactive.sites.values().all(|s| s.query.is_none()) {
        return;
    }
    let now = world.read_change_tick();
    let mut changed = Vec::new();
    for ((e, name), sub) in &reactive.subs {
        // A `has` pair: one archetype lookup, the same order of cost as a tick read.
        if let Some(was) = sub.present {
            if present(world, *e, Some(sub.id)) != was {
                changed.push((*e, name.clone()));
            }
            continue;
        }
        let moved = match target(world, *e, sub.id).and_then(|t| world.get_entity(t).ok()) {
            Some(em) => em.get_change_ticks_by_id(sub.id).is_none_or(|t| t.is_changed(sub.last, now)),
            // Despawned (or the resource removed): tell JS once (null), then stop watching.
            None => true,
        };
        if moved {
            changed.push((*e, name.clone()));
        }
    }
    let t = std::time::Instant::now();
    let rows = changed_rows(world, now);
    world.resource_mut::<TsxTree>().reactive.scan_us += t.elapsed().as_secs_f64() * 1e6;
    let reactive = &world.resource::<TsxTree>().reactive;
    if !reactive.due && reactive.events.is_empty() && changed.is_empty() && rows.is_empty() {
        return;
    }
    run(world, changed, rows);
}

/// Site rows whose own reads changed since the site's last delivery, scanned by tick column in
/// table order as `Changed<T>` does (column summary ticks would skip still tables, but untyped
/// writes do not keep them), and packed as raw component bytes for JS (see `decodeRows` in the
/// JSX runtime). No JS, no JSON. Empty when nothing changed.
fn changed_rows(world: &mut World, now: bevy::ecs::change_detection::Tick) -> Vec<u8> {
    let mut tree = world.remove_resource::<TsxTree>().expect("tsx tree");
    let mut out = vec![0u8; 4];
    let mut sites = 0u32;
    for (site_id, site) in tree.reactive.sites.iter_mut() {
        let Some(query) = site.query.as_mut() else { continue };
        query.update_archetypes(world);
        let last = site.last;
        let start = out.len();
        out.extend(site_id.to_le_bytes());
        out.extend((site.reads.len() as u16).to_le_bytes());
        for (name, _, layout) in &site.reads {
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(name.as_bytes());
            out.extend((layout.size as u32).to_le_bytes());
        }
        let count_at = out.len();
        out.extend(0u32.to_le_bytes());
        let mut rows = 0u32;
        let tables = &world.storages().tables;
        for table_id in query.matched_tables() {
            let Some(table) = tables.get(table_id) else { continue };
            let ticks: Vec<_> = site.reads.iter().filter_map(|(_, id, _)| table.get_changed_ticks_slice_for(*id)).collect();
            let len = table.entity_count() as usize;
            // SAFETY: the column holds `len` values of `layout.size` bytes, contiguous.
            let columns: Vec<&[std::cell::UnsafeCell<u8>]> = site
                .reads
                .iter()
                .filter_map(|(_, id, layout)| table.get_column(*id).map(|c| unsafe { c.get_data_slice::<u8>(len * layout.size) }))
                .collect();
            for (row, e) in table.entities().iter().enumerate() {
                // SAFETY: no one writes ticks while this exclusive system holds the world.
                if !ticks.iter().any(|t| unsafe { *t[row].get() }.is_newer_than(last, now)) {
                    continue;
                }
                let Some(instance) = site.instances.get(e) else { continue };
                out.extend(instance.to_le_bytes());
                for (c, (_, _, layout)) in columns.iter().zip(&site.reads) {
                    // SAFETY: `row` is in the table; nothing writes the column during this system.
                    let bytes = unsafe { std::slice::from_raw_parts(c[row * layout.size].get(), layout.size) };
                    out.extend_from_slice(bytes);
                }
                rows += 1;
            }
        }
        site.last = now;
        if rows == 0 {
            out.truncate(start);
        } else {
            out[count_at..count_at + 4].copy_from_slice(&rows.to_le_bytes());
            sites += 1;
        }
    }
    world.insert_resource(tree);
    if sites == 0 {
        return Vec::new();
    }
    out[..4].copy_from_slice(&sites.to_le_bytes());
    out
}

fn run(world: &mut World, changed: Vec<(Option<Entity>, String)>, mut rows: Vec<u8>) {
    let mut payload = Payload::default();
    {
        let mut tree = world.remove_resource::<TsxTree>().expect("tsx tree");
        payload.events = std::mem::take(&mut tree.reactive.events);
        payload.instances = std::mem::take(&mut tree.reactive.new_instances).into_iter().map(|(id, e)| (id, e.to_bits())).collect();
        let refs = std::mem::take(&mut tree.reactive.new_refs);
        for r in refs {
            if let Some(e) = tree.reactive.refs.get(&r).and_then(|n| entity_of(world, &tree, *n)) {
                payload.refs.push((r, e.to_bits()));
            }
        }
        // Only after a despawn can a target point at a gone node.
        let dead: Vec<u32> = if std::mem::take(&mut tree.reactive.stale) {
            tree.reactive.targets.iter().filter(|(_, t)| entity_of(world, &tree, t.node).is_none()).map(|(id, _)| *id).collect()
        } else {
            Vec::new()
        };
        for id in &dead {
            tree.reactive.targets.remove(id);
        }
        payload.drop = dead;
        world.insert_resource(tree);
    }
    payload.values = deliver(world, &changed);

    // Slot content still to start when the rounds ran out: the next frame calls again.
    let mut again = false;
    for _ in 0..MAX_ROUNDS {
        let t = std::time::Instant::now();
        // A u32 header length, the JSON header, then the binary rows (first round only).
        let header = serde_json::to_vec(&payload).expect("serializable");
        let mut bytes = Vec::with_capacity(4 + header.len() + rows.len());
        bytes.extend((header.len() as u32).to_le_bytes());
        bytes.extend(header);
        bytes.append(&mut rows);
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
        let t2 = std::time::Instant::now();
        world.resource_mut::<TsxTree>().reactive.call_us += (t2 - t).as_secs_f64() * 1e6;
        let more = response.more;
        again = more;
        let needs = apply(world, response);
        world.resource_mut::<TsxTree>().reactive.apply_us += t2.elapsed().as_secs_f64() * 1e6;
        let refs = std::mem::take(&mut world.resource_mut::<TsxTree>().reactive.new_refs);
        let instances = std::mem::take(&mut world.resource_mut::<TsxTree>().reactive.new_instances);
        if !more && needs.is_empty() && refs.is_empty() && instances.is_empty() {
            break;
        }
        let tree = world.resource::<TsxTree>();
        payload = Payload {
            refs: refs
                .into_iter()
                .filter_map(|r| tree.reactive.refs.get(&r).and_then(|n| entity_of(world, tree, *n)).map(|e| (r, e.to_bits())))
                .collect(),
            instances: instances.into_iter().map(|(id, e)| (id, e.to_bits())).collect(),
            ..Default::default()
        };
        payload.values = deliver(world, &needs);
    }

    let mut tree = world.remove_resource::<TsxTree>().expect("tsx tree");
    tree.reactive.due = again;
    tree.journal.close();
    let sweep = std::mem::take(&mut tree.reactive.sweep);
    let slots: Vec<_> = if sweep { tree.reactive.slots.values().cloned().collect() } else { vec![] };
    world.insert_resource(tree);
    // A fresh generation's slots have mounted what they show now; content an earlier
    // generation mounted that they did not mount again goes.
    for s in slots {
        reconcile::sweep(world, &s.root, &s.prefix, &s.groups);
    }
    world.resource_mut::<TsxTree>().journal.close();
}

/// The current values of `pairs`, marking them delivered now.
fn deliver(world: &mut World, pairs: &[(Option<Entity>, String)]) -> Vec<(Option<u64>, String, Value)> {
    let now = world.read_change_tick();
    let values: Vec<_> = pairs.iter().map(|(e, name)| (e.map(|e| e.to_bits()), name.clone(), read_pair(world, *e, name))).collect();
    let mut tree = world.resource_mut::<TsxTree>();
    for ((e, name), (_, _, v)) in pairs.iter().zip(&values) {
        if v.is_null() {
            tree.reactive.subs.remove(&(*e, name.clone()));
        } else if let Some(s) = tree.reactive.subs.get_mut(&(*e, name.clone())) {
            s.last = now;
            if s.present.is_some() {
                s.present = v.as_bool();
            }
        }
    }
    values
}

/// Applies a host call's output; returns the pairs JS read but has no value for.
fn apply(world: &mut World, r: Response) -> Vec<(Option<Entity>, String)> {
    for e in r.errors {
        log_error(world, e.message, e.location);
    }
    let now = world.read_change_tick();
    for (bits, name) in &r.unsubscribe {
        if let Some(e) = key(*bits) {
            world.resource_mut::<TsxTree>().reactive.subs.remove(&(e, name.clone()));
        }
    }
    for (bits, name) in &r.subscribe {
        let Some(e) = key(*bits) else { continue };
        if let Some(c) = name.strip_prefix('?') {
            match presence_component_mut(world, c) {
                Some(id) => {
                    // JS holds the value delivered this frame, so it is the presence now.
                    let was = present(world, e, Some(id));
                    world.resource_mut::<TsxTree>().reactive.subs.insert((e, name.clone()), Subscription { id, last: now, present: Some(was) });
                }
                None => log_error(world, format!("ref.has({c}): no registered component is named {c}"), None),
            }
            continue;
        }
        match pair_component(world, name) {
            Some(id) => {
                world.resource_mut::<TsxTree>().reactive.subs.insert((e, name.clone()), Subscription { id, last: now, present: None });
            }
            None => log_error(world, format!("ref.get({name}): no component named {name}"), None),
        }
    }
    for (site, names) in r.sites {
        let reads: Vec<(String, ComponentId, ComponentLayout)> = names.iter().filter_map(|n| component(world, n).map(|(id, l)| (n.clone(), id, l))).collect();
        if reads.len() != names.len() {
            log_error(world, format!("a bound field reads {names:?}; tsx reads components with a native layout only"), None);
            continue;
        }
        let mut builder = QueryBuilder::<Entity>::new(world);
        for (_, id, _) in &reads {
            builder.with_id(*id);
        }
        let query = builder.build();
        let mut tree = world.resource_mut::<TsxTree>();
        if let Some(s) = tree.reactive.sites.get_mut(&site) {
            s.reads = reads;
            s.query = Some(query);
            s.last = now;
        }
    }
    for (id, v) in r.patches {
        patch(world, id, v);
    }
    // Each slot's part of the call, in the order its slots first appear: one reconcile each.
    let mut slots: Vec<(u32, SlotOps)> = Vec::new();
    let mut at = std::collections::HashMap::new();
    for (slot, group) in r.unmounts {
        slot_ops(&mut slots, &mut at, slot).unmounts.push(group);
    }
    for (slot, group, nodes) in r.mounts {
        slot_ops(&mut slots, &mut at, slot).mounts.push((group, nodes));
    }
    for (slot, keys) in r.orders {
        slot_ops(&mut slots, &mut at, slot).order = Some(keys);
    }
    for (slot, ops) in slots {
        slot_apply(world, slot, ops);
    }
    r.needs.into_iter().filter_map(|(bits, name)| key(bits).map(|e| (e, name))).collect()
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
    } else if key == resolve::MATERIAL && owned.fields.contains_key("kind") {
        // A script material's instance param: the entity's `ScriptInstance`, never the asset.
        let mut props: Map<String, Value> = owned.fields.iter().filter(|(k, s)| !s.applied.is_null() && k.as_str() != "kind" && k.as_str() != "gltfMaterial").map(|(k, s)| (k.clone(), s.applied.clone())).collect();
        props.insert(prop.clone(), v.clone());
        let kind = match &owned.fields["kind"].applied {
            Value::String(s) => s.clone(),
            k => k.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        };
        bevy_ts_material::instance::patch(world, e, &kind, &prop, &props, source.clone())
    } else if resolve::asset_intrinsic(&key) || key == resolve::NAME {
        Err("this prop cannot be bound".to_string())
    } else if let Some(path) = world.resource::<TsComponents>().live(&key).is_none().then(|| resolve::reflect_path(world.resource::<NativeLayouts>(), &key)).flatten() {
        // A reflect-tier field: written through reflection, a string in a handle loaded.
        crate::reflected::write(world, e, &path, &prop, &v).map(|_| ())
    } else {
        // The field's layout, without cloning the component's.
        let field = match world.resource::<TsComponents>().live(&key) {
            Some(c) => Some((c.id, c.layout.field(&prop).cloned())),
            None => world.resource::<NativeLayouts>().get(&key).map(|n| (n.id, n.layout.field(&prop).cloned())),
        };
        match field {
            Some((id, Some(fl))) => {
                value::FieldValue::encode(&prop, &fl.ty, &v).map_err(|e| e.to_string()).map(|fv| {
                    let Ok(mut em) = world.get_entity_mut(e) else { return };
                    // A derived value equal to the row's writes nothing: its tick stays.
                    // SAFETY: the field lies inside the component's live value.
                    let same = em.get_by_id(id).is_ok_and(|p| unsafe { fv.same_at(p.as_ptr(), fl.offset) });
                    if !same && let Ok(mut m) = em.get_mut_by_id(id) {
                        // SAFETY: as above.
                        unsafe { fv.write_at(m.as_mut().as_ptr(), fl.offset) };
                    }
                })
            }
            Some((_, None)) => Err(format!("{key} has no field {prop}")),
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

/// One slot's part of a host call: the groups it unmounts and mounts, in order, and its new key
/// order, if JS sent one.
#[derive(Default)]
struct SlotOps {
    unmounts: Vec<String>,
    mounts: Vec<(String, Vec<NodeDesc>)>,
    order: Option<Vec<String>>,
}

fn slot_ops<'a>(slots: &'a mut Vec<(u32, SlotOps)>, at: &mut std::collections::HashMap<u32, usize>, slot: u32) -> &'a mut SlotOps {
    let i = *at.entry(slot).or_insert_with(|| {
        slots.push((slot, SlotOps::default()));
        slots.len() - 1
    });
    &mut slots[i].1
}

/// The node ids under one group's described nodes, its top-level ones, and the slots in it.
fn group_rows(nodes: &[RNode], rows: &mut GroupRows, top: bool) {
    for n in nodes {
        if let Some(s) = n.slot {
            rows.slots.push(s);
            continue;
        }
        rows.nodes.push(n.id);
        if top {
            rows.tops.push(n.id);
        }
        group_rows(&n.children, rows, false);
    }
}

/// The node ids of `rows` and of the slots in it, recursively (taking the inner slots' rows): top
/// nodes in `tops`, the rest in `rest`.
fn collect_rows(tree: &mut TsxTree, rows: GroupRows, tops: &mut Vec<u64>, rest: &mut Vec<u64>) {
    let top: std::collections::HashSet<u64> = rows.tops.iter().copied().collect();
    rest.extend(rows.nodes.iter().filter(|n| !top.contains(n)));
    tops.extend(rows.tops);
    for s in rows.slots {
        let Some(inner) = tree.reactive.slots.get_mut(&s) else { continue };
        let groups: Vec<GroupRows> = std::mem::take(&mut inner.rows).into_values().collect();
        inner.groups.clear();
        for g in groups {
            collect_rows(tree, g, tops, rest);
        }
    }
}

/// Applies one slot's unmounts, mounts and order in one reconcile. A group that fails its check
/// logs its error and is left out; the rest apply.
fn slot_apply(world: &mut World, slot: u32, ops: SlotOps) {
    let Some(state) = world.resource::<TsxTree>().reactive.slots.get(&slot) else { return };
    let (root, parent, prefix, later) = (state.root.clone(), state.parent, state.prefix.clone(), state.later.clone());
    let mut types = state.types.clone();
    types.push("group".into());
    let registry = world.resource::<AppTypeRegistry>().clone();
    let mut resolved = Vec::new();
    let mut mounted = Vec::new();
    for (group, mut nodes) in ops.mounts {
        let mut path = prefix.clone();
        path.push(group.clone());
        normalize_sources(&mut nodes);
        let mut errors = Vec::new();
        resolve::check_nodes(&nodes, world.resource::<TsComponents>(), world.resource::<NativeLayouts>(), &registry.read(), &mut errors);
        if let Some(e) = errors.into_iter().next() {
            log_error(world, e.message, e.location);
            continue;
        }
        match resolve_tree(world, &root, &nodes, (&path, &types)) {
            Ok(r) => {
                let mut rows = GroupRows::default();
                group_rows(&r, &mut rows, true);
                resolved.extend(r);
                mounted.push((group, rows));
            }
            Err(e) => log_error(world, e.message, e.location),
        }
    }
    let (mut drop, mut rest, mut stale) = (Vec::new(), Vec::new(), Vec::new());
    {
        let mut tree = world.resource_mut::<TsxTree>();
        let tree = &mut *tree;
        let Some(state) = tree.reactive.slots.get_mut(&slot) else { return };
        let mut gone = Vec::new();
        for g in &ops.unmounts {
            state.groups.remove(g);
            if let Some(rows) = state.rows.remove(g) {
                gone.push(rows);
            }
        }
        for (g, rows) in &mounted {
            state.groups.insert(g.clone());
            if let Some(old) = state.rows.insert(g.clone(), rows.clone()) {
                stale.extend(old.nodes);
            }
        }
        for rows in gone {
            collect_rows(tree, rows, &mut drop, &mut rest);
        }
    }
    // Top nodes from the last: each leaves its parent's `Children` from the back, which is cheap,
    // and takes its descendants with it, so the rest are only checked.
    drop.reverse();
    drop.append(&mut rest);
    let prune = Prune::Groups { drop, stale };
    let order = ops.order.map(|keys| {
        let rows = &world.resource::<TsxTree>().reactive.slots[&slot].rows;
        keys.iter().filter_map(|k| rows.get(k)).flat_map(|r| r.tops.iter().copied()).collect::<Vec<u64>>()
    });
    let stats = apply_root(world, &root, None, &resolved, parent, prune);
    // New rows were appended to the parent's children: siblings described after the slot go
    // back behind them.
    if stats.spawned > 0
        && let Some(p) = parent
    {
        reconcile::keep_after(world, p, &later);
    }
    if let Some(tops) = order {
        reconcile::order_slot(world, &root, parent, &prefix, &tops);
    }
}

/// One scene component instance for [`expand`].
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandItem {
    /// The scene component's name (`enemies::Grunt`) or `module#export`.
    pub component: String,
    /// Its scene props and fields together, as a JSX call passes them.
    pub props: Map<String, Value>,
    /// Where the instance is placed (a level file), for errors.
    pub at: SourceLocation,
}

fn header(payload: &Payload) -> Vec<u8> {
    let header = serde_json::to_vec(payload).expect("serializable");
    let mut bytes = Vec::with_capacity(4 + header.len());
    bytes.extend((header.len() as u32).to_le_bytes());
    bytes.extend(header);
    bytes
}

/// Each item's description or its error, and the call's other errors.
type Expanded = (Vec<Result<NodeDesc, ScriptError>>, Vec<ScriptError>);

/// One host call expanding every item: each item's description or the error it raised, and the
/// call's other errors (all located in display form).
fn expand_call(world: &mut World, items: &[ExpandItem]) -> Result<Expanded, Vec<ScriptError>> {
    let payload = Payload {
        expand: Some(items.iter().map(|i| (i.component.clone(), i.props.clone(), i.at.clone())).collect()),
        ..Default::default()
    };
    world.resource_mut::<TsxTree>().reactive.calls += 1;
    let out = plugin_api::call_entry(world, ENTRY, &header(&payload)).map_err(|e| vec![e])?;
    let mut response: Response = serde_json::from_slice(&out).map_err(|e| vec![ScriptError::new(format!("tsx expand returned bad output: {e}"))])?;
    let mut expanded = response.expanded.take().unwrap_or_default();
    let mut failures = response.failures.take().unwrap_or_default();
    let errors = std::mem::take(&mut response.errors);
    // Whatever else the call produced (patches, mounts) applies as in any call; mounted slot
    // content starts its bindings in the next frame's call.
    if response.more {
        world.resource_mut::<TsxTree>().reactive.due = true;
    }
    apply(world, response);
    expanded.resize(items.len(), None);
    failures.resize(items.len(), None);
    let display = |mut e: ScriptError| {
        if let Some(l) = &mut e.location {
            l.file = bevy_ts::backend::display_file(&l.file);
        }
        e
    };
    let expanded = expanded
        .into_iter()
        .zip(failures)
        .zip(items)
        .map(|((x, f), item)| match (x, f) {
            (Some(n), _) => Ok(n),
            (None, Some(e)) => Err(display(e)),
            (None, None) => Err(ScriptError::at(format!("{} returned no description", item.component), Some(item.at.clone()))),
        })
        .collect();
    Ok((expanded, errors.into_iter().map(display).collect()))
}

/// Expands scene component instances through their TS `scene`, all in one host call: one
/// description per item, in order (the scene component added to its root, prefab sources
/// included), or every item's errors with their locations. Mount the result with
/// [`crate::reconcile_external`]; bindings in it start in the frame after.
pub fn expand(world: &mut World, items: &[ExpandItem]) -> Result<Vec<NodeDesc>, Vec<ScriptError>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let (expanded, mut errors) = expand_call(world, items)?;
    let mut nodes = Vec::with_capacity(expanded.len());
    for x in expanded {
        match x {
            Ok(n) => nodes.push(n),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    normalize_sources(&mut nodes);
    Ok(nodes)
}

/// One item's expansion: its description, or the error its `scene()` raised.
pub type ItemExpansion = Result<NodeDesc, Vec<ScriptError>>;

/// [`expand`], each item answering for itself: a throwing `scene()` fails its item only, with
/// the error it raised. The call's other errors (a binding a scene set off) are logged.
pub fn expand_each(world: &mut World, items: &[ExpandItem]) -> Result<Vec<ItemExpansion>, Vec<ScriptError>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let (expanded, errors) = expand_call(world, items)?;
    for e in errors {
        log_error(world, e.message, e.location);
    }
    // One `Files` for the call: one per item read the working directory 10,000 times for a
    // 10,000-instance level, three quarters of its compose (levels-scale.md).
    let mut files = Files::new();
    Ok(expanded
        .into_iter()
        .map(|x| {
            let mut n = x.map_err(|e| vec![e])?;
            normalize_with(&mut files, std::slice::from_mut(&mut n));
            Ok(n)
        })
        .collect())
}

/// `tsx/flush`: JS changed a binding outside a host call.
pub fn flush(world: &mut World, _bytes: &[u8]) -> Result<(), String> {
    world.resource_mut::<TsxTree>().reactive.due = true;
    Ok(())
}
