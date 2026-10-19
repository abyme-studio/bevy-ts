## Context

`bridge` gives bevy-ts TS components with native layouts, TS systems over zero-copy columns, a binary command buffer, startups, generations and hot reload, and the `bevy-ts/` protocol. It leaves TSX, the reconciler, BSN and UI to this change, with `spawnNative` and the entity stream as the room it kept. The README's open question is the one this design answers first: does TSX compile to static scene data, run in the runtime, or both.

Two bridge rules shape everything below:

- **JS module state dies on every reload** (bridge hot-reload rule 2). A reconciler whose retained tree lives in JS, as in React, react-three-fiber and bevy-react, loses its tree on every save.
- **Generations apply whole, at a frame boundary, or not at all** (rules 8 and 9). A scene that fails to evaluate must fail its generation, not half-mount.

Prior art, read 2026-10-19:

- **bevy-react 0.8.1** (2026-10-14, Bevy 0.20). A custom React reconciler on a deno_core thread; each React commit becomes one JSON batch of create/update/append/remove/set-text ops, decoded in one `op_flush`. It diffs props (style field by field), so identical values emit nothing. Keys never reach Bevy. Event handlers stay in JS and only a flag crosses. Hot reload uses Fast Refresh: hook state survives when hooks are unchanged, effects re-run, module state is lost unless kept on `globalThis`. Its numbers: 1 row update about 3 ms, 1k-row create 36 ms, a 2-row swap at 10k rows 383 ms, almost all JS. Taken: one batch per commit, diff before emitting, handlers stay in JS. Not taken: the JS-side tree, React's re-render model, JSON for value patches.
- **react-three-fiber 9.8.** `react-reconciler` 0.34 host config; `commitUpdate` diffs props itself (`diffProps`) and `applyProps` mutates the live object, rebuilding it only when constructor `args` change. Its own pitfalls page says fast updates go through mutation in `useFrame`, never `setState` at 60 Hz. Taken: "constructor args change means rebuild, other props patch in place" becomes "element type change respawns, prop change patches". Taken as a rule: per-frame motion belongs to systems, not to the reconciler.
- **Solid 1.9.** Components run once; `createSignal`/`createEffect`/`createMemo` track reads; the compiler (dom-expressions) clones static structure and wraps only dynamic expressions in effects, so only bound nodes update. `solid-js/universal`'s `createRenderer` targets non-DOM hosts but needs Solid's Babel preset. **bevy_superui 0.4** does the same with its own oxc pass lowering JSX to a `$ss` ABI. Taken: run-once components and fine-grained bindings. Not taken: a compiler pass (see decision 3).
- **Triplex** (trytriplex/triplex). A Babel plugin rewrites compiled `jsx`/`jsxs` calls to inject `__meta` (`path`, `line`, `column`, `astPath`, where `astPath` is element names from the export down with `.N` for repeated siblings) and wraps host elements so a click in the viewport finds its element. Edits go through `ts-morph` on the server (`getJsxElementAt` by line and column) and `saveSync`; undo is a stack of whole-file snapshots. Taken: source location per element, lookup by line and column, an element-path fallback, and the rule that the editor, not the runtime, parses and rewrites source.
- **Bevy 0.20 BSN** (`bevy_scene` 0.20, BSN introduced in 0.19). `bsn!` builds a `Scene` and `bsn_list!` a `SceneList`. A BSN expression is a patch that writes only the fields it names, layered in order. Templates may read the world and the spawn context, for example to load assets. `#Name` names an entity, `:"file.bsn"` inherits a cached scene, `on(...)` attaches observers, and 0.20 adds a bottom-up `Ready` event. No `.bsn` loader ships yet (0.20 "What's Next"). Nothing retains or reconciles: BSN spawns, it does not update.
- **bevy_ui 0.20.** `Node`, `Text`, `ImageNode`, `BackgroundColor`, `em()`/`rem()` units, feathers widgets; interaction through `bevy_picking` (`Pointer<Click>`, `Pointer<Over>`, `Pointer<Out>`) and observers.
- **JSX source info.** React 19.2 stopped using `__source`/`__self`, but the automatic dev transform (`jsxDEV`) still carries `{fileName, lineNumber, columnNumber}` in TypeScript's `react-jsxdev`, and oxc 0.154's transformer has `development` (the `jsx_source` plugin) and `importSource`. Checked 2026-10-20 (task 1.2): oxc 0.154 passes source as `jsxDEV`'s fifth argument, with `fileName` the absolute path bridge compiled and line and column in TS source coordinates; Rust makes the file project-relative. The runtime also reads a `__source` prop.

## Goals / Non-Goals

**Goals:**
- Scenes, prefabs and UI written as TSX with no React, typed from `component()` declarations.
- Writes to the world only on mount, reload and reactive change; zero JS calls in a quiet frame.
- A reload changes what the source changed and nothing else: simulation state survives, `initial*` values are never re-applied.
- UI where `useComponent(player, Health)` patches one text node when `Health` changes and nothing otherwise.
- Every reconciled entity maps back to a TSX element, so an editor can select it and write a prop edit back.

**Non-Goals:**
- The editor, source rewriting, undo. Only what the reconciler must expose.
- A JSX compiler pass beyond oxc's standard automatic transform.
- `.bsn` file export or import, BSN inheritance from files.
- Query-driven reactive lists (`useQuery`), TS-defined resources and events, animation, a widget kit beyond `node`, `text`, `image`, `button`.
- Per-frame motion through TSX. Systems move things; TSX describes them.

## Decisions

### 1. Hybrid: TSX runs in the runtime and produces scene data; Rust reconciles the data
A scene is a TS function. bevy-ts runs it inside the generation's load (and later in reactive scopes), and the JSX runtime returns a plain-data description: element type, key, props, children, source. Functions in props are replaced by binding and handler ids. Rust receives the description and owns everything after that.

- *Beat: compile TSX to static scene data at build time (a `.bsn`-like artefact).* Prefabs are functions with parameters, loops and conditionals; a static extractor either rejects them or becomes a second evaluator with its own semantics. Hot reload would also need two paths, one for static files and one for code.
- *Beat: run TSX dynamically with a JS-side retained tree (bevy-react, react-three-fiber).* The tree dies with module state on every reload (bridge rule 2), so every save would remount or need a snapshot round-trip. The editor would also have to ask JS for the tree, and JS has no access to the world.
- *What "hybrid" buys:* full TypeScript for authoring, and data (which Rust can diff, cache as BSN, show over the protocol, and later export to `.bsn`) as the output.

### 2. The reconciler lives in Rust and diffs description against description
The retained tree is a Rust resource plus a `TsxNode` component on each reconciled entity. On every reconcile Rust compares the new description with the last applied one, never with the world, and writes only what changed. Commands are applied directly by `bevy-ts-tsx` with `&mut World`, batched per reconcile.

- *Beat: a JS reconciler that emits bridge commands.* It needs the retained tree in JS, which reload kills (decision 1).
- *Beat: diff against the world.* Every reload would reset whatever a system moved, healed or despawned, which is exactly the simulation state a reload must keep.

### 3. Reactivity is explicit accessors over the standard JSX transform
Components and prefabs run once (Solid). A prop or child whose value is a function is a binding: `hp={() => base() * 2}`, `<text>{() => `HP ${health().hp}`}</text>`. The reactive core (`signal`, `computed`, `effect`, owners and cleanup) is a small module inside `bevy-ts`. Bindings re-run when what they read changes and queue one patch for one field of one node (a binding on a native-layout field runs per site instead, decision 11).

- *Beat: React-style re-render with a VDOM diff (bevy-react).* Every state change re-runs whole components and diffs subtrees in JS; bevy-react's own table shows that cost growing with tree size. Hook state would also need Fast Refresh machinery to survive reload.
- *Beat: Solid's compiler (automatic wrapping of `{expr}`).* It reads better, but needs a Babel or custom oxc pass in bridge's compile step, changes what `tsc` sees, and is a second compiler to keep in step with oxc. Explicit `() =>` costs four characters and keeps TSX standard. Revisit once the API settles.

### 4. Elements: entities, component elements, prefabs, UI intrinsics
```tsx
<entity name="enemy">
  <Transform translation={at} />
  <Health initialHp={hp} max={hp} />
  <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
</entity>
```
- `<entity>` is an entity; its children are component elements and child entities. This mirrors BSN, where one entity's patch holds both.
- A component element's tag is the component handle from `component()` (or a Rust component bevy-ts types, such as `Transform`). Its props are fields.
- A prefab is a TS function from props to one element. `<Enemy at={[3, 0, 0]} />` calls it.
- Asset intrinsics (`<mesh>`, `<material>`) are built by `bevy-ts-tsx` in Rust as BSN templates that load assets, replacing `spawnNative` for TSX.
- UI intrinsics `<node>`, `<text>`, `<image>`, `<button>` are entities with `Node` and friends (see the reactive-ui spec).

- *Beat: components as props on `<entity>` (`<entity health={{ hp: 100 }} />`).* Field-level `initial*`, source location per component and per-component removal all get harder, and the object literal hides which component a prop belongs to.
- *Beat: every element an entity with an implicit single component (bevy-react's `<node>`).* It works for UI, but a game entity has many components.

### 5. `initial*` seeds once; plain props are controlled
For each field `f` of a component, `<C f={v} />` is controlled and `<C initialF={v} />` is initial. TS types come from the `component()` declaration with template-literal keys; giving both for one field is a type error and a load error.

- Initial values are written only when the reconciler inserts the component: when the entity spawns, or when the component element is new on an existing entity. Never again, whatever reload or change happens.
- Controlled values are written when inserted and whenever the declared value differs from the last applied value (byte compare in native layout, reflect compare otherwise). Not when a system changed the field in the meantime and the declaration did not change.
- Fields given neither way take the declaration's default on insert and are never touched.

- *Beat: React-style controlled (the declared value is re-asserted every reconcile).* That writes over simulation on every reload, and with bindings it would fight systems every frame.
- *Beat: everything initial unless marked.* Then editing `size` in source does nothing until restart, which defeats live editing.

### 6. Stable keys never come from source positions
A node's identity is its key path from the scene root: each segment is the explicit `key`, or `type#n` where `n` counts preceding siblings of the same type. Prefab boundaries add no segment of their own beyond the element's own key, so renaming a prefab function keeps its entities. Line and column are metadata, never identity, so inserting a line above an element respawns nothing.

- *Beat: source location as key (Triplex's `__meta` line/column).* Any edit above an element would respawn it and lose its state.
- *Beat: plain sibling index (React's default).* Inserting a `<Light>` before an `<Enemy>` would shift the enemy's identity; type-scoped counting confines the shift to one type. Arrays from `.map` without keys warn in dev, as in React.

### 7. BSN for spawning, own patches for updates
On spawn, Rust turns a description subtree into a BSN scene and spawns it, so asset templates, `Name`, `on(...)` observers and `Ready` behave as in Bevy. Static subtrees (no bindings, no handlers) are cached by content hash, so the second `<Enemy>` resolves from the cached scene like BSN's `:"file.bsn"` inheritance. Changes after spawn are field patches applied by `bevy-ts-tsx` itself, because BSN has no update path.

- *Beat: BSN for everything.* Re-spawning to update would lose simulation state; BSN does not reconcile.
- *Beat: no BSN, raw `insert_by_id` and reflection.* It works, but loses Bevy's asset templates and diverges from where Bevy scenes are going. If BSN cannot be built from runtime data in 0.20 (unverified: its public API is macro-first), raw insertion is the fallback behind the same reconciler, decided in task 3.1.
- `.bsn` export of a description is room left: the description is already data with names and keys.

**Decided 2026-10-20 (task 3.1): direct insertion now; BSN stays open.** The spike
(`crates/bevy-ts-tsx/tests/bsn_spike.rs`) shows BSN *can* be built from runtime data in 0.20:
a struct implementing `Scene` pushes templates into a `ResolvedScene`, a TS component (only a
`ComponentId` and a byte layout) goes in as a `Clone` value whose `SceneEffect` calls
`BundleWriter::push_component_by_id`, children go through
`get_or_insert_related_resolved_scenes::<ChildOf>()`, and a resolved `ScenePatch` spawns any
number of times. 1,000 enemies of two entities each (release): BSN with the patch resolved once
0.43 ms, BSN resolved per spawn 0.64 ms, direct insertion 0.28 ms. The reconciler spawns by
direct insertion anyway, for three reasons: nothing in this change uses what BSN adds (asset
templates load by path, but `<mesh>` builds procedural meshes; observers and `Ready` arrive with
handlers, which are held); BSN needs `AssetPlugin` and `ScenePlugin` in every app that mounts a
scene, including headless tests; and a spawned BSN hierarchy has to be walked back to map each
entity to its node, where direct insertion knows each entity as it makes it. The static-subtree
cache (decision 7) is kept in that form: resolved and encoded component elements by content hash
(`TemplateCache`), and asset handles by props (`IntrinsicAssets`), so the second identical
`<Enemy>` resolves nothing and builds no asset. Switching spawns to BSN is local to
`reconcile::spawn` once `<mesh src>`, observers or `levels`' `.bsn` export want it.

### 8. Function values never cross; ids do
Bindings and handlers stay in JS and cross as ids (bevy-react's rule for handlers). Rust holds `(node, component, field) → binding id` and `(node, event) → handler id`, and calls JS only with ids and payloads. A generation rebuilds both tables, because the closures died with the old module state.

- *Beat: serialise expressions for Rust to evaluate.* That is a second language.

### 9. Change detection subscriptions are checked in Rust
`useComponent(target, Health)` registers `(entity or node, ComponentId)` with Rust. Each frame in `PostUpdate`, after the bevy-ts stages, `bevy-ts-tsx` checks those pairs' change ticks (no JS). Only if any moved, or picking events arrived, does it make one host call into JS carrying the changed values (native layout bytes) and the events; JS updates signals, effects queue patches, and the call returns one patch batch, applied before `Last`.

- *Beat: a bridge TS system with a `Changed` filter.* It wakes JS whenever any entity's `Health` changes, including every enemy taking damage, to find out that the HUD only shows the player.
- *Beat: polling from JS each frame.* One crossing per frame, forever, for UI that is usually still.
- Components with a native layout (TS components, `Transform`) are read from their columns; reflected Rust components (`Text`, `Node`) through bridge's reflected reads and change ticks (bridge decision 14).
- This per-pair check serves UI subscriptions and cross-entity reads. A binding that writes a native-layout field takes decision 11's per-site path, because per-pair checks cost 64 µs per frame at 10k subscriptions and 360 to 839 µs at 100k, quiet frames included (`docs/research/scene-bindings.md`).

### 10. The editor maps through the reconciler, and parses source itself
Each reconciled entity's `TsxNode` holds its key path, the source chain (the element that made it, then each prefab call site above it, outermost last), and per prop its provenance (`controlled`, `initial`, `bound`, `default`) and last applied value. The protocol serves these. The editor, not bevy-ts, parses the file at a location and rewrites it, choosing the frame in the chain that sits in the file it has open (Triplex's rule: you edit the call site in the file you are in). A prop is writable when the editor finds a literal at that site; bevy-ts does not judge.

- *Beat: a compile pass that marks literal props and spans.* It duplicates what the editor's parser must do anyway and couples the runtime to formatting.

### 11. A scene binding is a derived field with one owner, run per site
A binding on a component element's field, `<PointLight intensity={() => (alarm().on ? 8 : 2)} />`, is allowed on any element, scene or UI, and means: this field is a function of what the binding reads, and the binding is its only writer. A binding whose target has a native layout is not one Solid effect per instance. It is grouped by *site*, the element and prop in source, one per prefab element however many instances exist, and runs as a generated system:

- Rust finds the site's instances whose reads changed, with no JS call: the instance's own components by their change ticks in table order (what `Changed<T>` does), another entity's components through decision 9's per-pair check, a signal through its version (a change re-runs every instance of the site).
- One host call per frame for all sites with changes carries the changed rows. JS runs the site's one closure per row through the flyweight accessors a TS system uses (bridge decision 5), compares with the current value and writes through the column view with the run's tick (bridge decision 6). A derived value that did not change writes nothing, so `Changed<PointLight>` does not move.
- A site's reads are fixed at its first evaluation, as a system's query is. A later run that reads something new is a dev error naming the site.
- The reconciler never writes a bound field from a literal (provenance `bound`). A TS system whose `write` set holds the component warns in dev, naming the system and the site. A `.bsn` override of a bound field is the `levels` change's call; recommended is Godot's rule, the override pins that instance (it leaves the site) and the viewport shows a revert. The viewport shows a bound prop read-only with its expression and site location.
- A binding on a reflected component (`Text`, `Node`) keeps decision 9's path. The split is by target component kind, which the runtime can check, not by "is this UI".

Measured 2026-10-19 (`docs/research/scene-bindings.md`, `bench/bindings/`): per-instance effects cost 85 µs a frame at 10k instances with 1% changed against 10 to 18 for a system, 12.4 ms against 0.23 at 100k with 10% changed, 282 ms against 1.3 with every instance changed, plus 42 ms and 66 MB to mount 100k effects. Unreal moved UMG from polled bindings to change-driven bindings with declared sources and compare-before-write (Viewmodel); Unity's UI Toolkit bindings skip unchanged data-source versions; Bevy's own direction is reactivity through change detection with "something similar to system syntax" for reactive scenes.

- *Beat: UI only.* Enforceable, but the line is not where the problems are: a `<text>` binding and a `PointLight` binding have the same ownership question and the same cost curve, and a world-space label sits on both sides. It outlaws the most readable form of "this field follows that one" to dodge a cost cliff this decision removes.
- *Beat: Solid effects everywhere (the previous draft).* Right for UI, 50 to 200× too slow at prefab scale, a per-frame tick-check tax proportional to subscriptions, and a two-writer rule that is only a warning once the fight is on.

## Data model

**JS, per generation (dies on reload):**
- Description node: `{ type, key?, props, children, source? }`. `type` is `"entity"`, an intrinsic name, a component handle, or a prefab function. Prefab nodes are expanded at creation, so a finished description holds only entities, component elements and intrinsics, each entity carrying the source chain of the prefab calls that produced it.
- Owner tree: one owner per mounted scene root and per reactive scope (`<Show>`, `<For>` item), holding effects and cleanups; disposing an owner unsubscribes its bindings.
- Binding table: id → `{ node key path, component, field, effect }`. Handler table: id → function. Site table (decision 11): site id → `{ closure, reads, component, field }`; its instances live in Rust.
- Signals and `useComponent` accessors.

**Wire, JS to Rust:**
- *Mount and reload:* one JSON description per scene root, in the generation manifest's `tsx` section (bridge requirement B1). JSON is fine here: it runs once per reload. Values of TS components are field-name maps; Rust encodes them with the layouts it computed. Rust checks the section with the generation (`add_ts_section`) and reconciles it once the generation's components are live and migrated (`on_ts_section_applied`, which also runs, with no section, for a generation that declares no scenes). Errors the JSX runtime finds while describing (a component outside an entity, a duplicate key or component) travel in the section's `errors` with the element's location rather than as a throw, so the failed reload points at the element, not at `app.scene`.
- *Runtime mounts:* `commands.mount(el, { parent })` on a system's `Commands` param appends a `tsx/mount` op (bridge B3) carrying the same JSON for one subtree; `commands.unmount(key | entity)` a `tsx/unmount` op. A mount is checked whole before it writes; a bad one writes nothing and is logged with the run's other command warnings.
- *Runtime:* a binary tsx op stream returned from host calls (B2, B3): `patch(nodeId, componentId, fieldIndex, bytes)`, `patchReflect(nodeId, typePath, json)` for reflected components such as `Text`, `mountSubtree(parentNodeId, slot, json)`, `unmountSubtree(nodeId)`, `subscribe(subId, target, componentId)`, `unsubscribe(subId)`.

**Rust, survives reloads (it is ECS state):**
- `TsxNode` component: `node_id: u64` (hash of root name and key path), key path, element types, source chain, owned components (inserted by tsx), last applied controlled values per field, provenance per prop, binding and handler ids of the current generation.
- `TsxTree` resource: root name → root node; node id → entity; tombstones for nodes despawned by anything other than tsx; the static-subtree BSN cache by content hash; subscriptions; binding sites with their instances (write entity, read entities, signal versions); pending picking events.

## Hot-reload state rules

These apply on top of bridge's rules, which still hold (ECS state survives, JS state dies, failed generations are never partly applied).

1. A reload re-runs every `app.scene` function and reconciles each root against its last applied description.
2. Same key path, same entity-producing type: the entity stays. Its components are diffed element by element.
3. Controlled field whose declared value changed: written. Unchanged: not written, even if a system changed it.
4. `initial*` field: never written to an existing component, whatever the old and new values.
5. New component element on an existing entity: inserted with its initial, controlled and default values.
6. Component element gone from the description: removed, but only if tsx inserted it. Components systems inserted are never touched.
7. New key path: spawned. Key path gone: despawned with its descendants, including entities systems parented under it.
8. An entity despawned by something other than tsx (a system killing an enemy) leaves a tombstone: later reloads do not respawn it. `restart: true`, or changing its key, does.
9. A change of type at one key path (an `<entity>` became an `<Enemy>` that expands to different components) is handled as rule 2 on the expansion: identity follows key paths, not prefab names.
10. Bindings, handlers and subscriptions are rebuilt by the new generation; a bound value that evaluates the same as last applied writes nothing.
11. A scene function that throws, or a description Rust rejects (unknown component, both `f` and `initialF`), fails the generation (bridge rule 8): the world and the old bindings stay as they were.
12. Inspector edits (`bevy-ts/mutate_components`) are simulation state under rule 3: they survive reloads until the source changes that field's declared value.

## Depends on bridge

What tsx needs that `bridge` does not yet offer. Each is a requirement for bridge to add; tsx defines none of them itself.

- **B1. Manifest sections for feature crates.** A Rust plugin SHALL be able to register a named section of the generation manifest; JS fills it during module evaluation (`app.scene` writes `tsx`), and the plugin receives it when the generation is applied, in the same frame-boundary step as declarations. The plugin SHALL be able to reject the section with errors carrying TS locations, which fails the whole generation under bridge's failed-reload rule.
- **B2. Host-invoked JS entries.** A Rust plugin SHALL be able to call a named JS entry registered by the current generation, passing a byte payload, from an exclusive system on the main thread, and get back the bytes the entry returned plus a command buffer, with bridge's error rules (TS stack, dedup, discarded commands on throw). Zero calls SHALL be made when the plugin does not call. This is the seam's fifth operation next to load, run system, run startup and pump.
- **B3. Plugin command kinds.** A Rust plugin SHALL be able to register a command opcode with a decoder, so JS can append plugin ops to a system's `Commands` param in a system run and they apply in order with built-in commands. tsx uses it so a TS system can mount a prefab at runtime (`commands.mount(<Enemy at={p} />)`).
- **B4. TSX compilation.** bridge's oxc step SHALL compile `.tsx` with the automatic runtime and `importSource: "bevy-ts"`, resolve `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` as built-in modules, and in `bevy-ts dev` use the development transform so each element carries file, line and column in TS source coordinates.
- **B5. Layouts and ids to plugins.** Rust plugins SHALL be able to look up a TS component's `ComponentId` and layout by name, and encode a field-name map into native bytes, through a public API of `bevy-ts`.
- **B6. Despawn provenance.** bridge's `restart: true` SHALL also notify plugins, so tsx can clear tombstones and its tree before the generation is applied as if first.
- **B7. Protocol methods from feature crates.** `protocol/bevy-ts.openrpc.json` SHALL accept methods owned by other bevy-ts crates under the same `bevy-ts/` naming and versioning rules, generated by the same pipeline.

## First demo: an Enemy prefab and a HUD, edited live

The README names no Enemy or HUD yet; this is the demo this change ships as `examples/arena/`.

```tsx
// src/enemy.tsx
export const Health = component("game::Health", { hp: f32.default(100), max: f32.default(100) });
export const Enemy = ({ at, hp = 100 }: { at: Vec3; hp?: number }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <Health initialHp={hp} max={hp} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);

// src/hud.tsx
export const Hud = ({ player }: { player: NodeRef }) => {
  const health = useComponent(player, Health);
  return (
    <node style={{ position: "absolute", top: px(12), left: px(12), padding: px(8) }} background="#0008">
      <text size={20}>{() => `HP ${Math.ceil(health().hp)} / ${health().max}`}</text>
      <button onClick={(ctx) => ctx.commands.mount(<Enemy key={`e${ctx.frame}`} at={[0, 0, -6]} />)}>
        <text>Spawn</text>
      </button>
    </node>
  );
};

// src/game.tsx
const player = ref();
app.scene("arena", () => (
  <>
    <Player ref={player} />
    <Enemy key="a" at={[-3, 0, -4]} />
    <Enemy key="b" at={[3, 0, -4]} hp={250} />
    <Hud player={player} />
  </>
));
```
A `damage` system lowers `hp` of every `Health` near the player each frame; a `bob` system moves enemies.

1. `bevy-ts dev examples/arena/src/game.tsx`: two red cubes, the HUD reads `HP 100 / 100`.
2. Damage the player: only the HUD text changes; the bridge's per-system call counter shows no JS call on frames where the player's `Health` did not change.
3. Change enemy `b`'s `hp={250}` to `hp={400}` and save: `b`'s `max` becomes 400, its current `hp` stays where damage left it (`initialHp`), and neither cube jumps.
4. Change the cube colour in `Enemy` and save: both enemies recolour; their positions (moved by `bob`) are kept.
5. Let a system despawn enemy `a`, then save an unrelated edit: `a` stays dead. Reload with `restart: true`: it is back.
6. Add `<Enemy key="c" ... />` and save: one new enemy spawns; `a` and `b` are untouched.
7. Click Spawn: an enemy appears, mounted from a handler through the tsx op stream.
8. `curl bevy-ts/tsx_node` for enemy `b`: its source chain ends at `game.tsx`'s `<Enemy key="b">` line, with `hp` controlled and `initialHp` initial.

## Risks / Trade-offs

- [BSN may not be constructible from runtime data in 0.20; its API is macro-first] → Task 3.1 is a spike; the fallback is raw insertion behind the same reconciler, losing only the static-subtree cache.
- [Explicit `() =>` bindings are easy to forget: `{health().hp}` reads once and never updates] → A dev warning when an accessor is read outside a tracking scope during a scene or prefab call, naming the source location.
- [A bound field a TS system also writes has two owners] → Dev warning at load when a site's component is in some TS system's `write` set, naming the system and the site; the site's own tick (bridge decision 6) says who wrote a row last; docs: a bound field has one writer.
- [A site's reads are fixed at first evaluation, so `a() ? b() : c()` reads `b` on some instances and `c` on others] → Dev error naming the site; read both, or move the branch into the value.
- [Type-scoped sibling keys still shift within one type] → Dev warning for unkeyed siblings from arrays; docs recommend keys for anything with state.
- [Tombstones can surprise: an enemy deleted in play stays gone after a reload] → It is the point of rule 8; `bevy-ts/tsx_tree` lists tombstones and `restart` clears them.
- [The host call adds a JSON-free but still real crossing per frame with UI changes] → One call per frame at most, carrying all changes; a HUD that changes every frame costs one call per frame, which bevy-react measures at well under a millisecond for small batches.
- [Reflected Rust components (`Text`, `Node`) patch through reflection and JSON] → They change rarely and per node; native-layout components use byte patches.
- [oxc's automatic dev output shape for source] → Pinned by task 1.2: `jsxDEV`'s fifth argument. The runtime also accepts a `__source` prop.

## Migration Plan

None: new crate and modules. Protocol minor bump for the `bevy-ts/tsx_*` methods.

## Operator rulings

- 2026-10-19: reactive bindings are allowed on scene props, compiled to one generated system per
  binding site (decision 11, docs/research/scene-bindings.md). A bound prop has one owner, its
  binding; no system writes it, and the viewport shows it read-only.

- 2026-10-19: TSX defines prefabs, what a thing is. Levels (where things are) and per-instance
  overrides are `.bsn` text files, which the IDE's viewport writes, as Godot writes `.tscn` and
  Unity its scenes; Bevy's own editor plans the same. A literal prop inside a prefab is rewritten in its
  TSX only by an explicit "Apply to prefab" (`levels.prefabEdits` defaults to `override`, operator
  2026-10-19); a computed one is shown read-only. So the reconciler keeps each element's
  source location (file, line, column, key), and a `.bsn` level must be able to instance a TSX
  prefab by name and override its props. Loading and writing `.bsn` (bevy-ts ships its own loader
  until Bevy's lands) is a later change, `levels`; this change leaves room for it.

## Open Questions

- Whether `<For>` over a signal is enough for HUD lists until `useQuery` exists.
- Whether a `.bsn` export (`bevy-ts bake`) should wait for Bevy's `.bsn` loader or ship as a text writer first.
