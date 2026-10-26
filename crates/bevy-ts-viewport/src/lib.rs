//! bevy-ts viewport mode (openspec change `viewport`): the engine has no window of its own and
//! renders into a ring of IOSurfaces another process shows, paced by that process's display,
//! taking its input over the same Mach channel.
//!
//! macOS only, but for [`offscreen`], headless input, built everywhere; both map keys through
//! `bevy_ts_embed::input`. On other platforms [`supported`] is false.

/// The crate the protocol contract names as owner of every `bevy-ts/viewport_*` method.
pub const CRATE: &str = "bevy-ts-viewport";

pub mod offscreen;

#[cfg(target_os = "macos")]
pub mod assets;
#[cfg(target_os = "macos")]
pub mod edit;
#[cfg(target_os = "macos")]
pub mod edits;
#[cfg(target_os = "macos")]
mod feedback;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
#[cfg(target_os = "macos")]
pub mod mach;
#[cfg(target_os = "macos")]
mod methods;
#[cfg(target_os = "macos")]
pub mod orient;
#[cfg(target_os = "macos")]
pub mod preview;
#[cfg(target_os = "macos")]
pub mod qos;
#[cfg(target_os = "macos")]
mod ring;
#[cfg(target_os = "macos")]
pub mod runner;
#[cfg(target_os = "macos")]
pub mod shared;
#[cfg(target_os = "macos")]
pub mod surface;
#[cfg(target_os = "macos")]
mod trace;

#[cfg(target_os = "macos")]
pub use ring::{UpdateCount, ViewportWindow, VirtualWindow};

/// Whether viewport mode exists on this platform.
pub const fn supported() -> bool {
    cfg!(target_os = "macos")
}

/// The virtual window's size with no viewer: 1280×720 points at 2x.
pub const DEFAULT_SIZE: (f32, f32, f32) = (1280.0, 720.0, 2.0);

/// Spawns a primary window with no native window behind it, `w`×`h` physical pixels at `scale`,
/// with `extra` in the same spawn: viewport mode's virtual window, and the headless pointer's (see
/// [`offscreen`]). Bevy renders nothing for it, having no handle to render to.
pub fn spawn_windowless_primary(
    world: &mut bevy::prelude::World,
    title: &str,
    (w, h, scale): (u32, u32, f32),
    extra: impl bevy::prelude::Bundle,
) -> bevy::prelude::Entity {
    use bevy::window::{PrimaryWindow, Window, WindowResolution};
    let mut resolution = WindowResolution::new(w, h);
    resolution.set_scale_factor(scale);
    world.spawn((Window { title: title.into(), resolution, focused: false, ..Default::default() }, PrimaryWindow, extra)).id()
}

#[cfg(target_os = "macos")]
pub use plugin::*;

#[cfg(target_os = "macos")]
mod plugin {
    use super::*;
    use crate::shared::{Shared, SharedRes};
    use bevy::prelude::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Viewport mode, or the viewport's edit tools in a window of the engine's own.
    ///
    /// Viewport mode (`windowed: false`) needs an app built without `WinitPlugin` and with no
    /// primary window: it spawns the virtual primary window and installs the tick-driven runner.
    #[derive(Default)]
    pub struct ViewportPlugin {
        pub windowed: bool,
    }

    impl Plugin for ViewportPlugin {
        fn build(&self, app: &mut App) {
            let shared = Shared::game();
            app.insert_resource(SharedRes(shared.clone()))
                .init_resource::<UpdateCount>()
                .add_systems(First, count_update);
            edit::build(app);
            edits::build(app);
            assets::build(app);
            if !self.windowed {
                let (w, h, scale) = DEFAULT_SIZE;
                spawn_windowless_primary(app.world_mut(), "bevy-ts viewport", ((w * scale) as u32, (h * scale) as u32, scale), VirtualWindow);
                app.init_resource::<input::PendingInput>()
                    .init_resource::<input::Remote>()
                    .init_resource::<feedback::Cursor>()
                    .add_systems(First, (methods::drain, input::apply).chain().after(count_update))
                    .add_systems(Last, feedback::send_changes)
                    .add_systems(
                        PreUpdate,
                        input::gate_gamepads
                            .in_set(bevy::input::InputSystems)
                            .before(bevy::input::gamepad::gamepad_connection_system)
                            .run_if(resource_exists::<Messages<bevy::input::gamepad::RawGamepadEvent>>),
                    )
                    .set_runner(runner::runner);
                preview::build(app);
                orient::build(app);
                ring::build_render(app, shared);
            }
        }

        fn finish(&self, app: &mut App) {
            // Methods go in once `RemotePlugin` (added in any order) has made `RemoteMethods`.
            if app.world().contains_resource::<bevy::remote::RemoteMethods>() {
                if !self.windowed {
                    methods::add_viewer_methods(app);
                    preview::add_methods(app);
                }
                methods::add_common_methods(app);
                methods::add_edit_methods(app);
                methods::add_edit_result_methods(app);
                assets::add_methods(app);
            }
            if !self.windowed {
                ring::finish_render(app);
            }
        }
    }

    fn count_update(mut n: ResMut<UpdateCount>) {
        n.0 += 1;
    }

    static ATTACHES: AtomicU32 = AtomicU32::new(0);

    /// A bootstrap name unique to the process and the attach.
    pub fn service_name() -> String {
        format!("dev.bevy-ts.viewport.{}.{}", std::process::id(), ATTACHES.fetch_add(1, Ordering::Relaxed))
    }

    /// Opens a new channel for the game viewport's viewer, replacing any earlier one, and answers
    /// its bootstrap name.
    pub fn attach(world: &mut World) -> Result<String, String> {
        let shared = world.resource::<SharedRes>().0.clone();
        let name = service_name();
        shared.open(&name, input::INPUTS)?;
        Ok(name)
    }
}
