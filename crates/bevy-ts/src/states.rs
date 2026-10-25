//! TS state machines as Bevy states (ecs-schedules stage 2). Each machine takes one of 32 slot
//! types, [`TsState<N>`], the first time a generation declares it, by path, for the life of the
//! app. A slot is registered with Bevy's own state machinery at run time, when its machine first
//! gets a value, so an unused slot costs nothing per frame. Bevy drives transitions: TS
//! `OnEnter`/`OnExit`/`OnTransition` systems run from an exclusive system bevy-ts adds to Bevy's
//! schedules of those names, so Rust and TS transition systems share one order.
//!
//! A value is its name, interned for the life of the process, so reordering or adding values on
//! reload keeps every stored value valid.

use crate::messages::TsPayloads;
use crate::systems::ListKey;
use bevy::ecs::component::ComponentId;
use bevy::ecs::message::MessageRegistry;
use bevy::prelude::*;
use bevy::state::state::{FreelyMutableState, StateTransition, StateTransitionEvent, StateTransitionSystems};
use bevy::state::state_scoped::{
    DespawnOnEnter, DespawnOnExit, DisableOnEnter, DisableOnExit, EnableOnEnter, EnableOnExit, despawn_entities_on_enter_state, despawn_entities_on_exit_state, despawn_entities_when_state, disable_entities_on_enter_state,
    disable_entities_on_exit_state, disable_entities_when_state, enable_entities_on_enter_state, enable_entities_on_exit_state,
    enable_entities_when_state,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

/// How many TS state machines an app can have: the pool's size.
pub const SLOTS: usize = 32;

/// Value names by symbol, and symbols by name.
type Names = (Vec<&'static str>, Option<HashMap<&'static str, u32>>);

static NAMES: Mutex<Names> = Mutex::new((Vec::new(), None));

/// The symbol of a value name, interned for the life of the process.
pub fn sym(name: &str) -> u32 {
    let mut names = NAMES.lock().unwrap();
    let (list, map) = &mut *names;
    let map = map.get_or_insert_with(HashMap::new);
    if let Some(s) = map.get(name) {
        return *s;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    list.push(leaked);
    let s = (list.len() - 1) as u32;
    map.insert(leaked, s);
    s
}

/// The value name of a symbol.
pub fn name(sym: u32) -> &'static str {
    NAMES.lock().unwrap().0.get(sym as usize).copied().unwrap_or("?")
}

/// One slot of the pool: a TS machine's value, an interned name ([`sym`], [`name`]). Rust finds a
/// machine's slot with [`TsStates::slot`] and reads `Res<State<TsState<N>>>` as for any state.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TsState<const N: u8>(pub u32);

impl<const N: u8> std::fmt::Debug for TsState<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TsState<{N}>({})", name(self.0))
    }
}

impl<const N: u8> States for TsState<N> {}
impl<const N: u8> FreelyMutableState for TsState<N> {}

/// Where a TS transition list runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunKind {
    Enter,
    Exit,
    Transition,
}

/// What bevy-ts does to one slot, whatever its `N`, or to a Rust state TS names.
#[derive(Clone, Copy)]
pub struct SlotOps {
    register: fn(&mut World, u32),
    unregister: fn(&mut World),
    current: fn(&World) -> Option<u32>,
    previous: fn(&World) -> Option<u32>,
    set_next: fn(&mut World, u32),
    take_next: fn(&mut World) -> Option<u32>,
    state_id: fn(&World) -> Option<ComponentId>,
    add_runner: fn(&mut World, u8, RunKind, u32, u32),
    insert_scoped: fn(&mut World, Entity, u8, u32),
    remove_scoped: fn(&mut World, Entity, u8),
    make: fn(&mut World),
    set_depth: fn(&mut World, u8),
    set_derived: fn(&mut World, Option<u32>),
}

/// Where a machine's exit, transition and enter work runs, by its depth (a root is 1, a sub or
/// computed state one more than its deepest source): exits deepest first, the rest shallowest
/// first, as Bevy orders a state by its `DEPENDENCY_DEPTH`.
#[derive(bevy::ecs::schedule::SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TsDepth {
    Exit(u8),
    Transition(u8),
    Enter(u8),
}

/// The deepest chain of sub and computed states the depth sets order.
pub const MAX_DEPTH: u8 = 8;

/// Orders the depth sets inside Bevy's transition sets; called once by the plugin.
pub(crate) fn configure_depths(schedule: &mut Schedule) {
    for d in 1..=MAX_DEPTH {
        schedule.configure_sets((
            TsDepth::Exit(d).in_set(StateTransitionSystems::ExitSchedules),
            TsDepth::Transition(d).in_set(StateTransitionSystems::TransitionSchedules),
            TsDepth::Enter(d).in_set(StateTransitionSystems::EnterSchedules),
        ));
        if d > 1 {
            schedule.configure_sets((
                TsDepth::Exit(d).before(TsDepth::Exit(d - 1)),
                TsDepth::Transition(d - 1).before(TsDepth::Transition(d)),
                TsDepth::Enter(d - 1).before(TsDepth::Enter(d)),
            ));
        }
    }
}

fn set_depth<const N: u8>(world: &mut World, depth: u8) {
    let d = depth.clamp(1, MAX_DEPTH);
    world.resource_mut::<Schedules>().entry(StateTransition).configure_sets((
        bevy::state::state::ExitSchedules::<TsState<N>>::default().in_set(TsDepth::Exit(d)),
        bevy::state::state::TransitionSchedules::<TsState<N>>::default().in_set(TsDepth::Transition(d)),
        bevy::state::state::EnterSchedules::<TsState<N>>::default().in_set(TsDepth::Enter(d)),
    ));
}

/// Sets a sub or computed state's value (`None`: it does not exist), as Bevy applies one: the
/// `State` resource and a `StateTransitionEvent`, which Bevy's exit and enter systems then run.
/// `PreviousState` is not kept for these: Bevy offers no way to write it.
fn set_derived<const N: u8>(world: &mut World, value: Option<u32>) {
    let old = current::<N>(world);
    if old == value {
        return;
    }
    match value {
        Some(v) => {
            world.insert_resource(State::new(TsState::<N>(v)));
            if !world.contains_resource::<NextState<TsState<N>>>() {
                world.insert_resource(NextState::<TsState<N>>::Unchanged);
            }
        }
        None => drop(world.remove_resource::<State<TsState<N>>>()),
    }
    world.remove_resource::<PreviousState<TsState<N>>>();
    world.write_message(StateTransitionEvent { exited: old.map(TsState::<N>), entered: value.map(TsState::<N>), allow_same_state_transitions: false });
}

/// Bevy's six state-scoped components, by name, in the order of their kind number.
pub const SCOPED: [&str; 6] = ["DespawnOnExit", "DespawnOnEnter", "DisableOnExit", "DisableOnEnter", "EnableOnExit", "EnableOnEnter"];

/// The kind number of a state-scoped component's name.
pub fn scoped_kind(name: &str) -> Option<u8> {
    SCOPED.iter().position(|n| *n == name).map(|i| i as u8)
}

fn p_insert_scoped<const N: u8>(world: &mut World, e: Entity, kind: u8, value: u32) {
    let Ok(mut e) = world.get_entity_mut(e) else { return };
    let s = TsState::<N>(value);
    match kind {
        0 => drop(e.insert(DespawnOnExit(s))),
        1 => drop(e.insert(DespawnOnEnter(s))),
        2 => drop(e.insert(DisableOnExit(s))),
        3 => drop(e.insert(DisableOnEnter(s))),
        4 => drop(e.insert(EnableOnExit(s))),
        _ => drop(e.insert(EnableOnEnter(s))),
    }
}

fn p_remove_scoped<const N: u8>(world: &mut World, e: Entity, kind: u8) {
    let Ok(mut e) = world.get_entity_mut(e) else { return };
    match kind {
        0 => drop(e.remove::<DespawnOnExit<TsState<N>>>()),
        1 => drop(e.remove::<DespawnOnEnter<TsState<N>>>()),
        2 => drop(e.remove::<DisableOnExit<TsState<N>>>()),
        3 => drop(e.remove::<DisableOnEnter<TsState<N>>>()),
        4 => drop(e.remove::<EnableOnExit<TsState<N>>>()),
        _ => drop(e.remove::<EnableOnEnter<TsState<N>>>()),
    }
}

/// Registers slot `N` with Bevy's state machinery, once: its message, its transition systems and
/// the state-scoped systems. It has no value yet.
fn make<const N: u8>(world: &mut World) {
    if world.get_resource_or_init::<TsStates>().made.insert(N) {
        MessageRegistry::register_message::<StateTransitionEvent<TsState<N>>>(world);
        let mut schedules = world.resource_mut::<Schedules>();
        let schedule = schedules.entry(StateTransition);
        TsState::<N>::register_state(schedule);
        schedule
            .add_systems(
                (despawn_entities_on_exit_state::<TsState<N>>, disable_entities_on_exit_state::<TsState<N>>, enable_entities_on_exit_state::<TsState<N>>)
                    .in_set(StateTransitionSystems::ExitSchedules),
            )
            .add_systems(
                (
                    despawn_entities_on_enter_state::<TsState<N>>,
                    disable_entities_on_enter_state::<TsState<N>>,
                    enable_entities_on_enter_state::<TsState<N>>,
                )
                    .in_set(StateTransitionSystems::EnterSchedules),
            )
            .add_systems(
                (despawn_entities_when_state::<TsState<N>>, disable_entities_when_state::<TsState<N>>, enable_entities_when_state::<TsState<N>>)
                    .in_set(StateTransitionSystems::TransitionSchedules),
            );
    }
}

fn register<const N: u8>(world: &mut World, initial: u32) {
    make::<N>(world);
    world.insert_resource(State::new(TsState::<N>(initial)));
    world.insert_resource(NextState::<TsState<N>>::Unchanged);
    // As `init_state` does: the first `StateTransition` runs `OnEnter(initial)`.
    world.write_message(StateTransitionEvent { exited: None, entered: Some(TsState::<N>(initial)), allow_same_state_transitions: true });
}

fn unregister<const N: u8>(world: &mut World) {
    world.remove_resource::<State<TsState<N>>>();
    world.remove_resource::<NextState<TsState<N>>>();
    world.remove_resource::<PreviousState<TsState<N>>>();
}

fn current<const N: u8>(world: &World) -> Option<u32> {
    world.get_resource::<State<TsState<N>>>().map(|s| s.get().0)
}

fn previous<const N: u8>(world: &World) -> Option<u32> {
    world.get_resource::<PreviousState<TsState<N>>>().map(|s| s.get().0)
}

fn set_next<const N: u8>(world: &mut World, value: u32) {
    if let Some(mut next) = world.get_resource_mut::<NextState<TsState<N>>>() {
        next.set(TsState(value));
    }
}

fn take_next<const N: u8>(world: &mut World) -> Option<u32> {
    let mut next = world.get_resource_mut::<NextState<TsState<N>>>()?;
    let v = match &*next {
        NextState::Pending(s) | NextState::PendingIfDifferent(s) => Some(s.0),
        NextState::Unchanged => None,
    };
    if v.is_some() {
        next.reset();
    }
    v
}

fn state_id<const N: u8>(world: &World) -> Option<ComponentId> {
    world.components().component_id::<State<TsState<N>>>()
}

fn add_runner<const N: u8>(world: &mut World, _slot: u8, kind: RunKind, a: u32, b: u32) {
    let slot = N as usize;
    let mut schedules = world.resource_mut::<Schedules>();
    match kind {
        RunKind::Enter => {
            schedules.add_systems(OnEnter(TsState::<N>(a)), move |w: &mut World| crate::systems::run_state_list(w, ListKey::OnEnter(slot, a)));
        }
        RunKind::Exit => {
            schedules.add_systems(OnExit(TsState::<N>(a)), move |w: &mut World| crate::systems::run_state_list(w, ListKey::OnExit(slot, a)));
        }
        RunKind::Transition => {
            schedules.add_systems(OnTransition { exited: TsState::<N>(a), entered: TsState::<N>(b) }, move |w: &mut World| {
                crate::systems::run_state_list(w, ListKey::OnTransition(slot, a, b))
            });
        }
    }
}

macro_rules! ops {
    ($($n:literal)*) => {
        [$(SlotOps {
            register: register::<$n>,
            unregister: unregister::<$n>,
            current: current::<$n>,
            previous: previous::<$n>,
            set_next: set_next::<$n>,
            take_next: take_next::<$n>,
            state_id: state_id::<$n>,
            add_runner: add_runner::<$n>,
            insert_scoped: p_insert_scoped::<$n>,
            remove_scoped: p_remove_scoped::<$n>,
            make: make::<$n>,
            set_depth: set_depth::<$n>,
            set_derived: set_derived::<$n>,
        }),*]
    };
}

static OPS: [SlotOps; SLOTS] = ops!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31);

/// The ops of a slot: the pool's, or a Rust state's (slots from [`SLOTS`] up).
fn ops_of(world: &World, slot: u8) -> SlotOps {
    match (slot as usize).checked_sub(SLOTS) {
        None => OPS[slot as usize],
        Some(i) => world.resource::<RustStates>().0[i].ops,
    }
}

/// A Rust state TS names (`app.ts_state::<S>()`): its type path, short name and unit variants.
#[derive(Clone)]
pub struct RustState {
    pub path: String,
    pub short: String,
    pub values: Vec<String>,
    ops: SlotOps,
}

/// Rust states TS can name, in registration order: the `n`th takes slot `SLOTS + n`.
#[derive(Resource, Default, Clone)]
pub struct RustStates(pub Vec<RustState>);

fn variant<S: Reflect>(s: &S) -> Option<&str> {
    match s.reflect_ref() {
        bevy::reflect::ReflectRef::Enum(e) => Some(e.variant_name()),
        _ => None,
    }
}

fn from_sym<S: FromReflect>(sym: u32) -> Option<S> {
    let value = bevy::reflect::enums::DynamicEnum::new(name(sym), bevy::reflect::enums::DynamicVariant::Unit);
    S::from_reflect(&value)
}

fn r_none(_: &mut World) {}
fn r_none_value(_: &mut World, _: u32) {}
fn r_none_depth(_: &mut World, _: u8) {}
fn r_none_derived(_: &mut World, _: Option<u32>) {}
fn r_take_next(_: &mut World) -> Option<u32> {
    None
}
fn r_current<S: FreelyMutableState + Reflect>(world: &World) -> Option<u32> {
    world.get_resource::<State<S>>().and_then(|s| variant(s.get())).map(sym)
}
fn r_previous<S: FreelyMutableState + Reflect>(world: &World) -> Option<u32> {
    world.get_resource::<PreviousState<S>>().and_then(|s| variant(s.get())).map(sym)
}
fn r_set_next<S: FreelyMutableState + FromReflect>(world: &mut World, value: u32) {
    if let (Some(v), Some(mut next)) = (from_sym::<S>(value), world.get_resource_mut::<NextState<S>>()) {
        next.set(v);
    }
}
fn r_state_id<S: FreelyMutableState>(world: &World) -> Option<ComponentId> {
    world.components().component_id::<State<S>>()
}
fn r_add_runner<S: FreelyMutableState + FromReflect>(world: &mut World, slot: u8, kind: RunKind, a: u32, b: u32) {
    let (Some(va), vb) = (from_sym::<S>(a), from_sym::<S>(b)) else { return };
    let slot_i = slot as usize;
    let mut schedules = world.resource_mut::<Schedules>();
    match kind {
        RunKind::Enter => drop(schedules.add_systems(OnEnter(va), move |w: &mut World| crate::systems::run_state_list(w, ListKey::OnEnter(slot_i, a)))),
        RunKind::Exit => drop(schedules.add_systems(OnExit(va), move |w: &mut World| crate::systems::run_state_list(w, ListKey::OnExit(slot_i, a)))),
        RunKind::Transition => {
            if let Some(vb) = vb {
                schedules.add_systems(OnTransition { exited: va, entered: vb }, move |w: &mut World| {
                    crate::systems::run_state_list(w, ListKey::OnTransition(slot_i, a, b))
                });
            }
        }
    }
}
fn r_insert_scoped<S: FreelyMutableState + FromReflect>(world: &mut World, e: Entity, kind: u8, value: u32) {
    let (Some(s), Ok(mut e)) = (from_sym::<S>(value), world.get_entity_mut(e)) else { return };
    match kind {
        0 => drop(e.insert(DespawnOnExit(s))),
        1 => drop(e.insert(DespawnOnEnter(s))),
        2 => drop(e.insert(DisableOnExit(s))),
        3 => drop(e.insert(DisableOnEnter(s))),
        4 => drop(e.insert(EnableOnExit(s))),
        _ => drop(e.insert(EnableOnEnter(s))),
    }
}
fn r_remove_scoped<S: FreelyMutableState>(world: &mut World, e: Entity, kind: u8) {
    let Ok(mut e) = world.get_entity_mut(e) else { return };
    match kind {
        0 => drop(e.remove::<DespawnOnExit<S>>()),
        1 => drop(e.remove::<DespawnOnEnter<S>>()),
        2 => drop(e.remove::<DisableOnExit<S>>()),
        3 => drop(e.remove::<DisableOnEnter<S>>()),
        4 => drop(e.remove::<EnableOnExit<S>>()),
        _ => drop(e.remove::<EnableOnEnter<S>>()),
    }
}

/// Makes Rust state `S` reachable from TS (`app.ts_state::<S>()`): its unit variants are its
/// values, by name, in `bevy-ts/states`. Bevy keeps owning it; TS reads it, sets it (`NextState`),
/// gates on it and runs systems on its transitions. Costs nothing per frame.
pub fn expose_rust<S: FreelyMutableState + Reflect + FromReflect + bevy::reflect::Typed + bevy::reflect::TypePath>(app: &mut App) {
    let values: Vec<String> = match S::type_info() {
        bevy::reflect::TypeInfo::Enum(info) => {
            info.iter().filter(|v| matches!(v, bevy::reflect::enums::VariantInfo::Unit(_))).map(|v| v.name().to_string()).collect()
        }
        _ => Vec::new(),
    };
    assert!(!values.is_empty(), "bevy-ts: ts_state::<{}>() needs an enum with unit variants", S::type_path());
    let ops = SlotOps {
        register: r_none_value,
        unregister: r_none,
        current: r_current::<S>,
        previous: r_previous::<S>,
        set_next: r_set_next::<S>,
        take_next: r_take_next,
        state_id: r_state_id::<S>,
        add_runner: r_add_runner::<S>,
        insert_scoped: r_insert_scoped::<S>,
        remove_scoped: r_remove_scoped::<S>,
        make: r_none,
        set_depth: r_none_depth,
        set_derived: r_none_derived,
    };
    let state = RustState { path: S::type_path().to_string(), short: S::short_type_path().to_string(), values, ops };
    let mut states = app.world_mut().get_resource_or_init::<RustStates>();
    if states.0.iter().all(|s| s.path != state.path) {
        states.0.push(state);
    }
    // The compile step resolves `bevy-ts/states` as soon as a module imports it.
    register_module(app.world());
}

/// The `bevy-ts/states` module: one class per Rust state TS names, rebuilt as each is exposed.
pub(crate) fn register_module(world: &World) {
    let Some(states) = world.get_resource::<RustStates>() else { return };
    let mut js = String::from("// Rust states TS names (app.ts_state). Generated when the engine starts.\nimport { rustState } from \"bevy-ts\";\n");
    let mut dts = String::from("import type { RustState } from \"bevy-ts\";\n");
    for s in &states.0 {
        let values: Vec<String> = s.values.iter().map(|v| format!("{v:?}")).collect();
        js.push_str(&format!("export const {} = rustState({:?}, [{}]);\n", s.short, s.path, values.join(", ")));
        dts.push_str(&format!("/** Rust's `{}`. */\nexport declare const {}: RustState<{}>;\n", s.path, s.short, values.join(" | ")));
    }
    crate::modules::replace("bevy-ts/states", &js, &dts);
}

/// Slots by machine path, and what bevy-ts registered for them. Slots are kept for the life of
/// the app; a restart only takes their values away.
#[derive(Resource, Default, Debug)]
pub struct TsStates {
    by_path: BTreeMap<String, u8>,
    /// Slots registered with Bevy's state machinery.
    made: HashSet<u8>,
    /// TS runner systems added to Bevy's `OnEnter`/`OnExit`/`OnTransition` schedules.
    runners: HashSet<(u8, RunKind, u32, u32)>,
    /// Values set while the bevy-ts stages were paused, applied on the next running frame.
    held: BTreeMap<u8, u32>,
    /// Each slot's depth set, once placed.
    depth: HashMap<u8, u8>,
    /// A computed state's sources' values when it was last computed.
    computed_from: HashMap<u8, Vec<Option<u32>>>,
}

impl TsStates {
    /// The slot of the machine at `path` (`game::Game`), once a generation declared it.
    pub fn slot(&self, path: &str) -> Option<u8> {
        self.by_path.get(path).copied()
    }

    pub(crate) fn slots(&self) -> BTreeMap<String, u8> {
        self.by_path.clone()
    }

    pub(crate) fn restore_slots(&mut self, slots: BTreeMap<String, u8>) {
        self.by_path = slots;
    }

    /// The slot for `path`: its own, or the next free one. Fails past the pool's size.
    pub(crate) fn assign(&mut self, path: &str) -> Result<u8, String> {
        if let Some(s) = self.by_path.get(path) {
            return Ok(*s);
        }
        let next = self.by_path.len();
        if next >= SLOTS {
            return Err(format!("{path}: at most {SLOTS} state machines; an app has {SLOTS} state slots and every one is taken"));
        }
        self.by_path.insert(path.to_string(), next as u8);
        Ok(next as u8)
    }
}

/// A machine's current value, if it has one.
pub fn current_of(world: &World, slot: u8) -> Option<u32> {
    (ops_of(world, slot).current)(world)
}

/// A machine's value before its last transition.
pub fn previous_of(world: &World, slot: u8) -> Option<u32> {
    (ops_of(world, slot).previous)(world)
}

/// Queues a transition, as `NextState::set`. Returns false when the machine has no value yet.
pub fn set_next_of(world: &mut World, slot: u8, value: u32) -> bool {
    if current_of(world, slot).is_none() {
        return false;
    }
    (ops_of(world, slot).set_next)(world, value);
    true
}

/// The resource id of `State<TsState<N>>`, once registered: for `stateChanged`.
pub fn state_id_of(world: &World, slot: u8) -> Option<ComponentId> {
    (ops_of(world, slot).state_id)(world)
}

/// Inserts state-scoped component `kind` (crate::states::SCOPED) of machine `slot`'s `value`:
/// Bevy's own `DespawnOnExit<TsState<N>>` and kin, run by Bevy's systems.
pub fn insert_scoped_at(world: &mut World, e: Entity, kind: u8, slot: u8, value: u32) {
    (ops_of(world, slot).insert_scoped)(world, e, kind, value);
}

/// [`insert_scoped_at`] for a machine by path and a value by name, as TSX and levels give them.
pub fn insert_scoped(world: &mut World, e: Entity, kind: u8, state: &str, value: &str) -> Result<(), String> {
    let payloads = world.resource::<TsPayloads>();
    let m = payloads.state(state).map(|i| &payloads.states[i]).ok_or_else(|| format!("{}: no state machine {state}", SCOPED[kind as usize]))?;
    let sym = m.sym(value).ok_or_else(|| format!("{}: {state} has no value {value}", SCOPED[kind as usize]))?;
    let slot = m.slot;
    insert_scoped_at(world, e, kind, slot, sym);
    Ok(())
}

/// Removes state-scoped component `kind` of any machine.
pub fn remove_scoped(world: &mut World, e: Entity, kind: u8) {
    let slots: Vec<u8> = world.get_resource::<TsStates>().map(|s| s.by_path.values().copied().collect()).unwrap_or_default();
    for slot in slots {
        (ops_of(world, slot).remove_scoped)(world, e, kind);
    }
}

/// After a generation commits, and when the game starts: gives each machine with an `initState`
/// its first value once the game has started, moves a machine whose value the generation removed
/// to its initial value, and adds the TS runners its systems need.
pub(crate) fn sync(world: &mut World) {
    let started = crate::generation::started(world);
    // Slot, value symbols, initial value, derived, and the machines it derives from.
    type Machine = (u8, Vec<u32>, Option<u32>, bool, Vec<usize>);
    let machines: Vec<Machine> = world
        .resource::<TsPayloads>()
        .states
        .iter()
        .map(|m| (m.slot, m.syms.clone(), m.initial, m.derived(), m.source.iter().map(|s| s.0).chain(m.computed.iter().copied()).collect()))
        .collect();
    // Depth by declaration order: a source is declared before what it derives.
    let mut depths: Vec<u8> = Vec::with_capacity(machines.len());
    for (_, _, _, _, sources) in &machines {
        let d = sources.iter().filter_map(|i| depths.get(*i)).max().map_or(1, |d| d + 1);
        depths.push(d);
    }
    for ((slot, syms, initial, derived, _), depth) in machines.into_iter().zip(depths) {
        if world.resource::<TsStates>().depth.get(&slot) != Some(&depth) {
            (ops_of(world, slot).make)(world);
            (ops_of(world, slot).set_depth)(world, depth);
            world.resource_mut::<TsStates>().depth.insert(slot, depth);
        }
        if derived {
            (ops_of(world, slot).make)(world);
            continue;
        }
        match (current_of(world, slot), initial) {
            (None, Some(init)) if started => (ops_of(world, slot).register)(world, init),
            // A reload removed the current value: back to the initial one, as a transition.
            (Some(v), Some(init)) if !syms.contains(&v) => (ops_of(world, slot).set_next)(world, init),
            _ => {}
        }
    }
    let wanted: Vec<(u8, RunKind, u32, u32)> = match world.get_resource::<crate::systems::TsSystems>() {
        Some(s) => s
            .on_enter
            .keys()
            .map(|(slot, v)| (*slot as u8, RunKind::Enter, *v, 0))
            .chain(s.on_exit.keys().map(|(slot, v)| (*slot as u8, RunKind::Exit, *v, 0)))
            .chain(s.on_transition.keys().map(|(slot, a, b)| (*slot as u8, RunKind::Transition, *a, *b)))
            .collect(),
        None => Vec::new(),
    };
    for r in wanted {
        if world.get_resource_or_init::<TsStates>().runners.insert(r) {
            (ops_of(world, r.0).add_runner)(world, r.0, r.1, r.2, r.3);
        }
    }
}

/// A restart (a Stop, a `restart` reload): every machine loses its value; [`sync`] gives it back
/// its initial one when the game starts again.
pub(crate) fn restart(world: &mut World) {
    let made: Vec<u8> = world.get_resource::<TsStates>().map(|s| s.made.iter().copied().collect()).unwrap_or_default();
    for slot in made {
        (ops_of(world, slot).unregister)(world);
    }
    if let Some(mut s) = world.get_resource_mut::<TsStates>() {
        s.held.clear();
        s.computed_from.clear();
    }
}

/// `StateTransition`, before `DependentTransitions`: while the bevy-ts stages are paused, TS
/// machines' pending values wait, so no transition runs that TS systems would not see.
pub(crate) fn hold_while_paused(world: &mut World) {
    let Some(states) = world.get_resource::<TsStates>() else { return };
    if states.made.is_empty() {
        return;
    }
    let made: Vec<u8> = states.made.iter().copied().collect();
    if crate::runtime::stage_should_run(world) {
        let held = std::mem::take(&mut world.resource_mut::<TsStates>().held);
        for (slot, v) in held {
            if current_of(world, slot).is_some() {
                (ops_of(world, slot).set_next)(world, v);
            }
        }
    } else {
        for slot in made {
            if let Some(v) = (ops_of(world, slot).take_next)(world) {
                world.resource_mut::<TsStates>().held.insert(slot, v);
            }
        }
    }
}

/// `StateTransition`, after `DependentTransitions` and before `ExitSchedules`: sub and computed
/// states follow their sources, in declaration order. A sub state exists at its first value while
/// its source is in its value; a computed state's function is a JS call only when a source changed.
pub(crate) fn derive(world: &mut World) {
    let derived: Vec<(usize, u8)> =
        world.resource::<TsPayloads>().states.iter().enumerate().filter(|(_, m)| m.derived()).map(|(i, m)| (i, m.slot)).collect();
    if derived.is_empty() {
        return;
    }
    for (i, slot) in derived {
        // Sources as slots, read before anything is written.
        let (source, computed, first, name) = {
            let p = world.resource::<TsPayloads>();
            let m = &p.states[i];
            let source = m.source.map(|(src, value)| (p.states[src].slot, value));
            let computed: Vec<u8> = m.computed.iter().map(|j| p.states[*j].slot).collect();
            (source, computed, m.syms.first().copied(), m.name.clone())
        };
        let value = if let Some((src_slot, value)) = source {
            match current_of(world, src_slot) == Some(value) {
                true => current_of(world, slot).or(first),
                false => None,
            }
        } else {
            let sources: Vec<Option<u32>> = computed.iter().map(|j| current_of(world, *j)).collect();
            if world.resource::<TsStates>().computed_from.get(&slot) == Some(&sources) {
                continue;
            }
            world.resource_mut::<TsStates>().computed_from.insert(slot, sources.clone());
            let names: Vec<serde_json::Value> = sources.iter().map(|v| v.map_or(serde_json::Value::Null, name_value)).collect();
            let payload = serde_json::json!({ "state": name, "sources": names }).to_string();
            match crate::runtime::ScriptRuntime::call_entry(world, COMPUTE_ENTRY, payload.as_bytes()) {
                Ok(bytes) => {
                    let v = String::from_utf8_lossy(&bytes).to_string();
                    let m = &world.resource::<TsPayloads>().states[i];
                    if v.is_empty() { None } else { m.sym(&v) }
                }
                // The entry logged the throw; the state keeps its value.
                Err(_) => current_of(world, slot),
            }
        };
        (ops_of(world, slot).set_derived)(world, value);
    }
}

fn name_value(sym: u32) -> serde_json::Value {
    serde_json::Value::String(name(sym).to_string())
}

/// The JS entry that runs a computed state's function.
pub const COMPUTE_ENTRY: &str = "bevy-ts:computed";
