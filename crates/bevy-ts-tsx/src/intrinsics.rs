//! `<mesh>` and `<material>`: asset intrinsics built in Rust. Equal props share one asset
//! handle, so a thousand identical enemies make one mesh and one material. `<material kind={K}>`
//! is a script material (bevy-ts-material), shared the same way.

use bevy::prelude::*;
use bevy_ts_material::kinds::AlphaDecl;
use bevy_ts_material::{GltfMaterialFilter, ScriptMaterial3d};
use crate::shapes::MeshShape;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// `<mesh shape .../>`: a Bevy mesh primitive (see `shapes`) and an optional color.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshProps {
    pub shape: MeshShape,
    /// Shorthand for a `<material color>` on the same entity.
    pub color: Option<Value>,
}

// By hand: `color` comes out first, and the rest is the shape's (`flatten` and
// `deny_unknown_fields` do not combine).
impl<'de> Deserialize<'de> for MeshProps {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut props = Map::<String, Value>::deserialize(d)?;
        let color = match props.remove("color") {
            None | Some(Value::Null) => None,
            // Any colour form; checked by `check`.
            Some(v) => Some(v),
        };
        let shape = MeshShape::from_props(props).map_err(D::Error::custom)?;
        Ok(MeshProps { shape, color })
    }
}

impl MeshProps {
    pub fn check(&self) -> Result<(), String> {
        self.shape.check()?;
        if let Some(c) = &self.color {
            color(c)?;
        }
        Ok(())
    }

    fn mesh(&self) -> Mesh {
        self.shape.mesh()
    }

    fn cache_key(&self) -> String {
        format!("{:?}", self.shape)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialProps {
    #[serde(default)]
    pub color: Option<Value>,
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default)]
    pub roughness: Option<f32>,
    #[serde(default)]
    pub emissive: Option<Value>,
    #[serde(default)]
    pub unlit: Option<bool>,
    /// Replaces `color`'s alpha, 0 to 1.
    #[serde(default)]
    pub alpha: Option<f32>,
    /// A script material's `alphaMode` names; unset, `Blend` below alpha 1 and `Opaque` otherwise.
    #[serde(default, rename = "alphaMode")]
    pub alpha_mode: Option<AlphaDecl>,
}

impl MaterialProps {
    pub fn check(&self) -> Result<(), String> {
        for c in [&self.color, &self.emissive].into_iter().flatten() {
            color(c)?;
        }
        if let Some(a) = self.alpha
            && !(0.0..=1.0).contains(&a)
        {
            return Err(format!("alpha takes 0 to 1, not {a}"));
        }
        if let Some(m) = &self.alpha_mode {
            m.parse()?;
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
        if let Some(a) = self.alpha {
            m.base_color.set_alpha(a);
        }
        // Bevy's own rule in `StandardMaterial::from(Color)`.
        m.alpha_mode = match &self.alpha_mode {
            Some(d) => d.parse().expect("checked"),
            None if m.base_color.alpha() < 1.0 => AlphaMode::Blend,
            None => AlphaMode::Opaque,
        };
        m
    }

    fn cache_key(&self) -> String {
        format!("{self:?}")
    }
}

fn color(v: &Value) -> Result<Color, String> {
    bevy_ts::color::parse(v)
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
