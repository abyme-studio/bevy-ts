//! `<mesh>` and `<material>`: asset intrinsics built in Rust. Equal props share one asset
//! handle, so a thousand identical enemies make one mesh and one material. `<material kind={K}>`
//! is a script material (bevy-ts-material), shared the same way.

use bevy::asset::AssetLoadFailedEvent;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::image::ImageLoaderSettings;
use bevy::math::Affine2;
use bevy::mesh::UvChannel;
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::ParallaxMappingMethod;
use bevy::prelude::*;
use bevy_ts_material::kinds::AlphaDecl;
use bevy::render::render_resource::Face;
use bevy_ts::backend::SourceLocation;
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
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MaterialProps {
    /// `base_color`.
    pub color: Option<Value>,
    pub base_color_channel: Option<Uv>,
    pub base_color_texture: Option<String>,
    pub emissive: Option<Value>,
    pub emissive_exposure_weight: Option<f32>,
    pub emissive_channel: Option<Uv>,
    pub emissive_texture: Option<String>,
    /// `perceptual_roughness`.
    pub roughness: Option<f32>,
    pub metallic: Option<f32>,
    pub metallic_roughness_channel: Option<Uv>,
    pub metallic_roughness_texture: Option<String>,
    pub reflectance: Option<f32>,
    pub specular_tint: Option<Value>,
    pub diffuse_transmission: Option<f32>,
    pub specular_transmission: Option<f32>,
    pub thickness: Option<f32>,
    pub ior: Option<f32>,
    pub attenuation_distance: Option<f32>,
    pub attenuation_color: Option<Value>,
    pub normal_map_channel: Option<Uv>,
    pub normal_map_texture: Option<String>,
    pub flip_normal_map_y: Option<bool>,
    pub occlusion_channel: Option<Uv>,
    pub occlusion_texture: Option<String>,
    pub clearcoat: Option<f32>,
    pub clearcoat_perceptual_roughness: Option<f32>,
    pub anisotropy_strength: Option<f32>,
    pub anisotropy_rotation: Option<f32>,
    pub double_sided: Option<bool>,
    /// Unset keeps Bevy's `Some(Back)`; `null` is no culling.
    #[serde(deserialize_with = "present")]
    pub cull_mode: Option<CullMode>,
    pub unlit: Option<bool>,
    /// Replaces `color`'s alpha, 0 to 1.
    #[serde(default)]
    pub alpha: Option<f32>,
    /// A script material's `alphaMode` names; unset, `Blend` below alpha 1 and `Opaque` otherwise.
    #[serde(default, rename = "alphaMode")]
    pub alpha_mode: Option<AlphaDecl>,
    pub fog_enabled: Option<bool>,
    pub depth_bias: Option<f32>,
    pub depth_map: Option<String>,
    pub parallax_depth_scale: Option<f32>,
    pub parallax_mapping_method: Option<Parallax>,
    pub max_parallax_layer_count: Option<f32>,
    pub lightmap_exposure: Option<f32>,
    pub opaque_render_method: Option<RenderMethod>,
    pub deferred_lighting_pass_id: Option<u8>,
    pub uv_transform: Option<UvTransform>,
}

/// A UV channel prop: `"uv0"` or `"uv1"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Uv {
    Uv0,
    Uv1,
}

impl From<Uv> for UvChannel {
    fn from(u: Uv) -> Self {
        match u {
            Uv::Uv0 => UvChannel::Uv0,
            Uv::Uv1 => UvChannel::Uv1,
        }
    }
}

/// `cullMode`: `"back"`, `"front"` or `null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CullMode(pub Option<Face>);

impl<'de> Deserialize<'de> for CullMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Option::<String>::deserialize(d)?.as_deref() {
            None => Ok(CullMode(None)),
            Some("back") => Ok(CullMode(Some(Face::Back))),
            Some("front") => Ok(CullMode(Some(Face::Front))),
            Some(o) => Err(serde::de::Error::custom(format!("cullMode {o:?}: use \"back\", \"front\" or null"))),
        }
    }
}

/// A prop that is there, even as `null`.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// `opaqueRenderMethod`: `"forward"`, `"deferred"` or `"auto"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderMethod {
    Forward,
    Deferred,
    Auto,
}

impl From<RenderMethod> for OpaqueRendererMethod {
    fn from(r: RenderMethod) -> Self {
        match r {
            RenderMethod::Forward => OpaqueRendererMethod::Forward,
            RenderMethod::Deferred => OpaqueRendererMethod::Deferred,
            RenderMethod::Auto => OpaqueRendererMethod::Auto,
        }
    }
}

/// `parallaxMappingMethod`: `"occlusion"` or `{ relief: { maxSteps } }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Parallax {
    Occlusion,
    Relief { max_steps: u32 },
}

impl From<Parallax> for ParallaxMappingMethod {
    fn from(p: Parallax) -> Self {
        match p {
            Parallax::Occlusion => ParallaxMappingMethod::Occlusion,
            Parallax::Relief { max_steps } => ParallaxMappingMethod::Relief { max_steps },
        }
    }
}

/// `uvTransform`: Bevy's `Affine2::from_scale_angle_translation`, unset parts at identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UvTransform {
    pub scale: Option<[f32; 2]>,
    /// Radians.
    pub rotation: Option<f32>,
    pub translation: Option<[f32; 2]>,
}

impl UvTransform {
    fn affine(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(
            self.scale.map_or(Vec2::ONE, Vec2::from),
            self.rotation.unwrap_or(0.0),
            self.translation.map_or(Vec2::ZERO, Vec2::from),
        )
    }
}

/// Copy-type props named as their field: `$f` sets `m.$f`.
macro_rules! plain_fields {
    ($($f:ident),* $(,)?) => {
        impl MaterialProps {
            fn set_plain(&self, m: &mut StandardMaterial) {
                $(if let Some(v) = self.$f { m.$f = v.into(); })*
            }
        }
    };
}

plain_fields!(
    base_color_channel, emissive_exposure_weight, emissive_channel, metallic, metallic_roughness_channel,
    reflectance, diffuse_transmission, specular_transmission, thickness, ior, attenuation_distance,
    normal_map_channel, flip_normal_map_y, occlusion_channel, clearcoat, clearcoat_perceptual_roughness,
    anisotropy_strength, anisotropy_rotation, double_sided, unlit, fog_enabled, depth_bias,
    parallax_depth_scale, parallax_mapping_method, max_parallax_layer_count, lightmap_exposure,
    opaque_render_method, deferred_lighting_pass_id,
);

/// Texture props: the field, and whether it is colour (sRGB) rather than data (linear).
macro_rules! texture_fields {
    ($($f:ident $srgb:literal),* $(,)?) => {
        impl MaterialProps {
            /// The textures set: (prop, path, sRGB).
            pub fn textures(&self) -> Vec<(String, &str, bool)> {
                let mut out = Vec::new();
                $(if let Some(p) = &self.$f { out.push((camel(stringify!($f)), p.as_str(), $srgb)); })*
                out
            }

            fn set_textures(&self, m: &mut StandardMaterial, load: &mut dyn FnMut(&str, bool) -> Handle<Image>) {
                $(if let Some(p) = &self.$f { m.$f = Some(load(p, $srgb)); })*
            }
        }
    };
}

// Bevy's four PBR texture features gate nine more texture fields and their channels; they are
// off (docs/materials.md, "Blocked on Bevy").
texture_fields!(
    base_color_texture true, emissive_texture true, metallic_roughness_texture false, normal_map_texture false,
    occlusion_texture false, depth_map false,
);

/// Number props, for the finite check.
macro_rules! number_fields {
    ($($f:ident),* $(,)?) => {
        impl MaterialProps {
            fn numbers(&self) -> Vec<(&'static str, f32)> {
                let mut out = Vec::new();
                $(if let Some(v) = self.$f { out.push((stringify!($f), v)); })*
                out
            }
        }
    };
}

number_fields!(
    emissive_exposure_weight, roughness, metallic, reflectance, diffuse_transmission, specular_transmission,
    thickness, ior, attenuation_distance, clearcoat, clearcoat_perceptual_roughness, anisotropy_strength,
    anisotropy_rotation, depth_bias, parallax_depth_scale, max_parallax_layer_count, lightmap_exposure,
);

/// `normal_map_texture` -> `normalMapTexture`.
fn camel(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut up = false;
    for c in snake.chars() {
        match c {
            '_' => up = true,
            c if up => {
                out.extend(c.to_uppercase());
                up = false;
            }
            c => out.push(c),
        }
    }
    out
}

impl MaterialProps {
    pub fn check(&self) -> Result<(), String> {
        for c in [&self.color, &self.emissive, &self.specular_tint, &self.attenuation_color].into_iter().flatten() {
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
        let uv = self.uv_transform.unwrap_or_default();
        let uv_numbers = uv.scale.into_iter().chain(uv.translation).flatten().chain(uv.rotation);
        if uv_numbers.into_iter().any(|v| !v.is_finite()) {
            return Err("uvTransform takes finite numbers".into());
        }
        if let Some((f, v)) = self.numbers().into_iter().find(|(_, v)| !v.is_finite()) {
            return Err(format!("{} is {v}; it takes a finite number", camel(f)));
        }
        if let Some((f, ..)) = self.textures().into_iter().find(|(_, p, _)| p.is_empty()) {
            return Err(format!("{f} is an empty path"));
        }
        Ok(())
    }

    /// Needs vertex tangents: Bevy ignores these on a mesh without them.
    fn needs_tangents(&self) -> bool {
        self.normal_map_texture.is_some() || self.anisotropy_strength.is_some_and(|s| s != 0.0)
    }

    fn material(&self, load: &mut dyn FnMut(&str, bool) -> Handle<Image>) -> StandardMaterial {
        let mut m = StandardMaterial::default();
        self.set_plain(&mut m);
        self.set_textures(&mut m, load);
        if let Some(c) = &self.color {
            m.base_color = color(c).expect("checked");
        }
        if let Some(v) = self.roughness {
            m.perceptual_roughness = v;
        }
        if let Some(c) = &self.emissive {
            m.emissive = color(c).expect("checked").to_linear();
        }
        if let Some(c) = &self.specular_tint {
            m.specular_tint = color(c).expect("checked");
        }
        if let Some(c) = &self.attenuation_color {
            m.attenuation_color = color(c).expect("checked");
        }
        if let Some(CullMode(f)) = self.cull_mode {
            m.cull_mode = f;
        }
        if let Some(t) = &self.uv_transform {
            m.uv_transform = t.affine();
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
    /// The same shapes with vertex tangents, for materials that need them.
    tangent_meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<String, Handle<StandardMaterial>>,
    /// Each texture's path and the element that first asked for it, to report a failed load.
    textures: HashMap<AssetId<Image>, (String, Option<SourceLocation>)>,
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

/// The shape with tangents, built once; the plain mesh for a shape tangents fail on.
fn tangent_mesh_handle(world: &mut World, p: &MeshProps) -> Handle<Mesh> {
    let key = p.cache_key();
    if let Some(h) = world.resource::<IntrinsicAssets>().tangent_meshes.get(&key) {
        return h.clone();
    }
    let h = match p.mesh().with_generated_tangents() {
        Ok(mesh) => {
            world.resource_mut::<IntrinsicAssets>().built += 1;
            world.resource_mut::<Assets<Mesh>>().add(mesh)
        }
        Err(_) => mesh_handle(world, p),
    };
    world.resource_mut::<IntrinsicAssets>().tangent_meshes.insert(key, h.clone());
    h
}

fn material_handle(world: &mut World, p: &MaterialProps, at: Option<SourceLocation>) -> Handle<StandardMaterial> {
    let key = p.cache_key();
    if let Some(h) = world.resource::<IntrinsicAssets>().materials.get(&key) {
        return h.clone();
    }
    let server = world.get_resource::<AssetServer>().cloned();
    let mut loaded = Vec::new();
    let material = p.material(&mut |path, srgb| {
        // Checked: a texture fails the load without an AssetServer.
        let server = server.as_ref().expect("checked");
        let h: Handle<Image> = if srgb {
            server.load(path.to_string())
        } else {
            server.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false).load(path.to_string())
        };
        loaded.push((h.id(), path.to_string()));
        h
    });
    let h = world.resource_mut::<Assets<StandardMaterial>>().add(material);
    let mut cache = world.resource_mut::<IntrinsicAssets>();
    cache.built += 1;
    cache.materials.insert(key, h.clone());
    for (id, path) in loaded {
        cache.textures.entry(id).or_insert((path, at.clone()));
    }
    h
}

/// The entity's `<mesh>` props, so its material can pick the plain or the tangent mesh.
#[derive(Component)]
struct IntrinsicMesh(MeshProps);

/// The entity's `<material>` needs a mesh with tangents.
#[derive(Component)]
struct NeedsTangents;

/// Gives the entity's `<mesh>` the plain or the tangent variant, as its material needs.
fn fit_mesh(world: &mut World, e: Entity) {
    let Some(p) = world.get::<IntrinsicMesh>(e).map(|m| m.0.clone()) else { return };
    let h = if world.get::<NeedsTangents>(e).is_some() { tangent_mesh_handle(world, &p) } else { mesh_handle(world, &p) };
    if world.get::<Mesh3d>(e).is_none_or(|m| m.0 != h) {
        world.entity_mut(e).insert(Mesh3d(h));
    }
}

/// Inserts (or replaces) what a `<mesh>` owns.
pub fn insert_mesh(world: &mut World, e: Entity, p: &MeshProps) {
    let material = p.color.as_ref().map(|c| material_handle(world, &MaterialProps { color: Some(c.clone()), ..default() }, None));
    let mut em = world.entity_mut(e);
    em.insert(IntrinsicMesh(p.clone()));
    if let Some(m) = material {
        em.insert(MeshMaterial3d(m));
    }
    fit_mesh(world, e);
}

pub fn remove_mesh(world: &mut World, e: Entity, p: &MeshProps) {
    let mut em = world.entity_mut(e);
    em.remove::<(Mesh3d, IntrinsicMesh)>();
    if p.color.is_some() {
        em.remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

pub fn insert_material(world: &mut World, e: Entity, p: &MaterialProps, at: Option<SourceLocation>) {
    let m = material_handle(world, p, at);
    let mut em = world.entity_mut(e);
    em.remove::<(ScriptMaterial3d, GltfMaterialFilter)>().insert(MeshMaterial3d(m));
    if p.needs_tangents() {
        em.insert(NeedsTangents);
    } else {
        em.remove::<NeedsTangents>();
    }
    fit_mesh(world, e);
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
    at: Option<SourceLocation>,
) {
    if !world.contains_resource::<bevy_ts_material::MaterialKinds>() {
        crate::reconcile::log_error(world, "<material kind>: script materials need ScriptMaterialPlugin".into(), at);
        return;
    }
    match bevy_ts_material::kinds::instance(world, kind, props) {
        Some(h) => {
            let mut em = world.entity_mut(e);
            em.remove::<(MeshMaterial3d<StandardMaterial>, NeedsTangents)>().insert(ScriptMaterial3d(h));
            match gltf_material {
                Some(name) if em.get::<GltfMaterialFilter>().is_none_or(|f| f.0 != name) => {
                    em.insert(GltfMaterialFilter(name.into()));
                }
                Some(_) => {}
                None => {
                    em.remove::<GltfMaterialFilter>();
                }
            }
            fit_mesh(world, e);
        }
        None => crate::reconcile::log_error(world, format!("<material kind>: no material {kind}"), at),
    }
}

pub fn remove_material(world: &mut World, e: Entity) {
    world.entity_mut(e).remove::<(MeshMaterial3d<StandardMaterial>, ScriptMaterial3d, GltfMaterialFilter, NeedsTangents)>();
    fit_mesh(world, e);
}

/// A `<material>` texture that failed to load, reported at the element that set it.
pub fn report_texture_failures(world: &mut World, mut cursor: Local<MessageCursor<AssetLoadFailedEvent<Image>>>) {
    let Some(messages) = world.get_resource::<Messages<AssetLoadFailedEvent<Image>>>() else { return };
    let failed: Vec<_> = cursor.read(messages).map(|m| (m.id, m.error.to_string())).collect();
    for (id, error) in failed {
        let Some((path, at)) = world.resource::<IntrinsicAssets>().textures.get(&id).cloned() else { continue };
        crate::reconcile::log_error(world, format!("<material>: texture {path:?} failed to load: {error}"), at);
    }
}
