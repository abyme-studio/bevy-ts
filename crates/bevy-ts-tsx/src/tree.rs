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
    pub entities: HashMap<u64, Entity>,
    pub tombstones: HashMap<u64, Tombstone>,
    /// The last reconcile's writes.
    pub last: ReconcileStats,
    /// Reconciles run so far: generation applies plus runtime mounts and unmounts.
    pub reconciles: u64,
}

impl TsxTree {
    pub fn entity(&self, id: u64) -> Option<Entity> {
        self.entities.get(&id).copied()
    }
}

/// The root runtime mounts (`ctx.commands.mount`) go under.
pub const RUNTIME_ROOT: &str = "$runtime";
