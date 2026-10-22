## 1. Dependency (waits on the operator's go)

- [x] 1.1 Add `oxc_resolver = "11.24"` to `crates/bevy-ts-cli/Cargo.toml`; confirm `cargo tree -p bevy-ts-cli` adds only the six crates the proposal names

## 2. Resolution and rewrite (`compile.rs`)

- [x] 2.1 Build one `Resolver` per `Compiler` with the mode's conditions (`bevy-ts`, `import`, `module`, `default`, plus `development` or `production`), the extensions and `.js`/`.jsx` aliases from design 1, symlinks on, built-ins flagged, no tsconfig; clear its cache at the start of each `compile_graph`
- [x] 2.2 Add `is_package(path)` and the `Layout` (`Identity` for dev, `Game { root }` for build: `node_modules/<name>@<version>/<path in package>` for package code)
- [x] 2.3 Give `compile_with` a resolve callback; resolve each static import and re-export source, rewrite its string literal to `relative(layout(importer), layout(target))` before codegen, including the imports the declaration pass appends; keep built-ins and registered modules as written
- [x] 2.4 Delete `resolve_relative` and `resolve_shader`; a resolved `.wgsl` target becomes the shader module as before
- [x] 2.5 Errors with the importer's file and line: not installed, subpath not exported, Node built-in, CommonJS (design 5), `.json` import, and a package module declaring ECS types
- [x] 2.6 Compile package modules with `declare: false`; release source maps of package modules name the shipped path
- [x] 2.7 Record `file_dependencies` into `files` and `missing_dependencies` into a new `missing()` set
- [x] 2.8 Unit tests with fixture `node_modules` trees: `exports` with conditions (bevy-ts, development, production), scoped package, `main` fallback, pnpm-style symlink, a package's relative import leaving the package, unexported subpath, CJS, `node:fs` and bare `fs`, `.json`, a package declaring a component, the source map unchanged by the rewrite; update `a_bare_import_fails_with_its_location` to the not-installed error

## 3. Watcher (`dev.rs`)

- [x] 3.1 Watch the nearest existing ancestor folder of each `compiler.missing()` path, rewatched after each compile
- [x] 3.2 Test: a failed load on a missing package reloads once the package folder appears; editing a watched `package.json` reloads

## 4. Build and play (`build.rs`, `play.rs`)

- [x] 4.1 Write each module at `scripts/<layout(path)>`; drop the outside-the-project refusal; fail on two different files at one shipped path
- [x] 4.2 Round-trip test: build a fixture with a linked package that reaches outside itself, delete the fixture's `node_modules`, and load the folder with `read_scripts`; no shipped file contains the build machine's absolute paths

## 5. Loader (`bevy-ts-v8/src/loader.rs`)

- [x] 5.1 Change the bare-specifier message: a dynamic `import()` of a package is not in the compiled graph; import it statically
- [x] 5.2 Test: a guarded `import(url, { with: { type: "json" } })` of a file outside the graph rejects and the `catch` runs

## 6. Docs

- [x] 6.1 New `docs/scripts.md`, "Using npm packages": what resolves, conditions, what is refused and why, `import.meta.url` in dev and shipped, reload on install; link it from `README.md`
- [x] 6.2 `docs/build.md`: the `game/scripts/node_modules/<name>@<version>/` layout

## 7. Gate

- [x] 7.1 `cargo build`, `cargo test` and `cargo clippy` on the workspace, once, last
- [x] 7.2 `openspec validate script-packages --strict`
