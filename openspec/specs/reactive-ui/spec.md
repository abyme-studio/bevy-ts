# reactive-ui Specification

## Purpose
TSX components run once, and a function-valued prop or child is a binding. Reads inside a binding subscribe to Bevy change detection in Rust, with no hooks, so only the bound field is patched and quiet frames cost no calls into JS. The four UI intrinsics map onto bevy_ui, and `bevy_picking` events reach JS in one batched call per frame.

## Requirements

### Requirement: Components run once; functions are bindings
Prefab and UI component functions SHALL run once per mount and once per reload. A prop or child whose value is a function SHALL be a binding: bevy-ts SHALL run it in a tracking scope, write its result on mount, and re-run it when a signal it read changes, patching only that field of that node. `bevy-ts` SHALL export `signal`, `computed`, `effect`, `untrack`, `onCleanup` and `ref`, implemented inside the `bevy-ts` module.

#### Scenario: One text node updates
- **WHEN** a HUD with ten text nodes binds one of them to `() => count()` and `count` changes
- **THEN** exactly one `Text` component is written and the other nine change ticks do not move

### Requirement: TSX has no hooks
`bevy-ts` SHALL export no `use*` functions (operator ruling 2026-10-20). A binding SHALL read game state directly: `target.get(C)` for component `C` of the entity behind node ref `target` (`ref(entity)` wraps an `Entity`), and `res(R)` for resource `R`. `signal()` SHALL remain for UI-local state that is not game state, such as a dropdown's open flag or a text field's draft.

#### Scenario: A HUD without a hook
- **WHEN** a HUD writes `<text>{() => `HP ${player.get(Health).hp}`}</text>`
- **THEN** it shows the player's `hp` on mount and updates when it changes, with no hook call in the component body

### Requirement: Reads inside bindings subscribe to change detection in Rust
A `target.get(C)` or `res(R)` read inside a binding SHALL register the pair (entity or resource entity, `ComponentId`) with Rust for that binding. After the bevy-ts stages each frame, Rust SHALL check the change ticks of subscribed pairs without calling JS. Only when one moved SHALL it deliver the new values to JS, in one host call per frame for all subscriptions and events, and re-run the bindings that read them. A subscription SHALL be dropped when its binding's owner is disposed.

#### Scenario: Enemies take damage, the player does not
- **WHEN** the HUD subscribes to the player's `Health` and a system changes only enemies' `Health` for 60 frames
- **THEN** no host call into JS is made in those frames and the HUD is not written

#### Scenario: The player takes damage
- **WHEN** a system lowers the player's `hp`
- **THEN** in the same frame one host call delivers it and the HUD text shows the new value

### Requirement: Bindings on native-layout fields are derived fields with one owner
A binding on a field of a component with a native layout SHALL be grouped by site (the element and prop in source) together with every instance of that element. Each frame Rust SHALL find the instances whose reads changed without calling JS, and SHALL deliver them in the per-frame host call. JS SHALL evaluate the site's closure once per changed instance through the row accessors and SHALL write the field through the column view with the run's tick only when the value differs from the row's. The reconciler SHALL NOT write a bound field from a literal. A site's reads SHALL be fixed at its first evaluation; under `bevy-ts dev`, a later run that reads something new SHALL be an error naming the site. A binding on a reflected component SHALL follow the per-pair path above instead.

#### Scenario: Ten thousand bound lights, one hundred alarms change
- **WHEN** a prefab binds `PointLight.intensity` to its own `Alarm`, is instanced 10,000 times, and a system changes 100 alarms in one frame
- **THEN** one host call runs the site's closure 100 times, 100 `PointLight` rows are written, and the frame costs within 2× of a TS system doing the same

#### Scenario: A quiet frame
- **WHEN** no instance's reads changed in a frame
- **THEN** no host call is made and no `PointLight` change tick moves

#### Scenario: Same value
- **WHEN** an alarm changes but the binding evaluates to the value already in the row
- **THEN** the row is not written and its change tick does not move

### Requirement: Reading an accessor outside a tracking scope warns in dev
Under `bevy-ts dev`, reading a signal, `target.get(C)` or `res(R)` directly in a component body or a prop value (not inside a function binding, `computed` or `effect`) SHALL log a warning once per location, because the value will never update.

#### Scenario: A forgotten arrow
- **WHEN** a HUD writes `<text>{player.get(Health).hp}</text>`
- **THEN** a warning names `hud.tsx` and the line, and the text shows the value at mount

### Requirement: Control flow components
`bevy-ts` SHALL export `<Show when fallback>` and `<For each key>`. When their inputs change, they SHALL mount or unmount only the affected subtree through the reconciler, keyed by `key` for `<For>` items, and dispose the owners of unmounted subtrees.

#### Scenario: A list item removed
- **WHEN** a `<For each={items} key={(i) => i.id}>` loses the middle item
- **THEN** only that item's entities are despawned and the other items keep their entity ids

#### Scenario: Bound children of a mounted subtree
- **WHEN** `<Show when={() => s() !== null}><text>{() => `show ${s()}`}</text></Show>` mounts with `s` already set, or a `<Show>` or `<For>` mounts its content later (a signal set by a system or a handler)
- **THEN** each bound child shows its value in the frame its subtree mounts, and follows its signals afterwards

### Requirement: UI intrinsics over bevy_ui
`<node>` SHALL spawn an entity with `Node`; its `style` prop SHALL map to `Node`'s fields (lengths through `px`, `percent`, `vw`, `vh`, `em`, `rem` helpers) and `background`, `border`, `radius` to `BackgroundColor`, `BorderColor` and `BorderRadius`. `<text>` SHALL spawn `Text` with `size` and `color` mapping to `TextFont` and `TextColor`; string and bound children SHALL be its content. `<image src>` SHALL spawn `ImageNode` with the asset loaded. `<button>` SHALL be a `<node>` with `Button` and picking enabled. UI entities SHALL be reconciled by the same reconciler as scene entities.

#### Scenario: A styled panel
- **WHEN** `<node style={{ padding: px(8) }} background="#0008">` mounts
- **THEN** the entity's `Node.padding` is 8 px on every side and its `BackgroundColor` is black at 50% alpha

### Requirement: Events from bevy_picking, batched
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on any `<entity>` or UI element SHALL register handler ids with Rust, which SHALL observe the matching Bevy 0.20 picking event on that entity (`PointerClick`, `PointerPress`, `PointerRelease`, `PointerOver`, `PointerOut`) and queue it; each prop is an entity-local observer, as `on(...)` is in BSN (operator ruling 2026-10-20). The prop name is `on` plus the Bevy event's name, so the remaining picking events (`PointerDragStart`, `PointerScroll`, ...) follow the same rule when added. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (entity, pointer position, button) and a `ctx` with `commands` and `frame`. Events SHALL NOT bubble in this change.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order
