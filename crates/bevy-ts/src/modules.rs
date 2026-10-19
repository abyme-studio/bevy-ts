//! Built-in TS modules a port or native crate ships (`bevy-ts/rapier`): JS the engine serves
//! under the specifier, with its TS declarations for `bevy-ts types`. Registered with
//! `app.ts_module`, fixed for the life of the process like everything Rust registers.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// One module: its JS and its `.d.ts` body.
#[derive(Clone, Debug)]
pub struct TsModule {
    pub js: String,
    pub dts: String,
}

static MODULES: OnceLock<Mutex<BTreeMap<String, TsModule>>> = OnceLock::new();

fn modules() -> &'static Mutex<BTreeMap<String, TsModule>> {
    MODULES.get_or_init(Default::default)
}

/// Registers `specifier` (`bevy-ts/<name>`). Fails when another module already has it.
pub fn register(specifier: &str, js: &str, dts: &str) -> Result<(), String> {
    let name = specifier.strip_prefix("bevy-ts/").filter(|n| !n.is_empty() && !n.contains(':'));
    if name.is_none() || matches!(specifier, "bevy-ts/bevy" | "bevy-ts/jsx-runtime" | "bevy-ts/jsx-dev-runtime" | "bevy-ts/testing") {
        return Err(format!("bevy-ts: a module is named bevy-ts/<name> and cannot be one bevy-ts ships; got {specifier:?}"));
    }
    let mut m = modules().lock().unwrap();
    if let Some(prev) = m.get(specifier)
        && prev.js != js
    {
        return Err(format!("bevy-ts: module {specifier} is registered twice"));
    }
    m.insert(specifier.to_string(), TsModule { js: js.to_string(), dts: dts.to_string() });
    Ok(())
}

/// A registered module's JS.
pub fn js(specifier: &str) -> Option<String> {
    modules().lock().unwrap().get(specifier).map(|m| m.js.clone())
}

pub fn is_registered(specifier: &str) -> bool {
    modules().lock().unwrap().contains_key(specifier)
}

/// Every module, by specifier.
pub fn all() -> BTreeMap<String, TsModule> {
    modules().lock().unwrap().clone()
}
