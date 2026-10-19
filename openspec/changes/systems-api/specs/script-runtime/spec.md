## ADDED Requirements

### Requirement: Declarations are appended, never woven in
The compile step SHALL attach a module's system declarations by appending statements after the module's code, so every line of the original keeps its number. A component, resource, message, event or state named in a param SHALL be bound through the module whose source text names it. A name imported with `import type` that a param needs at run time SHALL fail the compile with its file and line.

#### Scenario: A type-only import
- **WHEN** `game.ts` writes `import type { Enemy } from "./enemy"` and exports `function chase(q: Query<[Enemy]>)`
- **THEN** the compile fails at that import's line with "Enemy is imported as a type; system params need its value"

### Requirement: The transform command
The `bevy-ts` binary SHALL offer `bevy-ts transform <file>`, printing the compiled module with its declarations, and the npm package SHALL ship Vite, vitest and esbuild plugins that call it, so a module compiled by those tools declares its systems as under `bevy-ts dev`.

#### Scenario: A system in a vitest test
- **WHEN** a vitest test imports a module through the bevy-ts plugin and registers its system with the `bevy-ts/testing` app
- **THEN** the system's declarations are present and the test can call it with hand-built params
