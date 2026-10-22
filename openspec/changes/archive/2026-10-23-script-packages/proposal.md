## Why

A game script cannot import an npm package: the compiler and the V8 loader both refuse a bare specifier. Agent town step 1 needs the game to import `@abyme/client/connect` unchanged, from a pnpm workspace link whose `exports` point at TypeScript source. Research (`research.md`) found one approach that keeps the runtime and the shipped game free of a resolver: resolve at compile time and rewrite each import to a relative path.

## What Changes

- **Bare specifiers resolve at compile time.** The compiler resolves every import with one Node-style resolver (`oxc_resolver`): `node_modules` walk-up, `exports` and `imports` maps with conditions, `main` as the fallback, scoped packages, and pnpm symlinks followed to real paths. It rewrites each import's string to a relative path to the resolved file, before codegen, so source maps stay right. The loader, `bevy-ts play` and a shipped game see only relative imports.
- **Relative imports go through the same resolver.** `resolve_relative`'s hand-written candidate list goes; game code resolves as before, and also finds `.js` and `.mjs` files written without an extension.
- **Conditions:** `bevy-ts`, `import`, `module`, `default`, plus `development` under `bevy-ts dev` and `production` under `bevy-ts build`. Not `node`, not `browser`, not `types`.
- **Package code compiles like project code.** `.ts` and `.tsx` in a package are type-stripped by oxc. A package's relative import may leave its package (`@abyme/client` reaches `../../kernel/src`).
- **Package code is library code.** A module outside the project root or under a `node_modules` folder gets no declaration pass; one that declares a component, resource, message, event or system fails the load naming the package.
- **Refused, each with a named error:** a CommonJS module, a Node built-in (`node:fs`, bare `fs`), a package that is not installed, a subpath its `exports` does not offer, and a JSON import. A dynamic `import()` resolves only against the compiled graph at run time and rejects, catchably, when the target is not in it.
- **`bevy-ts dev` reloads on `pnpm install`.** The watcher also watches the `package.json` files the resolver read and the nearest existing folder of each path it looked for and did not find.
- **`bevy-ts build` ships packages.** Game code keeps its path relative to the root. A package module ships at `game/scripts/node_modules/<name>@<version>/<path in package>`, and every import is rewritten for that layout. `build.rs`'s "outside the project" refusal goes. No path of the build machine is shipped.
- **No protocol change.** Errors use the existing script-error path.
- **Out of scope:** CommonJS, Node built-ins, JSON modules, tsconfig `paths`, a compile cache for large packages, packages that declare components or systems, and the web globals `@abyme/client` uses (`WebSocket`, timers; agent town step 2, research@script-networking).

Bevy's counterpart is a crate from crates.io: a plugin the build resolves and links ahead of time. This change does the same for scripts and adds no engine feature.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-runtime`: "Host-side TypeScript compilation" replaces the bare-specifier refusal with package resolution, and states what is refused and why.
- `hot-reload`: a new requirement: package files and missing paths are watched, so an install reloads.
- `game-build`: "`bevy-ts build` writes a runnable game folder" gains the package layout under `game/scripts/node_modules/`.

## Impact

- **Ask-first, operator's go needed: new dependency `oxc_resolver` 11.24** in `crates/bevy-ts-cli`, with six new transitive crates: `dashmap`, `simd-json`, `fast-glob`, `nodejs-built-in-modules`, `json-strip-comments`, `self_cell`. Independent of our `oxc` 0.154 crates. The alternative is a hand-written Node resolver (~600 lines plus edge cases).
- Performance: no measurable change for small packages. Every reload recompiles the whole graph today, so a package of hundreds of modules would slow startup and each reload in proportion; `bevy-ts build` compiles once, as now.
- Code: `crates/bevy-ts-cli/src/compile.rs` (resolver, rewrite, errors, files watched), `dev.rs` (missing-path watches), `build.rs` (layout), `reader.rs` (package code skips declarations), `crates/bevy-ts-v8/src/loader.rs` (error text only).
- Docs: `docs/scripts.md` (new, "Using npm packages"), `docs/build.md` (the folder layout).
- Tests: fixture projects with a `node_modules` tree (conditions, scoped, pnpm symlinks, a CJS package, a `node:` import, a workspace link that leaves its package), and a build round-trip through `play`.
