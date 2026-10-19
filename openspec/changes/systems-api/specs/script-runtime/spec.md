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

### Requirement: Data classes declare themselves when defined
The compile step SHALL give every class it reads as a component, scene component, resource, message, event, state machine or system set a static block, with no source mapping, that hands the runtime the class's name, fields and scene props, so the class carries its resolved name (`componentName`), `source`, `fieldNames` and, for a scene component, `sceneProps` from the moment it is defined. A field the compile step cannot type SHALL fail the load when the class is defined, with the field's file and line. The runtime SHALL keep the generation's scene components by name and by `module#export`.

#### Scenario: A scene reads a component at module load
- **WHEN** `game.tsx` declares `class Health extends Component { hp: f32 = 100 }` and, later in the same module, a scene uses `<Health hp={3} />`
- **THEN** the element is keyed by `game::Health` while the module evaluates, before the generation commits
