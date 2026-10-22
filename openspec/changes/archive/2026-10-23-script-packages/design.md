## Context

`Compiler::compile_graph` (`crates/bevy-ts-cli/src/compile.rs`) walks static imports from the entry. `compile_with` parses, reads declarations, strips types, collects each import's specifier and span, and generates code. Bare specifiers are refused there and again in the V8 loader (`crates/bevy-ts-v8/src/loader.rs`), which resolves a relative import by joining it to the referrer's `file://` URL and finding it in the `ModuleSet`. `dev.rs` watches the parent folder of each file in `compiler.files()`, non-recursively. `build.rs` writes each module at its path relative to the root and refuses one outside it. `play.rs` reads every non-hidden script file under `game/scripts/` back into a `ModuleSet`.

The one consumer today is `@abyme/client`: ESM, `exports` pointing at `.ts` source, linked by pnpm `workspace:*`, and reaching `../../kernel/src` outside its package. Research: `research.md`.

## Goals / Non-Goals

**Goals:** import ESM packages from npm and from workspace links, in `dev`, `build` and `play`, with one resolver and no resolver at run time. Clear errors for what is refused. Reload on install.

**Non-Goals:** CommonJS, Node built-ins, JSON modules, tsconfig `paths`, packages that declare ECS types, a compile cache, web globals (agent town step 2).

## Decisions

### 1. `oxc_resolver`, one per `Compiler`
`Compiler` owns one `oxc_resolver::Resolver`, built once with the conditions for its mode and cleared at the start of each compile (its cache would otherwise hide an install). It resolves every specifier that is not in `BUILTINS` and not `bevy_ts::modules::is_registered`, relative ones included, so `resolve_relative` and `resolve_shader` go. Options: `condition_names` per the spec; `extensions` `.ts`, `.tsx`, `.js`, `.mjs`, `.jsx`; `extension_alias` `.js` → `.ts`, `.tsx`, `.js` and `.jsx` → `.tsx`, `.jsx` (today's `.js`-for-`.ts` rule); `main_files` `index`; `symlinks: true`; `builtin_modules: true`, so `fs` and `node:fs` come back as a built-in error; no `tsconfig`. Each resolution runs with a `ResolveContext`, whose `file_dependencies` and `missing_dependencies` go into `files` and a new `missing` set.

Alternative: write Node resolution by hand. Rejected: ~600 lines and a long tail (`exports` patterns, `imports`, conditions, scoped names, symlinks) that Rolldown and Rspack already rely on `oxc_resolver` for. It needs the operator's go as a new dependency.

### 2. Resolve inside `compile_with`, rewrite before codegen
`compile_with` takes a resolve callback `FnMut(&str, &SourceLocation) -> Result<Resolved, ScriptError>`. Where it now collects imports, it calls the callback for each `ImportDeclaration`, `ExportAllDeclaration` and `ExportFromDeclaration` source and, for a file target, replaces the `StringLiteral`'s value with the relative path from the importer's shipped place to the target's (`./x.ts` or `../y/z.ts`). Only the value changes, never the span, so the source map is unchanged. Built-ins stay as written. The imports the declaration pass appends are relative to the same file and go through the same callback. `Compiled.imports` becomes the resolved targets, which the walk queues. A game module's own relative imports are rewritten too (`./util.js` becomes `./util.ts`), which the loader finds the same way.

Alternative: resolve in the V8 loader at run time. Rejected: `play` and shipped games would need a resolver and the package's `package.json` files; Vite's dev server rewrites too.

### 3. One layout function
`Compiler` has `layout: Layout`: `Identity` in `dev` (the shipped place of a file is its real path) and `Game { root }` in `build`, which maps game code to its path under the root and package code to `node_modules/<name>@<version>/<path from its package.json folder>`. The rewrite always computes `relative(layout(importer), layout(target))`, so one function decides both where `build.rs` writes a file and what every import says. `layout` needs only the file and the nearest `package.json` above it, so no second pass is needed. `build.rs` writes `m.path` at `scripts/<layout(m.path)>` and checks no two different files map to one path (the check lives in `compile_graph`, which knows each file's shipped place). `build.rs` asks the compiler's `layout` for each path, so the two never disagree. Source maps of package modules in a release name the shipped path, not the real one.

The layout avoids hidden folders, because `play.rs` skips them: pnpm's real paths run through `node_modules/.pnpm/`, which `<name>@<version>` replaces.

Alternative: lay out by the nearest common ancestor of all modules. Rejected: it can ship the build machine's folder names (`Users/rjm/dev`) and moves game files when a package is added.

### 4. Game code and package code
`is_package(path) = !path.starts_with(root) || path has a node_modules component`. One function, used by the layout and the declaration pass. Package modules compile with `declare: false`; `reader::extract` already runs and reports data classes and registrations, and a non-empty result from a module that imports `bevy-ts` fails the load (spec: "Package code is library code").

### 5. ESM check
After resolution, a target is CommonJS when it ends `.cjs`, or ends `.js` and its nearest `package.json` has no `"type": "module"` and the parsed module has no module syntax (oxc's `module_record`). The check happens when the file is compiled, with the importer's location kept on the queue entry, so the error names both. `.json` targets fail before reading.

### 6. Watching missing paths
`Compiler::files()` already holds what the watcher watches; it gains the `package.json` files (`file_dependencies`). A new `Compiler::missing()` returns `missing_dependencies`; `dev.rs` watches the nearest existing ancestor folder of each, non-recursively, except a folder above the project root: the resolver also misses `node_modules` in every folder up to `/`, and watching `/` or the home folder would be all noise. `pnpm install` creating `node_modules/` changes the project root folder, which is then watched. No new reload path.

### 7. Loader
The loader keeps resolving relative imports against the graph. The bare-specifier branch only changes its message: a dynamic `import()` of a package is not in the compiled graph; import it statically. The rejected promise is catchable, which `@abyme/client`'s guarded `import(url, { with: { type: "json" } })` relies on. No protocol change.

### 8. Bevy's principles
Bevy has no script packages; its counterpart is a crate, resolved and linked before the game runs. Compile-time resolution keeps the same shape: nothing resolves at run time and the shipped game carries only what it loads.

## Risks / Trade-offs

- [A large package slows every reload, since the whole graph recompiles] → fine for `@abyme/client` (a handful of files). A content-keyed per-file compile cache is the fix if a real package needs it; out of scope.
- [A package that is ESM but ships `.js` with no `type` and only dynamic `import()` looks CommonJS] → rare; the error names the file, and the package can set `"type": "module"`.
- [Two pnpm peer variants of one `name@version` ship at the same path] → identical files share the path; different files fail the build naming both.
- [Game code gains extensionless `.js`/`.mjs` resolution] → widening only; nothing that loaded before changes target, since `.ts` and `.tsx` are tried first.
- [`import.meta.url` differs between dev and a shipped game] → spec states both; a URL that leaves a package finds nothing in a shipped game, and dynamic imports of it reject catchably.

## Migration Plan

None. Projects without packages compile and ship exactly as before: game code keeps its paths, and relative imports resolve to the same files. Rollback is reverting the change.

## Open Questions

None blocking. The dependency waits on the operator's go.
