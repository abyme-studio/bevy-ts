## ADDED Requirements

### Requirement: Components run once; functions are bindings
Prefab and UI component functions SHALL run once per mount and once per reload. A prop or child whose value is a function SHALL be a binding: bevy-ts SHALL run it in a tracking scope, write its result on mount, and re-run it when a signal it read changes, patching only that field of that node. `bevy-ts` SHALL export `signal`, `computed`, `effect`, `untrack`, `onCleanup` and `ref`, implemented inside the `bevy-ts` module.

#### Scenario: One text node updates
- **WHEN** a HUD with ten text nodes binds one of them to `() => count()` and `count` changes
- **THEN** exactly one `Text` component is written and the other nine change ticks do not move

### Requirement: useComponent subscribes to change detection in Rust
`useComponent(target, C)`, where `target` is a node ref or an `Entity` and `C` is a component with a native layout, SHALL return an accessor for `C`'s current value and SHALL register the pair with Rust. After the bevy-ts stages each frame, Rust SHALL check the change ticks of subscribed pairs without calling JS. Only when one moved SHALL it deliver the new values to JS, in one host call per frame for all subscriptions and events. A subscription SHALL be dropped when its owner is disposed.

#### Scenario: Enemies take damage, the player does not
- **WHEN** the HUD subscribes to the player's `Health` and a system changes only enemies' `Health` for 60 frames
- **THEN** no host call into JS is made in those frames and the HUD is not written

#### Scenario: The player takes damage
- **WHEN** a system lowers the player's `hp`
- **THEN** in the same frame one host call delivers it and the HUD text shows the new value

### Requirement: Reading an accessor outside a tracking scope warns in dev
Under `bevy-ts dev`, reading a signal or `useComponent` accessor directly in a component body or a prop value (not inside a function binding, `computed` or `effect`) SHALL log a warning once per location, because the value will never update.

#### Scenario: A forgotten arrow
- **WHEN** a HUD writes `<text>{health().hp}</text>`
- **THEN** a warning names `hud.tsx` and the line, and the text shows the value at mount

### Requirement: Control flow components
`bevy-ts` SHALL export `<Show when fallback>` and `<For each key>`. When their inputs change, they SHALL mount or unmount only the affected subtree through the reconciler, keyed by `key` for `<For>` items, and dispose the owners of unmounted subtrees.

#### Scenario: A list item removed
- **WHEN** a `<For each={items} key={(i) => i.id}>` loses the middle item
- **THEN** only that item's entities are despawned and the other items keep their entity ids

### Requirement: UI intrinsics over bevy_ui
`<node>` SHALL spawn an entity with `Node`; its `style` prop SHALL map to `Node`'s fields (lengths through `px`, `percent`, `vw`, `vh`, `em`, `rem` helpers) and `background`, `border`, `radius` to `BackgroundColor`, `BorderColor` and `BorderRadius`. `<text>` SHALL spawn `Text` with `size` and `color` mapping to `TextFont` and `TextColor`; string and bound children SHALL be its content. `<image src>` SHALL spawn `ImageNode` with the asset loaded. `<button>` SHALL be a `<node>` with `Button` and picking enabled. UI entities SHALL be reconciled by the same reconciler as scene entities.

#### Scenario: A styled panel
- **WHEN** `<node style={{ padding: px(8) }} background="#0008">` mounts
- **THEN** the entity's `Node.padding` is 8 px on every side and its `BackgroundColor` is black at 50% alpha

### Requirement: Events from bevy_picking, batched
`onClick`, `onPress`, `onRelease`, `onOver` and `onOut` on any element SHALL register handler ids with Rust, which SHALL observe the matching `Pointer<...>` events on that entity and queue them. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (entity, pointer position, button) and a `ctx` with `commands` and `frame`. Events SHALL NOT bubble in this change.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order
