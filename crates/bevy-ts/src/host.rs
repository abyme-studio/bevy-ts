//! Exported system params (native decision 8) over the sixth seam operation, call host function
//! (N2). A Rust crate exports a `SystemParam` with `#[bevy_ts_native::export]` and
//! `app.ts_param::<P>()`; each `pub fn` becomes a host function JS calls synchronously during a
//! run. The param may write (`ResMut`, `Query<&mut T>`) and may defer (`Commands`, `Gizmos`).
//!
//! Every call fetches the param with `SystemParam::get_param` on the run's `UnsafeWorldCell`, at
//! the calling TS system's own `(last_run, tick)` ([`RunTicks`]): no call spends a world tick, a
//! write carries the run's tick as a TS `Mut` write does, and `is_changed` means "since this TS
//! system last ran". A method sees the system's writes so far; a write it makes in place is
//! seen by the system's next read. Deferred state applies after the run, after TS's own output,
//! in the system's parameter order; a run that throws drops it. One state per exported param is
//! shared by every TS system that takes it, `Local`s included.
//!
//! JS holds raw pointers into columns and resource rows for the run, so structural changes go
//! only through deferred params, a param that writes every component or resource is refused at
//! registration, and a slice argument overlapping a run's column is refused when the slice is
//! `&mut` or the param writes that column.
//!
//! Wire format, engine-neutral: arguments and results are little-endian `f64` slots (scalars,
//! `bool` as 0/1, `Entity` as its bits, vectors as their lanes, records field by field, `Option`
//! as a flag slot then the value). Slices cross in place: up to four views over JS memory.

use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::system::{SystemAccess, SystemMeta, SystemParam, SystemParamItem, SystemState};
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};

/// The type of one argument or result, for codecs, JS glue and TS declarations.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A number: `f32`, `f64`, an integer.
    Num(&'static str),
    Bool,
    Entity,
    Vec2,
    Vec3,
    Quat,
    /// A `NativeLayout` record: its TS name and fields.
    Record(&'static str, Vec<(String, Kind)>),
    Option(Box<Kind>),
    Unit,
}

impl Kind {
    /// Slots the value takes.
    pub fn slots(&self) -> usize {
        match self {
            Kind::Num(_) | Kind::Bool | Kind::Entity => 1,
            Kind::Vec2 => 2,
            Kind::Vec3 => 3,
            Kind::Quat => 4,
            Kind::Record(_, fields) => fields.iter().map(|(_, k)| k.slots()).sum(),
            Kind::Option(k) => 1 + k.slots(),
            Kind::Unit => 0,
        }
    }

    fn ts_in(&self) -> String {
        match self {
            Kind::Num(_) => "number".into(),
            Kind::Bool => "boolean".into(),
            Kind::Entity => "Entity".into(),
            Kind::Vec2 => "{ readonly x: number; readonly y: number } | readonly [number, number]".into(),
            Kind::Vec3 => "{ readonly x: number; readonly y: number; readonly z: number } | readonly [number, number, number]".into(),
            Kind::Quat => "{ readonly x: number; readonly y: number; readonly z: number; readonly w: number } | readonly [number, number, number, number]".into(),
            Kind::Record(name, _) => format!("Partial<{name}>"),
            Kind::Option(k) => format!("{} | null | undefined", k.ts_in()),
            Kind::Unit => "void".into(),
        }
    }

    fn ts_out(&self) -> String {
        match self {
            Kind::Num(_) => "number".into(),
            Kind::Bool => "boolean".into(),
            Kind::Entity => "Entity".into(),
            Kind::Vec2 => "{ readonly x: number; readonly y: number }".into(),
            Kind::Vec3 => "{ readonly x: number; readonly y: number; readonly z: number }".into(),
            Kind::Quat => "{ readonly x: number; readonly y: number; readonly z: number; readonly w: number }".into(),
            Kind::Record(name, _) => name.to_string(),
            Kind::Option(k) => format!("{} | undefined", k.ts_out()),
            Kind::Unit => "void".into(),
        }
    }

    fn records(&self, out: &mut BTreeMap<&'static str, Vec<(String, Kind)>>) {
        match self {
            Kind::Record(name, fields) => {
                out.insert(name, fields.clone());
                fields.iter().for_each(|(_, k)| k.records(out));
            }
            Kind::Option(k) => k.records(out),
            _ => {}
        }
    }
}

/// Reads `f64` slots.
pub struct SlotReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> SlotReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        SlotReader { bytes, at: 0 }
    }

    pub fn slot(&mut self) -> Result<f64, String> {
        let b = self.bytes.get(self.at..self.at + 8).ok_or("too few arguments")?;
        self.at += 8;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }
}

/// Writes `f64` slots into a buffer of fixed capacity.
pub struct SlotWriter<'a> {
    bytes: &'a mut [u8],
    at: usize,
    overflow: bool,
}

impl<'a> SlotWriter<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        SlotWriter { bytes, at: 0, overflow: false }
    }

    pub fn push(&mut self, v: f64) {
        match self.bytes.get_mut(self.at..self.at + 8) {
            Some(b) => b.copy_from_slice(&v.to_le_bytes()),
            None => self.overflow = true,
        }
        self.at += 8;
    }

    /// Slots written, or an error when the result did not fit.
    pub fn finish(self) -> Result<usize, String> {
        if self.overflow { Err("the result is too large for one call; return it through a &mut slice".into()) } else { Ok(self.at / 8) }
    }
}

/// A value that crosses as slots: an argument or a result of an exported method.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot cross to TS as an argument or result of an exported method",
    note = "exported methods take and return numbers, bool, Entity, Vec2, Vec3, Quat, NativeLayout records, Option of these, and &[T] / &mut [T] slices"
)]
pub trait HostValue: Sized {
    fn kind() -> Kind;
    fn read(r: &mut SlotReader) -> Result<Self, String>;
    fn write(&self, w: &mut SlotWriter);
}

macro_rules! numbers {
    ($($t:ty),*) => {
        $(impl HostValue for $t {
            fn kind() -> Kind {
                Kind::Num(stringify!($t))
            }
            fn read(r: &mut SlotReader) -> Result<Self, String> {
                Ok(r.slot()? as $t)
            }
            fn write(&self, w: &mut SlotWriter) {
                w.push(*self as f64);
            }
        })*
    };
}
numbers!(f32, f64, i8, u8, i16, u16, i32, u32);

impl HostValue for bool {
    fn kind() -> Kind {
        Kind::Bool
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(r.slot()? != 0.0)
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(if *self { 1.0 } else { 0.0 });
    }
}

impl HostValue for Entity {
    fn kind() -> Kind {
        Kind::Entity
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let bits = r.slot()? as u64;
        Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(self.to_bits() as f64);
    }
}

impl HostValue for Vec2 {
    fn kind() -> Kind {
        Kind::Vec2
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Vec2::new(r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for Vec3 {
    fn kind() -> Kind {
        Kind::Vec3
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Vec3::new(r.slot()? as f32, r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for Quat {
    fn kind() -> Kind {
        Kind::Quat
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Quat::from_xyzw(r.slot()? as f32, r.slot()? as f32, r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl<T: HostValue> HostValue for Option<T> {
    fn kind() -> Kind {
        Kind::Option(Box::new(T::kind()))
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let present = r.slot()? != 0.0;
        // The value's slots are always there, so a `None` still advances past them.
        let v = T::read(r);
        if present { v.map(Some) } else { Ok(None) }
    }
    fn write(&self, w: &mut SlotWriter) {
        match self {
            Some(v) => {
                w.push(1.0);
                v.write(w);
            }
            None => {
                w.push(0.0);
                for _ in 0..T::kind().slots() {
                    w.push(0.0);
                }
            }
        }
    }
}

impl HostValue for () {
    fn kind() -> Kind {
        Kind::Unit
    }
    fn read(_: &mut SlotReader) -> Result<Self, String> {
        Ok(())
    }
    fn write(&self, _: &mut SlotWriter) {}
}

/// An element type of a slice argument: plain data viewed in place.
///
/// # Safety
/// Every bit pattern of `size_of::<Self>()` bytes must be a valid `Self`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a slice element of an exported method",
    note = "slices hold numbers or NativeLayout records of numbers and vectors"
)]
pub unsafe trait HostElem: Sized + 'static {
    /// The TS view type for a slice of it.
    fn view() -> &'static str;
}

macro_rules! elems {
    ($($t:ty => $v:literal),*) => {
        $(unsafe impl HostElem for $t {
            fn view() -> &'static str {
                $v
            }
        })*
    };
}
elems!(f32 => "Float32Array", f64 => "Float64Array", i8 => "Int8Array", u8 => "Uint8Array", i16 => "Int16Array", u16 => "Uint16Array", i32 => "Int32Array", u32 => "Uint32Array", Vec2 => "Float32Array", Vec3 => "Float32Array", Vec4 => "Float32Array");

fn cast_check<T>(bytes: &[u8], what: &str) -> Result<usize, String> {
    let size = size_of::<T>().max(1);
    if !bytes.len().is_multiple_of(size) {
        return Err(format!("{what}: {} bytes is not a whole number of {size}-byte elements", bytes.len()));
    }
    if !(bytes.as_ptr() as usize).is_multiple_of(align_of::<T>()) {
        return Err(format!("{what}: the array is not aligned to {} bytes", align_of::<T>()));
    }
    Ok(bytes.len() / size)
}

/// Views JS memory as `&[T]` in place.
pub fn slice_ref<'a, T: HostElem>(bytes: &'a [u8], what: &str) -> Result<&'a [T], String> {
    let n = cast_check::<T>(bytes, what)?;
    // SAFETY: length and alignment checked; any bit pattern is a valid T (HostElem).
    Ok(unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const T, n) })
}

/// Views JS memory as `&mut [T]` in place.
pub fn slice_mut<'a, T: HostElem>(bytes: &'a mut [u8], what: &str) -> Result<&'a mut [T], String> {
    let n = cast_check::<T>(bytes, what)?;
    // SAFETY: as above, and the bytes are exclusively borrowed.
    Ok(unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut T, n) })
}

/// The state an exported param keeps, type-erased: an [`Exported`].
pub type ParamState = Box<dyn ExportedState>;

/// What the host does with a param's state without knowing its type.
pub trait ExportedState: Any + Send + Sync {
    /// Applies deferred state (`Commands`, `Gizmos`) to the world.
    fn apply(&mut self, world: &mut World);
    /// Whether the param has deferred state at all.
    fn deferred(&self) -> bool;
    /// The world access the param registers (`init_access`).
    fn access(&self, world: &mut World) -> SystemAccess;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<P: SystemParam + 'static> ExportedState for Exported<P> {
    fn apply(&mut self, world: &mut World) {
        self.state.apply(world);
    }
    fn deferred(&self) -> bool {
        self.meta.has_deferred()
    }
    fn access(&self, world: &mut World) -> SystemAccess {
        let mut meta = self.meta.clone();
        let mut access = SystemAccess::default();
        P::init_access(self.state.param_state(), &mut meta, &mut access, world);
        access
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// An exported param's state: its `SystemState` and a meta whose `last_run` each call sets to
/// the calling TS system's.
pub struct Exported<P: SystemParam + 'static> {
    pub state: SystemState<P>,
    pub meta: SystemMeta,
}

/// The ticks of the run a call belongs to: the TS system's last run and this run's tick.
#[derive(Clone, Copy, Debug)]
pub struct RunTicks {
    pub last_run: Tick,
    pub this_run: Tick,
}

/// A column or resource row the run exposes to JS: its bytes and the component it holds.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub start: usize,
    pub end: usize,
    pub id: ComponentId,
}

/// One method's host function.
pub type HostFn = fn(&mut ParamState, UnsafeWorldCell<'_>, RunTicks, &mut SlotReader, &mut SlotWriter, &mut [&mut [u8]]) -> Result<(), String>;

/// A param's state, for `ParamSpec::init`.
pub fn init<P: SystemParam + 'static>(world: &mut World) -> ParamState {
    let state = SystemState::<P>::new(world);
    let meta = state.meta().clone();
    Box::new(Exported { state, meta })
}

/// Fetches a param for one call at the run's ticks. Spends no world tick.
///
/// # Safety
/// `world` is the world the state was made with, the caller holds it exclusively for the run,
/// nothing else reads or writes the data `P`'s access names for the item's life, and `P`'s
/// access was checked by `init_access` (as [`init`] does).
pub unsafe fn fetch<'w, 's, P: SystemParam + 'static>(state: &'s mut ParamState, world: UnsafeWorldCell<'w>, ticks: RunTicks) -> Result<SystemParamItem<'w, 's, P>, String> {
    let s = state.as_any_mut().downcast_mut::<Exported<P>>().ok_or("exported param state has the wrong type")?;
    if !s.state.matches_world(world.id()) {
        return Err("exported param state belongs to another world".into());
    }
    s.meta.set_last_run(ticks.last_run);
    // SAFETY: the caller's contract; the state is only borrowed for this call, as a `Mut` the
    // param holds; the param state is only handed to `P` itself.
    unsafe { P::get_param(s.state.param_state_mut(), &s.meta, world, ticks.this_run) }.map_err(|e| e.to_string())
}

/// A slice argument: name, TS view type, whether Rust writes it.
#[derive(Clone, Debug)]
pub struct SliceSpec {
    pub name: &'static str,
    pub view: &'static str,
    pub mutable: bool,
}

/// One argument in declaration order.
#[derive(Clone, Debug)]
pub enum ArgSpec {
    Value(&'static str, Kind),
    Slice(SliceSpec),
}

#[derive(Clone)]
pub struct MethodSpec {
    /// The TS name (`castRay`).
    pub name: String,
    pub args: Vec<ArgSpec>,
    pub ret: Kind,
    pub call: HostFn,
}

/// What `#[export]` generates for a param.
#[derive(Clone)]
pub struct ParamSpec {
    /// The TS type (`ReadRapierContext`).
    pub ts: &'static str,
    /// The built-in module that declares it (`bevy-ts/rapier`).
    pub module: &'static str,
    pub rust: &'static str,
    /// The Bevy spelling the export stands for (`ResMut<Time<Virtual>>`), normalised: a TS
    /// param written that way is this export.
    pub stands_for: Option<&'static str>,
    pub init: fn(&mut World) -> ParamState,
    pub methods: Vec<MethodSpec>,
}

/// The JS slot capacity for one call's arguments and results.
pub const SLOTS: usize = 512;
/// At most this many slice arguments per method.
pub const MAX_SLICES: usize = 4;

struct Registered {
    spec: ParamSpec,
    state: ParamState,
    /// Host function id of the first method.
    first: u32,
    /// Components and resources the param may write, sorted.
    writes: Vec<ComponentId>,
    /// Called since its deferred state was last applied or dropped.
    dirty: bool,
}

/// Every exported param, with its state and host function ids.
#[derive(Resource, Default)]
pub struct HostParams {
    params: Vec<Registered>,
    /// Host function id to (param, method).
    fns: Vec<(usize, usize)>,
    /// Calls from JS into Rust so far, for tests and stats.
    pub calls: u64,
}

/// Param kind names every engine in this process exports, for the compile step.
static KINDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
/// Claimed spellings to the TS name of the export claiming them.
static CLAIMS: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();

/// The exported param type names the compile step reads as system param kinds.
pub fn param_kinds() -> Vec<String> {
    KINDS.get().map(|k| k.lock().unwrap().clone()).unwrap_or_default()
}

/// Claimed Bevy spellings (`ResMut<Time<Virtual>>`) and the TS name of the export claiming each.
pub fn claims() -> Vec<(String, String)> {
    CLAIMS.get().map(|k| k.lock().unwrap().clone()).unwrap_or_default()
}

/// A Bevy param spelling, normalised: no whitespace, `Res`/`ResMut` and generic arguments by
/// their last path segment (`bevy::time::Time<bevy::time::Virtual>` is `Time<Virtual>`).
pub fn normalise_spelling(s: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(""));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(""));
    out
}

/// The components and resources `access` may write, or why it is refused.
fn bounded_writes(rust: &str, access: &SystemAccess) -> Result<Vec<ComponentId>, String> {
    let refuse = || {
        format!(
            "bevy-ts: {rust} cannot be exported to TS: it may write every component or resource (EntityMut, EntityMutExcept, DeferredWorld); name the components and resources it writes"
        )
    };
    match access {
        SystemAccess::None => Ok(Vec::new()),
        SystemAccess::Exclusive => Err(refuse()),
        SystemAccess::Shared(set) => {
            let mut out: Vec<ComponentId> = set.combined_access().writes().as_finite_set().ok_or_else(refuse)?.iter().collect();
            for f in set.filtered_accesses() {
                out.extend(f.access().writes().as_finite_set().ok_or_else(refuse)?.iter());
            }
            out.sort();
            out.dedup();
            Ok(out)
        }
    }
}

impl HostParams {
    /// Registers a param. Fails on a duplicate TS name, a second claim of a spelling, or a param
    /// that may write every component or resource.
    pub fn register(&mut self, world: &mut World, spec: ParamSpec) -> Result<(), String> {
        if let Some(prev) = self.params.iter().find(|p| p.spec.ts == spec.ts) {
            return Err(format!("bevy-ts: {} is exported twice ({} and {})", spec.ts, prev.spec.rust, spec.rust));
        }
        if let Some(claim) = spec.stands_for
            && let Some(prev) = self.params.iter().find(|p| p.spec.stands_for == Some(claim))
        {
            return Err(format!("bevy-ts: {claim} is claimed by two exports ({} and {}); one export per spelling", prev.spec.rust, spec.rust));
        }
        assert!(
            spec.methods.iter().all(|m| m.args.iter().filter(|a| matches!(a, ArgSpec::Slice(_))).count() <= MAX_SLICES),
            "{}: at most {MAX_SLICES} slice arguments per method",
            spec.ts
        );
        let state = (spec.init)(world);
        let writes = bounded_writes(spec.rust, &state.access(world))?;
        let first = self.fns.len() as u32;
        let index = self.params.len();
        for m in 0..spec.methods.len() {
            self.fns.push((index, m));
        }
        let kinds = KINDS.get_or_init(Default::default);
        let mut k = kinds.lock().unwrap();
        if !k.iter().any(|n| n == spec.ts) {
            k.push(spec.ts.to_string());
        }
        if let Some(claim) = spec.stands_for {
            let mut c = CLAIMS.get_or_init(Default::default).lock().unwrap();
            c.retain(|(s, _)| s != claim);
            c.push((claim.to_string(), spec.ts.to_string()));
        }
        self.params.push(Registered { spec, state, first, writes, dirty: false });
        Ok(())
    }

    pub fn get(&self, ts: &str) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.spec.ts == ts).map(|p| &p.spec)
    }

    /// The index of the param TS names `ts`.
    pub fn index(&self, ts: &str) -> Option<usize> {
        self.params.iter().position(|p| p.spec.ts == ts)
    }

    /// The TS name of the export claiming `spelling`.
    pub fn claimed(&self, spelling: &str) -> Option<&'static str> {
        let n = normalise_spelling(spelling);
        self.params.iter().find(|p| p.spec.stands_for == Some(n.as_str())).map(|p| p.spec.ts)
    }

    pub fn specs(&self) -> impl Iterator<Item = &ParamSpec> {
        self.params.iter().map(|p| &p.spec)
    }

    /// Components and resources param `index` may write.
    pub fn writes(&self, index: usize) -> &[ComponentId] {
        &self.params[index].writes
    }

    /// Whether param `index` has deferred state (`Commands`, `Gizmos`).
    pub fn deferred(&self, index: usize) -> bool {
        self.params[index].state.deferred()
    }

    /// Param `index`'s TS name and the world access it registers, which the TS systems taking it
    /// declare.
    pub fn access(&self, index: usize, world: &mut World) -> Option<(&'static str, SystemAccess)> {
        let reg = self.params.get(index)?;
        Some((reg.spec.ts, reg.state.access(world)))
    }

    /// Applies param `index`'s deferred state if a call since the last apply queued any.
    pub fn apply(&mut self, index: usize, world: &mut World) {
        let reg = &mut self.params[index];
        if std::mem::take(&mut reg.dirty) && reg.state.deferred() {
            reg.state.apply(world);
        }
    }

    /// Drops param `index`'s deferred state from a failed run by rebuilding its state, which
    /// also resets its `Local`s and query cache.
    pub fn discard(&mut self, index: usize, world: &mut World) {
        let reg = &mut self.params[index];
        if std::mem::take(&mut reg.dirty) && reg.state.deferred() {
            reg.state = (reg.spec.init)(world);
        }
    }

    /// Applies, or on `ok == false` drops, every called param's deferred state, in
    /// registration order (an entry call, which has no parameter order).
    pub fn settle_all(&mut self, world: &mut World, ok: bool) {
        for i in 0..self.params.len() {
            if ok { self.apply(i, world) } else { self.discard(i, world) }
        }
    }

    /// Calls host function `id` at the run's ticks. A panic in the method becomes an error
    /// naming it; a slice overlapping `regions` that the call could alias is refused, and so is a
    /// param the running system does not take (`allowed`, by index; `None` for an entry, which
    /// may call any).
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        world: UnsafeWorldCell<'_>,
        ticks: RunTicks,
        regions: &[Region],
        allowed: Option<&[usize]>,
        id: u32,
        args: &[u8],
        ret: &mut [u8],
        slices: &mut [&mut [u8]],
    ) -> Result<usize, String> {
        self.calls += 1;
        let &(p, m) = self.fns.get(id as usize).ok_or_else(|| format!("no host function {id}"))?;
        let reg = &mut self.params[p];
        let method = &reg.spec.methods[m];
        let (ts, call) = (reg.spec.ts, method.call);
        // A param instance kept from another system's run would skip this run's overlap check
        // and settle with the wrong system.
        if allowed.is_some_and(|a| !a.contains(&p)) {
            return Err(format!("{ts}.{}: this system does not take {ts}; take it as a parameter of the system that calls it", method.name));
        }
        if !regions.is_empty() {
            let specs = method.args.iter().filter_map(|a| match a {
                ArgSpec::Slice(s) => Some(s),
                ArgSpec::Value(..) => None,
            });
            for (spec, bytes) in specs.zip(slices.iter()) {
                let (start, end) = (bytes.as_ptr() as usize, bytes.as_ptr() as usize + bytes.len());
                if start == end {
                    continue;
                }
                if let Some(r) = regions.iter().find(|r| start < r.end && r.start < end && (spec.mutable || reg.writes.binary_search(&r.id).is_ok())) {
                    let why = if spec.mutable { "it is a &mut slice" } else { "the param may write that column" };
                    return Err(format!("{ts}.{}: argument {} views a column or resource row of this run ({:?}), and {why}; copy it first", method.name, spec.name, r.id));
                }
            }
        }
        reg.dirty = true;
        let state = &mut reg.state;
        let mut reader = SlotReader::new(args);
        let mut writer = SlotWriter::new(ret);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(state, world, ticks, &mut reader, &mut writer, slices)));
        let name = || reg.spec.methods[m].name.as_str();
        match r {
            Ok(Ok(())) => writer.finish().map_err(|e| format!("{ts}.{}: {e}", name())),
            Ok(Err(e)) => Err(format!("{ts}.{}: {e}", name())),
            Err(p) => {
                let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                Err(format!("{ts}.{} panicked: {msg}", name()))
            }
        }
    }

    /// The JS glue for every param, by TS name: a function of the host object returning the
    /// param's class.
    pub fn glue(&self) -> BTreeMap<String, String> {
        self.params.iter().map(|r| (r.spec.ts.to_string(), glue(&r.spec, r.first))).collect()
    }

    /// TS declarations of every param, grouped by module.
    pub fn declarations(&self) -> BTreeMap<&'static str, String> {
        let mut by_module: BTreeMap<&'static str, String> = BTreeMap::new();
        for r in &self.params {
            by_module.entry(r.spec.module).or_default().push_str(&declaration(&r.spec));
        }
        by_module
    }
}

/// The engine-side end of the seam during a run: the run's world, held exclusively for the run,
/// its ticks, the columns it exposes to JS, and the params.
pub struct WorldHost<'a> {
    pub world: UnsafeWorldCell<'a>,
    pub ticks: RunTicks,
    pub regions: &'a [Region],
    /// The params the running system takes, by index; `None` for an entry call.
    pub allowed: Option<&'a [usize]>,
    pub params: &'a mut HostParams,
}

impl crate::backend::HostCalls for WorldHost<'_> {
    fn call(&mut self, id: u32, args: &[u8], ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String> {
        self.params.call(self.world, self.ticks, self.regions, self.allowed, id, args, ret, slices)
    }
}

fn write_js(kind: &Kind, expr: &str, out: &mut String) {
    match kind {
        Kind::Num(_) => {
            let _ = write!(out, "a[i++] = +{expr};");
        }
        Kind::Bool => {
            let _ = write!(out, "a[i++] = {expr} ? 1 : 0;");
        }
        Kind::Entity => {
            let _ = write!(out, "a[i++] = Number({expr});");
        }
        Kind::Vec2 | Kind::Vec3 | Kind::Quat => {
            let n = kind.slots();
            let lanes = ["x", "y", "z", "w"];
            let _ = write!(out, "{{ const v = {expr}; if (Array.isArray(v)) {{ ");
            for i in 0..n {
                let _ = write!(out, "a[i++] = v[{i}];");
            }
            out.push_str(" } else { ");
            for l in &lanes[..n] {
                let _ = write!(out, "a[i++] = v.{l};");
            }
            out.push_str(" } }");
        }
        Kind::Record(_, fields) => {
            let _ = write!(out, "{{ const r = {expr} || {{}}; ");
            for (f, k) in fields {
                // A missing optional field is `None`; any other missing field is zero.
                let e = if matches!(k, Kind::Option(_)) { format!("r[{f:?}]") } else { format!("(r[{f:?}] ?? 0)") };
                write_js(k, &e, out);
            }
            out.push('}');
        }
        Kind::Option(k) => {
            let _ = write!(out, "{{ const o = {expr}; if (o === null || o === undefined) {{ a[i++] = 0; i += {}; }} else {{ a[i++] = 1; ", k.slots());
            write_js(k, "o", out);
            out.push_str(" } }");
        }
        Kind::Unit => {}
    }
}

/// JS that reads a result from `a` at `i` (advancing it) into a fresh or reused value.
fn read_js(kind: &Kind) -> String {
    match kind {
        Kind::Num(_) => "a[i++]".into(),
        Kind::Bool => "a[i++] !== 0".into(),
        Kind::Entity => "a[i++]".into(),
        Kind::Vec2 => "({ x: a[i++], y: a[i++] })".into(),
        Kind::Vec3 => "({ x: a[i++], y: a[i++], z: a[i++] })".into(),
        Kind::Quat => "({ x: a[i++], y: a[i++], z: a[i++], w: a[i++] })".into(),
        Kind::Record(_, fields) => {
            let parts: Vec<String> = fields.iter().map(|(f, k)| format!("{f:?}: {}", read_js(k))).collect();
            format!("({{ {} }})", parts.join(", "))
        }
        Kind::Option(k) => format!("(a[i++] !== 0 ? {} : (i += {}, undefined))", read_js(k), k.slots()),
        Kind::Unit => "undefined".into(),
    }
}

/// The JS class for one param: `H` is the host object (`call`, `error`, the slot scratch).
fn glue(spec: &ParamSpec, first: u32) -> String {
    let mut s = format!("const NAME = {:?};\nreturn class {} {{\n", spec.ts, spec.ts);
    for (m, method) in spec.methods.iter().enumerate() {
        let id = first as usize + m;
        let names: Vec<String> = method
            .args
            .iter()
            .map(|a| match a {
                ArgSpec::Value(n, _) => n.to_string(),
                ArgSpec::Slice(sl) => sl.name.to_string(),
            })
            .collect();
        let _ = writeln!(s, "  {}({}) {{", method.name, names.join(", "));
        s.push_str("    const a = H.slots; let i = 0;\n    ");
        let mut slices = Vec::new();
        for arg in &method.args {
            match arg {
                ArgSpec::Value(n, k) => write_js(k, n, &mut s),
                ArgSpec::Slice(sl) => slices.push(format!("H.bytes({}, {:?})", sl.name, sl.name)),
            }
        }
        while slices.len() < MAX_SLICES {
            slices.push("H.EMPTY".into());
        }
        let _ = writeln!(s, "\n    const n = H.call({id}, i, {});", slices.join(", "));
        let _ = writeln!(s, "    if (n < 0) H.fail(NAME, {:?});", method.name);
        let _ = writeln!(s, "    i = 0; const r = H.result;\n    {{ const a = r; return {}; }}\n  }}", read_js(&method.ret));
    }
    s.push_str("};\n");
    s
}

fn camel_records(spec: &ParamSpec) -> BTreeMap<&'static str, Vec<(String, Kind)>> {
    let mut out = BTreeMap::new();
    for m in &spec.methods {
        m.ret.records(&mut out);
        for a in &m.args {
            if let ArgSpec::Value(_, k) = a {
                k.records(&mut out);
            }
        }
    }
    out
}

/// The TS declaration of a param and the records its methods use.
fn declaration(spec: &ParamSpec) -> String {
    let mut s = String::new();
    for (name, fields) in camel_records(spec) {
        let _ = writeln!(s, "export interface {name} {{");
        for (f, k) in &fields {
            let _ = writeln!(s, "  readonly {f}: {};", k.ts_out());
        }
        s.push_str("}\n");
    }
    let _ = writeln!(s, "/** `{}`, a system param exported from Rust; take it as a system parameter. */", spec.rust);
    let _ = writeln!(s, "export interface {} {{", spec.ts);
    for m in &spec.methods {
        let args: Vec<String> = m
            .args
            .iter()
            .map(|a| match a {
                ArgSpec::Value(n, k) => format!("{n}: {}", k.ts_in()),
                ArgSpec::Slice(sl) => format!("{}: {}", sl.name, sl.view),
            })
            .collect();
        let _ = writeln!(s, "  {}({}): {};", m.name, args.join(", "), m.ret.ts_out());
    }
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_round_trip_with_options() {
        let mut buf = vec![0u8; 8 * 16];
        let mut w = SlotWriter::new(&mut buf);
        Some(Vec3::new(1.0, 2.0, 3.0)).write(&mut w);
        None::<Vec3>.write(&mut w);
        Entity::from_bits(42).write(&mut w);
        assert_eq!(w.finish().unwrap(), 4 + 4 + 1);
        let mut r = SlotReader::new(&buf);
        assert_eq!(Option::<Vec3>::read(&mut r).unwrap(), Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(Option::<Vec3>::read(&mut r).unwrap(), None);
        assert_eq!(Entity::read(&mut r).unwrap(), Entity::from_bits(42));
    }

    #[test]
    fn a_full_writer_reports_instead_of_writing_past_the_end() {
        let mut buf = vec![0u8; 8];
        let mut w = SlotWriter::new(&mut buf);
        Vec3::ONE.write(&mut w);
        assert!(w.finish().is_err());
    }

    #[test]
    fn spellings_normalise() {
        assert_eq!(normalise_spelling("ResMut< bevy::time::Time<bevy::time::Virtual> >"), "ResMut<Time<Virtual>>");
    }

    #[test]
    fn slices_check_length_and_alignment() {
        let v = [1.0f32, 2.0, 3.0];
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, 12) };
        assert_eq!(slice_ref::<f32>(bytes, "xs").unwrap(), &[1.0, 2.0, 3.0]);
        assert!(slice_ref::<f32>(&bytes[1..], "xs").is_err());
    }
}
