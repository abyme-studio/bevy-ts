//! Native spawn recipes for things TS cannot build yet: `camera3d`, `directionalLight` and
//! `cuboid`. Params cross as JSON; this path is for rare spawns, not per-frame work.

use crate::plugin_api::BevyTsAppExt;
use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Placement {
    #[serde(default)]
    position: Option<[f32; 3]>,
    #[serde(default)]
    look_at: Option<[f32; 3]>,
}

impl Placement {
    fn transform(&self) -> Transform {
        let mut t = Transform::from_translation(self.position.map_or(Vec3::ZERO, Vec3::from));
        if let Some(target) = self.look_at {
            t.look_at(Vec3::from(target), Vec3::Y);
        }
        t
    }
}

fn parse<T: for<'de> Deserialize<'de> + Default>(params: &Value) -> Result<T, String> {
    if params.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(params.clone()).map_err(|e| e.to_string())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LightParams {
    #[serde(flatten)]
    place: PlacementFields,
    #[serde(default)]
    illuminance: Option<f32>,
    #[serde(default)]
    shadows: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CuboidParams {
    #[serde(default)]
    size: Option<[f32; 3]>,
    #[serde(default)]
    color: Option<String>,
    #[serde(flatten)]
    place: PlacementFields,
}

/// Placement fields for structs that also carry their own (serde cannot flatten with
/// `deny_unknown_fields` on the inner struct).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PlacementFields {
    #[serde(default)]
    position: Option<[f32; 3]>,
    #[serde(default)]
    look_at: Option<[f32; 3]>,
}

impl PlacementFields {
    fn transform(&self) -> Transform {
        Placement { position: self.position, look_at: self.look_at }.transform()
    }
}

fn color(s: &str) -> Result<Color, String> {
    Srgba::hex(s).map(Color::from).map_err(|e| format!("color {s:?}: {e}"))
}

pub fn register(app: &mut App) {
    app.add_ts_native_recipe("camera3d", |world, e, params| {
        let p: Placement = parse(params)?;
        world.entity_mut(e).insert((Camera3d::default(), p.transform()));
        Ok(())
    });
    app.add_ts_native_recipe("directionalLight", |world, e, params| {
        let p: LightParams = parse(params)?;
        let mut light = DirectionalLight { shadow_maps_enabled: p.shadows.unwrap_or(true), ..default() };
        if let Some(lux) = p.illuminance {
            light.illuminance = lux;
        }
        world.entity_mut(e).insert((light, p.place.transform()));
        Ok(())
    });
    app.add_ts_native_recipe("cuboid", |world, e, params| {
        let p: CuboidParams = parse(params)?;
        // Placement first, so the entity is placed even if rendering is missing.
        world.entity_mut(e).insert(p.place.transform());
        let size = Vec3::from(p.size.unwrap_or([1.0, 1.0, 1.0]));
        let base_color = color(p.color.as_deref().unwrap_or("#ffffff"))?;
        let mesh = world
            .get_resource_mut::<Assets<Mesh>>()
            .ok_or("cuboid needs the render plugins (no Assets<Mesh>)")?
            .add(Cuboid::from_size(size));
        let material = world
            .get_resource_mut::<Assets<StandardMaterial>>()
            .ok_or("cuboid needs the PBR plugin (no Assets<StandardMaterial>)")?
            .add(StandardMaterial { base_color, ..default() });
        world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(material)));
        Ok(())
    });
}
