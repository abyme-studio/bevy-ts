//! `<mesh>` and `<material>`: asset intrinsics built in Rust. Equal props share one asset
//! handle, so a thousand identical enemies make one mesh and one material. `<material kind={K}>`
//! is a script material (bevy-ts-material), shared the same way.

use bevy::prelude::*;
use bevy_ts_material::{GltfMaterialFilter, ScriptMaterial3d};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshProps {
    /// `cuboid` (`size: [x, y, z]`), `sphere` (`radius`) or `plane` (`size: [x, z]`).
    pub shape: String,
    #[serde(default)]
    pub size: Option<Vec<f32>>,
    #[serde(default)]
    pub radius: Option<f32>,
    /// Shorthand for a `<material color>` on the same entity.
    #[serde(default)]
    pub color: Option<String>,
}

impl MeshProps {
    pub fn check(&self) -> Result<(), String> {
        match (self.shape.as_str(), self.size.as_ref().map(Vec::len)) {
            ("cuboid", None | Some(3)) | ("plane", None | Some(2)) | ("sphere", None) => {}
            ("cuboid", Some(n)) => return Err(format!("a cuboid's size has 3 numbers, not {n}")),
            ("plane", Some(n)) => return Err(format!("a plane's size has 2 numbers, not {n}")),
            ("sphere", Some(_)) => return Err("a sphere takes radius, not size".into()),
            (other, _) => return Err(format!("unknown shape {other:?}; use cuboid, sphere or plane")),
        }
        if let Some(c) = &self.color {
            color(c)?;
        }
        Ok(())
    }

    fn mesh(&self) -> Mesh {
        match self.shape.as_str() {
            "cuboid" => {
                let s = self.size.clone().unwrap_or(vec![1.0, 1.0, 1.0]);
                Cuboid::new(s[0], s[1], s[2]).into()
            }
            "plane" => {
                let s = self.size.clone().unwrap_or(vec![1.0, 1.0]);
                Plane3d::default().mesh().size(s[0], s[1]).into()
            }
            _ => Sphere::new(self.radius.unwrap_or(0.5)).into(),
        }
    }

    fn cache_key(&self) -> String {
        format!("{}|{:?}|{:?}", self.shape, self.size, self.radius)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialProps {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default)]
    pub roughness: Option<f32>,
    #[serde(default)]
    pub emissive: Option<String>,
    #[serde(default)]
    pub unlit: Option<bool>,
}

impl MaterialProps {
    pub fn check(&self) -> Result<(), String> {
        for c in [&self.color, &self.emissive].into_iter().flatten() {
            color(c)?;
        }
        Ok(())
    }

    fn material(&self) -> StandardMaterial {
        let mut m = StandardMaterial::default();
        if let Some(c) = &self.color {
            m.base_color = color(c).expect("checked");
        }
        if let Some(v) = self.metallic {
            m.metallic = v;
        }
        if let Some(v) = self.roughness {
            m.perceptual_roughness = v;
        }
        if let Some(c) = &self.emissive {
            m.emissive = color(c).expect("checked").to_linear();
        }
        if let Some(u) = self.unlit {
            m.unlit = u;
        }
        m
    }

    fn cache_key(&self) -> String {
        format!("{self:?}")
    }
}

fn color(s: &str) -> Result<Color, String> {
    Srgba::hex(s).map(Color::from).map_err(|e| format!("color {s:?}: {e}"))
}

/// Asset handles by props, shared by every element with the same props.
#[derive(Resource, Default)]
pub struct IntrinsicAssets {
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<String, Handle<StandardMaterial>>,
    /// Assets built, for tests: a repeat of known props builds nothing.
    pub built: usize,
}

/// True when the app can build meshes and materials.
pub fn available(world: &World) -> bool {
    world.contains_resource::<Assets<Mesh>>() && world.contains_resource::<Assets<StandardMaterial>>()
}

fn mesh_handle(world: &mut World, p: &MeshProps) -> Handle<Mesh> {
    let key = p.cache_key();
    if let Some(h) = world.resource::<IntrinsicAssets>().meshes.get(&key) {
        return h.clone();
    }
    let h = world.resource_mut::<Assets<Mesh>>().add(p.mesh());
    let mut cache = world.resource_mut::<IntrinsicAssets>();
    cache.built += 1;
    cache.meshes.insert(key, h.clone());
    h
}

fn material_handle(world: &mut World, p: &MaterialProps) -> Handle<StandardMaterial> {
    let key = p.cache_key();
    if let Some(h) = world.resource::<IntrinsicAssets>().materials.get(&key) {
        return h.clone();
    }
    let h = world.resource_mut::<Assets<StandardMaterial>>().add(p.material());
    let mut cache = world.resource_mut::<IntrinsicAssets>();
    cache.built += 1;
    cache.materials.insert(key, h.clone());
    h
}

/// Inserts (or replaces) what a `<mesh>` owns.
pub fn insert_mesh(world: &mut World, e: Entity, p: &MeshProps) {
    let mesh = mesh_handle(world, p);
    let material = p.color.as_ref().map(|c| material_handle(world, &MaterialProps { color: Some(c.clone()), ..default() }));
    let mut em = world.entity_mut(e);
    em.insert(Mesh3d(mesh));
    if let Some(m) = material {
        em.insert(MeshMaterial3d(m));
    }
}

pub fn remove_mesh(world: &mut World, e: Entity, p: &MeshProps) {
    let mut em = world.entity_mut(e);
    em.remove::<Mesh3d>();
    if p.color.is_some() {
        em.remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

pub fn insert_material(world: &mut World, e: Entity, p: &MaterialProps) {
    let m = material_handle(world, p);
    world.entity_mut(e).remove::<(ScriptMaterial3d, GltfMaterialFilter)>().insert(MeshMaterial3d(m));
}

/// `<material kind={K} .../>`: the kind's instance for these props (equal props share one). On a
/// glTF scene root, bevy-ts-material puts it on the scene's meshes, those named `gltf_material`
/// when given.
pub fn insert_script_material(
    world: &mut World,
    e: Entity,
    kind: &str,
    props: &serde_json::Map<String, serde_json::Value>,
    gltf_material: Option<&str>,
    at: Option<bevy_ts::backend::SourceLocation>,
) {
    if !world.contains_resource::<bevy_ts_material::MaterialKinds>() {
        crate::reconcile::log_error(world, "<material kind>: script materials need ScriptMaterialPlugin".into(), at);
        return;
    }
    match bevy_ts_material::kinds::instance(world, kind, props) {
        Some(h) => {
            let mut em = world.entity_mut(e);
            em.remove::<MeshMaterial3d<StandardMaterial>>().insert(ScriptMaterial3d(h));
            match gltf_material {
                Some(name) if em.get::<GltfMaterialFilter>().is_none_or(|f| f.0 != name) => {
                    em.insert(GltfMaterialFilter(name.into()));
                }
                Some(_) => {}
                None => {
                    em.remove::<GltfMaterialFilter>();
                }
            }
        }
        None => crate::reconcile::log_error(world, format!("<material kind>: no material {kind}"), at),
    }
}

pub fn remove_material(world: &mut World, e: Entity) {
    world.entity_mut(e).remove::<(MeshMaterial3d<StandardMaterial>, ScriptMaterial3d, GltfMaterialFilter)>();
}
