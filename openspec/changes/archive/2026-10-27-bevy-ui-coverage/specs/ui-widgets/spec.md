## ADDED Requirements

### Requirement: One intrinsic per Bevy widget
Each `bevy_ui_widgets` widget SHALL be a UI intrinsic named as its component in lower camel case, a `<node>` plus that component, taking the node props: `<checkbox>` (`Checkbox`), `<radioGroup>` (`RadioGroup`), `<radio>` (`RadioButton`), `<slider>` (`Slider`, `SliderValue`, `SliderRange`, `SliderStep`, `SliderPrecision`), `<sliderThumb>` (`SliderThumb`), `<scrollbar>` (`Scrollbar`), `<scrollbarThumb>` (`ScrollbarThumb`, with no `Node`), `<tabList>` (`TabList`), `<tab>` (`Tab`), `<listBox>` (`ListBox`), `<listItem>` (`ListItem`), `<menuButton>` (`MenuButton`), `<menuPopup>` (`MenuPopup`), `<menuItem>` (`MenuItem`), `<popover>` (`Popover`), `<dialog>` (`Dialog`), `<modalDialogBarrier>` (`ModalDialogBarrier`) and `<dialogDragHandle>` (`DialogDragHandle`). `<node scrollArea>` SHALL add `ScrollArea`. Each widget's configuration SHALL be props named as Bevy's fields: `orientation` and `trackClick` on `<slider>`, `min`, `max`, `step` and `precision` for its range, step and precision; `target` (a node ref), `orientation` and `minThumbLength` on `<scrollbar>`; `orientation` and `activation` on `<tabList>`; `multiSelect` on `<listBox>`; `layout` on `<menuPopup>`; `positions` and `windowMargin` on `<popover>`; `modal` on `<dialog>` (adds `ModalDialog`). Focus, keyboard and pointer behaviour SHALL be Bevy's own.

#### Scenario: A slider
- **WHEN** `<slider initialValue={0.5} min={0} max={1} step={0.1}><sliderThumb /></slider>` mounts
- **THEN** the entity has `Slider`, `SliderValue(0.5)`, `SliderRange` 0 to 1 and `SliderStep(0.1)`, and its child has `SliderThumb`

#### Scenario: A scrollbar
- **WHEN** `<scrollbar target={list} orientation="vertical"><scrollbarThumb background="#888" /></scrollbar>` mounts beside a `<node ref={list} scrollArea style={{ overflow: { y: "scroll" } }}>`
- **THEN** the scrollbar's `Scrollbar.target` is the list's entity and dragging the thumb writes the list's `ScrollPosition`

### Requirement: Widget state is controlled or initial
A stateful widget's state prop SHALL have a controlled form and an `initial*` form, and giving both SHALL be a type error and a load error naming the prop: `checked` / `initialChecked` on `<checkbox>` (`Checked`), `value` / `initialValue` on `<slider>` (`SliderValue`), `checked` / `initialChecked` on `<radio>`, `selected` / `initialSelected` on `<tabList>` (`SelectedTab`, by the tab's key). A controlled prop SHALL be written from its binding and Bevy SHALL NOT change it: the widget only emits `ValueChange`. An `initial*` prop SHALL be written on mount only, and the widget SHALL get Bevy's self-update observer (`checkbox_self_update`, `slider_self_update`, `tablist_self_update`; for radios, `radio_self_update` on the group). A `<radioGroup>` whose radios mix `checked` and `initialChecked` SHALL fail the load naming the group. `<listItem>` SHALL take `selected` only, controlled, since Bevy has no list self-update.

#### Scenario: A controlled checkbox
- **WHEN** `<checkbox checked={() => s.sound} onValueChange={({ value }) => (s.sound = value)} />` is clicked
- **THEN** the handler runs with `value` true, `s.sound` becomes true, and the binding inserts `Checked`

#### Scenario: Controlled without a handler
- **WHEN** `<checkbox checked={() => false} />` is clicked
- **THEN** it never gets `Checked`

#### Scenario: An initial checkbox
- **WHEN** `<checkbox initialChecked={false} />` is clicked
- **THEN** it has `Checked`, and a reload does not remove it

#### Scenario: Both forms
- **WHEN** a project writes `<slider value={() => v()} initialValue={0} />`
- **THEN** `tsc` reports a conflict on `value`, and loading fails naming `value`

### Requirement: Widget events, entity-local
`onActivate` (`Activate`) on `<button>` and `<menuItem>`, and `onValueChange` on `<checkbox>`, `<radio>`, `<slider>`, `<radioGroup>`, `<tabList>` and `<listBox>`, SHALL be entity-local observers of the event Bevy triggers on that widget, queued and delivered in the frame's one host call with `ctx`. `onValueChange` SHALL give `{ entity, value, isFinal }`; on `<radioGroup>`, `<tabList>` and `<listBox>` `value` is the chosen child's entity (or `null` for no tab) and the payload SHALL add `key`, that child's TSX key, or `null` for a child TSX did not make. These events SHALL NOT be heard on any other entity.

#### Scenario: A radio group
- **WHEN** `<radioGroup onValueChange={h}><radio key="easy" /><radio key="hard" /></radioGroup>` has `hard` clicked
- **THEN** `h` runs once with `key` `"hard"`, `value` its entity and `isFinal` true

#### Scenario: Not heard by a parent
- **WHEN** a `<checkbox>` inside a `<radioGroup onValueChange={h}>` is toggled
- **THEN** `h` does not run

#### Scenario: A dragged slider
- **WHEN** a `<slider onValueChange={h}>`'s thumb is dragged and released
- **THEN** `h` runs with `isFinal` false while dragging and once with `isFinal` true on release

### Requirement: Owner events are heard by the nearest owner
`onMenuEvent` (`MenuEvent`) and `onRequestClose` (`RequestClose`), events Bevy propagates up to an owner, SHALL be taken on any UI intrinsic and SHALL hear the event raised by the entity itself or a descendant. The nearest entity on the propagation path that has the prop SHALL take it and stop its propagation; no entity above it hears it. `onMenuEvent` SHALL give `{ entity, source, action }` with `action` one of `"toggle"`, `"open-first"`, `"open-last"`, `"close-all"`, `"focus-root"`; `onRequestClose` SHALL give `{ entity, source }`.

#### Scenario: A menu
- **WHEN** `<node onMenuEvent={h}><menuButton>…</menuButton><Show when={open}><menuPopup><menuItem onActivate={a}>…</menuItem></menuPopup></Show></node>` has its button clicked, then the item clicked
- **THEN** `h` runs with `action` `"toggle"`, the script opens the popup, focus moves into it, then `a` runs once and `h` runs with `"close-all"`

#### Scenario: A modal closed by Escape
- **WHEN** `<modalDialogBarrier onRequestClose={c}><dialog modal>…</dialog></modalDialogBarrier>` is open and `Escape` is pressed in viewport mode
- **THEN** `c` runs once with `source` the barrier's entity

### Requirement: Focus props and focus events
Every UI intrinsic SHALL take `tabIndex` (`TabIndex`), `tabGroup` (`{ order, modal }` to `TabGroup`) and `autoFocus` (`AutoFocus`), and `onFocusGained` (`FocusGained`, `{ entity, cause }`) and `onFocusLost` (`FocusLost`, `{ entity }`), entity-local. With `tabNavigation` on (script-input), and focus on a widget, Tab and Shift-Tab SHALL move focus in tab order within the innermost modal group, as Bevy's `TabNavigation` does; as in Bevy, only a `tabIndex` element inside a `tabGroup` is reached.

#### Scenario: Tab through a form
- **WHEN** a project with `"tabNavigation": true` mounts three `<button tabIndex={0}>`s in a `<node tabGroup={{}}>` and `Tab` is pressed three times in viewport mode
- **THEN** `InputFocus` holds the first, second and third in turn, and each gets `onFocusGained` once

#### Scenario: A modal traps focus
- **WHEN** with `"tabNavigation": true`, a `<dialog modal>` with two focusable children is open over a page with buttons, and `Tab` is pressed three times
- **THEN** focus stays on the dialog's two children

### Requirement: The UI gallery shows every element
`examples/ui-gallery` SHALL mount every UI intrinsic and every prop family of this capability and of reactive-ui's UI intrinsics, and SHALL NOT ship in the npm package. A test SHALL load it headless and fail when an intrinsic in `IntrinsicElements` is missing from it.

#### Scenario: A new intrinsic without a gallery entry
- **WHEN** an intrinsic is added to `jsx-runtime.d.ts` and not to the gallery
- **THEN** the gallery test fails naming it
