//! What the running engine binary is, for `bevy-ts/hello`: the prebuilt engine from npm, or a
//! project engine built from a project's native crate and adopted plugins.

use bevy::prelude::*;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineKind {
    Prebuilt,
    Project,
}

/// The project's native crate, when there is one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NativeIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EngineIdentity {
    pub kind: EngineKind,
    /// The engine hash for a project engine; `prebuilt-<version>-<target>` for the prebuilt one.
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeIdentity>,
}

impl EngineIdentity {
    pub fn prebuilt(version: &str, target: &str) -> Self {
        EngineIdentity { kind: EngineKind::Prebuilt, hash: format!("prebuilt-{version}-{target}"), native: None }
    }

    pub fn project(hash: &str, native: Option<NativeIdentity>) -> Self {
        EngineIdentity { kind: EngineKind::Project, hash: hash.to_string(), native }
    }
}
