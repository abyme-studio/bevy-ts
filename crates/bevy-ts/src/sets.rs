//! Rust system sets exposed to TS under stable names (native decision 7). A crate exposes a set
//! with `app.ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)`; TS systems order
//! against the name and run in a slot made around the set the first time a generation names it.

use crate::BevyTsSet;
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
    assert!(valid_name(name), "bevy-ts: exposed set name {name:?} must be <crate>:<name>, such as \"rapier:step\"");
    let entry = ExposedSet { name: name.to_string(), schedule: schedule.intern(), set: set.intern(), location: Location::caller() };
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

/// bevy-ts's own anchors: transform propagation and the four bridge stages.
pub(crate) fn expose_anchors(app: &mut App) {
    expose(app, "bevy:transform-propagate", PostUpdate, bevy::transform::TransformSystems::Propagate);
    expose(app, "bevy-ts:pre-update", PreUpdate, BevyTsSet::PreUpdate);
    expose(app, "bevy-ts:update", Update, BevyTsSet::Update);
    expose(app, "bevy-ts:post-update", PostUpdate, BevyTsSet::PostUpdate);
    expose(app, "bevy-ts:fixed-update", FixedUpdate, BevyTsSet::FixedUpdate);
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
