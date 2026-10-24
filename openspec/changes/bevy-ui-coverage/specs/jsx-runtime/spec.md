## MODIFIED Requirements

### Requirement: Entity, component, prefab and intrinsic elements
`<entity>` SHALL describe one entity; its children SHALL be component elements and child entities in any order. A component element's tag SHALL be a component class or a Rust component typed by bevy-ts (`Transform`, `Name`), and its props SHALL be that component's fields. A Rust component that is a tuple struct of one field SHALL take that field as the prop `value`, as `Name` does (`<WorldAssetRoot value="models/Fox.glb#Scene0" />`); a `Handle<T>` field, there or in a named field, SHALL take an asset path. A prefab SHALL be either a plain TS function from props to one element, called when its element is created (usable in TSX only, like a Rust scene function), or a scene component, a class extending `SceneComponent` with `static scene(props)`, whose element takes its fields and its scene's props (`<Enemy team={1} hp={250} />`), adds the component to the scene's root, and is placeable from a level (levels). A scene component whose field and prop share a name SHALL fail the load. `<mesh>`, `<material>`, `<node>`, `<text>`, `<span>`, `<image>`, `<button>`, `<input>`, `<viewport>`, `<checkbox>`, `<radioGroup>`, `<radio>`, `<slider>`, `<sliderThumb>`, `<scrollbar>`, `<scrollbarThumb>`, `<tabList>`, `<tab>`, `<listBox>`, `<listItem>`, `<menuButton>`, `<menuPopup>`, `<menuItem>`, `<popover>`, `<dialog>`, `<modalDialogBarrier>` and `<dialogDragHandle>` SHALL be intrinsics, each typed in `IntrinsicElements` with its own props. A fragment SHALL describe several siblings. A component element outside an `<entity>` or an intrinsic, or a component given twice on one entity, SHALL be an error naming the element's source location.

#### Scenario: Two Transforms on one entity
- **WHEN** an `<entity>` has two `<Transform>` children
- **THEN** the scene fails with an error naming the second element's file and line

#### Scenario: A glTF scene from TSX
- **WHEN** a scene describes `<entity><WorldAssetRoot value="models/Fox.glb#Scene0" /></entity>`
- **THEN** the entity carries `WorldAssetRoot` holding the handle loaded from `models/Fox.glb#Scene0`, and the fox's meshes spawn under it

#### Scenario: A one-field tuple struct given a field name
- **WHEN** a scene describes `<WorldAssetRoot path="models/Fox.glb#Scene0" />`
- **THEN** the scene fails with an error naming the element's file and line and saying `WorldAssetRoot` takes `value`

#### Scenario: An input is typed
- **WHEN** a project writes `<input initialValue="" onKeyboardInput={(e) => e.value.length} />`
- **THEN** `tsc` reports no errors, and `<input value="x" />` or `onKeyboardInput={(e) => e.foo}` is a type error

#### Scenario: A widget is typed
- **WHEN** a project writes `<checkbox checked={() => on()} onValueChange={(e) => e.value} />`
- **THEN** `tsc` reports no errors, and `<checkbox onChange={f} />` or `onValueChange={(e) => e.key}` on a `<checkbox>` is a type error
