//! `Cameras`, the engine's own exported param (`bevy-ts/camera`): projects world points to a
//! camera's viewport, in logical pixels from its top-left, the units of a UI node's `left` and
//! `top`. Bevy's `Camera::world_to_viewport`, reachable from TS.

use bevy::ecs::system::SystemParam;
use bevy_ts_native::prelude::*;

/// Exports `Cameras`. Every engine adds it.
pub struct CamerasPlugin;

impl Plugin for CamerasPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<Cameras<'static, 'static>>();
    }
}

/// Every camera and its transform as last propagated: from `Update`, the previous frame's.
#[derive(SystemParam)]
pub struct Cameras<'w, 's> {
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform)>,
}

impl Cameras<'_, '_> {
    fn camera(&self, camera: Entity) -> Result<(&Camera, &GlobalTransform), String> {
        self.cameras.get(camera).map_err(|_| format!("entity {} is not a camera", camera.to_bits()))
    }
}

#[export(ts = "Cameras", module = "bevy-ts/camera")]
impl Cameras<'_, '_> {
    /// Where `point` lands in `camera`'s viewport, or `undefined` when it does not project
    /// (behind the camera, past the far plane, no viewport size yet). Off screen in front gives
    /// coordinates outside the viewport.
    pub fn world_to_viewport(&self, camera: Entity, point: Vec3) -> Result<Option<Vec2>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.world_to_viewport(at, point).ok())
    }

    /// The viewport's logical size, or `undefined` before Bevy knows it.
    pub fn logical_viewport_size(&self, camera: Entity) -> Result<Option<Vec2>, String> {
        Ok(self.camera(camera)?.0.logical_viewport_size())
    }

    /// Many points in one call: `points` holds three floats per point, `out` receives two per
    /// point (`NaN, NaN` for one that does not project). Returns how many projected.
    pub fn worlds_to_viewport(&self, camera: Entity, points: &[f32], out: &mut [f32]) -> Result<u32, String> {
        let (cam, at) = self.camera(camera)?;
        let mut n = 0;
        for (p, o) in points.chunks_exact(3).zip(out.chunks_exact_mut(2)) {
            let v = cam.world_to_viewport(at, Vec3::new(p[0], p[1], p[2])).ok();
            let v = v.unwrap_or(Vec2::NAN);
            o.copy_from_slice(&[v.x, v.y]);
            n += v.is_finite() as u32;
        }
        Ok(n)
    }
}
