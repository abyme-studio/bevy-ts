//! The Rust half of one exported-param host call, as `crates/bevy-ts/src/host.rs` runs it
//! today and as a writing param would. Benchmarked in `benches/call.rs`.
//!
//! - `read`: today. The state is a type-erased `SystemState<P>`; each call downcasts it and
//!   calls `SystemState::get(&World)`, which bumps the world's change tick.
//! - `write`: the proposal. The state keeps the `SystemState<P>` and a copy of its `SystemMeta`;
//!   each call sets `last_run` to the TS system's, then calls `SystemParam::get_param` on the
//!   run's `UnsafeWorldCell` with the TS system's own tick, so writes are stamped as the TS
//!   system's writes and no tick is spent.
//! - `write_commands`: `write` plus one queued `Commands` insert per call, applied once per run.
//! - `get_mut`: Bevy's plain `SystemState::get_mut(&mut World)`, for reference.

use bevy_ecs::change_detection::Tick;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{SystemMeta, SystemParam, SystemState};
use bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell;
use std::any::Any;

#[derive(Resource)]
pub struct Ground(pub f32);

#[derive(Component)]
pub struct Pos(pub f32);

#[derive(Component)]
pub struct Tag;

/// A read-only param, as exported today.
#[derive(SystemParam)]
pub struct Probe<'w, 's> {
    ground: Res<'w, Ground>,
    q: Query<'w, 's, &'static Pos>,
}

impl Probe<'_, '_> {
    pub fn height(&self, e: Entity) -> f32 {
        self.q.get(e).map_or(0.0, |p| p.0) - self.ground.0
    }
}

/// A writing param: `ResMut` and `Query<&mut T>`.
#[derive(SystemParam)]
pub struct Mover<'w, 's> {
    ground: ResMut<'w, Ground>,
    q: Query<'w, 's, &'static mut Pos>,
}

impl Mover<'_, '_> {
    pub fn nudge(&mut self, e: Entity) -> f32 {
        self.ground.0 += 1.0;
        let mut p = self.q.get_mut(e).unwrap();
        p.0 += 1.0;
        p.0
    }
}

/// A writing param with `Commands`.
#[derive(SystemParam)]
pub struct Spawner<'w, 's> {
    ground: ResMut<'w, Ground>,
    q: Query<'w, 's, &'static mut Pos>,
    commands: Commands<'w, 's>,
}

impl Spawner<'_, '_> {
    pub fn tag(&mut self, e: Entity) -> f32 {
        self.ground.0 += 1.0;
        self.q.get_mut(e).unwrap().0 += 1.0;
        self.commands.entity(e).insert(Tag);
        self.ground.0
    }
}

pub type ParamState = Box<dyn Any + Send + Sync>;

/// The proposed state of a writing param: its `SystemState` and the meta handed to `get_param`.
pub struct WriteState<P: SystemParam + 'static> {
    pub state: SystemState<P>,
    pub meta: SystemMeta,
}

impl<P: SystemParam + 'static> WriteState<P> {
    pub fn new(world: &mut World) -> Self {
        let state = SystemState::<P>::new(world);
        let meta = state.meta().clone();
        Self { state, meta }
    }

    /// The param for one call inside a TS system's run.
    ///
    /// # Safety
    /// `world` is the run's world, held exclusively by the run, with nothing else borrowing
    /// what `P` accesses for the duration of the item's use.
    pub unsafe fn fetch<'w, 's>(&'s mut self, world: UnsafeWorldCell<'w>, last_run: Tick, this_run: Tick) -> P::Item<'w, 's> {
        self.meta.set_last_run(last_run);
        // SAFETY: upheld by the caller; the state was made from this world.
        unsafe { P::get_param(self.state.param_state_mut(), &self.meta, world, this_run) }.expect("param valid")
    }

    /// Applies what the param deferred (`Commands`), at the end of the TS system's run.
    pub fn apply(&mut self, world: &mut World) {
        // SAFETY: the state is not otherwise borrowed.
        P::apply(unsafe { self.state.param_state_mut() }, &self.meta, world);
    }
}

/// A world with `n` entities and the resource; returns it and every entity.
pub fn world(n: usize) -> (World, Vec<Entity>) {
    let mut world = World::new();
    world.insert_resource(Ground(0.0));
    let es = (0..n).map(|i| world.spawn(Pos(i as f32)).id()).collect();
    (world, es)
}

/// Today's call: downcast the erased state, `SystemState::get`, the method.
pub fn read_call(state: &mut ParamState, world: &World, e: Entity) -> f32 {
    let st = state.downcast_mut::<SystemState<Probe<'static, 'static>>>().unwrap();
    let p = st.get(world).unwrap();
    p.height(e)
}

/// The proposed writing call: downcast, `fetch` at the run's ticks, the method.
///
/// # Safety
/// As [`WriteState::fetch`].
pub unsafe fn write_call(state: &mut ParamState, world: UnsafeWorldCell, last_run: Tick, this_run: Tick, e: Entity) -> f32 {
    let st = state.downcast_mut::<WriteState<Mover<'static, 'static>>>().unwrap();
    let mut p = unsafe { st.fetch(world, last_run, this_run) };
    p.nudge(e)
}

/// # Safety
/// As [`WriteState::fetch`].
pub unsafe fn commands_call(state: &mut ParamState, world: UnsafeWorldCell, last_run: Tick, this_run: Tick, e: Entity) -> f32 {
    let st = state.downcast_mut::<WriteState<Spawner<'static, 'static>>>().unwrap();
    let mut p = unsafe { st.fetch(world, last_run, this_run) };
    p.tag(e)
}

pub fn get_mut_call(state: &mut ParamState, world: &mut World, e: Entity) -> f32 {
    let st = state.downcast_mut::<SystemState<Mover<'static, 'static>>>().unwrap();
    let mut p = st.get_mut(world).unwrap();
    p.nudge(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A write through `fetch` is stamped with the TS system's tick: a later system sees it
    /// changed, and the TS system itself, next run, does not (Bevy's rule for a system's own
    /// writes). `SystemState::get_mut` stamps a fresh tick, which the same TS system would see.
    #[test]
    fn writes_carry_the_ts_systems_tick() {
        let (mut world, es) = world(4);
        let mut st: ParamState = Box::new(WriteState::<Mover<'static, 'static>>::new(&mut world));
        // As `run_one`: the run's tick is what `increment_change_tick` returns, the tick before
        // the increment; the system last ran some ticks earlier.
        let last_run = world.increment_change_tick();
        world.increment_change_tick();
        let this_run = world.increment_change_tick();
        let after = world.change_tick();
        let cell = world.as_unsafe_world_cell();
        // SAFETY: nothing else borrows the world.
        let v = unsafe { write_call(&mut st, cell, last_run, this_run, es[2]) };
        assert_eq!(v, 3.0);
        let ticks = world.entity(es[2]).get_change_ticks::<Pos>().unwrap();
        assert_eq!(ticks.changed, this_run);
        let next = Tick::new(this_run.get() + 1);
        assert!(!ticks.is_changed(this_run, next), "the TS system's own write, next run");
        assert!(ticks.is_changed(last_run, next), "a system that last ran before it");
        assert_eq!(world.change_tick(), after, "no tick spent per call");
    }

    #[test]
    fn commands_apply_at_the_end_of_the_run() {
        let (mut world, es) = world(4);
        let mut st: ParamState = Box::new(WriteState::<Spawner<'static, 'static>>::new(&mut world));
        let last_run = world.change_tick();
        let this_run = world.increment_change_tick();
        let cell = world.as_unsafe_world_cell();
        // SAFETY: nothing else borrows the world.
        unsafe { commands_call(&mut st, cell, last_run, this_run, es[1]) };
        assert!(!world.entity(es[1]).contains::<Tag>());
        st.downcast_mut::<WriteState<Spawner<'static, 'static>>>().unwrap().apply(&mut world);
        assert!(world.entity(es[1]).contains::<Tag>());
    }

    /// Today's read-only fetch spends a tick per call and moves `last_run` to it, so a native
    /// `is_changed` means "since the previous call of this param", not "since this TS system
    /// last ran".
    #[test]
    fn todays_get_spends_a_tick_per_call() {
        let (mut world, es) = world(4);
        let mut st: ParamState = Box::new(SystemState::<Probe<'static, 'static>>::new(&mut world));
        let before = world.change_tick();
        read_call(&mut st, &world, es[0]);
        read_call(&mut st, &world, es[0]);
        assert_eq!(world.change_tick().get(), before.get() + 2);
        let meta = st.downcast_ref::<SystemState<Probe<'static, 'static>>>().unwrap().meta();
        // `last_run` is the tick the last call took: the one before the world's current tick.
        assert_eq!(meta.get_last_run().get(), world.change_tick().get() - 1);
    }
}
