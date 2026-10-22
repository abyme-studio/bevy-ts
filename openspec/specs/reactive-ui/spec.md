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
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on any `<entity>` or UI element SHALL register handler ids with Rust, which SHALL observe the matching Bevy 0.20 picking event on that entity (`PointerClick`, `PointerPress`, `PointerRelease`, `PointerOver`, `PointerOut`) and queue it; each prop is an entity-local observer, as `on(...)` is in BSN (operator ruling 2026-10-20). The prop name is `on` plus the Bevy event's name, so the remaining picking events (`PointerDragStart`, `PointerScroll`, ...) follow the same rule when added. Events SHALL bubble up `ChildOf` as Bevy's do: a handler SHALL also run for an event whose original target is a descendant of its entity, including a mesh of a scene spawned under a `WorldAssetRoot`. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (`entity`, the entity whose handler runs; `target`, the entity the pointer hit; pointer position; button) and a `ctx` with `commands` and `frame`. A handler SHALL NOT be able to stop propagation: it runs after Bevy's walk.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order

#### Scenario: A click bubbles to its parent's handler
- **WHEN** an entity with `onPointerClick` has a child entity, and the child is clicked
- **THEN** the parent's handler runs with `entity` the parent and `target` the child

#### Scenario: A click on a glTF scene's mesh
- **WHEN** an entity carries `WorldAssetRoot("models/Fox.glb#Scene0")` and `onPointerClick`, and a mesh of the spawned fox is clicked
- **THEN** the root's handler runs once, with `entity` the root and `target` the mesh's entity

### Requirement: An input intrinsic over Bevy's TextInput
`<input>` SHALL be a `<node>` with Bevy 0.20's `TextInput` (which requires `EditableText`), as `<button>` is a `<node>` with `Button`. It SHALL take the node props (`style`, `background`, `border`, `radius`), `size` and `color` (mapping to `TextFont` and `TextColor` as on `<text>`), and:
- `initialValue`: the text at mount, set through `EditableText::new` or the editor in Rust, never through reflection (`EditableText` has no `Reflect`). Like every `initial*` prop, a reload SHALL NOT reset it.
- `maxCharacters`: `EditableText::max_characters`; edits past it are ignored, as Bevy does.
- `autoFocus`: when true, the entity gets `AutoFocus`, so `InputFocus` moves to it when it is added.

The field SHALL be one line, Bevy's default (`allow_newlines: false`, one visible line). Focus, typing, selection, paste and Escape SHALL be Bevy's own `TextInput` behaviour; bevy-ts SHALL NOT capture keys for the field itself.

#### Scenario: A focused field with a starting value
- **WHEN** `<input initialValue="hi" autoFocus />` mounts
- **THEN** the entity has `Node`, `TextInput` and `EditableText` whose value is `"hi"`, and `InputFocus` holds it

#### Scenario: A reload keeps what was typed
- **WHEN** the operator types `"abc"` into a field mounted with `initialValue=""` and the file reloads
- **THEN** the field's entity id and its value `"abc"` are kept

#### Scenario: Too many characters
- **WHEN** `<input maxCharacters={3} autoFocus />` mounts and `a`, `b`, `c`, `d` are typed
- **THEN** its value is `"abc"`

### Requirement: Events from an input, batched
`<input>` SHALL take `onTextEditChange`, `onKeyboardInput`, `onSubmit` and `onCancel`. Each SHALL be an entity-local observer on the field, as the picking props are:
- `onTextEditChange` of `TextEditChange`, named `on` plus the Bevy event; the handler gets `{ entity, value }`. It fires when Bevy fires it, which includes cursor moves with no change to `value`.
- `onKeyboardInput` of `FocusedInput<KeyboardInput>` whose original target is the field, named `on` plus the Bevy event; the handler gets `{ entity, key, code, text, state, repeat, value }`: `key` the logical key as a W3C `KeyboardEvent.key` string (`"Enter"`, `"Escape"`, `"a"`), `code` the `KeyCode` as a W3C `KeyboardEvent.code` string (`"KeyA"`), `text` the key's text or `null`, `state` `"down"` or `"up"`.
- `onSubmit` from the same observer, on `Enter` down that is not a repeat; the handler gets `{ entity, value }`.
- `onCancel` from the same observer, on `Escape` down that is not a repeat; the handler gets `{ entity }`.

`onSubmit` and `onCancel` are the one place bevy-ts names an event Bevy lacks: Bevy 0.20 has no submit, and its own widgets that need one (feathers' number input, `ValueChange { is_final: true }` on Enter) build it the same way, from a keyboard observer on the field.

`value` SHALL be the field's text with this key and every earlier key applied: Bevy's global `TextInput` observer runs first and queues this key's edit, and bevy-ts applies the field's pending edits before reading `EditableText::value()` in Rust. It is sent as a string. Events SHALL be queued and delivered with picking events in the frame's one host call, in arrival order, each with a `ctx` holding `commands` and `frame`; for one key, `onKeyboardInput` comes before `onSubmit` or `onCancel`. Escape SHALL keep Bevy's behaviour: the field loses focus, and the handlers still run.

#### Scenario: Type and submit
- **WHEN** a focused `<input onKeyboardInput={k} onSubmit={s} />` in viewport mode receives key down events `h`, `i` and `Enter` through `bevy-ts/viewport_input`
- **THEN** `k` runs with `value` `"h"`, `"hi"` and `"hi"`, and `s` runs once with `value` `"hi"`

#### Scenario: Three keys in one frame
- **WHEN** `h`, `i` and `Enter` reach the field in the same frame
- **THEN** `onSubmit` still gets `value` `"hi"`

#### Scenario: Escape
- **WHEN** a focused `<input onCancel={c} onSubmit={s} />` receives `Escape`
- **THEN** `c` runs once, `s` does not, and `InputFocus` is empty afterwards

#### Scenario: A held Enter
- **WHEN** `Enter` is held and the viewer sends repeats
- **THEN** `onSubmit` runs once, and `onKeyboardInput` runs for each repeat with `repeat` true

#### Scenario: A change event
- **WHEN** `a` is typed into a focused `<input onTextEditChange={h} />`
- **THEN** `h` runs with `value` `"a"` in the next frame's call
