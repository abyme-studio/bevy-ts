//! The retained tree, as ECS state that survives every reload: a `TsxNode` on each entity tsx
//! spawned and the `TsxTree` resource. JS holds no copy of it.

use bevy::prelude::*;
use bevy_ts::backend::SourceLocation;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// How a field got its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Provenance {
    /// `f={v}`: written on insert and whenever the declared value changes.
    Controlled,
    /// `initialF={v}`: written on insert only.
    Initial,
    /// Given neither way: the declared default, on insert.
    Default,
    /// `f={() => ...}`: written only by its binding, never from a literal.
    Bound,
    /// Set by a level file's patch (levels): written as a controlled field is, from the file.
    Override,
}

/// One field of a component tsx inserted.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldState {
    pub provenance: Provenance,
    /// The last value tsx wrote (controlled, initial or default). An `initial` field changed in
    /// source keeps the value written on insert.
    pub applied: Value,
}

/// A component (or intrinsic) tsx inserted on a node's entity.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OwnedComponent {
    pub fields: BTreeMap<String, FieldState>,
    /// The element that set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
    /// Where each `override` field is set in a level file.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, SourceLocation>,
}

/// On every entity tsx spawned: what made it and what tsx owns on it.
#[derive(Component, Clone, Debug, PartialEq, Serialize)]
pub struct TsxNode {
    /// A stable hash of the root and key path.
    pub id: u64,
    pub root: String,
    #[serde(rename = "keyPath")]
    pub key_path: Vec<String>,
    /// The element type at each step of the key path (`entity` or the outermost prefab).
    pub types: Vec<String>,
    /// The element that made it, then each prefab call site above it, innermost first.
    pub source: Vec<SourceLocation>,
    /// Components tsx inserted, by element name (`game::Health`, `Transform`, `Name`, `mesh`).
    pub components: BTreeMap<String, OwnedComponent>,
}

/// A node despawned by something other than tsx: not respawned until a restart or a key change.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Tombstone {
    pub root: String,
    #[serde(rename = "keyPath")]
    pub key_path: Vec<String>,
    pub source: Vec<SourceLocation>,
}

/// Counts of one reconcile's world writes, for tests and the watch stream.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReconcileStats {
    pub spawned: usize,
    pub despawned: usize,
    pub inserted: usize,
    pub removed: usize,
    /// Controlled fields written on existing components.
    pub written: usize,
    pub tombstoned: usize,
}

impl ReconcileStats {
    pub fn is_empty(&self) -> bool {
        *self == ReconcileStats::default()
    }

    pub fn add(&mut self, o: &ReconcileStats) {
        self.spawned += o.spawned;
        self.despawned += o.despawned;
        self.inserted += o.inserted;
        self.removed += o.removed;
        self.written += o.written;
        self.tombstoned += o.tombstoned;
    }
}

/// Where a binding writes: a prop of a component element on a node.
#[derive(Clone, Debug, PartialEq)]
pub struct BindTarget {
    pub node: u64,
    pub component: String,
    pub prop: String,
}

/// A `<Show>` or `<For>`: its content mounts under `prefix` (the slot's key path), in groups.
#[derive(Clone, Debug, Default)]
pub struct SlotState {
    pub root: String,
    pub parent: Option<Entity>,
    pub prefix: Vec<String>,
    pub types: Vec<String>,
    /// Groups mounted since the generation applied.
    pub groups: BTreeSet<String>,
}

/// A site: bindings on one element's native-layout field, across every instance. Rust scans the
/// components the site reads on its own entity in table order and calls JS for changed rows.
#[derive(Default)]
pub struct SiteState {
    /// Own reads, once JS reports them after the site's first run, with their layouts.
    pub reads: Vec<(String, bevy::ecs::component::ComponentId, bevy_ts::layout::ComponentLayout)>,
    /// Instance binding id by entity.
    pub instances: HashMap<Entity, u32>,
    /// Matches the tables holding every read, to scan their tick columns.
    pub query: Option<QueryState<Entity>>,
    pub last: bevy::ecs::change_detection::Tick,
}

impl std::fmt::Debug for SiteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SiteState").field("reads", &self.reads).field("instances", &self.instances.len()).finish()
    }
}

/// A component of an entity a binding read, watched by change tick.
#[derive(Clone, Debug)]
pub struct Subscription {
    pub id: bevy::ecs::component::ComponentId,
    /// The tick the value was last delivered at.
    pub last: bevy::ecs::change_detection::Tick,
}

/// The live generation's bindings, refs, slots and subscriptions. JS holds the closures; Rust
/// holds where they write and what they read. Rebuilt by every applied generation.
#[derive(Default, Debug)]
pub struct Reactive {
    pub targets: HashMap<u32, BindTarget>,
    /// Ref id to the node that fills it.
    pub refs: HashMap<u32, u64>,
    /// Refs JS has not been told about.
    pub new_refs: Vec<u32>,
    pub slots: HashMap<u32, SlotState>,
    /// Pairs bindings read: an entity's component, or (no entity) a resource.
    pub subs: HashMap<(Option<Entity>, String), Subscription>,
    pub sites: HashMap<u32, SiteState>,
    /// Picking events queued by observers for the next host call: handler id, event.
    pub events: Vec<(u32, serde_json::Value)>,
    /// Site instances JS has not been given their entity for.
    pub new_instances: Vec<(u32, Entity)>,
    /// A host call is due: a generation applied, a mount brought bindings, or JS asked.
    pub due: bool,
    /// Slots of a fresh generation to sweep after their first host call.
    pub sweep: bool,
    /// Binding targets may point at gone nodes: check before the next call.
    pub stale: bool,
    /// Host calls made, and bound values written, for tests.
    pub calls: u64,
    pub patched: u64,
    /// Time spent, in microseconds: finding changed rows, in the host call (serialize, JS,
    /// parse), and applying its output. For measurements.
    pub scan_us: f64,
    /// The last frame's tsx work (scan, call, apply), in microseconds; 0 for a frame with none.
    pub last_frame_us: f64,
    pub call_us: f64,
    pub apply_us: f64,
}

/// One mounted root: the node ids it owns, in description order.
#[derive(Clone, Debug, Default)]
pub struct RootState {
    pub location: Option<SourceLocation>,
    pub nodes: BTreeSet<u64>,
}

/// The tree's index: roots, node id to entity, tombstones.
#[derive(Resource, Default, Debug)]
pub struct TsxTree {
    pub roots: BTreeMap<String, RootState>,
    /// Roots a Rust plugin describes (a level's `level:<path>`): no generation unmounts them.
    pub external: BTreeSet<String>,
    pub entities: HashMap<u64, Entity>,
    pub tombstones: HashMap<u64, Tombstone>,
    /// The last reconcile's writes.
    pub last: ReconcileStats,
    /// Reconciles run so far: generation applies plus runtime mounts and unmounts.
    pub reconciles: u64,
    pub reactive: Reactive,
    pub journal: Journal,
    pub timings: Timings,
}

/// The last generation's tsx time, for measurements: checking the section, applying it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub check_ms: f64,
    pub apply_ms: f64,
    pub parse_ms: f64,
    pub resolve_ms: f64,
    pub reconcile_ms: f64,
}

impl TsxTree {
    pub fn entity(&self, id: u64) -> Option<Entity> {
        self.entities.get(&id).copied()
    }
}

/// What one reconcile changed, by node id, for `bevy-ts/tsx_tree+watch`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    pub tombstoned: Vec<u64>,
    /// Nodes whose source chain moved (lines inserted above them, a prefab renamed).
    pub moved: Vec<u64>,
    /// Nodes whose values tsx wrote (controlled fields, inserted or removed components).
    pub written: Vec<u64>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        *self == Change::default()
    }
}

/// Recent reconciles' changes, numbered.
#[derive(Debug, Default)]
pub struct Journal {
    pub seq: u64,
    pub open: Change,
    pub messages: std::collections::VecDeque<(u64, Change)>,
}

impl Journal {
    /// Ends the reconcile in progress: a non-empty one becomes the next message.
    pub fn close(&mut self) {
        if self.open.is_empty() {
            return;
        }
        self.seq += 1;
        if self.messages.len() == 256 {
            self.messages.pop_front();
        }
        self.messages.push_back((self.seq, std::mem::take(&mut self.open)));
    }
}

/// The root runtime mounts (`ctx.commands.mount`) go under.
pub const RUNTIME_ROOT: &str = "$runtime";
