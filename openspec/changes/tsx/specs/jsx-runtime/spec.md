## ADDED Requirements

### Requirement: A custom JSX runtime with no React
The `bevy-ts` package SHALL provide `bevy-ts/jsx-runtime` (`jsx`, `jsxs`, `Fragment`) and `bevy-ts/jsx-dev-runtime` (`jsxDEV`, `Fragment`) for TypeScript's automatic runtime with `jsxImportSource: "bevy-ts"`. It SHALL NOT depend on React or any npm package, and SHALL ship the `JSX` namespace types so `tsc` checks elements and props.

#### Scenario: A project with no React installed
- **WHEN** a project with `"jsx": "react-jsx"` and `"jsxImportSource": "bevy-ts"` in its tsconfig and no `react` package compiles `game.tsx`
- **THEN** `tsc` reports no errors and `bevy-ts dev` loads it

### Requirement: Elements describe, they do not execute
Calling the JSX runtime SHALL return a plain-data description node (`type`, `key`, `props`, `children`, `source`) and SHALL NOT spawn, insert or call into Rust. Function-valued props and children SHALL be kept as bindings or handlers to be replaced by ids when the description is sent.

#### Scenario: Building an element in a unit test
- **WHEN** a test calls `<Enemy at={[1, 0, 0]} />` with no engine running
- **THEN** it gets a description tree whose entity has a `Transform` element with `translation` `[1, 0, 0]`

### Requirement: Entity, component, prefab and intrinsic elements
`<entity>` SHALL describe one entity; its children SHALL be component elements and child entities in any order. A component element's tag SHALL be a component class or a Rust component typed by bevy-ts (`Transform`, `Name`), and its props SHALL be that component's fields. A prefab SHALL be either a plain TS function from props to one element, called when its element is created (usable in TSX only, like a Rust scene function), or a scene component, a class extending `SceneComponent` with `static scene(props)`, whose element takes its fields and its scene's props (`<Enemy team={1} hp={250} />`), adds the component to the scene's root, and is placeable from a level (levels). A scene component whose field and prop share a name SHALL fail the load. `<mesh>`, `<material>`, `<node>`, `<text>`, `<image>` and `<button>` SHALL be intrinsics. A fragment SHALL describe several siblings. A component element outside an `<entity>` or an intrinsic, or a component given twice on one entity, SHALL be an error naming the element's source location.

#### Scenario: Two Transforms on one entity
- **WHEN** an `<entity>` has two `<Transform>` children
- **THEN** the scene fails with an error naming the second element's file and line

### Requirement: initial and controlled props are typed per field
For each field `f` of a component, the element's props SHALL accept `f` (controlled) and `initialF` (initial, `f` with its first letter capitalised), typed from the `component()` declaration with no codegen. Giving both for one field SHALL be a type error and, if it reaches the runtime, an error naming the field.

#### Scenario: Both spellings for one field
- **WHEN** a prefab writes `<Health hp={10} initialHp={10} />`
- **THEN** `tsc` reports a conflict on `hp`, and loading the scene fails with an error naming `game::Health.hp`

### Requirement: Source location on every element in dev
Under `bevy-ts dev`, every description node SHALL carry its file (project-relative), line and column in TS source coordinates, taken from the development JSX transform. Each entity SHALL also carry the call-site locations of every prefab that produced it, innermost first. Release builds MAY omit source.

#### Scenario: An entity made inside a prefab
- **WHEN** `game.tsx` line 12 writes `<Enemy key="b" />` and `enemy.tsx` line 4 writes the `<entity>` it returns
- **THEN** that entity's source chain is `enemy.tsx:4`, then `game.tsx:12`

### Requirement: Keys are explicit or type-scoped, never positional in source
An element's key SHALL be its `key` prop if given; otherwise `type#n`, where `type` is the intrinsic name, component or prefab name and `n` counts the preceding siblings with that type. Source line and column SHALL NOT take part in a key. Sibling elements produced from an array without keys SHALL produce a dev warning naming the location. Two siblings with the same explicit key SHALL be an error.

#### Scenario: Adding a blank line
- **WHEN** a blank line is inserted above `<Enemy key="b" />` and the file reloads
- **THEN** enemy `b` keeps its entity id

#### Scenario: Duplicate keys
- **WHEN** two sibling `<Enemy key="a" />` elements are mounted
- **THEN** the scene fails with an error naming both locations
