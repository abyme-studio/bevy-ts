//! TS systems: resolving their params, ordering them, evaluating run conditions in Rust, and
//! running them over zero-copy column views with per-row change ticks.

use crate::backend::{CallContext, ColumnFrame, ParamFrame, ScriptError, ScriptKey, SystemRun, TableFrame};
use crate::layout::{ComponentLayout, FieldType, NativeLayouts};
use crate::conditions::{self, Condition};
use crate::manifest::{ComponentRef, ComponentSource, DataDecl, FilterDecl, GroupDecl, Lifecycle, ParamDecl, ScheduleDecl, Stage, SystemDecl};
use crate::memory::SystemMemory;
use crate::messages::TsPayloads;
use crate::observers::{ObserverQueue, Trigger};
#[cfg(feature = "reflect")]
use crate::reflect_codec::ReflectTargets;
use crate::runtime::{self, ScriptRuntime};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::archetype::{Archetype, ArchetypeId};
use bevy::ecs::storage::{SparseSets, TableId};
use bevy::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

// JS stores ticks as u32 straight into Bevy's tick column.
const _: () = assert!(size_of::<Tick>() == 4 && align_of::<Tick>() == 4);

/// The resource name `Res<Time>` takes; each clock's is `Time<Real>`, `Time<Virtual>`, `Time<Fixed>`.
pub const TIME: &str = "Time";

/// Which of Bevy's clocks a `Res<Time<C>>` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    /// `Time` (`Time<()>`): the virtual clock in `Update`, the fixed one in `FixedUpdate`.
    Generic,
    Real,
    Virtual,
    Fixed,
}

impl Clock {
    pub const ALL: [Clock; 4] = [Clock::Generic, Clock::Real, Clock::Virtual, Clock::Fixed];

    /// The resource name the clock resolves under.
    pub fn resource(self) -> &'static str {
        match self {
            Clock::Generic => TIME,
            Clock::Real => "Time<Real>",
            Clock::Virtual => "Time<Virtual>",
            Clock::Fixed => "Time<Fixed>",
        }
    }

    /// The clock a resource name stands for: `Time`, `Time<C>` or a `bevy_time::time::Time<..>`
    /// path. `None` for a name that is not Time; an error for a context no clock has.
    pub fn of(name: &str) -> Option<Result<Clock, String>> {
        let (head, ctx) = match name.split_once('<') {
            Some((h, rest)) => (h.trim(), Some(rest.strip_suffix('>').unwrap_or(rest).trim())),
            None => (name, None),
        };
        if !matches!(head, "Time" | "bevy_time::Time" | "bevy_time::time::Time" | "bevy::time::Time" | "bevy::prelude::Time") {
            return None;
        }
        let ctx = ctx.map(|c| c.rsplit("::").next().unwrap_or(c));
        Some(match ctx {
            None | Some("()") => Ok(Clock::Generic),
            Some("Real") => Ok(Clock::Real),
            Some("Virtual") => Ok(Clock::Virtual),
            Some("Fixed") => Ok(Clock::Fixed),
            Some(c) => Err(format!("{name}: Time has no clock {c}; use Time, Time<Real>, Time<Virtual> or Time<Fixed>")),
        })
    }
}

/// A clock's view: every clock's fields first (`Res<Time>`'s four at their old offsets), then
/// its own. Bevy's getter names; every `Duration` in seconds.
pub fn time_layout(clock: Clock) -> &'static ComponentLayout {
    static LAYOUTS: std::sync::LazyLock<[ComponentLayout; 4]> = std::sync::LazyLock::new(|| {
        let common: [(&str, FieldType); 7] = [
            ("delta_secs", FieldType::F32),
            ("elapsed_secs", FieldType::F32),
            ("delta_secs_f64", FieldType::F64),
            ("elapsed_secs_f64", FieldType::F64),
            ("elapsed_secs_wrapped_f64", FieldType::F64),
            ("wrap_period", FieldType::F64),
            ("elapsed_secs_wrapped", FieldType::F32),
        ];
        let virt: [(&str, FieldType); 7] = [
            ("is_paused", FieldType::Bool),
            ("was_paused", FieldType::Bool),
            ("relative_speed", FieldType::F32),
            ("effective_speed", FieldType::F32),
            ("relative_speed_f64", FieldType::F64),
            ("effective_speed_f64", FieldType::F64),
            ("max_delta", FieldType::F64),
        ];
        let fixed: [(&str, FieldType); 4] =
            [("overstep_fraction", FieldType::F32), ("timestep", FieldType::F64), ("overstep", FieldType::F64), ("overstep_fraction_f64", FieldType::F64)];
        let layout = |own: &[(&str, FieldType)]| ComponentLayout::c_layout(common.iter().chain(own).map(|(n, t)| (*n, t)));
        [layout(&[]), layout(&[]), layout(&virt), layout(&fixed)]
    });
    &LAYOUTS[clock as usize]
}

/// One scalar of a clock's view, in layout order.
#[derive(Clone, Copy)]
enum TimeValue {
    F32(f32),
    F64(f64),
    Bool(bool),
}

/// Appends a clock's view, read from the world with Bevy's getters, as little-endian words.
fn push_time(world: &World, clock: Clock, out: &mut Vec<u64>) {
    use bevy::time::{Fixed, Real, Virtual};
    use TimeValue::*;
    fn common<T: Default>(t: Option<&Time<T>>) -> [TimeValue; 7] {
        let Some(t) = t else { return [F32(0.0), F32(0.0), F64(0.0), F64(0.0), F64(0.0), F64(0.0), F32(0.0)] };
        [
            F32(t.delta_secs()),
            F32(t.elapsed_secs()),
            F64(t.delta_secs_f64()),
            F64(t.elapsed_secs_f64()),
            F64(t.elapsed_secs_wrapped_f64()),
            F64(t.wrap_period().as_secs_f64()),
            F32(t.elapsed_secs_wrapped()),
        ]
    }
    let mut values = [F32(0.0); 18];
    let n = match clock {
        Clock::Generic => {
            values[..7].copy_from_slice(&common(world.get_resource::<Time>()));
            7
        }
        Clock::Real => {
            values[..7].copy_from_slice(&common(world.get_resource::<Time<Real>>()));
            7
        }
        Clock::Virtual => {
            let t = world.get_resource::<Time<Virtual>>();
            values[..7].copy_from_slice(&common(t));
            if let Some(t) = t {
                values[7..14].copy_from_slice(&[
                    Bool(t.is_paused()),
                    Bool(t.was_paused()),
                    F32(t.relative_speed()),
                    F32(t.effective_speed()),
                    F64(t.relative_speed_f64()),
                    F64(t.effective_speed_f64()),
                    F64(t.max_delta().as_secs_f64()),
                ]);
            }
            14
        }
        Clock::Fixed => {
            let t = world.get_resource::<Time<Fixed>>();
            values[..7].copy_from_slice(&common(t));
            if let Some(t) = t {
                values[7..11].copy_from_slice(&[
                    F32(t.overstep_fraction()),
                    F64(t.timestep().as_secs_f64()),
                    F64(t.overstep().as_secs_f64()),
                    F64(t.overstep_fraction_f64()),
                ]);
            }
            11
        }
    };
    let layout = time_layout(clock);
    debug_assert_eq!(layout.fields.len(), n);
    let mut bytes = [0u8; 128];
    for (f, v) in layout.fields.iter().zip(&values[..n]) {
        let at = &mut bytes[f.offset..];
        match *v {
            F32(x) => at[..4].copy_from_slice(&x.to_le_bytes()),
            F64(x) => at[..8].copy_from_slice(&x.to_le_bytes()),
            Bool(x) => at[0] = x as u8,
        }
    }
    let (words, _) = bytes[..layout.size].as_chunks::<8>();
    out.extend(words.iter().map(|w| u64::from_le_bytes(*w)));
}

/// One data column of a query.
#[derive(Clone, Debug)]
pub struct Access {
    pub name: String,
    pub id: ComponentId,
    pub size: usize,
    pub write: bool,
    pub optional: bool,
    /// `Ref<C>`: JS gets the changed and added ticks to read.
    pub tracked: bool,
    /// A reflect-tier term: its read target ([`crate::reflect_codec::ReflectTargets`]). Its frame
    /// is the component's changed-tick column; values cross through host calls.
    pub reflect: Option<u32>,
    /// A layout-tier term with holes: its frame carries its changed ticks even when read, which
    /// key the holes' cache.
    pub holes: bool,
}

/// A filter term evaluated per row (`Changed`, `Added`, `Spawned`), or inside an `Or` a term
/// constant per table (`With`, `Without`).
#[derive(Clone, Debug)]
enum Term {
    Changed(ComponentId),
    Added(ComponentId),
    Spawned,
    With(ComponentId),
    Without(ComponentId),
    Or(Vec<Term>),
}

impl Term {
    /// Whether it reads anything per row.
    fn per_row(&self) -> bool {
        match self {
            Term::Changed(_) | Term::Added(_) | Term::Spawned => true,
            Term::With(_) | Term::Without(_) => false,
            Term::Or(ts) => ts.iter().any(Term::per_row),
        }
    }

    /// The term for one table, or for one archetype's rows in it (a sparse-set query): tick
    /// slices or sparse sets looked up once, `With`/`Without` decided.
    fn live<'t>(&self, table: &'t bevy::ecs::storage::Table, arch: Option<&Archetype>, sparse: &'t SparseSets) -> Live<'t> {
        let has = |id: ComponentId| arch.map_or_else(|| table.has_column(id), |a| a.contains(id));
        match self {
            Term::Changed(id) => match table.get_changed_ticks_slice_for(*id) {
                Some(t) => Live::Ticks(t),
                None => sparse.get(*id).map_or(Live::Const(false), |s| Live::Sparse(s, false)),
            },
            Term::Added(id) => match table.get_added_ticks_slice_for(*id) {
                Some(t) => Live::Ticks(t),
                None => sparse.get(*id).map_or(Live::Const(false), |s| Live::Sparse(s, true)),
            },
            Term::Spawned => Live::Spawned,
            Term::With(id) => Live::Const(has(*id)),
            Term::Without(id) => Live::Const(!has(*id)),
            Term::Or(ts) => Live::Or(ts.iter().map(|t| t.live(table, arch, sparse)).collect()),
        }
    }

    /// Its components, for the dense check.
    fn ids(&self, out: &mut Vec<ComponentId>) {
        match self {
            Term::Changed(id) | Term::Added(id) | Term::With(id) | Term::Without(id) => out.push(*id),
            Term::Spawned => {}
            Term::Or(ts) => ts.iter().for_each(|t| t.ids(out)),
        }
    }
}

/// A [`Term`] for one table.
enum Live<'t> {
    Ticks(&'t [std::cell::UnsafeCell<Tick>]),
    /// A sparse-set component's ticks by entity: added (`true`) or changed.
    Sparse(&'t bevy::ecs::storage::ComponentSparseSet, bool),
    Const(bool),
    Spawned,
    Or(Vec<Live<'t>>),
}

impl Live<'_> {
    fn holds(&self, row: usize, entity: Entity, world: &World, last_run: Tick, tick: Tick) -> bool {
        match self {
            // SAFETY: exclusive stage; ticks are plain u32 cells.
            Live::Ticks(t) => unsafe { *t[row].get() }.is_newer_than(last_run, tick),
            Live::Sparse(s, added) => {
                let cell = if *added { s.get_added_tick(entity) } else { s.get_changed_tick(entity) };
                // SAFETY: as above.
                cell.is_some_and(|c| unsafe { *c.get() }.is_newer_than(last_run, tick))
            }
            Live::Const(b) => *b,
            Live::Spawned => world.entities().entity_get_spawn_or_despawn_tick(entity).is_some_and(|t| t.is_newer_than(last_run, tick)),
            Live::Or(ls) => ls.iter().any(|l| l.holds(row, entity, world, last_run, tick)),
        }
    }
}

/// What a `Single` or `Populated` param needs to match for the system to run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Need {
    Any,
    One,
    AtMostOne,
    SomeRows,
}

pub struct QueryParam {
    state: QueryState<Entity>,
    /// Data components in order (`Entity` and `Has` data are not columns; `AnyOf` gives one per
    /// part).
    pub columns: Vec<Access>,
    /// Terms every row must pass (all of), each with a per-row part.
    rows: Vec<Term>,
    /// `Has<C>` data, in order: one bit each per table.
    has: Vec<ComponentId>,
    pub need: Need,
    /// Bevy's rule: every filter, `Has` and disabling component is in a table, so a matched
    /// table holds only matching rows. Otherwise the run goes by matched archetype, each a row
    /// list into its table.
    dense: bool,
}

/// A resolved param, flattened (bundles depth-first) in the order frames are handed over.
pub enum RunParam {
    Query(Box<QueryParam>),
    /// `optional`: `Option<Res<R>>`, absent gives no frame instead of failing the run.
    /// `heap`: the resource has a heap field, so even a read-only frame carries its changed tick,
    /// which keys JS's cache of its heap reads.
    Res { name: String, id: ComponentId, size: usize, write: bool, heap: bool, optional: bool },
    /// A reflect-tier Rust resource: its frame is the resource's changed-tick cell.
    ReflectRes { name: String, id: ComponentId, target: u32, write: bool, optional: bool },
    Time(Clock),
    /// The nth `Local` of the system.
    Local(usize),
    MessageReader { index: usize },
    /// `RemovedComponents<C>`: its cursor is kept in `SystemMemory` under `key`.
    Removed { id: ComponentId, key: String },
    /// `SystemChangeTick`.
    ChangeTick,
    /// A Rust message, read per run from the system's cursor: with `reflect`, a count then each
    /// message along `node`, its codec (`reflect_codec`); without, one JSON array.
    RustMessageReader {
        path: String,
        reader: crate::rust_events::ReflectTsMessage,
        #[cfg(feature = "reflect")]
        node: crate::reflect_codec::Node,
    },
    /// `State<S>` (`previous: false`) or `PreviousState<S>`: a machine by index in `TsPayloads`.
    State { index: usize, previous: bool },
    /// An observer's trigger.
    Event,
    /// An exported Rust param: its index in `HostParams`.
    Port(usize),
    None,
}

/// What an observer listens to.
#[derive(Clone, Debug, PartialEq)]
pub struct ObserverOf(pub Trigger);

/// A live TS system or observer.
pub struct TsSystem {
    pub key: ScriptKey,
    pub decl: SystemDecl,
    pub params: Vec<RunParam>,
    pub condition: Option<Condition>,
    pub observes: Option<ObserverOf>,
    pub last_run_us: u64,
    pub last_error: Option<ScriptError>,
    warned_read_write: bool,
}

/// The live generation's systems, by schedule, in run order.
#[derive(Resource, Default)]
pub struct TsSystems {
    pub stages: BTreeMap<Stage, Vec<TsSystem>>,
    /// Custom schedules' systems, by manifest name (`game::Waves`, `net:tick`).
    pub schedules: BTreeMap<String, Vec<TsSystem>>,
    /// Systems placed before or after an exposed Rust set, by slot.
    pub slots: BTreeMap<SlotKey, Vec<TsSystem>>,
    pub pre_startup: Vec<TsSystem>,
    pub startup: Vec<TsSystem>,
    pub post_startup: Vec<TsSystem>,
    /// `OnEnter`/`OnExit` lists by (state slot, value symbol), run from Bevy's schedules.
    pub on_enter: HashMap<(usize, u32), Vec<TsSystem>>,
    pub on_exit: HashMap<(usize, u32), Vec<TsSystem>>,
    /// `OnTransition` lists by (state slot, exited, entered).
    pub on_transition: HashMap<(usize, u32, u32), Vec<TsSystem>>,
    pub observers: Vec<TsSystem>,
    /// One-shot systems by identity; `None` while one runs.
    pub one_shot: BTreeMap<String, Option<TsSystem>>,
    /// Collective `runIf` conditions, by group id.
    pub groups: HashMap<String, Condition>,
    /// Native condition instances the generation uses.
    pub natives: HashSet<String>,
    /// Observers attached to entities by `commands.entity(e).observe(fn)`, by declaration.
    pub targeted: BTreeMap<String, TsSystem>,
    pub generation: u64,
}

/// Where a list of systems lives in [`TsSystems`].
#[derive(Clone, Debug)]
pub(crate) enum ListKey {
    Stage(Stage),
    Schedule(String),
    Slot(SlotKey),
    PreStartup,
    Startup,
    PostStartup,
    OnEnter(usize, u32),
    OnTransition(usize, u32, u32),
    OnExit(usize, u32),
}

/// A slot: the TS systems running just before or just after an exposed Rust set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotKey {
    /// The exposed set's name (`rapier:step`).
    pub set: String,
    pub after: bool,
}

impl TsSystems {
    pub fn iter(&self) -> impl Iterator<Item = &TsSystem> {
        self.pre_startup
            .iter()
            .chain(self.startup.iter())
            .chain(self.post_startup.iter())
            .chain(self.stages.values().flatten())
            .chain(self.schedules.values().flatten())
            .chain(self.one_shot.values().flatten())
            .chain(self.slots.values().flatten())
            .chain(self.on_enter.values().flatten())
            .chain(self.on_exit.values().flatten())
            .chain(self.on_transition.values().flatten())
            .chain(self.observers.iter())
            .chain(self.targeted.values())
    }

    fn take(&mut self, k: &ListKey) -> Vec<TsSystem> {
        match k {
            ListKey::Stage(s) => self.stages.remove(s),
            ListKey::Schedule(n) => self.schedules.remove(n),
            ListKey::Slot(s) => self.slots.remove(s),
            ListKey::PreStartup => Some(std::mem::take(&mut self.pre_startup)),
            ListKey::Startup => Some(std::mem::take(&mut self.startup)),
            ListKey::PostStartup => Some(std::mem::take(&mut self.post_startup)),
            ListKey::OnEnter(i, v) => self.on_enter.remove(&(*i, *v)),
            ListKey::OnExit(i, v) => self.on_exit.remove(&(*i, *v)),
            ListKey::OnTransition(i, a, b) => self.on_transition.remove(&(*i, *a, *b)),
        }
        .unwrap_or_default()
    }

    fn put(&mut self, k: &ListKey, list: Vec<TsSystem>) {
        if list.is_empty() && !matches!(k, ListKey::PreStartup | ListKey::Startup | ListKey::PostStartup) {
            return;
        }
        match k {
            ListKey::Stage(s) => drop(self.stages.insert(*s, list)),
            ListKey::Schedule(n) => drop(self.schedules.insert(n.clone(), list)),
            ListKey::Slot(s) => drop(self.slots.insert(s.clone(), list)),
            ListKey::PreStartup => self.pre_startup = list,
            ListKey::Startup => self.startup = list,
            ListKey::PostStartup => self.post_startup = list,
            ListKey::OnEnter(i, v) => drop(self.on_enter.insert((*i, *v), list)),
            ListKey::OnExit(i, v) => drop(self.on_exit.insert((*i, *v), list)),
            ListKey::OnTransition(i, a, b) => drop(self.on_transition.insert((*i, *a, *b), list)),
        }
    }

    /// The Rust events the generation's observers name, for Bevy observers to feed.
    pub fn watched_rust(&self) -> Vec<std::any::TypeId> {
        let mut ids: Vec<std::any::TypeId> = self
            .observers
            .iter()
            .chain(self.targeted.values())
            .filter_map(|o| match &o.observes {
                Some(ObserverOf(Trigger::Rust(id))) => Some(*id),
                _ => None,
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// The observers a lifecycle trigger needs Bevy to watch.
    pub fn watched_lifecycles(&self) -> Vec<(Lifecycle, ComponentId)> {
        self.observers
            .iter()
            .chain(self.targeted.values())
            .filter_map(|o| match &o.observes {
                Some(ObserverOf(Trigger::Lifecycle(k, id))) => Some((*k, *id)),
                _ => None,
            })
            .collect()
    }
}

/// Resolves a TS component name to its id, its size and whether it has a heap field, in the
/// generation being applied.
pub type TsLookup<'a> = dyn Fn(&str) -> Option<(ComponentId, usize, bool)> + 'a;

/// Encodes field values as a TS component or resource of the generation being applied: its
/// value (owning its heap fields) and layout, or `None` when the name is no TS type.
pub type TsEncode<'a> = dyn Fn(&str, &serde_json::Map<String, serde_json::Value>) -> Option<Result<(crate::heap::HeapBytes, ComponentLayout), String>> + 'a;

/// Finds a registered Rust component by type path, or by its short name when only one type has
/// it: `None` when none has it, and an error naming their paths when several share it.
pub fn rust_component_by_name(world: &World, name: &str) -> Result<Option<ComponentId>, String> {
    let mut short = Vec::new();
    for (id, info) in world.components().iter_registered() {
        let full = info.name().to_string();
        if full == name {
            return Ok(Some(id));
        }
        if full.rsplit("::").next() == Some(name) {
            short.push((full, id));
        }
    }
    match short.as_mut_slice() {
        [] => Ok(None),
        [(_, id)] => Ok(Some(*id)),
        many => {
            many.sort();
            let paths: Vec<&str> = many.iter().map(|(p, _)| p.as_str()).collect();
            Err(format!("{name} names more than one Rust type ({}); use its type path", paths.join(", ")))
        }
    }
}

/// Whether a component is stored in a sparse set (it then splits no tables).
fn is_sparse(world: &World, id: ComponentId) -> bool {
    world.components().get_info(id).is_some_and(|i| i.storage_type() == bevy::ecs::component::StorageType::SparseSet)
}

/// Resolves a component reference as query data (TS, or native layout) or as a filter (any
/// registered component).
fn resolve_ref(world: &World, r: &ComponentRef, ts: &TsLookup, native: &NativeLayouts, data: bool) -> Result<(ComponentId, usize), String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).map(|(id, size, _)| (id, size)).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => {
            // A sparse-set column is not in table order, so it cannot be a zero-copy column.
            // Checked after the resource check: a resource is a sparse-set component too.
            let sparse = || format!("`{}` is a sparse-set component; it can be a filter but not query data", r.name);
            // A short name is looked up among every registered component, not only the tiered
            // ones, so a tiered and an untiered type that share it are refused too.
            let id = match native.iter().find(|n| n.type_path == r.name) {
                Some(n) => Some(n.id),
                None => rust_component_by_name(world, &r.name)?.or_else(|| native.get(&r.name).map(|n| n.id)),
            };
            let id = id.ok_or_else(|| format!("no Rust component named {} is registered in this engine", r.name))?;
            if let Some(n) = native.by_id(id) {
                if n.resource {
                    return Err(format!("{} is a resource, not a component; take Res<{}>", r.name, r.name));
                }
                if data && is_sparse(world, n.id) {
                    return Err(sparse());
                }
                return Ok((n.id, n.layout.size));
            }
            if data && is_sparse(world, id) {
                return Err(sparse());
            }
            if data {
                let why = native.reflect_tier(&r.name).map(|t| format!(" ({})", t.reason)).unwrap_or_default();
                return Err(format!(
                    "{} is a Rust component without a native layout{why}; it can be used in With/Without but not as query data",
                    r.name
                ));
            }
            Ok((id, 0))
        }
    }
}

/// Resolves a `Res`/`ResMut` name other than `Time`: a TS resource first, then a layout-tier
/// Rust resource by type path or by short name when only one type has it.
/// A resource's id, size and whether it has a heap field.
fn resolve_res(world: &mut World, name: &str, write: bool, ts: &TsLookup) -> Result<(ComponentId, usize, bool), String> {
    if let Some(hit) = ts(name) {
        return Ok(hit);
    }
    crate::layout_reflect::retry(world, name);
    // A short name is looked up among every registered type, not only the tiered ones, so a
    // tiered and an untiered resource that share it are refused too.
    rust_component_by_name(world, name)?;
    let native = world.resource::<NativeLayouts>();
    if let Some(t) = native.reflect_tier(name) {
        return Err(if t.resource {
            format!("{name} is a Rust resource without a native layout ({})", t.reason)
        } else {
            format!("{name} is a component, not a resource")
        });
    }
    if let Some(n) = native.get(name) {
        if !n.resource {
            return Err(format!("{name} is a component, not a resource"));
        }
        if write && !n.writable {
            return Err(format!("{name} is immutable; take Res<{name}>"));
        }
        return Ok((n.id, n.layout.size, false));
    }
    if let Some(paths) = native.ambiguous(name) {
        return Err(format!("{name} names more than one Rust type ({}); use its type path", paths.join(", ")));
    }
    Err(format!("no resource named {name}"))
}

/// A reflect-tier Rust component named as query data: its id and its read target. `None` for
/// any other reference (TS, layout tier, unknown), which [`resolve_ref`] resolves or names.
#[cfg(feature = "reflect")]
fn reflect_term(world: &mut World, r: &ComponentRef) -> Result<Option<(ComponentId, u32)>, String> {
    if r.source != ComponentSource::Rust {
        return Ok(None);
    }
    let native = world.resource::<NativeLayouts>();
    if native.get(&r.name).is_some() {
        return Ok(None);
    }
    let Some(id) = native.reflect_tier(&r.name).map(|t| t.id) else { return Ok(None) };
    let info = world.components().get_info(id).ok_or_else(|| format!("{} is not registered", r.name))?;
    if info.storage_type() != bevy::ecs::component::StorageType::Table {
        return Err(format!("{} is stored in a sparse set; TS reads only table components as query data", r.name));
    }
    let type_id = info.type_id().ok_or_else(|| format!("{} has no Rust type", r.name))?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    if registry.get_type_data::<bevy::ecs::reflect::ReflectResource>(type_id).is_some() {
        return Err(format!("{} is a resource, not a component; take Res<{}>", r.name, r.name));
    }
    let target = world.get_resource_or_init::<ReflectTargets>().component(&registry, id, type_id).map_err(|e| format!("{}: {e}", r.name))?;
    Ok(Some((id, target)))
}

/// A reflect-tier Rust resource named by `Res`/`ResMut`: its id and read target, or `None` when
/// the name is a TS resource, has a layout, or is no reflected resource.
#[cfg(feature = "reflect")]
fn reflect_resource(world: &mut World, name: &str, ts: &TsLookup) -> Result<Option<(ComponentId, u32)>, String> {
    if ts(name).is_some() {
        return Ok(None);
    }
    crate::layout_reflect::retry(world, name);
    let native = world.resource::<NativeLayouts>();
    if native.get(name).is_some() {
        return Ok(None);
    }
    let Some(id) = native.reflect_tier(name).map(|t| t.id) else { return Ok(None) };
    let Some(type_id) = world.components().get_info(id).and_then(|i| i.type_id()) else { return Ok(None) };
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    if registry.get_type_data::<bevy::ecs::reflect::ReflectResource>(type_id).is_none() {
        return Err(format!("{name} is a component, not a resource"));
    }
    let target = world.get_resource_or_init::<ReflectTargets>().component(&registry, id, type_id).map_err(|e| format!("{name}: {e}"))?;
    Ok(Some((id, target)))
}

/// Without the `reflect` feature reflect-tier terms stay filters only.
#[cfg(not(feature = "reflect"))]
fn reflect_term(_: &mut World, _: &ComponentRef) -> Result<Option<(ComponentId, u32)>, String> {
    Ok(None)
}

#[cfg(not(feature = "reflect"))]
fn reflect_resource(_: &mut World, _: &str, _: &TsLookup) -> Result<Option<(ComponentId, u32)>, String> {
    Ok(None)
}

/// The changed-tick cell of `id` on entity `e`, in a table or a sparse set.
fn changed_cell(world: &World, id: ComponentId, e: Entity) -> Option<*mut u32> {
    if let Some(set) = world.storages().sparse_sets.get(id) {
        let (_, cells) = set.get_with_ticks(e)?;
        return Some(cells.changed.get() as *mut u32);
    }
    let loc = world.entities().get_spawned(e).ok()?;
    let table = world.storages().tables.get(loc.table_id)?;
    let ticks = table.get_changed_ticks_slice_for(id)?;
    Some(ticks.get(loc.table_row.index())?.get() as *mut u32)
}

/// Orders systems by registration order adjusted by `after`/`before` (on systems or sets). Errors
/// on unknown targets and cycles.
fn order(decls: &[(usize, &SystemDecl)], exposed: &HashSet<String>) -> Result<Vec<usize>, ScriptError> {
    let n = decls.len();
    let mut members: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (_, d)) in decls.iter().enumerate() {
        members.entry(d.name.as_str()).or_default().push(i);
        for s in &d.sets {
            members.entry(s.as_str()).or_default().push(i);
        }
    }
    let mut indegree = vec![0; n];
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut edge = |from: usize, to: usize, indegree: &mut Vec<usize>| {
        if from != to && edges[from].insert(to) {
            indegree[to] += 1;
        }
    };
    for (i, (_, d)) in decls.iter().enumerate() {
        for (targets, after) in [(&d.after, true), (&d.before, false)] {
            for t in targets {
                if exposed.contains(t) {
                    // An exposed Rust set places the system in a slot; it orders nothing inside it.
                    continue;
                }
                let Some(js) = members.get(t.as_str()) else {
                    return Err(ScriptError::at(
                        format!("system {} is ordered {} {t}, which is no system or set in its schedule", d.name, if after { "after" } else { "before" }),
                        d.location.clone(),
                    ));
                };
                for &j in js {
                    if after { edge(j, i, &mut indegree) } else { edge(i, j, &mut indegree) }
                }
            }
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut ready: std::collections::BTreeSet<usize> = (0..n).filter(|i| indegree[*i] == 0).collect();
    while let Some(i) = ready.pop_first() {
        out.push(i);
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                ready.insert(j);
            }
        }
    }
    if out.len() < n {
        let stuck: Vec<&str> = (0..n).filter(|i| indegree[*i] > 0).map(|i| decls[i].1.name.as_str()).collect();
        let d = decls.iter().find(|(_, d)| d.name == stuck[0]).unwrap().1;
        return Err(ScriptError::at(format!("cycle in system ordering among {}", stuck.join(", ")), d.location.clone()));
    }
    Ok(out.into_iter().map(|i| decls[i].0).collect())
}

/// A schedule's systems, with their indices in the manifest.
type ScheduleGroup<'a> = (Placement, Vec<(usize, &'a SystemDecl)>);

/// Where a group of systems runs: a declared schedule, or a slot around an exposed set.
#[derive(Clone, Debug)]
enum Placement {
    Schedule(ScheduleDecl),
    Slot(SlotKey),
}

/// Builds the systems of a generation. Fails on unusable params, duplicate identities and bad
/// ordering. `TsPayloads` must already hold the generation's messages, events and states.
///
/// `targeted` are observers attached to entities in an earlier generation; their keys follow
/// `decls`'.
pub fn build(world: &mut World, generation: u64, decls: &[SystemDecl], targeted: &[SystemDecl], groups: &[GroupDecl], ts: &TsLookup, encode: &TsEncode) -> Result<TsSystems, Vec<ScriptError>> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for d in decls {
        if d.schedule != ScheduleDecl::Observer && !seen.insert((d.name.as_str(), format!("{:?}", d.schedule))) {
            errors.push(ScriptError::at(format!("system {} is registered twice in one schedule", d.name), d.location.clone()));
        }
    }
    let mut out = TsSystems { generation, ..Default::default() };
    for g in groups {
        let mut c = conditions::Compile { ts, encode, config: &g.id, natives: &mut out.natives };
        match conditions::compile(world, &g.condition, "", &mut c) {
            Ok(cond) => drop(out.groups.insert(g.id.clone(), cond)),
            Err(e) => {
                let at = e.location.or_else(|| decls.iter().find(|d| d.groups.contains(&g.id)).and_then(|d| d.location.clone()));
                errors.push(ScriptError::at(format!("runIf of the group starting at {}: {}", g.id.split('@').next().unwrap_or(&g.id), e.message), at));
            }
        }
    }
    let mut lists: BTreeMap<String, ScheduleGroup> = BTreeMap::new();
    let exposed: HashSet<String> = world.get_resource::<crate::sets::TsSets>().map(|s| s.iter().map(|x| x.name.clone()).collect()).unwrap_or_default();
    for (i, d) in decls.iter().enumerate() {
        if matches!(d.schedule, ScheduleDecl::Observer | ScheduleDecl::OneShot) {
            match build_one(world, generation, i, d, ts, encode, &mut out.natives) {
                Ok(s) if d.schedule == ScheduleDecl::Observer => out.observers.push(s),
                Ok(s) => drop(out.one_shot.insert(d.name.clone(), Some(s))),
                Err(e) => errors.push(e),
            }
            continue;
        }
        match crate::sets::placement(world, d) {
            Ok(Some(key)) => {
                lists.entry(format!("{key:?}")).or_insert_with(|| (Placement::Slot(key), Vec::new())).1.push((i, d));
            }
            Ok(None) => lists.entry(format!("{:?}", d.schedule)).or_insert_with(|| (Placement::Schedule(d.schedule.clone()), Vec::new())).1.push((i, d)),
            Err(e) => errors.push(e),
        }
    }
    for (_, (placement, list)) in lists {
        let ordered = match order(&list, &exposed) {
            Ok(o) => o,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        let mut built = Vec::new();
        for index in ordered {
            match build_one(world, generation, index, &decls[index], ts, encode, &mut out.natives) {
                Ok(s) => built.push(s),
                Err(e) => errors.push(e),
            }
        }
        let payloads = world.resource::<TsPayloads>();
        // A state value as (slot, symbol): the key Bevy's schedules run TS lists by.
        let state_key = |state: &str, value: &str| -> Result<(usize, u32), String> {
            let m = &payloads.states[payloads.state(state).ok_or_else(|| format!("no state {state}"))?];
            Ok((m.slot as usize, m.sym(value).ok_or_else(|| format!("{state} has no value {value}"))?))
        };
        let schedule = match placement {
            Placement::Slot(key) => {
                out.slots.insert(key, built);
                continue;
            }
            Placement::Schedule(s) => s,
        };
        match &schedule {
            ScheduleDecl::PreStartup => out.pre_startup = built,
            ScheduleDecl::Startup => out.startup = built,
            ScheduleDecl::PostStartup => out.post_startup = built,
            ScheduleDecl::Stage { stage } => drop(out.stages.insert(*stage, built)),
            ScheduleDecl::Schedule { label } => drop(out.schedules.insert(label.clone(), built)),
            ScheduleDecl::OnEnter { state, value } | ScheduleDecl::OnExit { state, value } => match state_key(state, value) {
                Ok(k) => {
                    let map = if matches!(schedule, ScheduleDecl::OnEnter { .. }) { &mut out.on_enter } else { &mut out.on_exit };
                    map.insert(k, built);
                }
                Err(e) => errors.push(ScriptError::new(e)),
            },
            ScheduleDecl::OnTransition { state, exited, entered } => match (state_key(state, exited), state_key(state, entered)) {
                (Ok((slot, a)), Ok((_, b))) => drop(out.on_transition.insert((slot, a, b), built)),
                (Err(e), _) | (_, Err(e)) => errors.push(ScriptError::new(e)),
            },
            ScheduleDecl::Observer | ScheduleDecl::OneShot => unreachable!(),
        }
    }
    for (i, d) in targeted.iter().enumerate() {
        match build_targeted(world, generation, decls.len() + i, d, ts, encode, &mut out.natives) {
            Ok(s) => {
                out.targeted.insert(d.name.clone(), s);
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

/// An observer for `commands.entity(e).observe(fn)`: its first param must be an entity event.
fn build_targeted(world: &mut World, generation: u64, index: usize, d: &SystemDecl, ts: &TsLookup, encode: &TsEncode, natives: &mut HashSet<String>) -> Result<TsSystem, ScriptError> {
    let s = build_one(world, generation, index, d, ts, encode, natives)?;
    let entity_event = match &s.observes {
        Some(ObserverOf(Trigger::Event(i))) => world.resource::<TsPayloads>().events[*i].entity,
        Some(ObserverOf(Trigger::Lifecycle(..))) => true,
        // An event with only Bevy's type data may be either; its observer reads an `entity` field.
        Some(ObserverOf(Trigger::Rust(id))) => {
            world.resource::<AppTypeRegistry>().read().get(*id).and_then(|r| r.data::<crate::rust_events::ReflectTsEvent>()).is_none_or(|t| t.entity)
        }
        None => false,
    };
    if !entity_event {
        return Err(ScriptError::at(format!("observe({}): its first param must be On<E> of an EntityEvent", d.name), d.location.clone()));
    }
    Ok(s)
}

/// Builds the observers `observe` attached since the last drain, once per declaration, and
/// watches any lifecycle they name.
fn adopt_targeted(world: &mut World, rt: &mut ScriptRuntime, systems: &mut TsSystems) {
    let Some(new) = world.get_resource_mut::<crate::observers::NewTargeted>().map(|mut n| std::mem::take(&mut n.0)) else { return };
    if new.is_empty() {
        return;
    }
    let before = (systems.watched_lifecycles(), systems.watched_rust());
    for (index, decl) in new {
        if systems.targeted.contains_key(&decl.name) {
            continue;
        }
        let generation = systems.generation;
        let built = world.resource_scope(|world, ts: Mut<crate::components::TsComponents>| {
            let lookup = |name: &str| ts.live(name).map(|c| (c.id, c.layout.size, c.layout.has_heap()));
            let encode = |name: &str, values: &serde_json::Map<String, serde_json::Value>| {
                let c = ts.live(name)?;
                Some(ts.encode(name, values).map_err(|e| e.to_string()).map(|b| (b, c.layout.clone())))
            };
            build_targeted(world, generation, index as usize, &decl, &lookup, &encode, &mut systems.natives)
        });
        match built {
            Ok(s) => {
                systems.targeted.insert(decl.name.clone(), s);
            }
            Err(e) => {
                rt.log_error(world, &format!("observe {}", decl.name), &e);
                crate::observers::detach(world, |n| n == decl.name);
            }
        }
    }
    let watched = systems.watched_lifecycles();
    if watched != before.0 {
        crate::observers::install(world, &watched);
    }
    let rust = systems.watched_rust();
    if rust != before.1 {
        crate::observers::install_rust(world, &rust);
    }
}

fn build_query(world: &mut World, data: &[DataDecl], filter: &[FilterDecl], need: Need, ts: &TsLookup) -> Result<QueryParam, String> {
    // A Rust type that had no value to read offsets from at startup is tiered from its first row.
    for (component, _) in data.iter().flat_map(DataDecl::components) {
        if component.source == ComponentSource::Rust {
            crate::layout_reflect::retry(world, &component.name);
        }
    }
    let mut columns = Vec::new();
    let mut has = Vec::new();
    // Each `AnyOf`: its parts' ids, matched as an `or` of `with`s.
    let mut any_of: Vec<Vec<ComponentId>> = Vec::new();
    let native = world.resource::<NativeLayouts>().clone();
    let column = |world: &mut World, d: &DataDecl, optional_part: bool, columns: &mut Vec<Access>| -> Result<Option<ComponentId>, String> {
        let DataDecl::Component { component, mutable, optional, tracked } = d else {
            return Err("AnyOf takes components, Mut<C> or Ref<C>".into());
        };
        let optional = *optional || optional_part;
        if let Some((id, target)) = reflect_term(world, component)? {
            columns.push(Access { name: component.name.clone(), id, size: 0, write: *mutable, optional, tracked: *tracked, reflect: Some(target), holes: false });
            return Ok(Some(id));
        }
        let (id, size) = resolve_ref(world, component, ts, &native, true)?;
        if *mutable && native.by_id(id).is_some_and(|n| !n.writable) {
            return Err(format!("{} is an immutable Rust component; TS can read it but not take Mut<{}>", component.name, component.name));
        }
        // A TS component with a heap field reads it as a hole too: only it registers a drop fn.
        let holes = match native.by_id(id) {
            Some(n) => !n.holes.is_empty(),
            None => world.components().get_info(id).is_some_and(|i| i.drop().is_some()),
        };
        columns.push(Access { name: component.name.clone(), id, size, write: *mutable, optional, tracked: *tracked, reflect: None, holes });
        Ok(Some(id))
    };
    for d in data {
        match d {
            DataDecl::Entity { .. } => {}
            DataDecl::Component { .. } => {
                column(world, d, false, &mut columns)?;
            }
            DataDecl::Has { has: r } => {
                has.push(resolve_ref(world, r, ts, &native, false)?.0);
            }
            DataDecl::AnyOf { any_of: parts } => {
                if parts.is_empty() {
                    return Err("AnyOf needs at least one component".into());
                }
                let mut ids = Vec::new();
                for p in parts {
                    ids.extend(column(world, p, true, &mut columns)?);
                }
                any_of.push(ids);
            }
        }
    }
    if has.len() > 32 {
        return Err("a query takes at most 32 Has<C>".into());
    }
    let mut rows = Vec::new();
    let mut archetypal = Vec::new();
    for f in filter {
        let t = term(world, f, ts, &native)?;
        if t.per_row() {
            rows.push(t.clone());
        }
        archetypal.push(t);
    }
    let mut ids: Vec<ComponentId> = columns.iter().map(|c| c.id).chain(has.iter().copied()).collect();
    archetypal.iter().for_each(|t| t.ids(&mut ids));
    if let Some(d) = world.get_resource::<bevy::ecs::entity_disabling::DefaultQueryFilters>() {
        ids.extend(d.disabling_ids());
    }
    let dense = !ids.iter().any(|id| is_sparse(world, *id));
    let mut b = QueryBuilder::<Entity>::new(world);
    for c in &columns {
        match (c.optional, c.write) {
            (false, true) => {
                b.mut_id(c.id);
            }
            (false, false) => {
                b.ref_id(c.id);
            }
            (true, write) => {
                let id = c.id;
                b.optional(|b| {
                    if write {
                        b.mut_id(id);
                    } else {
                        b.ref_id(id);
                    }
                });
            }
        }
    }
    // `Has<C>` names `C` without access, so a disabling `C` (`Has<Disabled>`) lifts the default
    // filter, as Bevy's `Has` does.
    for id in &has {
        let mut access = bevy::ecs::query::FilteredAccess::default();
        access.access_mut().add_archetypal(*id);
        b.extend_access(access);
    }
    for ids in &any_of {
        b.or(|b| {
            for id in ids {
                b.with_id(*id);
            }
        });
    }
    for t in &archetypal {
        add_archetypal(&mut b, t);
    }
    b.without::<crate::ScriptHidden>();
    Ok(QueryParam { state: b.build(), columns, rows, has, need, dense })
}

/// A filter as a [`Term`].
fn term(world: &World, f: &FilterDecl, ts: &TsLookup, native: &NativeLayouts) -> Result<Term, String> {
    let id = |r: &ComponentRef| -> Result<ComponentId, String> { Ok(resolve_ref(world, r, ts, native, false)?.0) };
    Ok(match f {
        FilterDecl::With(r) => Term::With(id(r)?),
        FilterDecl::Without(r) => Term::Without(id(r)?),
        FilterDecl::Changed(r) => Term::Changed(id(r)?),
        FilterDecl::Added(r) => Term::Added(id(r)?),
        FilterDecl::Spawned => Term::Spawned,
        FilterDecl::Or(fs) => {
            if fs.is_empty() {
                return Err("Or needs at least one filter".into());
            }
            Term::Or(fs.iter().map(|f| term(world, f, ts, native)).collect::<Result<_, _>>()?)
        }
    })
}

/// Whether a term matches every archetype (so adds nothing to the builder).
fn matches_all(t: &Term) -> bool {
    match t {
        Term::Spawned => true,
        Term::Or(ts) => ts.iter().any(matches_all),
        _ => false,
    }
}

/// Adds a term's archetypal part: `Changed`/`Added` need the component, as Bevy's do.
fn add_archetypal<D: bevy::ecs::query::QueryData>(b: &mut QueryBuilder<D>, t: &Term) {
    match t {
        Term::With(id) | Term::Changed(id) | Term::Added(id) => {
            b.with_id(*id);
        }
        Term::Without(id) => {
            b.without_id(*id);
        }
        Term::Spawned => {}
        Term::Or(ts) => {
            if matches_all(t) {
                return;
            }
            b.or(|b| {
                for t in ts {
                    b.and(|b| add_archetypal(b, t));
                }
            });
        }
    }
}

fn flatten_params<'a>(params: &'a [ParamDecl], out: &mut Vec<&'a ParamDecl>) {
    for p in params {
        match p {
            ParamDecl::Bundle { fields } => {
                for f in fields {
                    flatten_params(std::slice::from_ref(&f.param), out);
                }
            }
            p => out.push(p),
        }
    }
}

fn build_one(world: &mut World, generation: u64, index: usize, d: &SystemDecl, ts: &TsLookup, encode: &TsEncode, natives: &mut HashSet<String>) -> Result<TsSystem, ScriptError> {
    let err = |m: String| ScriptError::at(format!("system {}: {m}", d.name), d.location.clone());
    let mut flat = Vec::new();
    flatten_params(&d.params, &mut flat);
    let mut params = Vec::new();
    let mut locals = Vec::new();
    let mut observes = None;
    // Rust readers new to this system start at their queue's end, so a restart replays nothing.
    let mut rust_cursors: Vec<(String, u64)> = Vec::new();
    for (i, p) in flat.iter().enumerate() {
        let rp = match p {
            ParamDecl::Query { data, filter } => RunParam::Query(Box::new(build_query(world, data, filter, Need::Any, ts).map_err(err)?)),
            ParamDecl::Single { data, filter, optional } => {
                let need = if *optional { Need::AtMostOne } else { Need::One };
                RunParam::Query(Box::new(build_query(world, data, filter, need, ts).map_err(err)?))
            }
            ParamDecl::Populated { data, filter } => RunParam::Query(Box::new(build_query(world, data, filter, Need::SomeRows, ts).map_err(err)?)),
            ParamDecl::RemovedComponents { component } => {
                let native = world.resource::<NativeLayouts>().clone();
                let id = resolve_ref(world, component, ts, &native, false).map_err(err)?.0;
                RunParam::Removed { id, key: format!("removed:{}", component.name) }
            }
            ParamDecl::SystemChangeTick => RunParam::ChangeTick,
            ParamDecl::Res { resource, optional, reflect } | ParamDecl::ResMut { resource, optional, reflect } => {
                let write = matches!(p, ParamDecl::ResMut { .. });
                let kind = if write { "ResMut" } else { "Res" };
                if let Some(clock) = Clock::of(resource) {
                    let clock = clock.map_err(err)?;
                    if write {
                        return Err(err(format!("ResMut<{resource}> is not allowed; clocks are read-only from TS")));
                    }
                    RunParam::Time(clock)
                } else if let Some((id, target)) = if *reflect { reflect_resource(world, resource, ts).map_err(err)? } else { None } {
                    RunParam::ReflectRes { name: resource.clone(), id, target, write, optional: *optional }
                } else {
                    // A spelling an export claims (`ResMut<Time<Virtual>>`) never gets here: the
                    // reader reads it as that export's port param, and refuses it under `Option`.
                    let (id, size, heap) = resolve_res(world, resource, write, ts).map_err(|m| err(format!("{kind}<{resource}>: {m}")))?;
                    RunParam::Res { name: resource.clone(), id, size, write, heap, optional: *optional }
                }
            }
            ParamDecl::Local { fields } => {
                locals.push(fields.clone());
                RunParam::Local(locals.len() - 1)
            }
            ParamDecl::MessageWriter { message } => {
                if world.resource::<TsPayloads>().message(message).is_none() {
                    let registry = world.resource::<AppTypeRegistry>().read();
                    let known = registry.get_with_type_path(message).is_some_and(|r| {
                        r.data::<crate::rust_events::ReflectTsMessage>().is_some() || r.data::<bevy::ecs::reflect::ReflectMessage>().is_some()
                    });
                    if !known {
                        return Err(err(format!("no message type {message}; call app.addMessage")));
                    }
                }
                RunParam::None
            }
            ParamDecl::Commands | ParamDecl::NextState { .. } => RunParam::None,
            ParamDecl::Port { name } => match world.get_resource::<crate::host::HostParams>().and_then(|h| h.index(name)) {
                Some(index) => RunParam::Port(index),
                None => return Err(err(format!("{name} is not a system param any loaded crate exports"))),
            },
            ParamDecl::MessageReader { message } => match world.resource::<TsPayloads>().message(message) {
                Some(index) => RunParam::MessageReader { index },
                None if message.contains("::") => {
                    let registry = world.resource::<AppTypeRegistry>().read();
                    let (_id, reader) = crate::rust_events::reader(&registry, message).map_err(|e| err(format!("MessageReader<{message}>: {e}")))?;
                    #[cfg(feature = "reflect")]
                    let node = match registry.get(_id).map(|r| r.type_info()) {
                        Some(info) => crate::reflect_codec::compile(&registry, info, 0),
                        None => crate::reflect_codec::Node::Json,
                    };
                    drop(registry);
                    rust_cursors.push((message.clone(), reader.end(world) as u64));
                    RunParam::RustMessageReader {
                        path: message.clone(),
                        reader,
                        #[cfg(feature = "reflect")]
                        node,
                    }
                }
                None => return Err(err(format!("no message type {message}; call app.addMessage"))),
            },
            ParamDecl::State { state } | ParamDecl::PreviousState { state } => {
                let index = world.resource::<TsPayloads>().state(state).ok_or_else(|| err(format!("no state {state}")))?;
                RunParam::State { index, previous: matches!(p, ParamDecl::PreviousState { .. }) }
            }
            ParamDecl::In { event } => {
                if i != 0 {
                    return Err(err(format!("In<{}> must be the first param", event.rsplit("::").next().unwrap_or(event))));
                }
                if d.schedule != ScheduleDecl::OneShot {
                    return Err(err(format!("In<{event}> is a one-shot system's input; register the system with app.registerSystem")));
                }
                world.resource::<TsPayloads>().event(event).ok_or_else(|| err(format!("no event type {event}")))?;
                RunParam::Event
            }
            ParamDecl::On { event, lifecycle, component } => {
                if i != 0 || d.schedule != ScheduleDecl::Observer {
                    return Err(err("On<...> must be the first param of an observer".into()));
                }
                let trigger = match (event, lifecycle, component) {
                    (Some(e), _, _) => match world.resource::<TsPayloads>().event(e) {
                        Some(i) => Trigger::Event(i),
                        None if e.contains("::") => Trigger::Rust(crate::rust_events::event(&world.resource::<AppTypeRegistry>().read(), e).map_err(|m| err(format!("On<{e}>: {m}")))?),
                        None => return Err(err(format!("no event type {e}"))),
                    },
                    (None, Some(k), Some(c)) => {
                        let native = world.resource::<NativeLayouts>().clone();
                        Trigger::Lifecycle(*k, resolve_ref(world, c, ts, &native, false).map_err(err)?.0)
                    }
                    _ => return Err(err("On<...> names no event".into())),
                };
                observes = Some(ObserverOf(trigger));
                RunParam::Event
            }
            ParamDecl::Bundle { .. } => unreachable!("flattened"),
        };
        params.push(rp);
    }
    if d.schedule == ScheduleDecl::Observer && observes.is_none() {
        return Err(err("an observer's first param must be On<...>".into()));
    }
    let condition = match &d.condition {
        Some(c) => {
            let mut cx = conditions::Compile { ts, encode, config: &d.name, natives };
            Some(conditions::compile(world, c, "", &mut cx).map_err(|e| match e.location {
                Some(at) => ScriptError::at(format!("system {}: {}", d.name, e.message), Some(at)),
                None => err(e.message),
            })?)
        }
        None => None,
    };
    let local_refs: Vec<&[crate::manifest::FieldDecl]> = locals.iter().map(|l| l.as_slice()).collect();
    let mem = world.get_resource_or_init::<SystemMemory>().into_inner().declare(&d.name, &local_refs);
    for (path, end) in rust_cursors {
        mem.cursors.entry(path).or_insert(end);
    }
    Ok(TsSystem {
        key: ScriptKey { generation, index: index as u32 },
        decl: d.clone(),
        params,
        condition,
        observes,
        last_run_us: 0,
        last_error: None,
        warned_read_write: false,
    })
}

/// A fast 64-bit hash of a column, for the dev-only check that read columns stay unwritten.
fn hash_bytes(b: &[u8]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    let (chunks, rest) = b.as_chunks::<8>();
    for c in chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(0x5851_F42D_4C95_7F2D);
    }
    for &x in rest {
        h = (h.rotate_left(5) ^ x as u64).wrapping_mul(0x5851_F42D_4C95_7F2D);
    }
    h
}

/// Per-runtime scratch kept between runs so little allocates per frame once warm.
#[derive(Default)]
pub(crate) struct Scratch {
    pub(crate) arena: Vec<u8>,
    entities: Vec<Vec<u32>>,
    rows: Vec<Vec<u32>>,
    time: Vec<u64>,
    json: Vec<Vec<u8>>,
    /// A query's parts: its matched tables (dense), or its matched archetypes and their tables.
    parts: Vec<(TableId, Option<ArchetypeId>)>,
}

/// Runs every system of a stage. Called from the stage's exclusive system.
pub fn run_stage(world: &mut World, stage: Stage) {
    if !runtime::stage_should_run(world) {
        return;
    }
    let empty = world.get_resource::<TsSystems>().is_none_or(|s| s.stages.get(&stage).is_none_or(|l| l.is_empty()));
    if !stage.eager() && empty {
        return;
    }
    // Two clock reads, only when the plugin is listed and the stage has TS systems to time.
    let timed = !empty && world.contains_resource::<StageTiming>();
    let started = timed.then(Instant::now);
    drive(world, &ListKey::Stage(stage), None);
    if let Some(t) = started {
        record_stage(world, stage, t.elapsed().as_secs_f64() * 1000.0);
    }
}

/// bevy-ts's own diagnostics plugin, shaped as Bevy's render diagnostics are: each TS stage that
/// runs systems records its CPU time as `bevy_ts/<stage>/elapsed_cpu`, in milliseconds,
/// bevy-ts's own work in the stage (column views, commands) included.
pub struct ScriptStageDiagnosticsPlugin;

#[derive(Resource)]
struct StageTiming;

impl Plugin for ScriptStageDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<bevy::diagnostic::DiagnosticsStore>().insert_resource(StageTiming);
    }
}

/// The diagnostic path of a stage's CPU time: the stage in snake case, as Bevy's paths are.
pub fn stage_path(stage: Stage) -> bevy::diagnostic::DiagnosticPath {
    use bevy::diagnostic::DiagnosticPath as P;
    const PATHS: [P; 12] = [
        P::const_new("bevy_ts/first/elapsed_cpu"),
        P::const_new("bevy_ts/pre_update/elapsed_cpu"),
        P::const_new("bevy_ts/run_fixed_main_loop/elapsed_cpu"),
        P::const_new("bevy_ts/update/elapsed_cpu"),
        P::const_new("bevy_ts/spawn_scene/elapsed_cpu"),
        P::const_new("bevy_ts/post_update/elapsed_cpu"),
        P::const_new("bevy_ts/last/elapsed_cpu"),
        P::const_new("bevy_ts/fixed_first/elapsed_cpu"),
        P::const_new("bevy_ts/fixed_pre_update/elapsed_cpu"),
        P::const_new("bevy_ts/fixed_update/elapsed_cpu"),
        P::const_new("bevy_ts/fixed_post_update/elapsed_cpu"),
        P::const_new("bevy_ts/fixed_last/elapsed_cpu"),
    ];
    let i = Stage::ALL.iter().position(|s| *s == stage).expect("every stage is in ALL");
    PATHS[i].clone()
}

/// Adds one measurement, registering the stage's path the first time it runs.
fn record_stage(world: &mut World, stage: Stage, ms: f64) {
    use bevy::diagnostic::{Diagnostic, DiagnosticMeasurement, DiagnosticsStore};
    let Some(mut store) = world.get_resource_mut::<DiagnosticsStore>() else { return };
    let path = stage_path(stage);
    if store.get(&path).is_none() {
        store.add(Diagnostic::new(path.clone()).with_suffix("ms"));
    }
    if let Some(d) = store.get_mut(&path).filter(|d| d.is_enabled) {
        d.add_measurement(DiagnosticMeasurement { time: bevy::platform::time::Instant::now(), value: ms });
    }
}

/// Runs a custom schedule's TS systems. Called from its stage's exclusive system.
pub fn run_custom_stage(world: &mut World, name: &str) {
    if !runtime::stage_should_run(world) {
        return;
    }
    if world.get_resource::<TsSystems>().is_none_or(|s| s.schedules.get(name).is_none_or(|l| l.is_empty())) {
        return;
    }
    drive(world, &ListKey::Schedule(name.to_string()), None);
}

/// Runs a TS `OnEnter`/`OnExit`/`OnTransition` list: called from the exclusive system
/// crate::states adds to Bevy's schedule of that name.
pub(crate) fn run_state_list(world: &mut World, key: ListKey) {
    if !runtime::stage_should_run(world) {
        return;
    }
    drive(world, &key, None);
}

/// Runs the systems of one slot. Called from the slot's exclusive system, which
/// [`crate::sets::ensure_slots`] adds around the exposed set the first time a generation names it.
pub fn run_slot(world: &mut World, key: &SlotKey) {
    if !runtime::stage_should_run(world) {
        return;
    }
    if world.get_resource::<TsSystems>().is_none_or(|s| s.slots.get(key).is_none_or(|l| l.is_empty())) {
        return;
    }
    drive(world, &ListKey::Slot(key.clone()), None);
}

/// Runs a startup list's systems whose identity is in `due`.
pub(crate) fn run_startup_list(world: &mut World, key: ListKey, due: &HashSet<String>) {
    drive(world, &key, Some(due));
}

/// Runs a list of systems in order: a group's collective `runIf` once per call, each system's
/// own conditions, observers after each, and the schedules and one-shot systems a system's
/// commands queue before the next system runs. Takes the runtime and `TsSystems` out of the world
/// while JS runs and puts them back for nested runs.
fn drive(world: &mut World, key: &ListKey, only: Option<&HashSet<String>>) {
    let mut groups: HashMap<String, bool> = HashMap::new();
    let mut from = 0;
    loop {
        let mut caller = None;
        with_systems(world, |world, rt, systems| {
            drain_observers(world, rt, systems);
            let mut list = systems.take(key);
            for (i, sys) in list.iter_mut().enumerate().skip(from) {
                if only.is_some_and(|o| !o.contains(&sys.decl.name)) {
                    continue;
                }
                if gate(world, systems, sys, &mut groups) {
                    run_one(world, rt, sys, None);
                }
                drain_observers(world, rt, systems);
                if crate::schedules::has_pending(world) {
                    caller = Some((i + 1, format!("system {}", sys.decl.name)));
                    break;
                }
            }
            systems.put(key, list);
        });
        match caller {
            Some((next, name)) => {
                crate::schedules::claim(world, &name);
                crate::schedules::run_pending(world);
                from = next;
            }
            None => {
                if crate::schedules::has_pending(world) {
                    crate::schedules::run_pending(world);
                }
                return;
            }
        }
    }
}

/// Whether a system runs now: its groups' collective conditions (each evaluated once per
/// `drive`, the first time a member comes), then its own, every one evaluated as Bevy does so
/// stateful conditions advance.
fn gate(world: &mut World, systems: &mut TsSystems, sys: &mut TsSystem, groups: &mut HashMap<String, bool>) -> bool {
    let mut ok = true;
    for g in &sys.decl.groups {
        let pass = match groups.get(g) {
            Some(b) => *b,
            None => {
                let b = systems.groups.get_mut(g).is_none_or(|c| conditions::evaluate(world, c));
                groups.insert(g.clone(), b);
                b
            }
        };
        ok &= pass;
    }
    if let Some(c) = &mut sys.condition {
        ok &= conditions::evaluate(world, c);
    }
    ok
}

/// Runs a one-shot system by identity, with its `In<E>` bytes. The runs its commands queue run
/// once it is back, so it may run itself, as in Bevy.
pub(crate) fn run_one_shot(world: &mut World, identity: &str, input: Option<&[u8]>, caller: &str) {
    let taken = world.get_resource_mut::<TsSystems>().map(|mut s| s.one_shot.get_mut(identity).map(Option::take));
    let mut sys = match taken {
        Some(Some(Some(sys))) => sys,
        Some(Some(None)) => {
            crate::schedules::log_error(world, format!("runSystem({identity}) from {caller}: it is already running"));
            return;
        }
        _ => {
            crate::schedules::log_error(world, format!("runSystem({identity}) from {caller}: no one-shot system {identity}; register it with app.registerSystem"));
            return;
        }
    };
    // The input must be the In<E> the system takes, in E's layout.
    let wants = match sys.decl.params.first() {
        Some(ParamDecl::In { event }) => {
            let p = world.resource::<TsPayloads>();
            Some((event.clone(), p.event(event).map_or(0, |i| p.events[i].layout.size)))
        }
        _ => None,
    };
    let bad = match (&wants, input) {
        (Some((e, _)), None) => Some(format!("it takes In<{e}>; run it with runSystemWith")),
        (None, Some(_)) => Some("it takes no In<E>; run it with runSystem".to_string()),
        (Some((e, size)), Some(b)) if b.len() != *size => Some(format!("its input is {} bytes, In<{e}> takes {size}", b.len())),
        _ => None,
    };
    if let Some(why) = bad {
        crate::schedules::log_error(world, format!("runSystem({identity}) from {caller}: {why}"));
        if let Some(slot) = world.resource_mut::<TsSystems>().one_shot.get_mut(identity) {
            *slot = Some(sys);
        }
        return;
    }
    with_systems(world, |world, rt, systems| {
        run_one(world, rt, &mut sys, input.map(|b| (b, (u64::MAX, u64::MAX))));
        drain_observers(world, rt, systems);
    });
    if let Some(mut s) = world.get_resource_mut::<TsSystems>()
        && let Some(slot) = s.one_shot.get_mut(identity)
    {
        *slot = Some(sys);
    }
    crate::schedules::claim(world, &format!("system {identity}"));
}


/// Takes the runtime and the systems out of the world for `f`.
pub(crate) fn with_systems(world: &mut World, f: impl FnOnce(&mut World, &mut ScriptRuntime, &mut TsSystems)) {
    let Some(mut rt) = world.remove_non_send::<ScriptRuntime>() else { return };
    let Some(mut systems) = world.remove_resource::<TsSystems>() else {
        world.insert_non_send(rt);
        return;
    };
    f(world, &mut rt, &mut systems);
    world.insert_resource(systems);
    rt.free_pool(world);
    world.insert_non_send(rt);
}

/// Runs queued observers until the queue is empty (bounded, so a trigger loop cannot hang).
pub(crate) fn drain_observers(world: &mut World, rt: &mut ScriptRuntime, systems: &mut TsSystems) {
    for _ in 0..10_000 {
        // An observer's own `observe` commands are adopted before the next event.
        adopt_targeted(world, rt, systems);
        let Some(p) = world.get_resource_mut::<ObserverQueue>().and_then(|mut q| q.0.pop_front()) else { return };
        let targets: Vec<Option<Entity>> = if p.path.is_empty() { vec![None] } else { p.path.iter().copied().map(Some).collect() };
        // Every observer and hop reads its heap fields at the observer place; `p` holds the value
        // until the last of them has run, and drops it once, here.
        let of = match p.trigger {
            crate::observers::Trigger::Lifecycle(_, id) => crate::places::ValueOf::Component(id),
            crate::observers::Trigger::Event(i) => crate::places::ValueOf::Event(i),
            crate::observers::Trigger::Rust(_) => crate::places::ValueOf::Rust,
        };
        let value = crate::places::ObserverValue { ptr: p.bytes.bytes().as_ptr(), of };
        crate::places::with_observer(world, Some(value), |world| {
            'path: for (hop, target) in targets.into_iter().enumerate() {
                let mut stop = false;
                let bits = (target.map_or(u64::MAX, Entity::to_bits), p.original.map_or(u64::MAX, Entity::to_bits));
                let bytes = hop.checked_sub(1).and_then(|h| p.hops.get(h)).map_or(p.bytes.bytes(), Vec::as_slice);
                // Bevy's order: global observers, then the target's own.
                for o in systems.observers.iter_mut() {
                    if o.observes.as_ref().is_some_and(|ob| ob.0 == p.trigger) {
                        stop |= run_one(world, rt, o, Some((bytes, bits)));
                    }
                }
                // Attachments as they stood when the trigger was queued.
                for name in p.attached.get(hop).into_iter().flatten() {
                    if let Some(o) = systems.targeted.get_mut(name)
                        && o.observes.as_ref().is_some_and(|ob| ob.0 == p.trigger)
                    {
                        stop |= run_one(world, rt, o, Some((bytes, bits)));
                    }
                }
                if stop {
                    break 'path;
                }
            }
        });
    }
    warn!(target: "bevy_ts", "observer queue still busy after 10,000 events; the rest wait for the next stage");
}

/// Runs one system (or one observer event). Returns whether an observer stopped propagation.
fn run_one(world: &mut World, rt: &mut ScriptRuntime, sys: &mut TsSystem, event: Option<(&[u8], (u64, u64))>) -> bool {
    let identity = sys.decl.name.clone();
    let dev = rt.dev;
    let tick = world.increment_change_tick();
    let last_run = world.resource::<SystemMemory>().0.get(&identity).map_or(Tick::new(0), |m| m.last_run);
    let frame = runtime::frame_info(world, tick.get());
    let started = Instant::now();

    let mut scratch = std::mem::take(&mut rt.scratch);
    scratch.time.clear();
    let mut frames: Vec<ParamFrame> = Vec::with_capacity(sys.params.len());
    // Written columns per (table) for summary ticks; read columns per frame for the dev guard.
    let mut written: Vec<(TableId, ComponentId)> = Vec::new();
    let mut read_cols: Vec<(usize, usize, usize, ComponentId)> = Vec::new();
    let mut written_ids: HashSet<ComponentId> = HashSet::new();
    let mut reader_cursors: Vec<(String, u64)> = Vec::new();
    let (mut ents_used, mut rows_used, mut json_used) = (0usize, 0usize, 0usize);
    let mut failure: Option<ScriptError> = None;
    // A `Single` or `Populated` that does not match skips the run, as Bevy's param validation does.
    let mut skip = false;
    let mut memory = world.remove_resource::<SystemMemory>().unwrap_or_default();
    let mem = memory.0.entry(identity.clone()).or_default();
    // Columns and resource rows the run exposes, for the slice-overlap check of host calls.
    let ports = sys.params.iter().any(|p| matches!(p, RunParam::Port(_)));
    let mut regions: Vec<crate::host::Region> = Vec::new();
    {
        if let Some(host) = world.get_resource::<crate::host::HostParams>() {
            for p in &sys.params {
                if let RunParam::Port(i) = p {
                    // A native write to a column a TS query only reads is not TS's mistake.
                    written_ids.extend(host.writes(*i).iter().copied());
                }
            }
        }
        for p in &sys.params {
            if let RunParam::Query(q) = p {
                written_ids.extend(q.columns.iter().filter(|c| c.write).map(|c| c.id));
            }
            if let RunParam::Res { id, write: true, .. } = p {
                written_ids.insert(*id);
            }
        }
        for p in sys.params.iter_mut() {
            let frame = match p {
                RunParam::Query(q) => {
                    q.state.update_archetypes(world);
                    let tables = &world.storages().tables;
                    let sparse = &world.storages().sparse_sets;
                    let mut out = Vec::new();
                    let mut count = 0usize;
                    let mut parts = std::mem::take(&mut scratch.parts);
                    parts.clear();
                    if q.dense {
                        parts.extend(q.state.matched_tables().map(|t| (t, None)));
                    } else {
                        parts.extend(q.state.matched_archetypes().map(|a| (world.archetypes()[a].table_id(), Some(a))));
                    }
                    // A sparse-set query's tables, each with its entities' slot, built once per table.
                    let mut table_ents: Vec<(TableId, usize)> = Vec::new();
                    for &(table_id, arch_id) in &parts {
                        let table = tables.get(table_id).expect("matched table exists");
                        let arch = arch_id.map(|a| &world.archetypes()[a]);
                        let len = table.entity_count() as usize;
                        if arch.map_or(len, |a| a.len() as usize) == 0 {
                            continue;
                        }
                        // Row filters: the rows that pass against this system's last run. An
                        // archetype's rows are always a list, whether or not anything filters.
                        let rows = if q.rows.is_empty() && arch.is_none() {
                            count += len;
                            None
                        } else {
                            if scratch.rows.len() <= rows_used {
                                scratch.rows.push(Vec::new());
                            }
                            let list = &mut scratch.rows[rows_used];
                            list.clear();
                            let live: Vec<Live> = q.rows.iter().map(|t| t.live(table, arch, sparse)).collect();
                            let pass = |row: usize, e: Entity| live.iter().all(|l| l.holds(row, e, world, last_run, tick));
                            match arch {
                                None => {
                                    for (row, e) in table.entities().iter().enumerate() {
                                        if pass(row, *e) {
                                            list.push(row as u32);
                                        }
                                    }
                                }
                                Some(a) => {
                                    for ae in a.entities() {
                                        let row = ae.table_row().index_u32();
                                        if pass(row as usize, ae.id()) {
                                            list.push(row);
                                        }
                                    }
                                }
                            }
                            if list.is_empty() {
                                continue;
                            }
                            count += list.len();
                            rows_used += 1;
                            Some((list.as_ptr(), list.len()))
                        };
                        let slot = match table_ents.iter().find(|(t, _)| *t == table_id) {
                            Some(&(_, slot)) => slot,
                            None => {
                                if scratch.entities.len() <= ents_used {
                                    scratch.entities.push(Vec::new());
                                }
                                let ents = &mut scratch.entities[ents_used];
                                ents.clear();
                                for e in table.entities() {
                                    let bits = e.to_bits();
                                    ents.push(bits as u32);
                                    ents.push((bits >> 32) as u32);
                                }
                                if arch.is_some() {
                                    table_ents.push((table_id, ents_used));
                                }
                                ents_used += 1;
                                ents_used - 1
                            }
                        };
                        let ents = &scratch.entities[slot];
                        let mut columns = Vec::with_capacity(q.columns.len());
                        for (ci, c) in q.columns.iter().enumerate() {
                            let Some(column) = table.get_column(c.id) else {
                                columns.push(ColumnFrame { data: std::ptr::null_mut(), bytes: 0, ticks: None, added: None });
                                continue;
                            };
                            if c.reflect.is_some() {
                                // A reflected term's frame is its changed ticks: JS keys its cache by
                                // them and fetches a value only for a row it reads whose tick moved.
                                let ticks = table.get_changed_ticks_slice_for(c.id).expect("column has ticks").as_ptr() as *mut u32;
                                let added = c.tracked.then(|| table.get_added_ticks_slice_for(c.id).expect("column has ticks").as_ptr() as *mut u32);
                                columns.push(ColumnFrame { data: ticks as *mut u8, bytes: len * 4, ticks: (c.write || c.tracked).then_some(ticks), added });
                                continue;
                            }
                            // SAFETY: the column holds `len × size` bytes; only the start pointer is
                            // taken. Cells are `UnsafeCell`s and the stage is exclusive.
                            let data = unsafe { column.get_data_slice::<u8>(len) }.as_ptr() as *mut u8;
                            let changed = || table.get_changed_ticks_slice_for(c.id).expect("column has ticks").as_ptr() as *mut u32;
                            let ticks = (c.write || c.tracked || c.holes).then(changed);
                            let added = c.tracked.then(|| table.get_added_ticks_slice_for(c.id).expect("column has ticks").as_ptr() as *mut u32);
                            if c.write {
                                written.push((table_id, c.id));
                            } else if dev && !written_ids.contains(&c.id) {
                                read_cols.push((frames.len(), out.len(), ci, c.id));
                            }
                            if ports {
                                regions.push(crate::host::Region { start: data as usize, end: data as usize + len * c.size, id: c.id });
                            }
                            columns.push(ColumnFrame { data, bytes: len * c.size, ticks, added });
                        }
                        let has = q.has.iter().enumerate().fold(0u32, |bits, (i, id)| {
                            let here = arch.map_or_else(|| table.has_column(*id), |a| a.contains(*id));
                            bits | (here as u32) << i
                        });
                        out.push(TableFrame { len, entities: ents.as_ptr(), columns, rows, has });
                    }
                    scratch.parts = parts;
                    let ok = match q.need {
                        Need::Any => true,
                        Need::One => count == 1,
                        Need::AtMostOne => count <= 1,
                        Need::SomeRows => count > 0,
                    };
                    skip |= !ok;
                    ParamFrame::Query { tables: out, last_run: last_run.get() }
                }
                RunParam::Res { name, id, size, write, heap, optional } => match world.resource_entities().get(*id).and_then(|e| Some((e, world.entities().get_spawned(e).ok()?))) {
                    // A Rust resource is stored in a sparse set (`#[derive(Resource)]`); a TS one in a table.
                    Some((e, _)) if world.storages().sparse_sets.get(*id).is_some() => {
                        let set = world.storages().sparse_sets.get(*id).expect("sparse set");
                        let (ptr, cells) = set.get_with_ticks(e).expect("resource row");
                        // SAFETY: as for table rows; the stage is exclusive and the cells are `UnsafeCell`s.
                        let data = ptr.as_ptr() as *mut u8;
                        let ticks = write.then(|| cells.changed.get() as *mut u32);
                        ParamFrame::Row { data, bytes: *size, ticks }
                    }
                    Some((_, loc)) => {
                        let table = world.storages().tables.get(loc.table_id).expect("resource table");
                        let row = loc.table_row.index();
                        let col = table.get_column(*id).expect("resource column");
                        let len = table.entity_count() as usize;
                        // SAFETY: as for query columns; `row < len`.
                        let base = unsafe { col.get_data_slice::<u8>(len) }.as_ptr() as *mut u8;
                        let data = unsafe { base.add(row * *size) };
                        let ticks = (*write || *heap).then(|| unsafe { (table.get_changed_ticks_slice_for(*id).unwrap().as_ptr() as *mut u32).add(row) });
                        if *write {
                            written.push((loc.table_id, *id));
                        }
                        if ports {
                            regions.push(crate::host::Region { start: data as usize, end: data as usize + *size, id: *id });
                        }
                        ParamFrame::Row { data, bytes: *size, ticks }
                    }
                    None if *optional => ParamFrame::None,
                    None => {
                        failure = Some(ScriptError::new(format!("resource {name} does not exist; insert it with app.insertResource, take Option<Res<{name}>> or guard with runIf(resourceExists({name}))")));
                        ParamFrame::None
                    }
                },
                RunParam::ReflectRes { name, id, write, optional, .. } => match world.resource_entities().get(*id).and_then(|e| changed_cell(world, *id, e)) {
                    Some(cell) => ParamFrame::Row { data: cell as *mut u8, bytes: 4, ticks: write.then_some(cell) },
                    None if *optional => ParamFrame::None,
                    None => {
                        failure = Some(ScriptError::new(format!("resource {name} does not exist; guard with runIf(resourceExists({name}))")));
                        ParamFrame::None
                    }
                },
                RunParam::Time(clock) => {
                    // The frame is pointed at once `scratch.time` stops growing (below).
                    push_time(world, *clock, &mut scratch.time);
                    ParamFrame::None
                }
                RunParam::Local(i) => {
                    let store = &mut mem.locals[*i];
                    let bytes = store.layout.size;
                    ParamFrame::Row { data: store.data_mut().as_mut_ptr(), bytes, ticks: None }
                }
                RunParam::MessageReader { index } => {
                    let payloads = world.resource::<TsPayloads>();
                    let q = &payloads.messages[*index];
                    let cursor = mem.cursors.get(&q.name).copied().unwrap_or(0);
                    let [a, b] = q.unread(cursor);
                    reader_cursors.push((q.name.clone(), q.next_seq));
                    ParamFrame::Messages { parts: [(a.0.as_ptr(), a.1), (b.0.as_ptr(), b.1)], stride: q.layout.size.max(1), seqs: [a.2, b.2] }
                }
                RunParam::Removed { id, key } => {
                    if scratch.entities.len() <= ents_used {
                        scratch.entities.push(Vec::new());
                    }
                    let ents = &mut scratch.entities[ents_used];
                    ents_used += 1;
                    ents.clear();
                    let cursor = mem.cursors.get(key.as_str()).copied().unwrap_or(0);
                    let mut next = cursor;
                    if let Some(messages) = world.removed_components().get(*id) {
                        for (m, mid) in messages.get_cursor().read_with_id(messages) {
                            if (mid.id as u64) < cursor {
                                continue;
                            }
                            let bits = Entity::from(m.clone()).to_bits();
                            ents.push(bits as u32);
                            ents.push((bits >> 32) as u32);
                            next = mid.id as u64 + 1;
                        }
                    }
                    reader_cursors.push((key.clone(), next));
                    ParamFrame::Entities { data: ents.as_ptr(), len: ents.len() / 2 }
                }
                RunParam::ChangeTick => ParamFrame::Ticks { this_run: tick.get(), last_run: last_run.get() },
                RunParam::RustMessageReader {
                    path,
                    reader,
                    #[cfg(feature = "reflect")]
                    node,
                } => {
                    let cursor = mem.cursors.get(path.as_str()).copied().unwrap_or(0) as usize;
                    if scratch.json.len() <= json_used {
                        scratch.json.push(Vec::new());
                    }
                    let buf = &mut scratch.json[json_used];
                    json_used += 1;
                    buf.clear();
                    let registry = world.resource::<AppTypeRegistry>().read();
                    let mut bad = None;
                    #[cfg(feature = "reflect")]
                    let next = {
                        // A `u32` count, then each message: the codec's `List` of the message.
                        buf.extend_from_slice(&[0; 4]);
                        let mut n = 0u32;
                        let next = reader.read(world, cursor, &mut |m: &dyn bevy::reflect::PartialReflect| {
                            if bad.is_some() {
                                return;
                            }
                            n += 1;
                            if let Err(e) = crate::reflect_codec::put(&registry, node, m, buf) {
                                bad = Some(e);
                            }
                        });
                        buf[..4].copy_from_slice(&n.to_le_bytes());
                        next
                    };
                    #[cfg(not(feature = "reflect"))]
                    let next = {
                        buf.push(b'[');
                        let next = reader.read(world, cursor, &mut |m: &dyn bevy::reflect::PartialReflect| {
                            if bad.is_some() {
                                return;
                            }
                            if buf.len() > 1 {
                                buf.push(b',');
                            }
                            if let Err(e) = crate::rust_events::write_json(&registry, m, buf) {
                                bad = Some(e);
                            }
                        });
                        buf.push(b']');
                        next
                    };
                    if let Some(e) = bad {
                        failure = Some(ScriptError::new(format!("MessageReader<{path}>: {e}")));
                    }
                    reader_cursors.push((path.clone(), next as u64));
                    // One run of one-byte records: the codec's bytes, or a JSON array.
                    ParamFrame::Messages { parts: [(buf.as_ptr(), buf.len()), (std::ptr::null(), 0)], stride: 1, seqs: [0, 0] }
                }
                RunParam::State { index, previous } => {
                    // JS numbers a machine's values in its declared order.
                    let m = &world.resource::<TsPayloads>().states[*index];
                    let v = if *previous { crate::states::previous_of(world, m.slot) } else { crate::states::current_of(world, m.slot) };
                    ParamFrame::State { value: v.and_then(|v| m.index_of_sym(v)).unwrap_or(u32::MAX) }
                }
                RunParam::Event => match event {
                    Some((bytes, (entity, original))) => ParamFrame::Event { data: bytes.as_ptr(), bytes: bytes.len(), entity, original },
                    None => ParamFrame::None,
                },
                RunParam::Port(_) | RunParam::None => ParamFrame::None,
            };
            frames.push(frame);
        }
        // Time views point into `scratch.time`, which no longer grows.
        const _: () = assert!(cfg!(target_endian = "little"));
        let mut t = 0;
        for (p, f) in sys.params.iter().zip(frames.iter_mut()) {
            if let RunParam::Time(clock) = p {
                let bytes = time_layout(*clock).size;
                // SAFETY: `bytes / 8` words per Time param were pushed above, in this order.
                let data = unsafe { scratch.time.as_mut_ptr().add(t) } as *mut u8;
                *f = ParamFrame::Row { data, bytes, ticks: None };
                t += bytes / 8;
            }
        }
    }

    if skip && failure.is_none() {
        // Skipped like a false run condition: no call into JS, and the last run stays.
        world.insert_resource(memory);
        rt.scratch = scratch;
        return false;
    }

    let hashes: Vec<u64> = read_cols
        .iter()
        .map(|(f, t, c, _)| match &frames[*f] {
            ParamFrame::Query { tables, .. } => {
                let cf = &tables[*t].columns[*c];
                // SAFETY: readable for the run.
                hash_bytes(unsafe { std::slice::from_raw_parts(cf.data, cf.bytes) })
            }
            _ => 0,
        })
        .collect();

    let result = match failure {
        Some(e) => Err(e),
        None => {
            rt.fill_pool(world);
            scratch.arena.resize(crate::commands::ARENA_BYTES, 0);
            let pool = std::mem::take(&mut rt.pool);
            let mut params = world.remove_resource::<crate::host::HostParams>().unwrap_or_default();
            // A `Local`'s heap field is read and written through its place; observers the run
            // triggers set their own and put this one back.
            let locals = crate::places::RunLocals::of(&mut mem.locals);
            let locals_before = world.get_resource_or_init::<crate::places::RunPlace>().locals.replace(locals);
            let r = {
                // The world is held exclusively for the run and not touched until it ends; host
                // calls fetch their params from this cell at the run's ticks.
                let ticks = crate::host::RunTicks { last_run, this_run: tick };
                let ports: Vec<usize> = sys.params.iter().filter_map(|p| if let RunParam::Port(i) = p { Some(*i) } else { None }).collect();
                let mut host = crate::host::WorldHost { world: world.as_unsafe_world_cell(), ticks, regions: &regions, allowed: Some(&ports), params: &mut params, reserved: Vec::new(), reserves: 0 };
                let mut run = SystemRun { ctx: CallContext { frame, arena: &mut scratch.arena, pool: &pool, host: Some(&mut host) }, params: &frames };
                let r = rt.backend.run_system(sys.key, &mut run);
                (r, host.reserved, host.reserves)
            };
            let (r, reserved, reserves) = r;
            world.resource_mut::<crate::places::RunPlace>().locals = locals_before;
            world.insert_resource(params);
            rt.pool = pool;
            rt.pool.extend(reserved);
            rt.reserve_calls += reserves;
            r
        }
    };

    // Post-run: summary ticks, whole-table marks, the dev guard.
    {
        let tables = &world.storages().tables;
        for (table_id, id) in &written {
            if let Some(s) = tables.get(*table_id).and_then(|t| t.get_summary_tick(*id)) {
                s.set(tick);
            }
        }
        if let Ok(out) = &result {
            for &(p, t, c) in &out.write_marks {
                if let Some(ParamFrame::Query { tables, .. }) = frames.get(p as usize)
                    && let Some(tf) = tables.get(t as usize)
                    && let Some(ticks) = tf.columns.get(c as usize).and_then(|c| c.ticks)
                {
                    // SAFETY: the tick column has `len` entries and the stage is exclusive.
                    let all = unsafe { std::slice::from_raw_parts_mut(ticks, tf.len) };
                    match tf.rows {
                        // Only the rows the query matched: a filtered or sparse-set frame's table
                        // holds others.
                        // SAFETY: the list has `n` entries, each a row of the table.
                        Some((rows, n)) => unsafe { std::slice::from_raw_parts(rows, n) }.iter().for_each(|r| all[*r as usize] = tick.get()),
                        None => all.fill(tick.get()),
                    }
                }
            }
        }
        if dev && !sys.warned_read_write {
            for ((f, t, c, id), before) in read_cols.iter().zip(&hashes) {
                if let ParamFrame::Query { tables, .. } = &frames[*f] {
                    let cf = &tables[*t].columns[*c];
                    // SAFETY: unchanged structurally since the run.
                    if hash_bytes(unsafe { std::slice::from_raw_parts(cf.data, cf.bytes) }) != *before {
                        sys.warned_read_write = true;
                        let name = sys
                            .params
                            .iter()
                            .find_map(|p| match p {
                                RunParam::Query(q) => q.columns.iter().find(|a| a.id == *id).map(|a| a.name.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        world.insert_resource(memory);
                        rt.log_warn(
                            world,
                            format!("system {} wrote to {name}, which no query of it declares Mut; declare it Mut", sys.decl.name),
                            sys.decl.location.clone(),
                        );
                        memory = world.remove_resource::<SystemMemory>().unwrap();
                        break;
                    }
                }
            }
        }
    }
    let mem = memory.0.entry(identity.clone()).or_default();
    mem.last_run = tick;
    let mut stop = false;
    if result.is_ok() {
        for (name, next) in reader_cursors {
            mem.cursors.insert(name, next);
        }
    }
    world.insert_resource(memory);
    sys.last_run_us = started.elapsed().as_micros() as u64;

    match result {
        Ok(out) => {
            sys.last_error = None;
            stop = out.stop_propagation;
            let label = format!("system {}", sys.decl.name);
            rt.apply_output(world, &scratch.arena, out, &label);
            settle_ports(world, &sys.params, true);
        }
        Err(e) => {
            settle_ports(world, &sys.params, false);
            // Assets a throwing run made still go in: their refs were handed out.
            #[cfg(feature = "reflect")]
            crate::reflect_asset::flush(world);
            rt.log_error(world, &format!("system {} threw", sys.decl.name), &e);
            sys.last_error = Some(e);
        }
    }
    rt.scratch = scratch;
    stop
}

/// After TS's own output: applies (or, for a failed run, drops) the deferred state of the
/// system's exported params, in its parameter order.
fn settle_ports(world: &mut World, params: &[RunParam], ok: bool) {
    if !params.iter().any(|p| matches!(p, RunParam::Port(_))) {
        return;
    }
    let Some(mut host) = world.remove_resource::<crate::host::HostParams>() else { return };
    for p in params {
        if let RunParam::Port(i) = p {
            if ok { host.apply(*i, world) } else { host.discard(*i, world) }
        }
    }
    world.insert_resource(host);
}

/// A row of `bevy-ts/systems`.
#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    pub id: String,
    pub name: String,
    pub schedule: ScheduleDecl,
    /// The stage, for systems in one (`preUpdate`, ...); `startup`, `onEnter`, `onExit` or
    /// `observer` otherwise.
    pub stage: String,
    pub params: Vec<ParamDecl>,
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub generation: u64,
    #[serde(rename = "lastRunMicros")]
    pub last_run_us: u64,
    #[serde(rename = "lastError", skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ScriptError>,
}

impl TsSystem {
    pub fn info(&self) -> SystemInfo {
        let (read, write) = self.decl.access();
        let stage = match &self.decl.schedule {
            ScheduleDecl::Stage { stage } => stage.as_str(),
            ScheduleDecl::PreStartup => "preStartup",
            ScheduleDecl::Startup => "startup",
            ScheduleDecl::PostStartup => "postStartup",
            ScheduleDecl::Schedule { .. } => "schedule",
            ScheduleDecl::OnEnter { .. } => "onEnter",
            ScheduleDecl::OnExit { .. } => "onExit",
            ScheduleDecl::OnTransition { .. } => "onTransition",
            ScheduleDecl::Observer => "observer",
            ScheduleDecl::OneShot => "oneShot",
        }
        .to_string();
        SystemInfo {
            id: self.decl.name.clone(),
            name: self.decl.name.rsplit('#').next().unwrap_or(&self.decl.name).to_string(),
            schedule: self.decl.schedule.clone(),
            stage,
            params: self.decl.params.clone(),
            read,
            write,
            generation: self.key.generation,
            last_run_us: self.last_run_us,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(name: &str, after: &[&str], before: &[&str], sets: &[&str]) -> SystemDecl {
        SystemDecl {
            name: name.into(),
            schedule: ScheduleDecl::Stage { stage: Stage::Update },
            params: vec![],
            after: after.iter().map(|s| s.to_string()).collect(),
            before: before.iter().map(|s| s.to_string()).collect(),
            sets: sets.iter().map(|s| s.to_string()).collect(),
            condition: None,
            groups: vec![],
            location: None,
        }
    }

    #[test]
    fn registration_order_adjusted_by_after_before_and_sets() {
        let ds = [sys("a", &["c"], &[], &[]), sys("b", &[], &[], &[]), sys("c", &[], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert_eq!(order(&refs, &HashSet::new()).unwrap(), vec![1, 2, 0]);
        let ds = [sys("x", &["enemies"], &[], &[]), sys("e1", &[], &[], &["enemies"]), sys("e2", &[], &["e1"], &["enemies"])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert_eq!(order(&refs, &HashSet::new()).unwrap(), vec![2, 1, 0]);
    }

    #[test]
    fn cycles_and_unknown_targets_fail() {
        let ds = [sys("a", &["b"], &[], &[]), sys("b", &["a"], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs, &HashSet::new()).unwrap_err().message.contains("cycle"));
        let ds = [sys("a", &["nope"], &[], &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs, &HashSet::new()).unwrap_err().message.contains("nope"));
    }
}
