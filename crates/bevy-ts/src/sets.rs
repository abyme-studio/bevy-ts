//! Rust system sets exposed to TS under stable names (native decision 7). A crate exposes a set
//! with `app.ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)`; TS systems order
//! against the name and run in a slot made around the set the first time a generation names it.

use crate::backend::ScriptError;
use crate::manifest::{ScheduleDecl, SystemDecl};
use crate::systems::{SlotKey, TsSystems};
use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
use bevy::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::panic::Location;

/// One exposed set.
#[derive(Clone, Debug)]
pub struct ExposedSet {
    pub name: String,
    pub schedule: Interned<dyn ScheduleLabel>,
    pub set: Interned<dyn SystemSet>,
    /// Where it was exposed, for the duplicate error.
    pub location: &'static Location<'static>,
}

/// A row of `bevy-ts/schema`'s sets and of `NativeSet`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SetInfo {
    pub name: String,
    pub schedule: String,
}

/// Every exposed set, by name.
#[derive(Resource, Default, Clone, Debug)]
pub struct TsSets {
    by_name: BTreeMap<String, ExposedSet>,
    /// Duplicate registrations, reported when the app finishes building.
    duplicates: Vec<(ExposedSet, ExposedSet)>,
}

impl TsSets {
    pub fn get(&self, name: &str) -> Option<&ExposedSet> {
        self.by_name.get(name)
    }

    /// Exposed sets in name order.
    pub fn iter(&self) -> impl Iterator<Item = &ExposedSet> {
        self.by_name.values()
    }

    pub fn infos(&self) -> Vec<SetInfo> {
        self.iter().map(|s| SetInfo { name: s.name.clone(), schedule: format!("{:?}", s.schedule) }).collect()
    }

    /// Fails with every duplicate name and both of its registrations.
    pub fn check(&self) -> Result<(), String> {
        if self.duplicates.is_empty() {
            return Ok(());
        }
        let lines: Vec<String> = self
            .duplicates
            .iter()
            .map(|(a, b)| format!("{:?} is exposed twice: at {} and at {}", a.name, a.location, b.location))
            .collect();
        Err(format!("bevy-ts: {}", lines.join("; ")))
    }
}

/// Checks a set name is `<crate>:<name>`.
pub fn valid_name(name: &str) -> bool {
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    matches!(name.split_once(':'), Some((a, b)) if ok(a) && ok(b))
}

/// Exposes `set` in `schedule` to TS as `name`. A duplicate name fails the app at startup,
/// naming both registrations.
#[track_caller]
pub fn expose(app: &mut App, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) {
    expose_interned(app, name, schedule.intern(), set.intern());
}

/// [`expose`], with the label and set already interned.
#[track_caller]
pub fn expose_interned(app: &mut App, name: &str, schedule: Interned<dyn ScheduleLabel>, set: Interned<dyn SystemSet>) {
    assert!(valid_name(name), "bevy-ts: exposed set name {name:?} must be <crate>:<name>, such as \"rapier:step\"");
    let entry = ExposedSet { name: name.to_string(), schedule, set, location: Location::caller() };
    let mut sets = app.world_mut().get_resource_or_init::<TsSets>();
    if let Some(prev) = sets.by_name.get(name).cloned() {
        sets.duplicates.push((prev, entry));
    } else {
        sets.by_name.insert(name.to_string(), entry);
    }
}

/// The slot sets Rust orders against: every TS system in the slot before or after an exposed set
/// runs inside `BevyTsSlot::before(name)` / `BevyTsSlot::after(name)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum BevyTsSlot {
    Before(String),
    After(String),
}

impl BevyTsSlot {
    pub fn before(name: &str) -> Self {
        BevyTsSlot::Before(name.to_string())
    }

    pub fn after(name: &str) -> Self {
        BevyTsSlot::After(name.to_string())
    }
}

/// Where a system runs when it orders itself against an exposed set: its slot, or `None` for its
/// declared schedule. A system names at most one exposed set, in the set's own schedule.
pub(crate) fn placement(world: &World, d: &SystemDecl) -> Result<Option<SlotKey>, ScriptError> {
    let Some(sets) = world.get_resource::<TsSets>() else { return Ok(None) };
    let named: Vec<(&String, bool)> = d
        .after
        .iter()
        .map(|t| (t, true))
        .chain(d.before.iter().map(|t| (t, false)))
        .filter(|(t, _)| sets.get(t).is_some())
        .collect();
    let err = |m: String| Err(ScriptError::at(format!("system {}: {m}", d.name), d.location.clone()));
    let [(name, after)] = named.as_slice() else {
        return match named.len() {
            0 => Ok(None),
            _ => err(format!(
                "it orders against {} exposed sets ({}); a system runs in one slot, so name at most one",
                named.len(),
                named.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            )),
        };
    };
    let set = sets.get(name).expect("filtered on exposed sets");
    let schedule = match &d.schedule {
        ScheduleDecl::Stage { stage } => crate::schedules::stage_label(*stage),
        ScheduleDecl::Schedule { label } => crate::schedules::label_of(world, label),
        _ => return err(format!("only a system in a schedule with a bevy-ts stage can order against {name}")),
    };
    if schedule != set.schedule {
        return err(format!("it is registered in {schedule:?} but orders against {name}, which runs in {:?}; register it in {:?}", set.schedule, set.schedule));
    }
    Ok(Some(SlotKey { set: name.to_string(), after: *after }))
}

/// Slots made so far: each is a set ordered against its exposed set, kept for the life of the app
/// (an emptied slot holds no system).
#[derive(Resource, Default, Debug)]
pub struct MadeSlots(pub std::collections::BTreeSet<SlotKey>);

/// Configures the slot sets a just-committed generation names for the first time, at this frame
/// boundary. Later generations reuse them.
pub(crate) fn ensure_slots(world: &mut World) {
    let wanted: Vec<SlotKey> = match world.get_resource::<TsSystems>() {
        Some(s) => s.slots.iter().filter(|(_, l)| !l.is_empty()).map(|(k, _)| k.clone()).collect(),
        None => return,
    };
    let made = world.get_resource_or_init::<MadeSlots>().0.clone();
    for key in wanted.into_iter().filter(|k| !made.contains(k)) {
        let Some(set) = world.resource::<TsSets>().get(&key.set).cloned() else { continue };
        let slot = if key.after { BevyTsSlot::after(&key.set) } else { BevyTsSlot::before(&key.set) };
        let label = set.schedule;
        let (after, exposed) = (key.after, set.set);
        crate::schedules::edit(world, label, false, move |world| {
            let placed = if after { slot.clone().after(exposed) } else { slot.clone().before(exposed) };
            let mut schedules = world.resource_mut::<Schedules>();
            schedules.configure_sets(label, placed);
            crate::bevy_system::configure_slot(&mut schedules, label, slot);
        });
        world.resource_mut::<MadeSlots>().0.insert(key);
    }
}

/// bevy-ts's own anchors: transform propagation, the fixed main loop and every bridge stage.
pub(crate) fn expose_anchors(app: &mut App) {
    expose(app, "bevy:transform-propagate", PostUpdate, bevy::transform::TransformSystems::Propagate);
    crate::schedules::expose_anchors(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
    struct Step;

    #[test]
    fn names_are_crate_colon_name_and_unique() {
        assert!(valid_name("rapier:step") && valid_name("bevy-ts:update"));
        assert!(!valid_name("step") && !valid_name("a:b:c") && !valid_name(":x"));
        let mut app = App::new();
        expose(&mut app, "rapier:step", FixedUpdate, Step);
        assert!(app.world().resource::<TsSets>().check().is_ok());
        expose(&mut app, "rapier:step", Update, Step);
        let e = app.world().resource::<TsSets>().check().unwrap_err();
        assert!(e.contains("rapier:step") && e.matches("sets.rs:").count() == 2, "{e}");
        let infos = app.world().resource::<TsSets>().infos();
        assert_eq!(infos, vec![SetInfo { name: "rapier:step".into(), schedule: "FixedUpdate".into() }]);
    }
}
