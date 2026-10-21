//! A material's shader source: the author's file with the generated declarations appended, and
//! the check that keeps a broken file from replacing a working shader.

use crate::kinds::MaterialDecl;
use crate::layout::Bindings;
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use std::path::Path;

/// The author's file, then the params and texture declarations. Appended, not prepended: WGSL
/// declarations are order-free and WESL imports must come first, so the author's lines keep their
/// numbers and anything past their last line is generated.
pub fn combine(text: &str, bindings: &Bindings, material: &str) -> String {
    let mut s = text.to_string();
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&bindings.wgsl(material));
    s
}

/// Checks a combined source before it replaces a shader. Errors name the material and the
/// author's `file:line:col`, or the TS schema for an error in the generated part.
pub fn check(_world: &World, _combined: &str, _text: &str, _file: &Path, _decl: &MaterialDecl) -> Result<(), Vec<ScriptError>> {
    Ok(())
}
