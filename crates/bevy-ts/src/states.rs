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

static NAMES: Mutex<(Vec<&'static str>, Option<HashMap<&'static str, u32>>)> = Mutex::new((Vec::new(), None));

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

/// What bevy-ts does to one slot, whatever its `N`.
pub struct SlotOps {
    register: fn(&mut World, u32),
    unregister: fn(&mut World),
    current: fn(&World) -> Option<u32>,
    previous: fn(&World) -> Option<u32>,
    set_next: fn(&mut World, u32),
    take_next: fn(&mut World) -> Option<u32>,
    state_id: fn(&World) -> Option<ComponentId>,
    add_runner: fn(&mut World, RunKind, u32, u32),
    insert_scoped: fn(&mut World, Entity, u8, u32),
    remove_scoped: fn(&mut World, Entity, u8),
}

/// Bevy's six state-scoped components, by name, in the order of their kind number.
pub const SCOPED: [&str; 6] = ["DespawnOnExit", "DespawnOnEnter", "DisableOnExit", "DisableOnEnter", "EnableOnExit", "EnableOnEnter"];

/// The kind number of a state-scoped component's name.
pub fn scoped_kind(name: &str) -> Option<u8> {
    SCOPED.iter().position(|n| *n == name).map(|i| i as u8)
}

fn insert_scoped<const N: u8>(world: &mut World, e: Entity, kind: u8, value: u32) {
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

fn remove_scoped<const N: u8>(world: &mut World, e: Entity, kind: u8) {
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

fn register<const N: u8>(world: &mut World, initial: u32) {
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

fn add_runner<const N: u8>(world: &mut World, kind: RunKind, a: u32, b: u32) {
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
            insert_scoped: insert_scoped::<$n>,
            remove_scoped: remove_scoped::<$n>,
        }),*]
    };
}

static OPS: [SlotOps; SLOTS] = ops!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31);

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
    (OPS[slot as usize].current)(world)
}

/// A machine's value before its last transition.
pub fn previous_of(world: &World, slot: u8) -> Option<u32> {
    (OPS[slot as usize].previous)(world)
}

/// Queues a transition, as `NextState::set`. Returns false when the machine has no value yet.
pub fn set_next_of(world: &mut World, slot: u8, value: u32) -> bool {
    if current_of(world, slot).is_none() {
        return false;
    }
    (OPS[slot as usize].set_next)(world, value);
    true
}

/// The resource id of `State<TsState<N>>`, once registered: for `stateChanged`.
pub fn state_id_of(world: &World, slot: u8) -> Option<ComponentId> {
    (OPS[slot as usize].state_id)(world)
}

/// Inserts state-scoped component `kind` (crate::states::SCOPED) of machine `slot`'s `value`:
/// Bevy's own `DespawnOnExit<TsState<N>>` and kin, run by Bevy's systems.
pub fn insert_scoped_at(world: &mut World, e: Entity, kind: u8, slot: u8, value: u32) {
    (OPS[slot as usize].insert_scoped)(world, e, kind, value);
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
        (OPS[slot as usize].remove_scoped)(world, e, kind);
    }
}

/// After a generation commits, and when the game starts: gives each machine with an `initState`
/// its first value once the game has started, moves a machine whose value the generation removed
/// to its initial value, and adds the TS runners its systems need.
pub(crate) fn sync(world: &mut World) {
    let started = crate::generation::started(world);
    let machines: Vec<(u8, Vec<u32>, Option<u32>)> =
        world.resource::<TsPayloads>().states.iter().map(|m| (m.slot, m.syms.clone(), m.initial)).collect();
    for (slot, syms, initial) in machines {
        match (current_of(world, slot), initial) {
            (None, Some(init)) if started => (OPS[slot as usize].register)(world, init),
            // A reload removed the current value: back to the initial one, as a transition.
            (Some(v), Some(init)) if !syms.contains(&v) => (OPS[slot as usize].set_next)(world, init),
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
            (OPS[r.0 as usize].add_runner)(world, r.1, r.2, r.3);
        }
    }
}

/// A restart (a Stop, a `restart` reload): every machine loses its value; [`sync`] gives it back
/// its initial one when the game starts again.
pub(crate) fn restart(world: &mut World) {
    let made: Vec<u8> = world.get_resource::<TsStates>().map(|s| s.made.iter().copied().collect()).unwrap_or_default();
    for slot in made {
        (OPS[slot as usize].unregister)(world);
    }
    if let Some(mut s) = world.get_resource_mut::<TsStates>() {
        s.held.clear();
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
                (OPS[slot as usize].set_next)(world, v);
            }
        }
    } else {
        for slot in made {
            if let Some(v) = (OPS[slot as usize].take_next)(world) {
                world.resource_mut::<TsStates>().held.insert(slot, v);
            }
        }
    }
}
