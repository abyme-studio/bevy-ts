# Research: npm packages in game scripts

For agent town step 1 (`~/.local/state/abyme/documents/bevy-ts/agent-town.md`, Build order 1):
the game imports `@abyme/client/connect` unchanged. Today bare specifiers are refused twice:
`crates/bevy-ts-cli/src/compile.rs:330` (compile) and `crates/bevy-ts-v8/src/loader.rs:99`
(runtime). Research by research@script-packages, 2026-10-23.

## What the one real consumer looks like

`~/dev/abyme/web/packages/client/package.json`: `"type": "module"`, an `exports` map whose targets
are **TypeScript source** (`"./connect": "./src/connect.ts"`), linked by pnpm `workspace:*`.
`connect.ts` imports `../../kernel/src/*.ts` (outside its own package), uses `.ts` extensions in
specifiers, the global `WebSocket`, and one `fetch` and one dynamic `import(url)` on paths a game
does not take. No CJS and no `node:` imports on the path a game uses.

So v1 must: read `exports`, follow pnpm symlinks, compile `.ts` from a package, and allow a
package's relative import to leave the package. It need not: CJS, `node:` builtins, dynamic
`import()` of a bare specifier.

## 1. Resolution: use `oxc_resolver`

How others do it:

- Node: `node_modules` walk up from the importer; `exports` (with conditions, subpath patterns)
  wins over `main`; `imports` for `#internal` specifiers; symlinks resolved to real paths.
- esbuild, Vite, Rolldown, Rspack, Biome all implement the same algorithm; Rolldown and Rspack use
  `oxc_resolver` (a port of webpack's enhanced-resolve).
- Deno: its own `node_resolver` crate inside the Deno CLI's node compat, not in `deno_core`.
  Not usable alone; it brings Deno's npm and permission model.

`oxc_resolver` 11.24.3 (2026-08-24, 5.5M downloads): `exports`/`imports` with conditions, `main`,
`browser` field, scoped packages, symlinks (real paths by default), tsconfig `paths`, and
`extension_alias` (`.js` → `.ts`), which matches what our `resolve_relative` does by hand.
It also reports **file and missing dependencies** per resolution (`ResolveContext`), which is
exactly what the watcher needs.

Not in our tree. Its own repo and versioning, no dependency on the `oxc` 0.154 crates, so no
version lock-step with our parser. New crates it brings: `dashmap`, `simd-json`, `fast-glob`,
`nodejs-built-in-modules`, `json-strip-comments`, `self_cell` (`indexmap`, `rustix`,
`compact_str` are already here). It needs `serde_json/preserve_order`, already on in our tree.

Recommendation: `oxc_resolver`, one resolver owned by `Compiler`, used for bare specifiers and
also for relative ones (replacing `resolve_relative`'s hand-written candidate list). High confidence.

## 2. What to allow

- **ESM only.** A module that is CJS (`.cjs`, or `.js` under `"type": "commonjs"` or no `type`
  with `require`/`module.exports` and no `import`/`export`) fails with an error naming the package
  and the importing line. CJS needs a `require` shim and named-export analysis (Deno uses its
  node compat plus a CJS lexer); that is Node compatibility, which Bevy has no reason to carry.
  Medium-high confidence this is right for now; revisit only on a real package that needs it.
- **`node:` builtins: refused**, with an error naming the builtin. Scripts are not Node: no `fs`,
  `process`, `Buffer`. A bare `fs`/`path` is caught too (`oxc_resolver` flags builtins).
- **Conditions:** `["bevy-ts", "import", "module", "default"]`, plus `"development"` under
  `bevy-ts dev` and `"production"` under `bevy-ts build` (Vite does the same). Not `node`: it picks
  Node-only builds (e.g. `ws`). Not `browser`: browser builds may touch `window`/`document`.
  `bevy-ts` lets a package ship a build for us. `main` is the fallback when there is no `exports`.
- **TS in packages: compiled**, like any project file (oxc strips; Vite, Bun and Deno do the same).
  Node refuses TS under `node_modules`; we need not, and pnpm workspace links are not under it.
- **Package code skips `reader` declarations** (systems, data classes): those are the game's own;
  package code is library code. Decide in spec; low cost either way.
- **`.json` imports**: not needed now; leave refused.
- **Dynamic `import()` of a bare specifier**: not in the static graph, so it fails at runtime with
  the current "not in the compiled graph" error. Keep that; say so in the spec.

## 3. Where it plugs in

Today: `Compiler::compile_graph` (`compile.rs:309`) walks static imports from the entry, compiles
each file with oxc, and returns a `ModuleSet` of `CompiledModule { path, code, source_map }`. The V8
`Loader` (`bevy-ts-v8/src/loader.rs`) resolves at runtime by joining the specifier to the
referrer's `file://` URL and finding it in that set. `dev.rs` watches exactly `compiler.files()`
(parent dirs, non-recursive). `build.rs:310` writes each module under `game/scripts/` at its path
relative to the project root, and errors when a path is outside it. `play.rs:63` reads them back.

Recommendation: **resolve at compile time and rewrite the specifier** in the compiled code to a
relative path to the resolved file. Then the loader, the shipped game and `play.rs` see only
relative imports and need no resolver. This is what Vite's dev server does (rewrites bare imports
to resolved URLs). The rewrite edits the import's string literal in the AST before codegen, so
source maps stay right.

- `compile.rs`: `oxc_resolver::Resolver` in `Compiler` (cache cleared each compile), resolve every
  non-builtin specifier, rewrite, CJS and builtin errors, record `file_dependencies` and
  `missing_dependencies` into `files`.
- `dev.rs`: watch those too, so editing a `package.json` or running `pnpm install` after a
  "cannot find package" error reloads. Missing dependencies are paths that do not exist yet, so
  watch their nearest existing parent dir.
- `build.rs`: a file outside the root (a pnpm workspace link resolves to `~/dev/abyme/web/...`)
  needs a place under `scripts/`. Lay out by package: `scripts/node_modules/<name>/<path in pkg>`,
  and the rewrite computes relative paths in that layout. `connect.ts`'s `../../kernel/src/` import
  leaves its package, so the layout must be "nearest common ancestor of all modules" or per-file
  mapping, not per-package. Simplest correct: map every module to a path under `scripts/` with one
  function, and rewrite all specifiers (relative ones too) through it.
- `loader.rs`: only the error text at `:99` changes ("not in the compiled graph"); no resolution.
- Spec: `openspec/specs/script-runtime/spec.md:34` (the "unsupported import" scenario becomes
  "a package import", plus CJS, `node:`, missing-package scenarios); hot-reload spec for the
  watched package files; game-build spec for the layout.
- Docs: the scripts guide gains "Using npm packages".
- Tests: fixture projects with a `node_modules` tree (exports with conditions, scoped, symlinked
  pnpm layout, a CJS package, a `node:` import), plus a build round-trip through `play`.

## 4. Ask-first items

- **New dependency: `oxc_resolver`** and its six new transitive crates (above). The alternative is
  writing Node resolution ourselves (~600 lines and a long edge-case tail); not recommended.
- **Performance:** none measurable expected. Resolution is file-system stat calls, cached per
  compile; a reload already re-reads every file. Package modules add compile time in proportion to
  their size: a large package (hundreds of modules) would add to startup and every reload, since
  the compiler recompiles the whole graph each time today. Not an issue for `@abyme/client`
  (~3 package files plus kernel). A per-file compile cache keyed by content is the fix if it
  ever is; out of scope.

## For abyme (not this change)

`@abyme/client` is `private` and `connect.ts` reaches into `../../kernel/src`. With this design it
works as is through a pnpm/file link, since resolution follows real paths and relative imports may
leave the package. Its stray `fetch` (`connect.ts:693`) and dynamic `import(url)` (`:523`) must not
run on the game's path; step 2 (networking) owns `fetch`.

Sources: crates.io `oxc_resolver` 11.24.3 and its dependency list; Deno docs on CommonJS
(docs.deno.com/runtime/fundamentals/node); bevy-ts and abyme code as cited.
