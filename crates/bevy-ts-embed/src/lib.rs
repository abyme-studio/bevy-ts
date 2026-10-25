//! bevy-ts as a library: the shipped game's app ([`play`], [`shipped`]), the SURFACE.md input
//! mapping ([`input`]), and the engine's built-in script modules. `bevy-ts-cli` builds its play
//! and dev apps from these; an embedding host drives the same app on a surface of its own
//! ([`engine`]), through the C ABI ([`ffi`], `include/bevy_ts_embed.h`).

pub mod camera;
pub mod engine;
pub mod ffi;
pub mod gpu;
pub mod input;
pub mod modules;
pub mod play;
pub mod script_input;
pub mod shipped;
pub mod window;
