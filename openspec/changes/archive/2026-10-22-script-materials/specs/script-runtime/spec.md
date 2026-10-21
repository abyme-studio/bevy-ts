## MODIFIED Requirements

### Requirement: Host-side TypeScript compilation
TS and TSX sources SHALL be compiled to JS by the host process with `oxc` (type stripping only, no type checking), producing a source map per module. `.tsx` SHALL compile with the automatic JSX runtime and `importSource: "bevy-ts"`; under `bevy-ts dev` it SHALL use the development transform, so each element carries its file, line and column in TS source coordinates. The engine SHALL load modules through its own module loader: relative imports and the built-in modules `bevy-ts`, `bevy-ts/bevy`, `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` are resolved; bare npm specifiers SHALL fail with an error naming the specifier. This change ships the two JSX runtime modules as the resolution targets only; what they do belongs to a later change. A relative import of a `.wgsl` file SHALL resolve to a synthetic module whose default export is an opaque shader-file value naming the file; the loader SHALL read the file host-side, not compile it, and watch it with the other loaded files. A missing `.wgsl` file SHALL fail the load with the importing file and line.

#### Scenario: A TSX element in dev
- **WHEN** `game.tsx` contains `<entity name="a" />` on line 7, column 3, and loads under `bevy-ts dev`
- **THEN** the compiled call imports from `bevy-ts/jsx-dev-runtime` and carries `game.tsx`, line 7, column 3

#### Scenario: A type error does not block a run
- **WHEN** `game.ts` contains a type error but valid syntax
- **THEN** it loads and runs; type checking is the editor's and `tsc`'s job

#### Scenario: An unsupported import
- **WHEN** a module imports `lodash`
- **THEN** loading fails with an error naming `lodash` and the importing file and line

#### Scenario: A shader file import
- **WHEN** `src/game.ts` writes `import glow from "./glow.wgsl"` and `src/glow.wgsl` exists
- **THEN** `glow` is a shader-file value naming `src/glow.wgsl`, the file is watched, and its WGSL is never run through `oxc`

#### Scenario: A missing shader file
- **WHEN** `src/game.ts` imports `./glw.wgsl` on line 3 and no such file exists
- **THEN** loading fails with an error naming `./glw.wgsl`, `src/game.ts` and line 3
