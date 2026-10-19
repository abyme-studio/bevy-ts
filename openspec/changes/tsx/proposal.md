## Why

The README promises "TSX describes the world; Bevy runs it", and `bridge` gives TypeScript components, systems, a command buffer and hot reload, but nothing that turns a description of a scene or a HUD into entities and keeps them in step with the source. Without it every scene is a startup full of `spawn` calls that a reload cannot touch, and abyme's future scene editor has nothing to map a selected entity back to. This change specs the TSX layer on top of `bridge`, so scenes, prefabs and UI are written once as TSX and edited live.

## What Changes

- A custom JSX runtime: `jsxImportSource: "bevy-ts"`, no React. Elements are entities (`<entity>`), components on them (`<Health initialHp={100} max={100} />`), prefabs (plain TS functions returning elements) and bevy_ui intrinsics (`<node>`, `<text>`, `<image>`, `<button>`). Dev builds compile with `jsxDEV`, so every element carries its file, line and column.
- Scenes are declared per generation (`app.scene(name, () => <Game />)`); evaluating them yields a plain-data entity description, never a live JS tree.
- A keyed, incremental reconciler in Rust that owns the retained tree, diffs each new description against the last applied one and writes the world only where the description changed: on mount, on reload, and on reactive changes, never per frame.
- `initial*` props seed a field once, when the component is inserted; plain props are controlled and are written whenever their declared value changes. Simulation-owned state (values systems wrote, entities systems despawned, components systems added) survives reloads.
- Fine-grained reactivity in the Solid mould: components run once, a function-valued prop or child is a binding, a read like `player.get(Health)` inside it subscribes to Bevy change detection in Rust (no hooks), and only the bound field of the bound node is patched. Quiet frames cost zero calls into JS.
- UI over bevy_ui 0.20: style props map to `Node`, text to `Text`, events (`onClick`, `onHover`) come from `bevy_picking` observers batched into one JS call per frame that has events.
- Relation to Bevy 0.20 BSN: the Rust side builds and caches BSN templates for static subtrees and applies changes as BSN-style patches where Bevy's API fits; `.bsn` file export is room left, not built.
- Hooks for a future visual editor: every reconciled entity carries its key path and source chain; the protocol gains `bevy-ts/tsx_*` methods to map an entity to its TSX element (and back) and to read each prop's provenance. The editor itself is out of scope.
- First demo: the Enemy prefab and a HUD bound to the player's `Health`, edited live.
- Out of scope: the editor and source rewriting, a Solid-style JSX compiler, `.bsn` files, TS-defined resources and events, query-driven lists (`<For each={query(...)}>`), animation and transitions, a widget library beyond the four intrinsics.

## Capabilities

### New Capabilities
- `jsx-runtime`: the `bevy-ts/jsx-runtime` module, element kinds, prefabs, keys, `initial*` typing, source capture, and the entity description format.
- `scene-reconciler`: scene declarations, the Rust-owned retained tree, keyed diffing, component ownership, controlled vs initial props, batched application, and use of BSN templates.
- `tsx-hot-reload`: what a reload does to reconciled entities, and what simulation-owned state it preserves.
- `reactive-ui`: signals, bindings that read components and resources without hooks, control flow, bevy_ui intrinsics and events.
- `scene-editor-hooks`: source locations, provenance, and the protocol methods a scene editor needs.

### Modified Capabilities

None. `bridge`'s capabilities (`script-runtime`, `ts-components`, `ts-systems`, `hot-reload`, `remote-protocol`) are not archived yet; what this change needs from them is written as requirements on `bridge` in design.md, "Depends on bridge".

## Impact

- New crate `bevy-ts-tsx` (the Rust reconciler, UI intrinsics, picking event batching, protocol methods), depending on `bevy-ts` and `bevy-ts-remote`. New modules in the npm package: `bevy-ts/jsx-runtime`, `bevy-ts/jsx-dev-runtime`, `bevy-ts/ui`.
- No new dependencies: the reactive core is written in the `bevy-ts` module, because `bridge`'s loader rejects bare npm specifiers. Bevy 0.20 features used: `bevy_ui`, `bevy_picking`, `bevy_scene` (BSN).
- `protocol/bevy-ts.openrpc.json` gains `bevy-ts/tsx_*` methods: a minor protocol bump.
- Requires additions to `bridge` (generation manifest sections, host-invoked JS entries, plugin command kinds, TSX compilation); listed in design.md.
