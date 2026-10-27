//! `bevy-ts/ready`, a built-in module every engine ships: the `ReadyHold` a TS system holds a run's
//! ready frames by, so a shot and the fixed step wait for data Bevy cannot see. `ReadyHold` is
//! bevy-ts's own (`bevy_ts_remote::screenshot::ReadyHold`): Bevy has no such hold.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_ts_native::prelude::*;
use bevy_ts_remote::screenshot::ReadyHold;

pub const MODULE: &str = "bevy-ts/ready";

/// Registers the module and its param.
pub struct ScriptReadyPlugin;

impl Plugin for ScriptReadyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReadyHold>()
            .ts_param::<ReadyParam<'static>>()
            .ts_module(MODULE, include_str!("../ts/ready.js"), include_str!("../ts/ready.d.ts"));
    }
}

/// `ReadyHold`: `ResMut<ReadyHold>`. Only a `hold` or `release` that changes it marks it changed.
#[derive(SystemParam)]
pub struct ReadyParam<'w> {
    hold: ResMut<'w, ReadyHold>,
}

#[export(ts = "ReadyHold", module = "bevy-ts/ready")]
impl ReadyParam<'_> {
    /// Holds the run's ready frames under `name`: while any name is held, no shot counts a frame
    /// and the fixed step does not step. Holding a held name changes nothing.
    pub fn hold(&mut self, name: &str) {
        if !self.hold.is_held(name) {
            self.hold.hold(name);
        }
    }

    /// Releases `name`; releasing a name not held changes nothing.
    pub fn release(&mut self, name: &str) {
        // Through `DerefMut` only when it was held.
        if self.hold.is_held(name) {
            self.hold.release(name);
        }
    }

    /// Whether `name` is held.
    pub fn is_held(&self, name: &str) -> bool {
        self.hold.is_held(name)
    }
}
