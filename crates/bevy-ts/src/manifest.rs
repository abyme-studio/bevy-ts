//! The generation manifest JS hands Rust once per generation, and the resolution Rust hands
//! back. JSON both ways; neither runs per frame.

use crate::backend::SourceLocation;
use crate::layout::{ComponentLayout, FieldType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Editor hints for one field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub hints: FieldHints,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

impl ComponentDecl {
    pub fn layout(&self) -> ComponentLayout {
        ComponentLayout::c_layout(self.fields.iter().map(|f| (f.name.as_str(), &f.ty)))
    }

    pub fn field(&self, name: &str) -> Option<&FieldDecl> {
        self.fields.iter().find(|f| f.name == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    PreUpdate,
    Update,
    PostUpdate,
    FixedUpdate,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::PreUpdate, Stage::Update, Stage::PostUpdate, Stage::FixedUpdate];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::PreUpdate => "preUpdate",
            Stage::Update => "update",
            Stage::PostUpdate => "postUpdate",
            Stage::FixedUpdate => "fixedUpdate",
        }
    }
}

/// A component a system names: a TS component by declared name, or a Rust component by the
/// name `bevy-ts/bevy` exports (`Transform`) or by type path.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentRef {
    pub name: String,
    /// `ts` for TS components, `rust` for Rust ones.
    pub source: ComponentSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentSource {
    Ts,
    Rust,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemDecl {
    pub name: String,
    pub stage: Stage,
    #[serde(default)]
    pub read: Vec<ComponentRef>,
    #[serde(default)]
    pub write: Vec<ComponentRef>,
    #[serde(default)]
    pub with: Vec<ComponentRef>,
    #[serde(default)]
    pub without: Vec<ComponentRef>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartupDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// Declarations collected while a generation's modules evaluated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub components: Vec<ComponentDecl>,
    #[serde(default)]
    pub systems: Vec<SystemDecl>,
    #[serde(default)]
    pub startups: Vec<StartupDecl>,
    #[serde(default)]
    pub entries: Vec<EntryDecl>,
    /// Command kinds the generation's code appends, by name, so Rust can check them up front.
    #[serde(default, rename = "commandKinds")]
    pub command_kinds: Vec<String>,
}

/// A plugin's manifest section: opaque bytes, and where in TS it was written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: String,
    pub bytes: Vec<u8>,
    pub location: Option<SourceLocation>,
}

/// What [`crate::backend::ScriptBackend::load`] returns.
#[derive(Clone, Debug, Default)]
pub struct LoadedGeneration {
    pub manifest: Manifest,
    pub sections: Vec<Section>,
}

/// A component as resolved for JS: its id and layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedComponent {
    pub id: u32,
    pub layout: ComponentLayout,
}

/// Rust's answer to a manifest: ids and layouts for every component the generation names, and
/// the opcode of every command kind.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    pub generation: u64,
    /// TS components by declared name, native ones by their `bevy-ts/bevy` name.
    pub components: BTreeMap<String, ResolvedComponent>,
    #[serde(rename = "commandKinds")]
    pub command_kinds: BTreeMap<String, u16>,
}
