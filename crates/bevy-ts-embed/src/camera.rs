//! The engine's own exported params in `bevy-ts/camera`. `Cameras` converts between world,
//! viewport and NDC: Bevy's `Camera` conversions, a viewport point in logical pixels from its
//! top-left, the units of a UI node's `left` and `top`. `MeshRayCast` casts a ray against meshes:
//! Bevy's `MeshRayCast`.

use bevy::ecs::system::SystemParam;
use bevy::picking::mesh_picking::ray_cast::{self, MeshRayCastSettings};
use bevy_ts_native::prelude::*;

/// Exports `Cameras` and `MeshRayCast`. Every engine adds it.
pub struct CamerasPlugin;

impl Plugin for CamerasPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<Cameras<'static, 'static>>().ts_param::<MeshRayCaster<'static, 'static>>();
    }
}

/// Bevy's `Ray3d`: `direction` is a unit vector when Bevy gives it.
#[derive(HostRecord, Clone, Copy, Debug)]
pub struct Ray3d {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl From<bevy::shape::Ray3d> for Ray3d {
    fn from(r: bevy::shape::Ray3d) -> Self {
        Ray3d { origin: r.origin, direction: *r.direction }
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
        if !points.len().is_multiple_of(3) || out.len() < points.len() / 3 * 2 {
            return Err(format!("points holds {} floats and out {}: give 3 per point and 2 out per point", points.len(), out.len()));
        }
        let mut n = 0;
        for (p, o) in points.as_chunks::<3>().0.iter().zip(out.as_chunks_mut::<2>().0) {
            let v = cam.world_to_viewport(at, Vec3::from_array(*p)).ok();
            let v = v.unwrap_or(Vec2::NAN);
            *o = v.to_array();
            n += v.is_finite() as u32;
        }
        Ok(n)
    }

    /// Where `point` lands in `camera`'s viewport, with its view-space depth as `z`, or
    /// `undefined` when it does not project.
    pub fn world_to_viewport_with_depth(&self, camera: Entity, point: Vec3) -> Result<Option<Vec3>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.world_to_viewport_with_depth(at, point).ok())
    }

    /// The ray from the near plane through viewport `point`, `direction` a unit vector, or
    /// `undefined` (no viewport size yet, or a projection Bevy cannot invert).
    pub fn viewport_to_world(&self, camera: Entity, point: Vec2) -> Result<Option<Ray3d>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.viewport_to_world(at, point).ok().map(Ray3d::from))
    }

    /// The world point under viewport `point` for a 2D camera, or `undefined`.
    pub fn viewport_to_world_2d(&self, camera: Entity, point: Vec2) -> Result<Option<Vec2>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.viewport_to_world_2d(at, point).ok())
    }

    /// `point` in normalized device coordinates, or `undefined` when it does not project.
    pub fn world_to_ndc(&self, camera: Entity, point: Vec3) -> Result<Option<Vec3>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.world_to_ndc(at, point))
    }

    /// The world point at `ndc`, or `undefined` when Bevy cannot invert the projection.
    pub fn ndc_to_world(&self, camera: Entity, ndc: Vec3) -> Result<Option<Vec3>, String> {
        let (cam, at) = self.camera(camera)?;
        Ok(cam.ndc_to_world(at, ndc))
    }

    /// Viewport `point` in normalized device coordinates, or `undefined` before Bevy knows the
    /// viewport.
    pub fn viewport_to_ndc(&self, camera: Entity, point: Vec2) -> Result<Option<Vec2>, String> {
        Ok(self.camera(camera)?.0.viewport_to_ndc(point).ok())
    }

    /// Many rays in one call: `points` holds two floats per viewport point, `out` receives six
    /// per point (origin, direction; six `NaN` for a miss). Returns how many converted.
    pub fn viewports_to_world(&self, camera: Entity, points: &[f32], out: &mut [f32]) -> Result<u32, String> {
        let (cam, at) = self.camera(camera)?;
        if !points.len().is_multiple_of(2) || out.len() < points.len() / 2 * 6 {
            return Err(format!("points holds {} floats and out {}: give 2 per point and 6 out per point", points.len(), out.len()));
        }
        let mut n = 0;
        for (p, o) in points.as_chunks::<2>().0.iter().zip(out.as_chunks_mut::<6>().0) {
            match cam.viewport_to_world(at, Vec2::from_array(*p)) {
                Ok(ray) => {
                    o[..3].copy_from_slice(&ray.origin.to_array());
                    o[3..].copy_from_slice(&ray.direction.to_array());
                    n += 1;
                }
                Err(_) => o.fill(f32::NAN),
            }
        }
        Ok(n)
    }
}

/// Which meshes a cast considers by visibility: Bevy's `RayCastVisibility`. The first variant is
/// the one a record that leaves it out gets, Bevy's default.
#[derive(HostRecord, Clone, Copy, Debug, Default)]
pub enum RayCastVisibility {
    /// Visible in the hierarchy and drawn by a camera or light last frame.
    #[default]
    VisibleInView,
    /// Visible in the hierarchy.
    Visible,
    /// Every mesh, hidden ones too.
    Any,
}

impl From<RayCastVisibility> for ray_cast::RayCastVisibility {
    fn from(v: RayCastVisibility) -> Self {
        match v {
            RayCastVisibility::VisibleInView => ray_cast::RayCastVisibility::VisibleInView,
            RayCastVisibility::Visible => ray_cast::RayCastVisibility::Visible,
            RayCastVisibility::Any => ray_cast::RayCastVisibility::Any,
        }
    }
}

/// A cast's settings. `early_exit` stops at the nearest hit (Bevy's default); `false` gathers
/// every hit along the ray.
#[derive(HostRecord, Clone, Copy, Debug)]
pub struct RayCastSettings {
    pub visibility: RayCastVisibility,
    pub early_exit: bool,
}

impl Default for RayCastSettings {
    fn default() -> Self {
        RayCastSettings { visibility: RayCastVisibility::VisibleInView, early_exit: true }
    }
}

/// A hit: Bevy's `RayMeshHit` and the entity hit. `normal` is the triangle's, not normalized
/// under non-uniform scale.
#[derive(HostRecord, Clone, Copy, Debug)]
pub struct RayMeshHit {
    pub entity: Entity,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub uv: Option<Vec2>,
    pub triangle_index: Option<u32>,
}

/// Bevy's `MeshRayCast`, exported to TS under its name. A mesh is hit from the frame after it
/// gets its `Aabb` (`PostUpdate`).
#[derive(SystemParam)]
pub struct MeshRayCaster<'w, 's> {
    cast: ray_cast::MeshRayCast<'w, 's>,
}

fn always(_: Entity) -> bool {
    true
}

fn never(_: Entity) -> bool {
    false
}

impl MeshRayCaster<'_, '_> {
    /// Every hit, nearest first. Bevy fills its `Local` scratch, so this takes `&mut self`.
    fn hits(&mut self, ray: Ray3d, settings: Option<RayCastSettings>, exclude: &[f64]) -> Result<&[(Entity, ray_cast::RayMeshHit)], String> {
        let direction = Dir3::new(ray.direction).map_err(|_| format!("ray direction {} is zero or not finite", ray.direction))?;
        let s = settings.unwrap_or_default();
        let filter = |e: Entity| !exclude.contains(&(e.to_bits() as f64));
        let early_exit: &dyn Fn(Entity) -> bool = if s.early_exit { &always } else { &never };
        let settings = MeshRayCastSettings { visibility: s.visibility.into(), filter: &filter, early_exit_test: early_exit };
        Ok(self.cast.cast_ray(bevy::shape::Ray3d::new(ray.origin, direction), &settings))
    }
}

#[export(ts = "MeshRayCast", module = "bevy-ts/camera")]
impl MeshRayCaster<'_, '_> {
    /// The nearest mesh `ray` hits, or `undefined`. `settings` `undefined` is Bevy's default
    /// (`VisibleInView`, stop at the nearest); `exclude` holds entity bits to skip.
    pub fn cast_ray(&mut self, ray: Ray3d, settings: Option<RayCastSettings>, exclude: &[f64]) -> Result<Option<RayMeshHit>, String> {
        Ok(self.hits(ray, settings, exclude)?.first().map(|(e, h)| RayMeshHit {
            entity: *e,
            point: h.point,
            normal: h.normal,
            distance: h.distance,
            uv: h.uv,
            triangle_index: h.triangle_index.map(|i| i as u32),
        }))
    }

    /// Every hit, nearest first: `hits` receives seven floats per hit (point, normal, distance)
    /// and `entity` its bits. Returns how many it wrote; hits past the room are dropped. Pass
    /// `earlyExit: false` for more than the nearest.
    pub fn cast_ray_all(&mut self, ray: Ray3d, settings: Option<RayCastSettings>, exclude: &[f64], hits: &mut [f32], entity: &mut [f64]) -> Result<u32, String> {
        if !hits.len().is_multiple_of(7) {
            return Err(format!("hits holds {} floats: give 7 per hit", hits.len()));
        }
        let found = self.hits(ray, settings, exclude)?;
        let mut n = 0;
        for ((e, h), (o, b)) in found.iter().zip(hits.as_chunks_mut::<7>().0.iter_mut().zip(entity.iter_mut())) {
            o[..3].copy_from_slice(&h.point.to_array());
            o[3..6].copy_from_slice(&h.normal.to_array());
            o[6] = h.distance;
            *b = e.to_bits() as f64;
            n += 1;
        }
        Ok(n)
    }
}
