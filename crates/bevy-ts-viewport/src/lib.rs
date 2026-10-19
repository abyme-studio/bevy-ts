//! bevy-ts viewport mode (openspec change `viewport`): the engine has no window of its own and
//! renders into a ring of IOSurfaces another process shows, paced by that process's display,
//! taking its input over the same Mach channel.
//!
//! macOS only. On other platforms the crate builds empty and [`supported`] is false.

#[cfg(target_os = "macos")]
pub mod mach;
#[cfg(target_os = "macos")]
mod ring;
#[cfg(target_os = "macos")]
pub mod shared;
#[cfg(target_os = "macos")]
pub mod surface;

#[cfg(target_os = "macos")]
pub use ring::VirtualWindow;

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

    /// Viewport mode. Add it to an app built without `WinitPlugin` and with no primary window;
    /// it spawns the virtual primary window.
    #[derive(Default)]
    pub struct ViewportPlugin;

    impl Plugin for ViewportPlugin {
        fn build(&self, app: &mut App) {
            let shared = Arc::new(Shared::default());
            app.insert_resource(SharedRes(shared.clone()));
            let (w, h, scale) = DEFAULT_SIZE;
            let mut resolution = WindowResolution::new((w * scale) as u32, (h * scale) as u32);
            resolution.set_scale_factor(scale);
            app.world_mut().spawn((
                Window { title: "bevy-ts viewport".into(), resolution, ..default() },
                PrimaryWindow,
                VirtualWindow,
            ));
            ring::build_render(app, shared);
        }

        fn finish(&self, app: &mut App) {
            ring::finish_render(app);
        }
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
