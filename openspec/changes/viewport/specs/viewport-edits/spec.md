## ADDED Requirements

### Requirement: Every selected field has an edit route
For the primary selection the engine SHALL compute, per field the gizmo can change (`Transform.translation`, `rotation`, `scale`), a route, sent with the selection and in each edit:
- `bsn`: a `.bsn` level placed the entity (`levels` change); the route names the level path, the instance and the field path of the override.
- `tsx`: the entity has a `TsxNode` and the field's provenance is `controlled`, `initial` or `default`; the route carries the source chain and the location of the component element that sets it (from tsx's provenance), and the prop name.
- `locked`: the field's provenance is `bound`; the route carries the binding's site location and its expression text.
- `transient`: no source describes the entity (spawned by a TS system, a native system or `commands.mount` with no level); the edit is ECS state only.
When an entity has both a level instance and a `TsxNode` (a `.bsn` level instancing a TSX prefab), the route SHALL be `bsn` for the instance root and `tsx` for entities inside the prefab.

#### Scenario: A TSX prefab instance at the scene root
- **WHEN** enemy `b` from `game.tsx`'s `<Enemy key="b" at={[3, 0, -4]} />` is selected
- **THEN** `Transform.translation`'s route is `tsx`, with the chain ending at that line and the `<Transform translation={at} />` element in `enemy.tsx`

#### Scenario: A bound field
- **WHEN** an entity whose `Transform.translation` is a binding `() => follow(player())` is selected
- **THEN** the route is `locked`, the translate handles are drawn dimmed and do not start a drag, and rotate and scale still work if their fields are not bound

### Requirement: A drag edits the world live and becomes one edit on release
In edit mode a transform gizmo drag SHALL write the entity's `Transform` every frame of the drag, directly in the world, and render it. On release the engine SHALL emit one `ViewportEdit` on `bevy-ts/viewport_edits+watch`: `id`, `entity`, `component` (type path), `field`, `before` and `after` (JSON as `bevy-ts/get_components` gives it), and `route`. `Escape` during a drag SHALL restore `before` and emit nothing. A drag SHALL NOT start on a `locked` field.

#### Scenario: Dragging enemy b along x
- **WHEN** the operator drags enemy `b`'s x handle from 3 to 5 and releases
- **THEN** the cube follows the pointer while dragging, and one edit arrives with `before.translation[0] = 3`, `after.translation[0] = 5` and a `tsx` route

### Requirement: The editor writes TSX, bevy-ts writes `.bsn`
For a `tsx` edit, bevy-ts SHALL NOT touch source. The editor SHALL decide whether the prop is a literal at a location in the chain (tsx's rule: the editor parses), rewrite it, save, and answer with `bevy-ts/viewport_edit_result` (`id`, `status`: `written`, `rejected`; `reason`, `location`). For a `bsn` edit, bevy-ts SHALL write the override into the level file itself through the `levels` change's writer and stream a `bsn` write confirmation with the file path; until `levels` exists no `bsn` route is produced. For a `transient` edit, nothing is written and the edit's `status` is `transient` at once.

#### Scenario: A literal at the call site
- **WHEN** the editor gets enemy `b`'s edit, finds `at={[3, 0, -4]}` literal at `game.tsx`, rewrites it to `[5, 0, -4]` and answers `written`
- **THEN** the reload that follows writes the same translation (no visible jump), and the `bevy-ts/tsx_tree+watch` message names `b` as written

#### Scenario: A computed prop
- **WHEN** the editor finds `at={spawnPoint(2)}` and answers `rejected` with `reason: "computed"`
- **THEN** the engine restores `before` within one frame, and the selection stream marks `Transform.translation` as `locked` with that reason for the rest of the session or until the source location changes

### Requirement: Unanswered and reverted edits
An edit with a `tsx` route that has no `viewport_edit_result` within 10 s SHALL stay applied in the world and be reported by `bevy-ts/viewport_edits+watch` as `unconfirmed`. `bevy-ts/viewport_revert` (`id`) SHALL restore an edit's `before` value. Undo and redo SHALL be the editor's: it reverts source and the reload brings the world back, or calls `viewport_revert` for a transient or unconfirmed edit.

#### Scenario: The editor is not running
- **WHEN** an edit is made with no editor attached to the watch stream
- **THEN** after 10 s it is `unconfirmed`, the entity keeps its new position, and a later `bevy-ts/viewport_revert` moves it back

### Requirement: Edits survive the reload they cause and are lost only as ECS state is
A reload triggered by a written `tsx` edit SHALL keep the mode, the selection (by TSX node id) and the editor camera. A `transient` or `unconfirmed` edit SHALL survive reloads as any inspector edit does (tsx reload rule 12) and SHALL be lost on process exit or `restart`.

#### Scenario: Selection across the edit's reload
- **WHEN** an edit to enemy `b` is written and the reload applies
- **THEN** enemy `b` is still selected and the gizmo is still on it
