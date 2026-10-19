//! The `bevy-ts` binary's library half: host-side TS compilation, the dev watcher and headless
//! runs.

pub mod cli;
pub mod compile;
pub mod dev;
pub mod engine;
pub mod headless;
pub mod project;
pub mod supervise;
pub mod types;

pub use bevy_ts::engine::{EngineIdentity, NativeIdentity};
pub use cli::{Engine, Prebuilt, run, run_engine};
pub use bevy;

/// Builds a plugin value from a type that is a unit struct or implements `Default`, for the
/// generated engine `main`.
#[doc(hidden)]
pub mod __plugin {
    use bevy::app::Plugin;

    /// The fallback value a generated module glob-imports under a plugin's name: an explicit
    /// import of a unit struct shadows it in the value namespace.
    pub struct UseDefault;

    pub trait Make<P> {
        fn make(self) -> P;
    }

    impl<P: Plugin> Make<P> for P {
        fn make(self) -> P {
            self
        }
    }

    impl<P: Plugin + Default> Make<P> for UseDefault {
        fn make(self) -> P {
            P::default()
        }
    }
}
