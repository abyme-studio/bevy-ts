# Scripts

## Using npm packages

A script imports a package by name, as in Node or a bundler:

```ts
import { connect } from "@abyme/client/connect";
```

bevy-ts resolves every import when it compiles, not at run time. It rewrites each one to a
relative path to the file it found, so the engine, `bevy-ts play` and a shipped game load only
relative imports and carry no resolver.

### What resolves

- `node_modules` folders, walked up from the importing file.
- A package's `exports` and `imports` maps, and `main` when it has no `exports`.
- Scoped packages (`@scope/name`).
- Symlinks, followed to the real file: pnpm and workspace links (`workspace:*`) load the linked
  package's own files.
- `.ts` and `.tsx` in a package compile like your own files (types stripped).
- A package's relative import may leave the package (`../../kernel/src/protocol.ts`).
- Relative imports in your own code resolve the same way: `./x.js` finds `x.ts`, and `./x`
  finds `x.ts`, `x.tsx`, `x.js`, `x.mjs`, `x.jsx` or `x/index.*`.

Export conditions, in a package's order: `bevy-ts`, `import`, `module`, `default`, plus
`development` under `bevy-ts dev` and `production` under `bevy-ts build`. Not `node` (scripts
are not Node), not `browser` (there is no DOM), not `require` (ES modules only), not `types`
(no declaration files are loaded). A package can ship a `bevy-ts` condition for code meant for
games.

tsconfig `paths` are not read.

### What is refused

Each fails the load with the importing file and line:

- A package that is not installed.
- A subpath the package's `exports` do not offer.
- A Node built-in, `node:fs` or bare `fs`: scripts have no Node APIs.
- A CommonJS module: a `.cjs` file, or a `.js` file whose nearest `package.json` does not say
  `"type": "module"` and that has no `import` or `export`. bevy-ts loads ES modules only.
- A `.json` import.
- A package module that declares a component, resource, message, event, state or system set, or
  a `system(...)`: packages cannot declare ECS types yet. A package may import `bevy-ts` for
  anything else.

Package code is any module under a `node_modules` folder or outside the project root, so shared
code reached by a relative import or a linked folder outside the root cannot declare ECS types
either.

### Dynamic `import()`

A dynamic `import()` is not resolved when compiling. At run time it finds its target only among
the compiled modules, and otherwise rejects with an error the script can catch: a package name,
a file outside the graph, or a JSON import attribute. Import a package statically.

### `import.meta.url`

A module's `import.meta.url` is its file's URL: the real path under `bevy-ts dev`, and the
shipped file under `game/scripts/` in a built game. A URL that leaves a package finds nothing in
a shipped game.

### Installing

`bevy-ts dev` watches the `package.json` files it read and the nearest existing folder of each
path it looked for and did not find, inside the project or a package. Running `pnpm add` after a
"not installed" error reloads on its own, as does editing a linked package's `exports`.

### Shipping

`bevy-ts build` ships each package module at
`game/scripts/node_modules/<name>@<version>/<path in its package>` ([build.md](build.md)).
