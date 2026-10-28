## 1. One list of script plugins

- [x] 1.1 `ScriptPlugins` (`PluginGroup`) in `cli.rs` with every plugin that ships a built-in module, `ScriptReadyPlugin` among them; `base_app` and `types_app` add it in place of their own lists
- [x] 1.2 `tests/it/types_modules.rs`: the run's headless app built in a child process of the test binary; every module it registers is declared in `bevy-ts types`' `native-modules.d.ts`
- [x] 1.3 `scripts/gen-bevy-types.sh` rerun; `docs/native.md` names `ScriptPlugins` as where a module's plugin goes
