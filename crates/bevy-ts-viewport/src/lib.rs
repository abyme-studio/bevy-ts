//! bevy-ts viewport mode (openspec change `viewport`): the engine has no window of its own and
//! renders into a ring of IOSurfaces another process shows, paced by that process's display,
//! taking its input over the same Mach channel.
//!
//! macOS only. On other platforms the crate builds empty and [`supported`] is false.

#[cfg(target_os = "macos")]
mod feedback;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
mod keys;
#[cfg(target_os = "macos")]
pub mod mach;
#[cfg(target_os = "macos")]
mod methods;
#[cfg(target_os = "macos")]
mod ring;
#[cfg(target_os = "macos")]
pub mod runner;
#[cfg(target_os = "macos")]
pub mod shared;
#[cfg(target_os = "macos")]
pub mod surface;

#[cfg(target_os = "macos")]
pub use ring::{UpdateCount, VirtualWindow};

/// Whether viewport mode exists on this platform.
pub const fn supported() -> bool {
    cfg!(target_os = "macos")
}

/// The virtual window's size with no viewer: 1280×720 points at 2x.
pub const DEFAULT_SIZE: (f32, f32, f32) = (1280.0, 720.0, 2.0);

#[cfg(target_os = "macos")]
pub use plugin::*;

#[cfg(target_os = "macos")]
mod plugin {
    use super::*;
    use crate::shared::{Shared, SharedRes};
    use bevy::prelude::*;
    use bevy::window::{PrimaryWindow, WindowResolution};
    use std::sync::Arc;
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
            let shared = Arc::new(Shared::default());
            app.insert_resource(SharedRes(shared.clone()))
                .init_resource::<UpdateCount>()
                .add_systems(First, count_update);
            if !self.windowed {
                let (w, h, scale) = DEFAULT_SIZE;
                let mut resolution = WindowResolution::new((w * scale) as u32, (h * scale) as u32);
                resolution.set_scale_factor(scale);
                app.world_mut().spawn((
                    Window { title: "bevy-ts viewport".into(), resolution, focused: false, ..default() },
                    PrimaryWindow,
                    VirtualWindow,
                ));
                app.init_resource::<input::PendingInput>()
                    .init_resource::<feedback::Cursor>()
                    .add_systems(First, (methods::drain, input::apply).chain().after(count_update))
                    .add_systems(Last, feedback::send_changes)
                    .set_runner(runner::runner);
                ring::build_render(app, shared);
            }
        }

        fn finish(&self, app: &mut App) {
            // Methods go in once `RemotePlugin` (added in any order) has made `RemoteMethods`.
            if app.world().contains_resource::<bevy::remote::RemoteMethods>() {
                if !self.windowed {
                    methods::add_viewer_methods(app);
                }
                methods::add_common_methods(app);
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

    /// Opens a new channel for a viewer, replacing any earlier one, and answers its bootstrap name.
    pub fn attach(world: &mut World) -> Result<String, String> {
        let shared = world.resource::<SharedRes>().0.clone();
        let name = format!("dev.bevy-ts.viewport.{}.{}", std::process::id(), ATTACHES.fetch_add(1, Ordering::Relaxed));
        shared.open(&name)?;
        Ok(name)
    }
}
