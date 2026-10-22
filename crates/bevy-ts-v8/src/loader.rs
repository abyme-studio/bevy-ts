//! The module loader: built-in `bevy-ts` modules, and a generation's compiled modules under
//! generation-tagged specifiers (`file:///…/game.ts?gen=7`) with their source maps.

use bevy_ts::backend::CompiledModule;
use deno_core::error::ModuleLoaderError;
use deno_core::{
    ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier,
    ModuleType, ResolutionKind,
};
use deno_error::JsErrorBox;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

pub const BUILTIN_CORE: &str = "bevy-ts:core";
pub const BUILTIN_BEVY: &str = "bevy-ts:bevy";
const BUILTIN_JSX: &str = "bevy-ts:jsx-runtime";
const BUILTIN_JSX_DEV: &str = "bevy-ts:jsx-dev-runtime";

const CORE_JS: &str = include_str!("../js/core.js");
const BEVY_JS: &str = include_str!("../js/bevy.js");
const JSX_JS: &str = include_str!("../js/jsx-runtime.js");
const JSX_DEV_JS: &str = include_str!("../js/jsx-dev-runtime.js");

#[derive(Default)]
pub struct Loader {
    /// The generation being loaded: specifier (with `?gen=`) to module.
    modules: RefCell<HashMap<String, CompiledModule>>,
    /// Source maps of every module loaded, by specifier; kept so old generations still map.
    maps: RefCell<HashMap<String, Vec<u8>>>,
    load: RefCell<u64>,
}

impl Loader {
    pub fn set_generation(&self, load: u64, modules: Vec<(String, CompiledModule)>) {
        *self.load.borrow_mut() = load;
        let mut maps = self.maps.borrow_mut();
        for (spec, m) in &modules {
            if let Some(map) = &m.source_map {
                maps.insert(spec.clone(), map.clone().into_bytes());
            }
        }
        *self.modules.borrow_mut() = modules.into_iter().collect();
    }

    pub fn source_of(&self, path: &Path) -> Option<String> {
        self.modules.borrow().values().find(|m| m.path == path).map(|m| m.code.clone())
    }

    fn builtin(specifier: &str) -> Option<&'static str> {
        Some(match specifier {
            "bevy-ts" | BUILTIN_CORE => BUILTIN_CORE,
            "bevy-ts/bevy" | BUILTIN_BEVY => BUILTIN_BEVY,
            "bevy-ts/jsx-runtime" | BUILTIN_JSX => BUILTIN_JSX,
            "bevy-ts/jsx-dev-runtime" | BUILTIN_JSX_DEV => BUILTIN_JSX_DEV,
            _ => return None,
        })
    }

    /// Finds the module a relative import names, trying the TS extensions a TS author omits or
    /// writes as `.js`.
    fn find(&self, url: &ModuleSpecifier) -> Option<String> {
        let modules = self.modules.borrow();
        let base = url.as_str().split('?').next().unwrap().to_string();
        let tag = format!("?gen={}", self.load.borrow());
        let mut candidates = vec![base.clone()];
        if let Some(stem) = base.strip_suffix(".js").or_else(|| base.strip_suffix(".jsx")) {
            candidates.push(format!("{stem}.ts"));
            candidates.push(format!("{stem}.tsx"));
        }
        for ext in [".ts", ".tsx", "/index.ts", "/index.tsx"] {
            candidates.push(format!("{base}{ext}"));
        }
        candidates.into_iter().map(|c| format!("{c}{tag}")).find(|c| modules.contains_key(c))
    }
}

impl ModuleLoader for Loader {
    fn resolve(&self, specifier: &str, referrer: &str, _kind: ResolutionKind) -> Result<ModuleSpecifier, ModuleLoaderError> {
        if let Some(b) = Self::builtin(specifier) {
            return ModuleSpecifier::parse(b).map_err(JsErrorBox::from_err);
        }
        // A port's module (`bevy-ts/rapier`), served as `bevy-ts:rapier`.
        if let Some(name) = specifier.strip_prefix("bevy-ts/")
            && bevy_ts::modules::is_registered(specifier)
        {
            return ModuleSpecifier::parse(&format!("bevy-ts:{name}")).map_err(JsErrorBox::from_err);
        }
        if specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/') || specifier.starts_with("file:") {
            let url = deno_core::resolve_import(specifier, referrer).map_err(JsErrorBox::from_err)?;
            return match self.find(&url) {
                Some(found) => ModuleSpecifier::parse(&found).map_err(JsErrorBox::from_err),
                None if referrer.starts_with("bevy-ts:") => Ok(url),
                None => Err(JsErrorBox::generic(format!(
                    "cannot import {specifier:?} from {}: no such module in the compiled graph",
                    bevy_ts::backend::display_file(referrer)
                ))),
            };
        }
        Err(JsErrorBox::generic(format!(
            "cannot import {specifier:?} from {}: a dynamic import() of a package is not in the compiled graph; import it statically",
            bevy_ts::backend::display_file(referrer)
        )))
    }

    fn load(&self, specifier: &ModuleSpecifier, _referrer: Option<&ModuleLoadReferrer>, _options: ModuleLoadOptions) -> ModuleLoadResponse {
        let code = match specifier.as_str() {
            BUILTIN_CORE => Some(CORE_JS.to_string()),
            BUILTIN_BEVY => Some(BEVY_JS.to_string()),
            BUILTIN_JSX => Some(JSX_JS.to_string()),
            BUILTIN_JSX_DEV => Some(JSX_DEV_JS.to_string()),
            s => match s.strip_prefix("bevy-ts:") {
                Some(name) => bevy_ts::modules::js(&format!("bevy-ts/{name}")),
                None => self.modules.borrow().get(s).map(|m| m.code.clone()),
            },
        };
        ModuleLoadResponse::Sync(match code {
            Some(code) => Ok(ModuleSource::new(ModuleType::JavaScript, ModuleSourceCode::String(code.into()), specifier, None)),
            None => Err(JsErrorBox::generic(format!("module {specifier} is not in the compiled graph"))),
        })
    }

    fn get_source_map(&self, file_name: &str) -> Option<Cow<'_, [u8]>> {
        self.maps.borrow().get(file_name).map(|m| Cow::Owned(m.clone()))
    }
}
