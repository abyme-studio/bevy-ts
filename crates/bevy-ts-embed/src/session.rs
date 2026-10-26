//! `bevy-ts/session`, a built-in module every engine ships: the `SessionBytes` store (string keys to
//! bytes, held in Rust, kept across every reload) as a TS system reads and writes it, and `get`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_ts::SessionBytes;
use bevy_ts_native::prelude::*;

pub const MODULE: &str = "bevy-ts/session";

/// Registers the module and its param.
pub struct ScriptSessionPlugin;

impl Plugin for ScriptSessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionBytes>()
            .ts_param::<SessionParam<'static>>()
            .ts_module(MODULE, include_str!("../ts/session.js"), include_str!("../ts/session.d.ts"));
    }
}

/// `SessionBytes`: `ResMut<SessionBytes>`. Only `put`, `delete` and `clear` mark it changed.
#[derive(SystemParam)]
pub struct SessionParam<'w> {
    store: ResMut<'w, SessionBytes>,
}

#[export(ts = "SessionBytes", module = "bevy-ts/session")]
impl SessionParam<'_> {
    /// Stores a copy of `value` under `key`. A key is 1 to 1,024 bytes of UTF-8 and the store holds
    /// at most 64 MiB; past either it throws and changes nothing.
    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        self.store.put(key, value)
    }

    /// The stored value's length.
    pub fn size(&self, key: &str) -> Option<u32> {
        self.store.size(key).map(|n| n as u32)
    }

    /// Copies the value into the start of `out` when it fits; its length either way.
    pub fn read(&self, key: &str, out: &mut [u8]) -> Option<u32> {
        let v = self.store.get(key)?;
        if let Some(o) = out.get_mut(..v.len()) {
            o.copy_from_slice(v);
        }
        Some(v.len() as u32)
    }

    pub fn has(&self, key: &str) -> bool {
        self.store.get(key).is_some()
    }

    /// Removes the value; whether one was there.
    pub fn delete(&mut self, key: &str) -> bool {
        // Through `DerefMut` only when there is something to remove.
        self.store.get(key).is_some() && self.store.delete(key)
    }

    pub fn clear(&mut self) {
        if self.store.count() > 0 {
            self.store.clear();
        }
    }

    /// How many keys.
    pub fn count(&self) -> u32 {
        self.store.count() as u32
    }

    /// The bytes the store holds, keys included.
    pub fn bytes(&self) -> f64 {
        self.store.bytes() as f64
    }
}
