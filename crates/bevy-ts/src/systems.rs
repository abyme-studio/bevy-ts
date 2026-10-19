//! TS systems: building their queries, ordering them within a stage, and running them over
//! zero-copy column views with per-row change ticks.

use crate::backend::{CallContext, ColumnFrame, ScriptError, ScriptKey, SystemRun, TableFrame};
use crate::layout::NativeLayouts;
use crate::manifest::{ComponentRef, ComponentSource, Stage, SystemDecl};
use crate::runtime::{self, ScriptRuntime};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

// JS stores ticks as u32 straight into Bevy's tick column.
const _: () = assert!(size_of::<Tick>() == 4 && align_of::<Tick>() == 4);

/// One component a system touches, resolved.
#[derive(Clone, Debug, Serialize)]
pub struct Access {
    pub name: String,
    #[serde(skip)]
    pub id: ComponentId,
    #[serde(skip)]
    pub size: usize,
    #[serde(skip)]
    pub write: bool,
}

/// A live TS system.
pub struct TsSystem {
    pub key: ScriptKey,
    pub decl: SystemDecl,
    /// `read` components first, then `write`, as the backend sees their columns.
    pub columns: Vec<Access>,
    query: QueryState<Entity>,
    pub last_run_us: u64,
    pub last_error: Option<ScriptError>,
    warned_read_write: bool,
}

/// The live generation's systems, by stage, in run order.
#[derive(Resource, Default)]
pub struct TsSystems {
    pub stages: BTreeMap<StageKey, Vec<TsSystem>>,
    pub generation: u64,
}

/// `Stage` as an ordered map key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StageKey(pub u8);

impl From<Stage> for StageKey {
    fn from(s: Stage) -> Self {
        StageKey(s as u8)
    }
}

impl TsSystems {
    pub fn iter(&self) -> impl Iterator<Item = &TsSystem> {
        self.stages.values().flatten()
    }
}

/// Resolves a component reference for `read`/`write` (`writable` true: TS or native layout) or
/// for `with`/`without` (any registered component).
fn resolve_ref(
    world: &World,
    r: &ComponentRef,
    ts: &TsLookup,
    native: &NativeLayouts,
    data: bool,
) -> Result<(ComponentId, usize), String> {
    match r.source {
        ComponentSource::Ts => ts(&r.name).ok_or_else(|| format!("no component named {}", r.name)),
        ComponentSource::Rust => {
            if let Some(n) = native.get(&r.name) {
                return Ok((n.id, n.layout.size));
            }
            if data {
                return Err(format!(
                    "{} is a Rust component without a native layout; it can be used in with/without but not read/write",
                    r.name
                ));
            }
            rust_component_by_name(world, &r.name).map(|id| (id, 0)).ok_or_else(|| format!("no Rust component named {}", r.name))
        }
    }
}

/// Finds a registered Rust component by type path or by its short name.
pub fn rust_component_by_name(world: &World, name: &str) -> Option<ComponentId> {
    let comps = world.components();
    let mut short = None;
    for (id, info) in comps.iter_registered() {
        let full = info.name().to_string();
        if full == name {
            return Some(id);
        }
        if full.rsplit("::").next() == Some(name) {
            short = Some(id);
        }
    }
    short
}

/// Orders systems by declaration order adjusted by `after`. Errors on unknown targets, targets
/// in another stage and cycles.
fn order(decls: &[(usize, &SystemDecl)]) -> Result<Vec<usize>, ScriptError> {
    let names: HashMap<&str, usize> = decls.iter().enumerate().map(|(i, (_, d))| (d.name.as_str(), i)).collect();
    let n = decls.len();
    let mut indegree = vec![0; n];
    let mut edges = vec![Vec::new(); n];
    for (i, (_, d)) in decls.iter().enumerate() {
        for a in &d.after {
            let j = *names.get(a.as_str()).ok_or_else(|| {
                ScriptError::at(format!("system {} runs after {a}, which is not a system in stage {}", d.name, d.stage.as_str()), d.location.clone())
            })?;
            edges[j].push(i);
            indegree[i] += 1;
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
        return Err(ScriptError::at(format!("cycle in `after` among systems {}", stuck.join(", ")), d.location.clone()));
    }
    Ok(out.into_iter().map(|i| decls[i].0).collect())
}

/// Resolves a TS component name to its id and size in the generation being applied.
pub type TsLookup<'a> = dyn Fn(&str) -> Option<(ComponentId, usize)> + 'a;

/// Builds the systems of a generation. Fails on unknown or unusable components, a component in
/// both `read` and `write`, duplicate names, and bad `after`.
pub fn build(world: &mut World, generation: u64, decls: &[SystemDecl], ts: &TsLookup) -> Result<TsSystems, Vec<ScriptError>> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for d in decls {
        if !seen.insert(d.name.as_str()) {
            errors.push(ScriptError::at(format!("system {} is declared twice", d.name), d.location.clone()));
        }
        for r in &d.read {
            if d.write.contains(r) {
                errors.push(ScriptError::at(
                    format!("system {} lists {} in both read and write; write already allows reading", d.name, r.name),
                    d.location.clone(),
                ));
            }
        }
    }
    let mut out = TsSystems { generation, ..Default::default() };
    for stage in Stage::ALL {
        let in_stage: Vec<(usize, &SystemDecl)> = decls.iter().enumerate().filter(|(_, d)| d.stage == stage).collect();
        match order(&in_stage) {
            Ok(o) => {
                let mut list = Vec::new();
                for index in o {
                    match build_one(world, generation, index, &decls[index], ts) {
                        Ok(s) => list.push(s),
                        Err(e) => errors.push(e),
                    }
                }
                out.stages.insert(stage.into(), list);
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

fn build_one(world: &mut World, generation: u64, index: usize, d: &SystemDecl, ts: &TsLookup) -> Result<TsSystem, ScriptError> {
    let native = world.resource::<NativeLayouts>();
    let err = |m: String| ScriptError::at(format!("system {}: {m}", d.name), d.location.clone());
    let mut columns = Vec::new();
    for (list, write) in [(&d.read, false), (&d.write, true)] {
        for r in list {
            let (id, size) = resolve_ref(world, r, ts, native, true).map_err(err)?;
            columns.push(Access { name: r.name.clone(), id, size, write });
        }
    }
    let mut with = Vec::new();
    for r in &d.with {
        with.push(resolve_ref(world, r, ts, native, false).map_err(err)?.0);
    }
    let mut without = Vec::new();
    for r in &d.without {
        without.push(resolve_ref(world, r, ts, native, false).map_err(err)?.0);
    }
    let mut b = QueryBuilder::<Entity>::new(world);
    for c in &columns {
        if c.write {
            b.mut_id(c.id);
        } else {
            b.ref_id(c.id);
        }
    }
    for id in with {
        b.with_id(id);
    }
    for id in without {
        b.without_id(id);
    }
    let query = b.build();
    Ok(TsSystem {
        key: ScriptKey { generation, index: index as u32 },
        decl: d.clone(),
        columns,
        query,
        last_run_us: 0,
        last_error: None,
        warned_read_write: false,
    })
}

/// A fast 64-bit hash of a column, for the dev-only check that `read` columns stay unwritten.
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

/// Per-stage scratch kept between runs so nothing allocates per frame once warm.
#[derive(Default)]
pub(crate) struct Scratch {
    pub(crate) tables: Vec<TableFrame>,
    pub(crate) entities: Vec<Vec<u32>>,
    pub(crate) arena: Vec<u8>,
}

/// Runs every system of a stage. Called from the stage's exclusive system.
pub fn run_stage(world: &mut World, stage: Stage) {
    if !runtime::stage_should_run(world) {
        return;
    }
    let Some(mut rt) = world.remove_non_send::<ScriptRuntime>() else {
        return;
    };
    let Some(mut systems) = world.remove_resource::<TsSystems>() else {
        world.insert_non_send(rt);
        return;
    };
    let dev = rt.dev;
    if let Some(list) = systems.stages.get_mut(&stage.into()) {
        for sys in list.iter_mut() {
            run_one(world, &mut rt, sys, dev);
        }
    }
    world.insert_resource(systems);
    rt.free_pool(world);
    world.insert_non_send(rt);
}

fn run_one(world: &mut World, rt: &mut ScriptRuntime, sys: &mut TsSystem, dev: bool) {
    let tick = world.increment_change_tick();
    sys.query.update_archetypes(world);
    let frame = runtime::frame_info(world, tick.get());
    let started = Instant::now();

    let mut scratch = std::mem::take(&mut rt.scratch);
    scratch.tables.clear();
    // (table, column index, component id) of every `write` column touched, for summary ticks.
    let mut written: Vec<(bevy::ecs::storage::TableId, ComponentId)> = Vec::new();
    let mut read_hashes: Vec<(usize, usize, u64)> = Vec::new();
    {
        let tables = &world.storages().tables;
        let mut t = 0;
        for table_id in sys.query.matched_tables() {
            let table = tables.get(table_id).expect("matched table exists");
            let len = table.entity_count() as usize;
            if len == 0 {
                continue;
            }
            if scratch.entities.len() <= t {
                scratch.entities.push(Vec::new());
            }
            let ents = &mut scratch.entities[t];
            ents.clear();
            for e in table.entities() {
                let bits = e.to_bits();
                ents.push(bits as u32);
                ents.push((bits >> 32) as u32);
            }
            let mut columns = Vec::with_capacity(sys.columns.len());
            for (ci, c) in sys.columns.iter().enumerate() {
                let column = table.get_column(c.id).expect("matched table has the column");
                // SAFETY: the column holds `len × size` bytes; only the start pointer is taken. Its
                // cells are `UnsafeCell`s and the stage is exclusive, so writing through it is sound.
                let data = unsafe { column.get_data_slice::<u8>(len) }.as_ptr() as *mut u8;
                let ticks = c.write.then(|| {
                    let s = table.get_changed_ticks_slice_for(c.id).expect("column has ticks");
                    s.as_ptr() as *mut u32
                });
                if c.write {
                    written.push((table_id, c.id));
                } else if dev {
                    // SAFETY: as above, `len × size` readable bytes.
                    let bytes = unsafe { std::slice::from_raw_parts(data, len * c.size) };
                    read_hashes.push((t, ci, hash_bytes(bytes)));
                }
                columns.push(ColumnFrame { data, bytes: len * c.size, ticks });
            }
            scratch.tables.push(TableFrame { len, entities: ents.as_ptr(), columns });
            t += 1;
        }
    }

    rt.fill_pool(world);
    scratch.arena.resize(crate::commands::ARENA_BYTES, 0);
    let pool = std::mem::take(&mut rt.pool);
    let result = {
        let mut run = SystemRun { ctx: CallContext { frame, arena: &mut scratch.arena, pool: &pool }, tables: &scratch.tables };
        rt.backend.run_system(sys.key, &mut run)
    };
    rt.pool = pool;

    // Post-run: summary ticks for every `write` column, then whole-table marks from `write(T)`.
    {
        let tables = &world.storages().tables;
        for (table_id, id) in &written {
            if let Some(s) = tables.get(*table_id).and_then(|t| t.get_summary_tick(*id)) {
                s.set(tick);
            }
        }
        if let Ok(out) = &result {
            for &(t, c) in &out.write_marks {
                let (Some(tf), Some(col)) = (scratch.tables.get(t as usize), sys.columns.get(c as usize)) else { continue };
                if let Some(ticks) = tf.columns[c as usize].ticks {
                    // SAFETY: the tick column has `len` entries and the stage is exclusive.
                    unsafe { std::slice::from_raw_parts_mut(ticks, tf.len) }.fill(tick.get());
                } else {
                    let _ = col;
                }
            }
        }
        if dev && !sys.warned_read_write {
            for (t, ci, before) in &read_hashes {
                let cf = &scratch.tables[*t].columns[*ci];
                // SAFETY: the column is unchanged structurally since the run.
                let after = hash_bytes(unsafe { std::slice::from_raw_parts(cf.data, cf.bytes) });
                if after != *before {
                    sys.warned_read_write = true;
                    let name = &sys.columns[*ci].name;
                    rt.log_warn(
                        world,
                        format!("system {} wrote to {name}, which it declared in read; declare it in write", sys.decl.name),
                        sys.decl.location.clone(),
                    );
                    break;
                }
            }
        }
        #[cfg(feature = "track_location")]
        fill_changed_by(world, &scratch.tables, &sys.columns, tick, &written);
    }
    sys.last_run_us = started.elapsed().as_micros() as u64;

    match result {
        Ok(out) => {
            sys.last_error = None;
            let label = format!("system {}", sys.decl.name);
            rt.apply_output(world, &scratch.arena, out, &label);
        }
        Err(e) => {
            rt.log_error(world, &format!("system {} threw", sys.decl.name), &e);
            sys.last_error = Some(e);
        }
    }
    rt.scratch = scratch;
}

#[cfg(feature = "track_location")]
fn fill_changed_by(
    world: &World,
    tables: &[TableFrame],
    columns: &[Access],
    tick: Tick,
    written: &[(bevy::ecs::storage::TableId, ComponentId)],
) {
    static TS_SYSTEM: std::sync::LazyLock<&'static std::panic::Location<'static>> =
        std::sync::LazyLock::new(|| std::panic::Location::caller());
    let storages = &world.storages().tables;
    for (table_id, id) in written {
        let Some(table) = storages.get(*table_id) else { continue };
        let ticks = table.get_changed_ticks_slice_for(*id).unwrap();
        let by = table.get_changed_by_slice_for(*id);
        if let Some(Some(by)) = by.into_option() {
            for (row, t) in ticks.iter().enumerate() {
                // SAFETY: exclusive stage; nothing else touches these cells.
                if unsafe { *t.get() } == tick {
                    unsafe { *by[row].get() = *TS_SYSTEM };
                }
            }
        }
    }
    let _ = (tables, columns);
}

/// A row of `bevy-ts/systems`.
#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    pub name: String,
    pub stage: Stage,
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
        SystemInfo {
            name: self.decl.name.clone(),
            stage: self.decl.stage,
            read: self.decl.read.iter().map(|r| r.name.clone()).collect(),
            write: self.decl.write.iter().map(|r| r.name.clone()).collect(),
            generation: self.key.generation,
            last_run_us: self.last_run_us,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(name: &str, after: &[&str]) -> SystemDecl {
        SystemDecl {
            name: name.into(),
            stage: Stage::Update,
            read: vec![],
            write: vec![],
            with: vec![],
            without: vec![],
            after: after.iter().map(|s| s.to_string()).collect(),
            location: None,
        }
    }

    #[test]
    fn declaration_order_adjusted_by_after() {
        let ds = [sys("a", &["c"]), sys("b", &[]), sys("c", &[])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert_eq!(order(&refs).unwrap(), vec![1, 2, 0]);
    }

    #[test]
    fn cycles_and_unknown_targets_fail() {
        let ds = [sys("a", &["b"]), sys("b", &["a"])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs).unwrap_err().message.contains("cycle"));
        let ds = [sys("a", &["nope"])];
        let refs: Vec<_> = ds.iter().enumerate().collect();
        assert!(order(&refs).unwrap_err().message.contains("nope"));
    }
}
