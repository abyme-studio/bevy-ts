//! `<mesh>` and `<material>`: asset intrinsics built in Rust. Equal props share one asset
//! handle, so a thousand identical enemies make one mesh and one material. `<material>` is the
//! asset value `StandardMaterial.of` of its props, mapped by [`MATERIAL_PROPS`], so it also shares
//! with a `Handle<StandardMaterial>` slot given that value. `<material kind={K}>` is a script
//! material (bevy-ts-material), shared the same way.
//! `<mesh2d>` and `<material2d>` are their 2D pair: a `Mesh2d` from the same shapes and mesh
//! cache, and a `ColorMaterial`.

use bevy::asset::AssetLoadFailedEvent;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::math::Affine2;
use bevy::mesh::UvChannel;
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::ParallaxMappingMethod;
use bevy::prelude::*;
use bevy_ts_material::kinds::AlphaDecl;
use bevy::render::render_resource::Face;
use bevy::sprite_render::{AlphaMode2d, ColorMaterial, MeshMaterial2d};
use bevy_ts::backend::SourceLocation;
use bevy_ts_material::{GltfMaterialFilter, MeshScriptMaterial2d, ScriptMaterial3d};
use crate::shapes::MeshShape;
use crate::vertex_mesh::VertexMesh;
use bevy::reflect::structs::Struct;
use bevy::reflect::{PartialReflect, TypeRegistry};
use bevy_ts::reflect_asset::{ScriptAssets, Slots};
use bevy_ts::reflect_value;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// `<mesh shape .../>` or `<mesh positions .../>`: a Bevy mesh primitive (see `shapes`) or vertex
/// data (see `vertex_mesh`), and an optional color.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshProps {
    pub source: MeshSource,
    /// Shorthand for a `<material color>` on the same entity.
    pub color: Option<Value>,
}

/// What a `<mesh>` builds from: picked by `positions` before the `shape` tag.
#[derive(Clone, Debug, PartialEq)]
pub enum MeshSource {
    Shape(MeshShape),
    Vertex(Box<VertexMesh>),
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
        let source = if props.contains_key("positions") {
            if props.contains_key("shape") {
                return Err(D::Error::custom("shape and positions: a <mesh> is a shape or vertex data, not both"));
            }
            MeshSource::Vertex(Box::new(VertexMesh::from_props(props).map_err(D::Error::custom)?))
        } else {
            MeshSource::Shape(MeshShape::from_props(props).map_err(D::Error::custom)?)
        };
        Ok(MeshProps { source, color })
    }
}

impl MeshProps {
    pub fn check(&self) -> Result<(), String> {
        match &self.source {
            MeshSource::Shape(s) => s.check()?,
            MeshSource::Vertex(v) => drop(v.build()?),
        }
        if let Some(c) = &self.color {
            color(c)?;
        }
        Ok(())
    }

    /// Call `check` first: vertex data `check` refused builds an empty mesh here.
    fn mesh(&self) -> Mesh {
        match &self.source {
            MeshSource::Shape(s) => s.mesh(),
            MeshSource::Vertex(v) => v.build().unwrap_or_else(|_| Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default())),
        }
    }

    fn cache_key(&self) -> String {
        format!("{:?}", self.source)
    }
}

/// `<mesh2d shape .../>`: one of `<mesh>`'s 2D shapes, flat in the XY plane, and an optional color.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh2dProps(pub MeshProps);

impl<'de> Deserialize<'de> for Mesh2dProps {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let props = Map::<String, Value>::deserialize(d)?;
        let shape = props.get("shape").and_then(Value::as_str).unwrap_or("shape");
        if let Some(k) = ["depth", "segments"].into_iter().find(|k| props.contains_key(*k)) {
            return Err(D::Error::custom(format!("{shape}: {k}: <mesh2d> is flat; extrude with <mesh depth>")));
        }
        let p = MeshProps::deserialize(Value::Object(props)).map_err(D::Error::custom)?;
        match &p.source {
            MeshSource::Shape(s) if !s.is_2d() => return Err(D::Error::custom(format!("shape: {} is a 3D shape; <mesh2d> takes a 2D one", s.name()))),
            MeshSource::Vertex(_) => return Err(D::Error::custom("positions: <mesh2d> takes a 2D shape; vertex data is <mesh>'s")),
            MeshSource::Shape(_) => {}
        }
        Ok(Mesh2dProps(p))
    }
}

/// `<material2d .../>`: a `ColorMaterial`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Material2dProps {
    pub color: Option<Value>,
    /// Replaces `color`'s alpha, 0 to 1.
    pub alpha: Option<f32>,
    /// `"opaque"`, `"blend"` or `{ mask }`; unset, `Blend` below alpha 1 and `Opaque` otherwise.
    #[serde(rename = "alphaMode")]
    pub alpha_mode: Option<AlphaDecl>,
    pub texture: Option<String>,
    pub uv_transform: Option<UvTransform>,
}

impl Material2dProps {
    pub fn check(&self) -> Result<(), String> {
        MaterialProps { color: self.color.clone(), alpha: self.alpha, uv_transform: self.uv_transform, ..default() }.check()?;
        if let Some(m) = &self.alpha_mode {
            alpha_2d(m)?;
        }
        if self.texture.as_deref() == Some("") {
            return Err("texture is an empty path".into());
        }
        Ok(())
    }

    fn material(&self, texture: Option<Handle<Image>>) -> ColorMaterial {
        let mut color = self.color.as_ref().map_or(Color::WHITE, |c| bevy_ts::color::parse(c).expect("checked"));
        if let Some(a) = self.alpha {
            color.set_alpha(a);
        }
        // Bevy's own rule in `ColorMaterial::from(Color)`.
        let alpha_mode = match &self.alpha_mode {
            Some(d) => alpha_2d(d).expect("checked"),
            None if color.alpha() < 1.0 => AlphaMode2d::Blend,
            None => AlphaMode2d::Opaque,
        };
        ColorMaterial { color, alpha_mode, uv_transform: self.uv_transform.unwrap_or_default().affine(), texture }
    }
}

/// The `AlphaMode2d` an `alphaMode` names: the modes of `AlphaDecl`'s that Bevy's 2D has.
fn alpha_2d(d: &AlphaDecl) -> Result<AlphaMode2d, String> {
    match d.parse()? {
        AlphaMode::Opaque => Ok(AlphaMode2d::Opaque),
        AlphaMode::Blend => Ok(AlphaMode2d::Blend),
        AlphaMode::Mask(t) => Ok(AlphaMode2d::Mask(t)),
        // A mask always maps, so what is left is a name.
        _ => {
            let name = match d {
                AlphaDecl::Named(n) => n.as_str(),
                AlphaDecl::Mask { .. } => "mask",
            };
            Err(format!("alphaMode {name:?}: <material2d> takes \"opaque\", \"blend\" or {{ mask }}"))
        }
    }
}

/// `<material>`'s props, parsed and checked here; [`MATERIAL_PROPS`] maps them onto
/// `StandardMaterial`'s fields, and the material is the asset value `StandardMaterial.of` of them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MaterialProps {
    /// `base_color`.
    pub color: Option<Value>,
    pub base_color_channel: Option<Uv>,
    pub base_color_texture: Option<Texture>,
    pub emissive: Option<Value>,
    pub emissive_exposure_weight: Option<f32>,
    pub emissive_channel: Option<Uv>,
    pub emissive_texture: Option<Texture>,
    /// `perceptual_roughness`.
    pub roughness: Option<f32>,
    pub metallic: Option<f32>,
    pub metallic_roughness_channel: Option<Uv>,
    pub metallic_roughness_texture: Option<Texture>,
    pub reflectance: Option<f32>,
    pub specular_tint: Option<Value>,
    pub diffuse_transmission: Option<f32>,
    pub specular_transmission: Option<f32>,
    pub thickness: Option<f32>,
    pub ior: Option<f32>,
    pub attenuation_distance: Option<f32>,
    pub attenuation_color: Option<Value>,
    pub normal_map_channel: Option<Uv>,
    pub normal_map_texture: Option<Texture>,
    pub flip_normal_map_y: Option<bool>,
    pub occlusion_channel: Option<Uv>,
    pub occlusion_texture: Option<Texture>,
    pub clearcoat: Option<f32>,
    pub clearcoat_perceptual_roughness: Option<f32>,
    pub anisotropy_strength: Option<f32>,
    pub anisotropy_rotation: Option<f32>,
    pub double_sided: Option<bool>,
    /// Unset keeps Bevy's `Some(Back)`; `null` is no culling.
    #[serde(deserialize_with = "present", skip_serializing)]
    pub cull_mode: Option<CullMode>,
    pub unlit: Option<bool>,
    /// Replaces `color`'s alpha, 0 to 1.
    #[serde(default)]
    pub alpha: Option<f32>,
    /// A script material's `alphaMode` names; unset, `Blend` below alpha 1 and `Opaque` otherwise.
    #[serde(default, rename = "alphaMode", skip_serializing)]
    pub alpha_mode: Option<AlphaDecl>,
    pub fog_enabled: Option<bool>,
    pub depth_bias: Option<f32>,
    pub depth_map: Option<Texture>,
    pub parallax_depth_scale: Option<f32>,
    pub parallax_mapping_method: Option<Parallax>,
    pub max_parallax_layer_count: Option<f32>,
    pub lightmap_exposure: Option<f32>,
    pub opaque_render_method: Option<RenderMethod>,
    pub deferred_lighting_pass_id: Option<u8>,
    pub uv_transform: Option<UvTransform>,
}

/// A UV channel prop: `"uv0"` or `"uv1"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
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

/// A `<material>` texture prop: an asset path, loaded as `Image.load` in its field's colour space,
/// or what any `Handle<Image>` slot takes (a ref, `Image.of(...)`, `Image.load(...)`).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Texture {
    Path(String),
    Asset(Map<String, Value>),
}

/// How a `<material>` prop's value becomes its `StandardMaterial` field's JSON.
#[derive(Clone, Copy)]
pub enum Mapping {
    /// As given: numbers, bools, and colours in any form `bevy_ts::color` takes.
    Same,
    /// Through the Bevy value it stands for, read in Bevy's shape (`"uv1"` is `"Uv1"`); `None` when
    /// unset.
    Typed(fn(&MaterialProps) -> Option<Box<dyn PartialReflect>>),
    /// An asset path becomes `Image.load` in the field's colour space, sRGB when true; a ref or
    /// value goes as given.
    Texture(bool),
    /// Not reflected by Bevy (`cull_mode`), so set after the write.
    Rust,
    /// Folded into `base_color` and `alpha_mode` with the unset `alphaMode` rule (`alpha`,
    /// `alphaMode`).
    Alpha,
}

fn boxed<T: PartialReflect>(v: T) -> Box<dyn PartialReflect> {
    Box::new(v)
}

/// Every `<material>` prop: its name, the `StandardMaterial` field it sets, and how. One per
/// reflected field but `alpha_mode`, and `cullMode`; `jsx-runtime.d.ts`' `MaterialProps` is
/// checked against it.
pub const MATERIAL_PROPS: &[(&str, &str, Mapping)] = &[
    ("color", "base_color", Mapping::Same),
    ("baseColorChannel", "base_color_channel", Mapping::Typed(|p| p.base_color_channel.map(|u| boxed(UvChannel::from(u))))),
    ("baseColorTexture", "base_color_texture", Mapping::Texture(true)),
    ("emissive", "emissive", Mapping::Typed(|p| p.emissive.as_ref().map(|c| boxed(color(c).expect("checked").to_linear())))),
    ("emissiveExposureWeight", "emissive_exposure_weight", Mapping::Same),
    ("emissiveChannel", "emissive_channel", Mapping::Typed(|p| p.emissive_channel.map(|u| boxed(UvChannel::from(u))))),
    ("emissiveTexture", "emissive_texture", Mapping::Texture(true)),
    ("roughness", "perceptual_roughness", Mapping::Same),
    ("metallic", "metallic", Mapping::Same),
    ("metallicRoughnessChannel", "metallic_roughness_channel", Mapping::Typed(|p| p.metallic_roughness_channel.map(|u| boxed(UvChannel::from(u))))),
    ("metallicRoughnessTexture", "metallic_roughness_texture", Mapping::Texture(false)),
    ("reflectance", "reflectance", Mapping::Same),
    ("specularTint", "specular_tint", Mapping::Same),
    ("diffuseTransmission", "diffuse_transmission", Mapping::Same),
    ("specularTransmission", "specular_transmission", Mapping::Same),
    ("thickness", "thickness", Mapping::Same),
    ("ior", "ior", Mapping::Same),
    ("attenuationDistance", "attenuation_distance", Mapping::Same),
    ("attenuationColor", "attenuation_color", Mapping::Same),
    ("normalMapChannel", "normal_map_channel", Mapping::Typed(|p| p.normal_map_channel.map(|u| boxed(UvChannel::from(u))))),
    ("normalMapTexture", "normal_map_texture", Mapping::Texture(false)),
    ("flipNormalMapY", "flip_normal_map_y", Mapping::Same),
    ("occlusionChannel", "occlusion_channel", Mapping::Typed(|p| p.occlusion_channel.map(|u| boxed(UvChannel::from(u))))),
    ("occlusionTexture", "occlusion_texture", Mapping::Texture(false)),
    ("clearcoat", "clearcoat", Mapping::Same),
    ("clearcoatPerceptualRoughness", "clearcoat_perceptual_roughness", Mapping::Same),
    ("anisotropyStrength", "anisotropy_strength", Mapping::Same),
    ("anisotropyRotation", "anisotropy_rotation", Mapping::Same),
    ("doubleSided", "double_sided", Mapping::Same),
    ("cullMode", "cull_mode", Mapping::Rust),
    ("unlit", "unlit", Mapping::Same),
    ("alpha", "base_color", Mapping::Alpha),
    ("alphaMode", "alpha_mode", Mapping::Alpha),
    ("fogEnabled", "fog_enabled", Mapping::Same),
    ("depthBias", "depth_bias", Mapping::Same),
    ("depthMap", "depth_map", Mapping::Texture(false)),
    ("parallaxDepthScale", "parallax_depth_scale", Mapping::Same),
    ("parallaxMappingMethod", "parallax_mapping_method", Mapping::Typed(|p| p.parallax_mapping_method.map(|m| boxed(ParallaxMappingMethod::from(m))))),
    ("maxParallaxLayerCount", "max_parallax_layer_count", Mapping::Same),
    ("lightmapExposure", "lightmap_exposure", Mapping::Same),
    ("opaqueRenderMethod", "opaque_render_method", Mapping::Typed(|p| p.opaque_render_method.map(|m| boxed(OpaqueRendererMethod::from(m))))),
    ("deferredLightingPassId", "deferred_lighting_pass_id", Mapping::Same),
    ("uvTransform", "uv_transform", Mapping::Typed(|p| p.uv_transform.map(|t| boxed(t.affine())))),
];

/// A `<material>` as the asset value it builds: the `StandardMaterial.of` props, and what is set
/// after the write, which joins the cache key as `extra`.
struct MaterialOf {
    props: Value,
    extra: Value,
    /// Texture paths: (prop, field, path), loaded in `props` when there is an asset server.
    textures: Vec<(&'static str, &'static str, String)>,
    cull_mode: Option<Option<Face>>,
}

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

    /// The props as given, by prop name: those set, but `cullMode` and `alphaMode`. Numbers are
    /// spelt as a script spells them (`0.3`, not the `f32`'s `0.30000001192092896`; `1`, not
    /// `1.0`), so equal props give the key a script's `StandardMaterial.of` does.
    fn given(&self) -> Map<String, Value> {
        fn spelt(v: Value) -> Value {
            match v {
                Value::Number(n) if n.is_f64() => serde_json::from_str(&format!("{}", n.as_f64().unwrap_or_default() as f32)).unwrap_or(Value::Number(n)),
                Value::Array(a) => Value::Array(a.into_iter().map(spelt).collect()),
                Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, spelt(v))).collect()),
                v => v,
            }
        }
        match serde_json::to_value(self) {
            Ok(Value::Object(o)) => o.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k, spelt(v))).collect(),
            _ => Map::new(),
        }
    }

    /// The textures set: (prop, path, sRGB).
    pub fn textures(&self) -> Vec<(&'static str, String, bool)> {
        let given = self.given();
        MATERIAL_PROPS
            .iter()
            .filter_map(|(prop, _, m)| match (m, given.get(*prop)) {
                (Mapping::Texture(srgb), Some(Value::String(p))) => Some((*prop, p.clone(), *srgb)),
                _ => None,
            })
            .collect()
    }

    /// Needs vertex tangents: Bevy ignores these on a mesh without them.
    fn needs_tangents(&self) -> bool {
        self.normal_map_texture.is_some() || self.anisotropy_strength.is_some_and(|s| s != 0.0)
    }

    /// The asset value these props build, by [`MATERIAL_PROPS`]. Texture paths load only with
    /// `load`, an asset server to load them.
    fn of(&self, registry: &TypeRegistry, load: bool) -> Result<MaterialOf, String> {
        let read = |v: &dyn PartialReflect| reflect_value::read(registry, v);
        let given = self.given();
        let mut props = Map::new();
        let mut extra = Map::new();
        let mut textures = Vec::new();
        for (prop, field, mapping) in MATERIAL_PROPS {
            match mapping {
                Mapping::Same => {
                    if let Some(v) = given.get(*prop) {
                        props.insert(field.to_string(), v.clone());
                    }
                }
                Mapping::Typed(f) => {
                    if let Some(v) = f(self) {
                        props.insert(field.to_string(), read(v.as_ref())?);
                    }
                }
                Mapping::Texture(srgb) => match given.get(*prop) {
                    Some(Value::String(path)) => {
                        textures.push((*prop, *field, path.clone()));
                        if load {
                            // `Image.load(path)`, linear for data textures: one key with a script's.
                            let v = if *srgb { json!({ "$load": path }) } else { json!({ "$load": path, "settings": { "is_srgb": false } }) };
                            props.insert(field.to_string(), v);
                        }
                    }
                    // An `Image.load`: its path is reported at the element if it fails to load, and
                    // with no `is_srgb` it takes the field's colour space.
                    Some(Value::Object(o)) if o.get("$load").is_some_and(Value::is_string) => {
                        textures.push((*prop, *field, o["$load"].as_str().unwrap_or_default().to_string()));
                        let mut v = o.clone();
                        if !*srgb && v.get("settings").and_then(|s| s.get("is_srgb")).is_none() {
                            let settings = v.entry("settings").or_insert_with(|| json!({}));
                            if let Some(s) = settings.as_object_mut() {
                                s.insert("is_srgb".into(), false.into());
                            }
                        }
                        props.insert(field.to_string(), Value::Object(v));
                    }
                    Some(v) => _ = props.insert(field.to_string(), v.clone()),
                    None => {}
                },
                Mapping::Rust | Mapping::Alpha => {}
            }
        }
        if let Some(CullMode(f)) = self.cull_mode {
            extra.insert("cull_mode".into(), format!("{f:?}").into());
        }
        let base = match &self.color {
            Some(c) => color(c)?,
            None => StandardMaterial::default().base_color,
        };
        if let Some(a) = self.alpha {
            props.insert("base_color".into(), read(&base.with_alpha(a))?);
        }
        // Bevy's own rule in `StandardMaterial::from(Color)`; `Opaque` is the default.
        let mode = match &self.alpha_mode {
            Some(d) => Some(d.parse()?),
            None if self.alpha.unwrap_or(base.alpha()) < 1.0 => Some(AlphaMode::Blend),
            None => None,
        };
        if let Some(m) = mode {
            props.insert("alpha_mode".into(), read(&m)?);
        }
        let extra = if extra.is_empty() { Value::Null } else { Value::Object(extra) };
        Ok(MaterialOf { props: Value::Object(props), extra, textures, cull_mode: self.cull_mode.map(|c| c.0) })
    }
}

fn color(v: &Value) -> Result<Color, String> {
    bevy_ts::color::parse(v)
}

/// Mesh and `<material2d>` handles by props, shared by every element with the same props;
/// `<material>`s share through the asset-value cache (`bevy_ts::reflect_asset`).
#[derive(Resource, Default)]
pub struct IntrinsicAssets {
    meshes: HashMap<String, Handle<Mesh>>,
    /// The same shapes with vertex tangents, for materials that need them.
    tangent_meshes: HashMap<String, Handle<Mesh>>,
    materials2d: HashMap<String, Handle<ColorMaterial>>,
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

/// The `<material>`'s handle: the asset value `StandardMaterial.of` of its props, shared with
/// every element and slot given the same value, its texture paths loaded as `Image.load` in their
/// colour space.
fn material_handle(world: &mut World, p: &MaterialProps, at: Option<SourceLocation>) -> Handle<StandardMaterial> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.get_resource::<AssetServer>().cloned();
    let assets = world.resource::<ScriptAssets>().clone();
    let mut loaded = Vec::new();
    let made = {
        let registry = registry.read();
        p.of(&registry, server.is_some()).and_then(|of| {
            // A section's check and a mount fail a texture with no AssetServer first; any other
            // way in draws the material untextured and says why.
            let unloaded = if server.is_none() { of.textures.first().map(|t| t.0) } else { None };
            let slots = Slots { registry: &registry, server: server.as_ref(), assets: &assets, world: Some(world), check: false };
            let made = slots.value_with::<StandardMaterial>(&of.props, &of.extra, &mut |m| {
                if let Some(f) = of.cull_mode {
                    m.cull_mode = f;
                }
                for (_, field, path) in &of.textures {
                    if let Some(Some(h)) = m.field(field).and_then(|f| f.try_downcast_ref::<Option<Handle<Image>>>()) {
                        loaded.push((h.id(), path.clone()));
                    }
                }
            });
            made.map(|m| (m, unloaded))
        })
    };
    let ((h, value), unloaded) = match made {
        Ok(m) => m,
        Err(e) => {
            crate::reconcile::log_error(world, format!("<material>: {e}"), at);
            return Handle::default();
        }
    };
    if let Some(prop) = unloaded {
        crate::reconcile::log_error(world, format!("<material>: {prop} needs an AssetServer to load"), at.clone());
    }
    if let Some(m) = value {
        // Whole: a reflected insert would drop `cull_mode`.
        let _ = world.resource_mut::<Assets<StandardMaterial>>().insert(h.id(), m);
        world.resource_mut::<IntrinsicAssets>().built += 1;
    }
    let mut cache = world.resource_mut::<IntrinsicAssets>();
    for (id, path) in loaded {
        cache.textures.entry(id).or_insert((path, at.clone()));
    }
    h
}

fn material2d_handle(world: &mut World, p: &Material2dProps, at: Option<SourceLocation>) -> Handle<ColorMaterial> {
    let key = format!("{p:?}");
    if let Some(h) = world.resource::<IntrinsicAssets>().materials2d.get(&key) {
        return h.clone();
    }
    let texture = p.texture.as_ref().map(|path| match world.get_resource::<AssetServer>() {
        Some(server) => server.load::<Image>(path.clone()),
        None => {
            crate::reconcile::log_error(world, "<material2d>: texture needs an AssetServer to load".into(), at.clone());
            Handle::default()
        }
    });
    let h = world.resource_mut::<Assets<ColorMaterial>>().add(p.material(texture.clone()));
    let mut cache = world.resource_mut::<IntrinsicAssets>();
    cache.built += 1;
    cache.materials2d.insert(key, h.clone());
    if let (Some(t), Some(path)) = (texture, &p.texture) {
        cache.textures.entry(t.id()).or_insert((path.clone(), at));
    }
    h
}

/// True when the app can build `<mesh2d>` and `<material2d>`.
pub fn available_2d(world: &World) -> bool {
    world.contains_resource::<Assets<Mesh>>() && world.contains_resource::<Assets<ColorMaterial>>()
}

/// Inserts (or replaces) what a `<mesh2d>` owns: the shared mesh, flat, and its color's material.
pub fn insert_mesh2d(world: &mut World, e: Entity, p: &Mesh2dProps) {
    let material = p.0.color.as_ref().map(|c| material2d_handle(world, &Material2dProps { color: Some(c.clone()), ..default() }, None));
    let mesh = mesh_handle(world, &p.0);
    let mut em = world.entity_mut(e);
    em.insert(Mesh2d(mesh));
    if let Some(m) = material {
        em.insert(MeshMaterial2d(m));
    }
}

pub fn remove_mesh2d(world: &mut World, e: Entity, p: &Mesh2dProps) {
    let mut em = world.entity_mut(e);
    em.remove::<Mesh2d>();
    if p.0.color.is_some() {
        em.remove::<MeshMaterial2d<ColorMaterial>>();
    }
}

pub fn insert_material2d(world: &mut World, e: Entity, p: &Material2dProps, at: Option<SourceLocation>) {
    let m = material2d_handle(world, p, at);
    if world.get::<MeshScriptMaterial2d>(e).is_some() {
        bevy_ts_material::instance::remove(world, e);
    }
    world.entity_mut(e).remove::<MeshScriptMaterial2d>().insert(MeshMaterial2d(m));
}

pub fn remove_material2d(world: &mut World, e: Entity) {
    if world.get::<MeshScriptMaterial2d>(e).is_some() {
        bevy_ts_material::instance::remove(world, e);
    }
    world.entity_mut(e).remove::<(MeshMaterial2d<ColorMaterial>, MeshScriptMaterial2d)>();
}

/// `<material2d kind={K} .../>`: the kind's 2D instance for these props (equal props share one),
/// in place of a `ColorMaterial`.
pub fn insert_script_material2d(world: &mut World, e: Entity, kind: &str, props: &serde_json::Map<String, serde_json::Value>, at: Option<SourceLocation>) {
    if !world.contains_resource::<bevy_ts_material::MaterialKinds>() {
        crate::reconcile::log_error(world, "<material2d kind>: script materials need ScriptMaterialPlugin".into(), at);
        return;
    }
    // A sprite a Rust system or another element gave the entity: see `resolve::SPRITE_2D`.
    if world.get::<Sprite>(e).is_some() {
        crate::reconcile::log_error(world, crate::resolve::SPRITE_2D.into(), at);
        return;
    }
    match bevy_ts_material::instance::apply2d(world, e, kind, props, at.clone()) {
        Ok(h) => {
            let mut em = world.entity_mut(e);
            em.remove::<MeshMaterial2d<ColorMaterial>>();
            // The same handle again (only instance props changed) is not a change.
            if em.get::<MeshScriptMaterial2d>().is_none_or(|m| m.0 != h) {
                em.insert(MeshScriptMaterial2d(h));
            }
        }
        Err(m) => crate::reconcile::log_error(world, format!("<material2d kind>: {m}"), at),
    }
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
    bevy_ts_material::instance::remove(world, e);
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
    // Instance props go to the entity's `ScriptInstance`; the rest key the shared asset.
    match bevy_ts_material::instance::apply(world, e, kind, props, at.clone()) {
        Ok(h) => put_script_material(world, e, h, gltf_material),
        Err(m) => crate::reconcile::log_error(world, format!("<material kind>: {m}"), at),
    }
}

/// `<material asset={ref} />`: the owned instance `Kind.add` made, by its ref number. A stale ref
/// or one of another type fails at the element.
pub fn insert_owned_material(world: &mut World, e: Entity, asset: u64, gltf_material: Option<&str>, at: Option<SourceLocation>) {
    if !world.contains_resource::<bevy_ts_material::MaterialKinds>() {
        crate::reconcile::log_error(world, "<material asset>: script materials need ScriptMaterialPlugin".into(), at);
        return;
    }
    let handle = {
        let registry = world.resource::<AppTypeRegistry>().read();
        world.resource::<ScriptAssets>().handle_for(&registry, std::any::TypeId::of::<bevy_ts_material::ScriptMaterial>(), asset)
    };
    match handle {
        Ok(h) => {
            let h = h.typed::<bevy_ts_material::ScriptMaterial>();
            bevy_ts_material::instance::owned(world, e, &h, at);
            put_script_material(world, e, h, gltf_material);
        }
        Err(m) => crate::reconcile::log_error(world, format!("<material asset>: {m}"), at),
    }
}

fn put_script_material(world: &mut World, e: Entity, h: Handle<bevy_ts_material::ScriptMaterial>, gltf_material: Option<&str>) {
    let mut em = world.entity_mut(e);
    em.remove::<(MeshMaterial3d<StandardMaterial>, NeedsTangents)>();
    // The same handle again (only instance props changed) is not a change: nothing respecialises.
    if em.get::<ScriptMaterial3d>().is_none_or(|m| m.0 != h) {
        em.insert(ScriptMaterial3d(h));
    }
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

pub fn remove_material(world: &mut World, e: Entity) {
    bevy_ts_material::instance::remove(world, e);
    world.entity_mut(e).remove::<(MeshMaterial3d<StandardMaterial>, ScriptMaterial3d, GltfMaterialFilter, NeedsTangents)>();
    fit_mesh(world, e);
}

/// `<pass kind={K} ...params />` on camera `e`: its params, by name, in the kind's slot.
pub fn insert_pass(world: &mut World, e: Entity, kind: &str, props: &serde_json::Map<String, serde_json::Value>, at: Option<SourceLocation>) {
    if !world.contains_resource::<bevy_ts_material::PassKinds>() {
        crate::reconcile::log_error(world, "<pass>: script passes need ScriptMaterialPlugin".into(), at);
        return;
    }
    let props = props.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
    bevy_ts_material::pass::set(world, e, kind, props, at);
}

/// The `<pass>` of `kind` came off camera `e`.
pub fn remove_pass(world: &mut World, e: Entity, kind: &str) {
    if world.contains_resource::<bevy_ts_material::PassKinds>() {
        bevy_ts_material::pass::remove(world, e, kind);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn table() -> BTreeSet<String> {
        MATERIAL_PROPS.iter().map(|(p, ..)| p.to_string()).collect()
    }

    #[test]
    fn the_table_has_every_prop_once() {
        assert_eq!(table().len(), MATERIAL_PROPS.len(), "a prop twice");
        // Every serialized field, and the two read by hand.
        let mut fields: BTreeSet<String> = serde_json::to_value(MaterialProps::default()).unwrap().as_object().unwrap().keys().cloned().collect();
        fields.extend(["cullMode".to_string(), "alphaMode".to_string()]);
        assert_eq!(table(), fields);
    }

    #[test]
    fn jsx_runtime_material_props_match_the_table() {
        let dts = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/bevy-ts/jsx-runtime.d.ts")).unwrap();
        let body = dts.split("export interface MaterialProps {").nth(1).and_then(|s| s.split("\n}").next()).expect("MaterialProps in jsx-runtime.d.ts");
        let typed: BTreeSet<String> = body
            .lines()
            .filter_map(|l| l.trim().split_once("?:").map(|(k, _)| k.to_string()))
            .filter(|k| k != "kind" && k != "asset")
            .collect();
        assert_eq!(typed, table(), "jsx-runtime.d.ts' MaterialProps against MATERIAL_PROPS");
    }

    #[test]
    fn texture_paths_are_image_loads_in_their_colour_space() {
        let p: MaterialProps = serde_json::from_value(json!({
            "baseColorTexture": "a.png",
            "normalMapTexture": "n.png",
            "emissiveTexture": { "$asset": 3, "type": "bevy_image::image::Image" },
            "occlusionTexture": { "$load": "o.png", "settings": {} },
            "depthMap": { "$load": "d.png", "settings": { "is_srgb": true } },
        }))
        .unwrap();
        let registry = TypeRegistry::default();
        let of = p.of(&registry, true).unwrap();
        assert_eq!(of.props["base_color_texture"], json!({ "$load": "a.png" }), "sRGB, Image.load's default");
        assert_eq!(of.props["normal_map_texture"], json!({ "$load": "n.png", "settings": { "is_srgb": false } }), "linear data");
        assert_eq!(of.props["emissive_texture"]["$asset"], 3, "a ref goes as given");
        assert_eq!(of.props["occlusion_texture"], json!({ "$load": "o.png", "settings": { "is_srgb": false } }), "Image.load takes the field's colour space");
        assert_eq!(of.props["depth_map"]["settings"]["is_srgb"], true, "unless it gives its own");
        assert!(of.extra.is_null(), "textures key as props, so they share with a script's value");
        let unloaded = p.of(&registry, false).unwrap();
        assert!(unloaded.props.get("base_color_texture").is_none(), "no asset server: a path is left untextured");
        assert_eq!(unloaded.props["emissive_texture"]["$asset"], 3);
    }

    /// `Mesh2dProps` takes exactly the shapes `jsx-runtime.d.ts`'s `Shape2dMesh` lists.
    #[test]
    fn mesh2d_takes_the_shapes_the_types_call_2d() {
        let dts = include_str!("../../../packages/bevy-ts/jsx-runtime.d.ts");
        // Each `*Mesh` interface's name and its `shape`; `VertexDataMesh` has none (checked below).
        let mut shapes = HashMap::new();
        let mut lines = dts.lines();
        while let Some(l) = lines.next() {
            let Some(name) = l.strip_prefix("export interface ").and_then(|r| r.split_whitespace().next()).filter(|n| n.ends_with("Mesh") && *n != "VertexDataMesh") else { continue };
            let shape = lines.next().and_then(|l| l.trim().strip_prefix("shape: \"")).and_then(|r| r.split('"').next()).expect("shape first");
            shapes.insert(name.to_string(), shape.to_string());
        }
        assert_eq!(shapes.len(), 21, "{shapes:?}");
        let union = dts.split("export type Shape2dMesh =").nth(1).and_then(|r| r.split(';').next()).expect("Shape2dMesh");
        let flat: Vec<&str> = union.split('|').map(str::trim).filter(|n| !n.is_empty()).collect();
        for (name, shape) in &shapes {
            let e = serde_json::from_value::<Mesh2dProps>(json!({ "shape": shape })).err().map(|e| e.to_string());
            let is_2d = !e.as_deref().is_some_and(|e| e.contains("is a 3D shape"));
            assert_eq!(is_2d, flat.contains(&name.as_str()), "{shape}: {e:?}");
        }
        // Vertex data is `<mesh>`'s alone.
        let e = serde_json::from_value::<Mesh2dProps>(json!({ "positions": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] })).err().map(|e| e.to_string());
        assert!(e.as_deref().is_some_and(|e| e.contains("vertex data is <mesh>'s")), "{e:?}");
    }
}
