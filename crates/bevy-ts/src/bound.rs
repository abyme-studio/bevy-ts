//! Bound fields (layout-bindable): the fields TSX bindings write, per component and entity, with
//! each binding's site. bevy-ts-tsx publishes them under `bevy-ts dev` only; a TS system that
//! changes one warns, once per system, field and site per generation. A bound field has one
//! writer, its binding.
//!
//! A column has no host call per write, so a TS system's run snapshots the bound columns of the
//! components it writes before its body and compares after. A hole is written through
//! `reflect_codec::write`, which compares the bound holes it touches before and after and records
//! a hit for the running system.

use crate::backend::SourceLocation;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Where a bound field lives in its component.
#[derive(Clone, Debug, PartialEq)]
pub enum BoundKind {
    /// A column: its bytes in the component's value.
    Column { offset: usize, size: usize },
    /// A hole, or a reflect-tier prop: written by reflection, by name.
    Hole,
}

/// One bound field of one entity.
#[derive(Clone, Debug)]
pub struct Bound {
    pub field: String,
    pub kind: BoundKind,
    /// The binding's element, `<Sprite color>`.
    pub site: String,
    pub at: Option<SourceLocation>,
}

/// A component's bound fields, by entity.
#[derive(Clone, Debug, Default)]
pub struct BoundComponent {
    /// The name its TSX element gives it (`Sprite`).
    pub name: String,
    /// Per entity: its label (its `Name`, else its id) and its bound fields.
    pub entities: HashMap<Entity, (String, Vec<Bound>)>,
}

/// A bound field a TS system's run changed: its component, entity and index in the entity's list.
pub type Hit = (ComponentId, Entity, usize);

/// Every bound field, present under `bevy-ts dev` only.
#[derive(Resource, Default)]
pub struct BoundFields {
    /// The generation the bindings belong to: a system warns once per field and site in it.
    pub generation: u64,
    pub components: HashMap<ComponentId, BoundComponent>,
    /// Each component's bound columns, flat: entity, index in its list, offset and size.
    columns: HashMap<ComponentId, Vec<(Entity, usize, usize, usize)>>,
    /// Bound holes the running TS system changed; `None` while no system is checked.
    hits: Mutex<Option<Vec<Hit>>>,
}

impl BoundFields {
    /// Replaces the bound fields; a new generation starts the warnings over.
    pub fn set(&mut self, generation: u64, components: HashMap<ComponentId, BoundComponent>) {
        self.generation = generation;
        self.columns = components
            .iter()
            .map(|(id, c)| {
                let mut flat = Vec::new();
                for (e, (_, fields)) in &c.entities {
                    for (i, b) in fields.iter().enumerate() {
                        if let BoundKind::Column { offset, size } = b.kind {
                            flat.push((*e, i, offset, size));
                        }
                    }
                }
                (*id, flat)
            })
            .collect();
        self.components = components;
    }

    /// The components among `writes` that have bound fields.
    pub fn watched(&self, writes: &[ComponentId]) -> Vec<ComponentId> {
        if self.components.is_empty() {
            return Vec::new();
        }
        writes.iter().filter(|w| self.components.contains_key(w)).copied().collect()
    }

    /// Starts recording hole hits for a system's run.
    pub fn start(&self) {
        *self.hits.lock().unwrap() = Some(Vec::new());
    }

    /// Ends the run's recording: the hole hits it made.
    pub fn finish(&self) -> Vec<Hit> {
        self.hits.lock().unwrap().take().unwrap_or_default()
    }

    /// The bound holes a write of `component`'s `hole` (the whole value when `None`) on `entity`
    /// touches, while a checked system runs.
    pub fn holes(&self, component: ComponentId, entity: Entity, hole: Option<&str>) -> Vec<(usize, &str)> {
        if self.hits.lock().unwrap().is_none() {
            return Vec::new();
        }
        let Some((_, fields)) = self.components.get(&component).and_then(|c| c.entities.get(&entity)) else { return Vec::new() };
        fields
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == BoundKind::Hole && hole.is_none_or(|h| h == b.field))
            .map(|(i, b)| (i, b.field.as_str()))
            .collect()
    }

    /// Records a changed bound hole for the running system.
    pub fn hit(&self, hit: Hit) {
        if let Some(h) = self.hits.lock().unwrap().as_mut() {
            h.push(hit);
        }
    }

    /// The warning for `hit` by `system`, at the binding's site.
    pub fn warning(&self, system: &str, hit: Hit) -> Option<(String, Option<SourceLocation>)> {
        let c = self.components.get(&hit.0)?;
        let (label, fields) = c.entities.get(&hit.1)?;
        let b = fields.get(hit.2)?;
        let at = b.at.as_ref().map_or(String::new(), |l| format!(" ({l})"));
        Some((
            format!("system {system} wrote {}.{} on {label}, which {}{at} binds: a bound field has one writer, its binding", c.name, b.field, b.site),
            b.at.clone(),
        ))
    }
}

/// A TS system's column snapshot: the bytes of each bound column of the components it writes.
#[derive(Default)]
pub struct Snapshot {
    /// The column's hit, its address and size, and where its bytes start. The address holds for
    /// the run: a system's structural changes wait for the next sync point.
    taken: Vec<(Hit, usize, usize, usize)>,
    bytes: Vec<u8>,
}

impl Snapshot {
    /// Copies the bound columns of `watched` before a run.
    ///
    /// # Safety
    /// The caller may read every component in `watched` on every entity.
    pub unsafe fn take(&mut self, world: UnsafeWorldCell, bound: &BoundFields, watched: &[ComponentId]) {
        use bevy::ecs::component::StorageType;
        self.taken.clear();
        self.bytes.clear();
        // SAFETY: tables are read for the columns the caller may read.
        let tables = unsafe { &world.storages().tables };
        for &id in watched {
            let Some(flat) = bound.columns.get(&id) else { continue };
            let table = world.components().get_info(id).is_some_and(|i| i.storage_type() == StorageType::Table);
            let mut last = None;
            for &(e, i, offset, size) in flat {
                // An entity despawned since is skipped.
                let Ok(loc) = world.entities().get_spawned(e) else { continue };
                let p = if table {
                    // One column lookup per table: bound entities mostly share one.
                    let col = match last {
                        Some((t, col)) if t == loc.table_id => col,
                        _ => {
                            let col = tables.get(loc.table_id).and_then(|t| t.get_column(id));
                            last = Some((loc.table_id, col));
                            col
                        }
                    };
                    // SAFETY: a spawned entity's row lies in its table; the caller may read it.
                    let Some(col) = col else { continue };
                    unsafe { col.get_data_unchecked(loc.table_row) }
                } else {
                    // SAFETY: the caller's contract.
                    let Some(p) = world.get_entity(e).ok().and_then(|em| unsafe { em.get_by_id(id) }) else { continue };
                    p
                };
                // SAFETY: the column lies inside the component's live value.
                let at = unsafe { p.as_ptr().add(offset) };
                self.taken.push(((id, e, i), at as usize, size, self.bytes.len()));
                // SAFETY: as above.
                self.bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(at, size) });
            }
        }
    }

    /// The columns whose bytes changed since [`Snapshot::take`].
    ///
    /// # Safety
    /// As [`Snapshot::take`], in the same run: no structural change since.
    pub unsafe fn changed(&self) -> Vec<Hit> {
        let mut out = Vec::new();
        for &(hit, at, size, start) in &self.taken {
            // SAFETY: the caller's contract keeps the address live.
            let now = unsafe { std::slice::from_raw_parts(at as *const u8, size) };
            if now != &self.bytes[start..start + size] {
                out.push(hit);
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.taken.is_empty()
    }
}

/// The warnings a system gave in a generation: each (component, field, site) once.
#[derive(Default)]
pub struct Warned {
    generation: u64,
    seen: HashSet<(ComponentId, String, String)>,
}

impl Warned {
    /// Whether `hit` warns now: false when its field and site warned this generation.
    pub fn first(&mut self, bound: &BoundFields, hit: Hit) -> bool {
        if self.generation != bound.generation {
            self.generation = bound.generation;
            self.seen.clear();
        }
        let Some(b) = bound.components.get(&hit.0).and_then(|c| c.entities.get(&hit.1)).and_then(|(_, f)| f.get(hit.2)) else { return false };
        let site = format!("{}{:?}", b.site, b.at);
        self.seen.insert((hit.0, b.field.clone(), site))
    }
}

/// A hole's value in a component's: a struct's field, a tuple struct's (`value` for a newtype's),
/// else the whole value (an enum component's `value`).
#[cfg(feature = "reflect")]
pub fn slot<'a>(v: &'a dyn bevy::reflect::PartialReflect, name: &str) -> &'a dyn bevy::reflect::PartialReflect {
    use bevy::reflect::ReflectRef;
    match v.reflect_ref() {
        ReflectRef::Struct(s) => s.field(name).unwrap_or(v),
        ReflectRef::TupleStruct(t) => {
            let i = if t.field_len() == 1 && name == "value" { Some(0) } else { name.parse().ok() };
            i.and_then(|i| t.field(i)).unwrap_or(v)
        }
        _ => v,
    }
}
