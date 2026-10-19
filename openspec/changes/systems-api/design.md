## Context

The bridge shipped a seam (manifest JSON per generation, column buffers per run, a command arena) and a first TS surface over it. The 2026-10-20 ruling replaced the surface. The specs in `openspec/specs` now say what to build; `docs/research/dx.md`, `bench/dx/bevy-ts/mod.ts` (the ruled types), `bench/dx/examples/*.ts` and `bench/dx/read-annotations.ts` (a prototype reader in the TypeScript API) show it. This design fixes how the ruled surface maps onto the seam the bridge built.

## Goals / Non-Goals

**Goals:**
- Every requirement in `ts-systems`, `ts-components`, `script-runtime` and `hot-reload` holds in the code.
- One call into JS per system run stays true; a skipped system (run condition false) costs none.
- `bench/dx/examples/systems.ts`, adapted to real imports, loads and runs.

**Non-Goals:** reflection-derived layouts and `bevy-ts.json` plugin adoption (`native`, unruled); TSX bindings without hooks (unruled); ports beyond the param-kind registry.

## Decisions

### 1. The compile step appends declarations; it never edits a body
After oxc strips types, the reader appends statements to the module's code (source maps keep every original line):

```js
;__bevyts.bind("src/enemies.ts", { Transform: () => Transform, Alive: () => Alive });
;__bevyts.declare(chase, { id: "src/game.ts#chase", at: {...}, kind: "system", params: [...] });
;__bevyts.declare(drift, { id: "src/game.ts#drift", at: {...}, kind: "setup" });
;__bevyts.declare(helper, { id: "src/game.ts#helper", at: {...}, error: "helper is not exported; ..." });
```

- `declare` runs for every top-level function declaration and every `export const x = system(...)`. A function whose params cannot be read gets `error` instead of `params`. So does a function that is not exported. The error is raised only if the function is registered: a helper function with odd params is fine.
- A component, resource or message named in a param is a reference `["<module path>", "<local name>"]`. It points into the module whose source text names it: the use site for inline types and generic arguments, and the alias's own module for names written inside an alias. Each module binds the local names its own text contributed, as thunks, so circular imports and declaration order do not matter.
- References resolve at the generation's end, after every module has evaluated. A name imported with `import type` has no value at run time, so the reader fails it at compile time: "X is imported as a type; system params need its value".
- Param descriptors (JSON):
  - `{ kind: "query", data: [{ entity: true } | { component: Ref, mut, optional }], filter: [{ with|without|changed|added: Ref }] }`
  - `{ kind: "res" | "resMut", resource: Ref }`
  - `{ kind: "local", fields: [{ name, type: FieldType }] }`
  - `{ kind: "commands" }`
  - `{ kind: "messageReader" | "messageWriter", message: Ref }`
  - `{ kind: "state" | "nextState", state: Ref }`
  - `{ kind: "on", event: Ref }` or `{ kind: "on", lifecycle: "add" | "insert" | "remove", component: Ref }`
  - `{ kind: "bundle", fields: [{ name, param }] }`
  - `{ kind: "app" }`
  - `{ kind: "port", name, args }`
- Kinds are recognised by name: `Query`, `Res`, `ResMut`, `Local`, `Commands`, `MessageReader`, `MessageWriter`, `State`, `NextState`, `On`, `App`, `Mut`, `Option`, `Entity`, `With`, `Without`, `Changed`, `Added`, `Add`, `Insert`, `Remove`. Port kinds come from a registry the compiler crate exposes; it is empty until `native` lands.
- Identity is `<path relative to the project root>#<export name>`. The project root is the nearest ancestor of the entry holding `package.json`, `tsconfig.json` or `bevy-ts.json`, else the entry's directory.

- *Beat: rewriting each export into a wrapper call.* That moves columns, and breakpoints and stacks would land in generated code.
- *Beat: binding names through a global registry of component names.* The TS identifier `Enemy` is not the Rust name `game::Enemy`, and two modules may each have a local `Score`.

### 2. The seam carries a run's params, not one query
`SystemRun` holds `params: &[ParamFrame]`, one per param, in order:

- `Query { tables: [TableFrame] }`: each table frame carries its columns. A query with `Changed`/`Added` also carries a row list (below), and an optional column absent from a table is a null pointer.
- `Resource { data, bytes, ticks? }`
- `Local { data, bytes }`
- `Messages { data, count, stride }`
- `State { value }`
- `Commands`, `Port { bytes }`

The backend still sees pointers and lengths only. Rust resolves `Entity`, filters and bundles to this shape; JS rebuilds bundles from the descriptor.

### 3. `Changed`/`Added` become per-table row lists
`QueryBuilder` has no dynamic `Changed`/`Added`. Rust scans the filter columns' tick slices against the system's last-run tick, which is kept per identity, and hands JS a `Uint32Array` of matching rows per table. It skips tables with no match. The scan reads only the ticks Bevy's own `Changed<T>` would read. `each`, `for…of`, `get` and `single` visit only the listed rows. `table.read/write/split` keep raw access to every row, and `table.rows` exposes the list.

### 4. Resources are one-row columns on their resource entity
- A TS resource is a dynamic component registered with the same C layout rules. It lives on its own entity with `IsResource(id)`, as Bevy 0.20 stores every resource.
- `app.insertResource(new R(...))` inserts it when absent at apply; a reload keeps the value.
- `Res<R>`/`ResMut<R>` take a one-row view: the entity's table row in the column, plus its tick for `ResMut`. The view is zero-copy and uses the same accessor classes.
- `Res<Time>` is the one computed view, with Bevy's own field names (operator ruling 2026-10-20). Rust writes `{ delta_secs: f32, elapsed_secs: f32, delta_secs_f64: f64, elapsed_secs_f64: f64 }` into a per-run scratch buffer from the schedule's clock. `ResMut<Time>` is a load error. The frame number is not `Time`'s; a system that counts frames keeps a `Local`.

### 5. `Local` lives in Rust under the identity
- A `HashMap<identity, (layout, bytes)>` is handed to JS as a one-row view.
- A changed layout migrates by field name, as components do. A renamed export starts from defaults. `restart: true` clears the map.

### 6. Messages: a Rust arena per type, read by cursor
- `Message(name, fields)` registers a layout. `app.addMessage(M)` creates a double buffer, swapped in `First` on Bevy's two-frame rule.
- `MessageWriter.write`/`next()` append through a command op, so a message is visible to later systems in the same stage, the order commands already give.
- A reader's run gets the bytes of messages newer than its cursor (identity-keyed) as one zero-copy buffer per buffer half. The cursor advances when the run returns.

### 7. Observers run on a queue
- Rust keeps a queue of `(observer, event bytes, target)`.
- A TS `commands.trigger` op pushes to it when commands apply, computing the propagation path (`ChildOf` ancestors) right then.
- Bevy lifecycle observers registered for `On<Add|Insert|Remove<C>>` push when Bevy fires them.
- The queue drains right after each TS system's commands apply, and again at the start of every stage. A trigger from a TS system therefore runs after that system and before the next; a trigger from Rust runs at the next stage boundary in the same frame.
- Each event is one JS call. `propagate(false)` stops the walk.

### 8. States in Rust, transitions in one exclusive system
- `States(name, values)` registers a state machine resource in Rust, with values as `u32`. `app.initState(S.V)` sets it.
- `NextState.set` is a command op.
- A bevy-ts exclusive system in `StateTransition` applies the pending value, then runs `OnExit(old)` and `OnEnter(new)` TS systems as pseudo-stages.
- `inState`, `resourceExists`, `onMessage` and `not` are compiled to a small Rust condition tree, evaluated before deciding to call JS.

### 9. Ordering: registration order, then `after`/`before`/sets
- `addSystems(schedule, ...items)` flattens `systems()`/`chain()` groups into per-system configs: after, before, sets and conditions, with conditions ANDed down the group.
- Sets expand to their members. The bridge's ordering pass (a stable topological sort by registration index) handles the edges. A cycle fails the load.

### 10. Plugins run during load; the setup closure runs at the end of load
- `app.addPlugins(fn)` calls `fn(app)` at once, while the module evaluates. Every generation re-evaluates its modules, so plugins re-run.
- A `system(setup)` runs `setup` when the generation's evaluation ends. Its declaring calls (`query`, `res`, `resMut`, `commands`, `local`) produce the same param descriptors, and `setup` returns the per-run body.
- A declaring call outside a setup throws with its location.

### 11. `bevy-ts transform` serves the tool plugins
- The `bevy-ts` binary gains `transform <file>`, which prints the compiled module, declarations included.
- The Vite, vitest and esbuild plugins in `packages/bevy-ts` run it through the platform binary already installed. There is one reader, in Rust.
- `__bevyts` is installed by the engine. Outside it, `bevy-ts/testing` provides an in-process stand-in that records declarations, so a test can call a system with hand-built params.

### 12. Compatibility kept
- The plugin API is unchanged: sections, entries, command kinds, lookup and encoding, reflection, restart.
- The tsx runtime's contract stays:
  - component classes carry static `componentName` and `source`, Rust ones `native`, scene components `fieldNames` and `sceneProps`, all at definition;
  - `Commands.mount`/`unmount`/`custom` stay;
  - `commands.spawn(element)` routes to `mount`.
- The manifest's `systems` entries gain `params`, `schedule`, `conditions` and `id`. `read`/`write` remain as the union, so the protocol and the dev read-column guard keep working.

### 13. Data classes declare themselves in a static block (rulings 2b02a46, 7977c08, 8681dfd)
- Components, resources, messages, events, states and system sets are plain classes: `class Health extends Component { hp: f32 = 100 }`. The reader reads each class whose base is imported from `bevy-ts`: typed fields and literal defaults, `declare` fields (no default, zero), `static path`, `static propagate`, and a scene component's `static scene(props)`, whose props come from its props type and their defaults from the destructuring. A field type is `f32`..`u32`, `bool`/`boolean`, `Vec2`/`Vec3`/`Vec4`/`Quat`, `Entity`, a string-literal union, or an alias of one; a prop may also be `string`.
- The name is the class's `static path`, else the module's path under `src/` with `/` as `::` and the extension dropped, then the export: `src/units/tank.ts` exporting `Tank` is `units::tank::Tank`. An exported class also carries `module#export` as its id.
- The compile step gives each such class a last member `static { __bevyts.data(this, {...}) }`, built with no source spans, so the source map is unchanged. The class therefore has `componentName`, `fieldNames` and `sceneProps` from the moment it is defined; tsx reads them at module load, before commit.
- Defaults come from the class's own initialisers at run time (`new C()`), so any expression works; the literal the reader read is what the manifest carries for levels and the inspector when the initialiser is not a literal (`hasDefault`).
- An error (a `string` field, a field and prop of one name, an untyped field) is carried in the declaration and thrown when the class is defined, failing the load at the field's line.
- An entity event's target is its `entity` field: it is carried beside the payload, not in its layout, and `On<E>` gives it as `entity` next to the event's fields.
- *Beat: appending `__bevyts.data(...)` after the module, as systems are.* Code in the module that reads `componentName` after the class (a TSX scene, `OnEnter(Game.Playing)`) would run before it.
- *Beat: a runtime registry keyed by class name.* Two modules may each declare a `Score`; the module path is what tells them apart.

### 14. Scene components are found by name
- The manifest's component declarations gain `id` and `scene` (`props`: name, type, literal default or `hasDefault`, `optional`). Every scene component a generation defines is in the manifest whether or not a system names it, so a level can place it.
- The plugin API adds `component_decl(name or id)` and `scene_components()`. The runtime keeps the generation's scene component classes by name and id (`sceneComponent(name)`), which tsx's expand entry calls; the entry itself is tsx's (levels decision 5).

### 15. `ScriptHidden` keeps editor entities out of TS
- `bevy_ts::ScriptHidden` is a marker component. Every TS query is built with `Without<ScriptHidden>`, TS lifecycle observers skip marked entities, and `bevy-ts/entities` and its watch leave them out. Viewport marks its editor camera and gizmos with it.

## Risks / Trade-offs

- [A name in an alias resolved in the alias's module can differ from a same-named binding at the use site] → That is the right scoping (it is what TS itself does); generic arguments resolve at the use site.
- [Row lists for `Changed` cost a tick scan per filter column per run] → It is the scan Bevy's `Changed<T>` does. Tables are not skipped by summary tick: a Rust `Mut` write does not update it, so a skip would miss changes.
- [One JS call per observer event] → Observers are rare by design; batching per observer is a later optimisation with no API change.
- [Messages through the command arena add a copy] → It is one memcpy into the Rust arena, the same cost as any spawn value.

## Migration Plan

Remove `app.system`/`app.startup`/`component()` from the runtime and types in the same commits that move tests, `examples/cube`, `walk.mjs` and `bob_bench` to the new surface. tsx moves its types and tests after this lands.
