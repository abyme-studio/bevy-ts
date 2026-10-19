## Context

`tsx` turns TSX into a plain-data description and reconciles it in Rust by key path, with per-field provenance (`controlled`, `initial`, `bound`, `default`), source chains and tombstones. Two of its rules matter most here: a controlled field is written only when its declared value changes, and a failed load is never partly applied. `viewport` routes each gizmo edit as `bsn`, `tsx`, `locked` or `transient`. It produces no `bsn` route until this change supplies its dependency D5: a level-instance component and a writer. The operator's rulings of 2026-10-19 set the frame:

- TSX defines prefabs. Levels and per-instance overrides are `.bsn` files the viewport writes. bevy-ts loads and writes `.bsn` itself until Bevy's loader lands, and stays close enough to upstream that the switch is a deletion.
- The editor follows a passed-through prop one level up to a literal call-site argument. Otherwise the edit goes to `.bsn`.
- A bound prop has one owner, its binding. No system writes it, and the viewport shows it read-only (tsx ruling). So a `.bsn` override of a bound field is refused.

Prior art, read 2026-10-19:

- **Bevy 0.20 `bsn!` grammar** (`crates/bevy_scene/macros/src/bsn/parse.rs`; 0.20 release post, 2026-10-08, says the syntax "should largely be nailed down"):
  - Entities are whitespace-separated entries, and `--` separates entities in lists. `,` grouping is deprecated.
  - `#Name` sets `Name` and makes a scope-local reference.
  - Components are written `C`, `C(a, b)` or `path::C { f: v }`. A patch writes only the fields it names, and entries layer in order.
  - `Children [ … ]` or any relationship target holds child entities.
  - `:"x.bsn"` is a cached scene inclusion and must be the first entry. `@scene_fn` and `@SceneComp { … }` are uncached inclusions with props.
  - `~T { … }` and `{expr}` are templates and expressions, and `on(…)` attaches observers.
  - A string in a `Handle<T>` field is an asset path.
- **Bevy PR #26043, "BSN: file format lexing, parsing, and AST"** (merged 2026-10-15, new crate `bevy_bsn`).
  - It is a hand-written recursive-descent parser for a strict subset of the macro grammar: names, a cached `:"…"` inclusion, type patches with named, tuple or unit fields, `Children […]`, and float, int, bool, string and nested-type values.
  - It has no templates, no expressions, no `@`, no `#Name` as a value, no lists and no ranges. It also appears to have no comment handling, though that is unverified.
  - The PR says a baseline loader is "queued up". None was open on 2026-10-19.
  - Cart's proposal (discussion #14437) lists function support in the asset format, versioned migrations and an auto-formatter as post-MVP.
- **Upstream writer and formatter**:
  - Draft #23639 "BSN scene writer: serialize ECS World to .bsn text" is open.
  - Tracking issue #23637 covers "write-back, asset catalog, persistent document".
  - `bsnfmt` (#23586) is open and fails on comments.
  - pcwalton's reflection-based asset PR #23576 was closed unmerged. `ReflectConvert` (#23742, string to handle) merged.
- **Jackdaw** (jbuehler23/jackdaw, Bevy 0.19 and 0.20, by the author of the upstream writer PRs). `crates/jackdaw_bsn` copies `bevy_bsn`'s syntax and adds comments, quoted names `#"…"`, generic paths, lists and maps.
  - It saves by re-emitting the whole document from its AST. The output is deterministic, but interior comments and hand formatting are lost. Only the leading comment header survives.
  - Identity is the `#Name` string. There are no stable ids, and each ECS entity is mapped to its AST node in a private world (`ecs_to_ast`).
  - It writes full type paths, keeps components that equal their default as a bare type path, and migrates old type paths by a stamped Bevy version.
  - Prefabs and sub-scenes are "not built yet" in its user guide.
  - Taken: names as identity, `bevy_bsn` as the syntax base, quoted names. Not taken: whole-document re-emit.
- **Godot `.tscn`**. An instanced scene's node, and every node inside it, writes only changed properties in the outer file. Children of an instance are addressed by path (`parent="Enemy/eye"`). An instance's root transform belongs to the outer scene. Taken: overrides addressed by path, root transform owned by the level.
- **Unity prefabs**. Instance edits are overrides by default. "Apply to Prefab" moves an override into the asset. Overrides show a blue bar and a revert. Taken for the open question as one policy (decision 9).
- **`DynamicWorld` (`.scn.ron`)**. It keys entities by raw ids and writes every component whole, with asset handles as `Path("…#Scene0")`. Not taken: raw ids are not stable across edits, and whole components defeat minimal diffs.

## Goals / Non-Goals

**Goals:**
- A level is a text file that an agent, a person and the viewport all edit, and `git diff` reads like the edit made.
- One reconciler. Levels become descriptions and go through `tsx`'s retained tree and state rules, so a `.bsn` save behaves like a `.tsx` save.
- Adopting Bevy's loader deletes our parser and loader, with no change to any level file that uses no extension.
- Every viewport edit to a level-placed entity persists. That is drag-to-place, which `viewport` could only promise once levels existed.

**Non-Goals:**
- Saving play-mode state into a level. Edits are explicit, as in Unity and Godot.
- Expressions, closures, templates, observers, lists and maps in files. Rust code belongs in Rust, and TS code in TSX prefabs.
- `.bsn`-defined prefabs as a concept of their own. An included level (`:"room.bsn"`) is the composition unit for placed groups.
- Asset GUIDs or `.meta` indirection, a binary twin format, and an auto-formatter.
- Undo stacks. The editor keeps them, as in `viewport`.

## Decisions

### 1. Upstream's file grammar plus four named extensions, checked against vendored `bevy_bsn`
bevy-ts reads exactly what `bevy_bsn` (PR #26043) parses, plus four extensions and U1 (negative numbers, found by the spike below). Each extension has a reason and a condition under which we drop it:

| Extension | Why | Leaves when |
|---|---|---|
| E1 `//` and `/* */` comments | Levels are read and annotated by people and agents. The macro grammar has comments. | Upstream's lexer gains them, which the "comment-preserving round tripping" goal in #26043 implies |
| E2 `@path::Comp { @prop: v, field: v }` in files | A level must place a TS scene component with props. It is the macro's `@SceneComp { … }` form, character for character. | Upstream ships function or scene-component support in files (post-MVP in #14437) |
| E3 a `#Name` child of an inclusion merges with the included child of that name | A level must override a prefab's inner entity. | Upstream defines child overrides of inclusions (decision 4) |
| E4 quoted names `#"mesh#0"` | They address unkeyed prefab children, whose key segment is not an identifier. | Upstream accepts them (Jackdaw already does) |
| U1 negative numbers `-4.0` | Levels place things at negative coordinates; the macro accepts them, #26043's lexer does not. | Upstream's file lexer accepts them |

A file that uses no extension parses with vendored `bevy_bsn` to the same tree as with ours. A differential test runs over every fixture and every level in `examples/`. `bevy-ts levels check --upstream` lists which extensions a file uses.

- *Beat: a bevy-ts dialect (lists, maps, expressions, as Jackdaw has).* Each addition is something the switch to Bevy's loader would have to migrate.
- *Beat: depend on `bevy_bsn` from Bevy's git main.* It tracks Bevy main while bevy-ts pins 0.20, and it has no spans or trivia for the writer. We vendor its syntax module at the #26043 commit for the differential test only.
- *Beat: wait for Bevy's loader.* It has no date, and the viewport needs `bsn` routes now.

### 2. One lossless parser; the writer splices text
bevy-ts's parser is hand-written, as upstream's is. It keeps every token with its leading trivia (whitespace and comments) and every node's byte span. Printing the tree gives back the file byte for byte. The writer never re-emits a file. Each operation becomes a list of text splices:

- Setting a field replaces the bytes of its value.
- A new field goes after the last field of that patch, in the style it already uses (one line or several).
- A new patch entry or entity goes on lines of its own, with indentation and separator style copied from its nearest sibling.
- A deletion removes the entity's span together with its leading comments and one adjacent `--`.

After splicing, the writer re-parses the result and checks that the tree differs from the old one by exactly the intended operation. It then writes atomically (a temporary file, then a rename). "Minimal" is tested: a one-field edit changes one line in `git diff`.

New text follows fixed rules. Floats are the shortest form that round-trips and always carry a `.`. Names use the shortest type path the registry resolves unambiguously, unless the file already uses full paths for that type. New entities use four-space indentation when the file gives no example.

- *Beat: re-emit from the AST (Jackdaw, and #23639's direction).* It is deterministic, but it loses interior comments and hand layout, and the first save of a hand-written file rewrites all of it, so `git diff` stops reading like the edit.
- *Beat: a formatter-normalised format (rustfmt style).* It needs `bsnfmt`, which is unmerged and fails on comments, and it would fight agents' hand edits.

### 3. A level is a reconciled root; its entities are descriptions
`app.level("levels/arena.bsn")` registers a level root. A `BsnLevel` asset loader (extension `bsn`) parses the file. Bevy's asset server resolves paths and watches files. bevy-ts turns the tree into a `tsx` description:

- Each entity becomes an `<entity>` node keyed by its `#Name`.
- Each type patch becomes a component element whose fields are `controlled`, with provenance `override` and the file location.
- `Children [ … ]` becomes child nodes.
- `@Comp { … }` is expanded by the TS scene component's `scene` in JS (decision 5).
- `:"other.bsn"` is expanded from that file's tree, recursively, with a cycle check.

The finished description goes to the same reconciler as an `app.scene` root named `level:<path>`. `commands.loadLevel(path)` and `commands.unloadLevel(path)` do the same at runtime.

- *Beat: spawn levels through BSN or `DynamicWorld` and stop there.* Nothing would reconcile, so every save respawns and loses simulation state, which `tsx` exists to avoid.
- *Beat: a second reconciler for levels.* It would bring two sets of state rules, and a TSX prefab inside a level would need both.
- *Beat: an own file watcher (`notify`).* Bevy's `file_watcher` already serves assets, and the switch then replaces one asset loader with another. The writer's conflict check (decision 8) reads the file itself, so it does not depend on watcher timing.

### 4. Identity is the name path; names are unique among siblings
- An entity's node id is a hash of the level path and the `#Name` path from the level's top: `arena.bsn/EnemyA/eye`.
- Inside a prefab expansion, the path continues with `tsx` key segments, which are keys or `type#n`.
- Two siblings with one name is a load error.
- An unnamed entity in a hand-written file gets a positional id (`#3` among unnamed siblings) and a dev warning. The first time the writer writes to that entity, it inserts a name, so an unnamed entity is named the moment it matters.
- Renaming by hand respawns the entity, as changing a TSX key does. The writer's `rename` operation also renames the node in the retained tree before the reload sees the file, so the entity and its state stay.

E3 extends this into prefab instances. In an instance's `Children [ … ]`, an entry whose name matches a child of the expansion patches that child: its patches layer after the prefab's. A name that matches no child adds a new child. Unkeyed prefab children are addressed with E4, for example `#"entity#1"`, and a dev warning suggests a `key` in the TSX.

- *Beat: a GUID component on every entity (Unity's fileID).* It survives renames, but it puts noise in every entity, invents a component upstream does not have, and makes diffs and agent edits opaque. Renames are rare and go through the writer.
- *Beat: positional ids.* Inserting one entity would shift every later one.
- *Beat: an `Override("eye")` marker component for child overrides.* It parses with upstream today, but it reads worse, and it is no closer to the semantics upstream will choose.

### 5. Placeable prefabs are scene components, placed with Bevy's own syntax; the instance root's Transform is the level's
A prefab a level can place is Bevy's scene component (`#[derive(SceneComponent)]`, `bevy_scene/src/scene_component.rs`) written in TS: a component class whose `static scene(props)` builds its scene (operator ruling 2026-10-20).

```tsx
export class Enemy extends SceneComponent {
  team: u8 = 0;                                        // component field
  static scene({ hp = 100 }: { hp?: f32 }) {           // props
    return (
      <entity name="enemy">
        <Health initialHp={hp} max={hp} />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
        <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
      </entity>
    );
  }
}
```
```
#EnemyB
@enemies::Enemy { @hp: 250.0, team: 1 }
Transform { translation: Vec3 { x: 3.0, z: -4.0 } }
```

- **Bevy 0.20's syntax.** `@Comp { @prop: v, field: v }` is exactly the macro's form. `@prop` sets a scene prop, which is evaluated when the scene is included and is not patchable. A plain `field: v` patches the component's own field (the "SceneComponent Props" section of `bevy_scene/src/lib.rs`). That answers the old open question: props take `@`.
- **The scene component is always added** to the instance root, as in Bevy, so `Query<[Enemy]>` finds every placed enemy.
- **Naming.** The name is a Rust-style path from the module, unless `static path` pins it: `src/enemies.tsx` exporting `Enemy` is `enemies::Enemy`, and `src/units/tank.ts` exporting `Tank` is `units::tank::Tank`. The compile step reads the class's fields and its `scene` props (types from the props type, defaults from the destructuring), with components' field types. That lets Rust type-check `.bsn` values, the inspector show fields and props, and the writer emit them.
- **Plain function prefabs** (`const Eye = (props) => <entity/>`) stay usable inside TSX only, like Rust's scene functions (`@eye(0.2)` in `bsn!`). They are not placeable from a file, because Bevy's file format has no function calls (post-MVP in #14437).
- *Beat: any exported function prefab placeable by name with `@Prefab { prop: v }` (the previous draft).* Bevy reads `prop: v` inside `@Comp { }` as a component field, not a prop, so the same text would mean different things in Bevy and bevy-ts. It also gave levels no component to query placed instances by.
- *Beat: `prefab(name, props, fn)` with a runtime props schema (an earlier draft).* A name string and a schema object beside the TS type said the same thing twice.
- Expansion is one host call per reconcile that has any instances to expand (`tsx`'s B2 entry). It sends `(name, props)` per instance and gets descriptions back.
- An instance root's `Transform` is the level's, unless the prefab binds it (decision 6). A prefab that sets the root `Transform` with a literal or a prop gets it overridden by the level's patch, which follows the expansion. Placement is therefore always a component override and never a prop the engine would have to trace. tsx decision 10 says the engine cannot tell whether `translation={at}` came from a literal.

- *Beat: name prefabs by file and export as a string (`:"src/enemy.tsx#Enemy"`).* It parses with upstream today, but a cached inclusion takes no props, so props would need a fake component. A Rust-style path derived from the module reads like any other `.bsn` type path, and `path` pins it against renames.
- *Beat: untyped props (JSON passed through).* The writer could not emit them, the inspector could not show them, and errors would surface inside the prefab function rather than at the line in the level.

### 6. Override values are controlled; overriding a bound field is a load error
- An override is a declared value with provenance `override`. It is written when the declared value changes against the last applied value, and is otherwise left alone (tsx rule 3). A system that moves an enemy keeps it moved across reloads that leave its `Transform` override unchanged.
- Removing an override makes the field's declared value the prefab's again, so it is written.
- An override of a field the prefab binds (`bound`) fails the load, with the level's file, line and column and the binding's site location and expression. That holds for an instance root's `Transform` too. The viewport never produces one, because the field's route is `locked`. A level designer who needs one lamp brighter than the rest makes the binding read a prop (`intensity={() => base * alarm()}` with `base` a prefab prop) and overrides the prop.

- *Beat: the override pins that instance (Godot's rule, recommended by scene-bindings research).* It breaks the ruling that a bound prop has one owner, its binding, and makes a field's owner depend on a level file the TSX author cannot see.
- *Beat: `initial` semantics for overrides* (seed once, never re-write). Editing a value in the file would then do nothing until restart, the same reason tsx rejected all-initial.

### 7. Edits arrive as operations on name paths; a policy picks the target file
`bevy-ts/level_write` takes a batch of operations: `set` (field path and value), `revert` (remove an override), `set_prop`, `add_instance` (a prefab with props, or an included level), `add_entity`, `delete`, `rename` and `reparent`. Each names its level file, the base content hash, and the target by name path. It returns the new hash, the changed line range and the inverse batch for the editor's undo stack. Viewport `bsn` edits become these operations inside the engine.

The `bsn` route carries the owner chain of the field: every place that could hold the edit, innermost first. For example: the TSX element in `enemy.tsx`, then `#EnemyB`'s instance in `room.bsn`, then `#Room`'s instance in `arena.bsn`. The default target is the outermost level file, the one the selection came from, which is the level that placed the instance root (Godot and Unity both do this). "Apply to prefab" (decision 9) or "Apply to room.bsn" moves an override one step inward and removes it outward. When the inward target is TSX, it does that as a two-step transaction: bevy-ts emits a `tsx` route edit, and removes the `.bsn` override only after the editor answers `written`.

Deleting a prefab-internal entity from a level is rejected (`reason: "prefab-internal"`). Godot also cannot delete an instanced child, and bevy-ts has no "removed child" override.

- *Beat: the editor writes `.bsn` too.* Ruled out in `viewport` decision 10: there would be two serialisers of one format.
- *Beat: text diffs as the edit format (an LSP-style `TextEdit`).* They break as soon as the file moves under them (decision 8). A name path survives an agent's unrelated edit.

### 8. The disk wins; edits are replayed by name path, never text-merged
The writer keeps, per file, the hash of the content it last parsed or wrote. For each write:

1. It reads the file.
2. If the hash matches the base, it splices and writes.
3. If not, someone else wrote the file (an agent, a person in another editor, `git checkout`). The writer first applies the disk version to the world as a normal reload (decision 10). It then replays the operation by name path on the new tree.
   - If the target and field still exist, it writes and reports `rebased`.
   - If they are gone, or the same field changed on disk, it rejects with `conflict`, `reason` and the disk value. The world follows the disk, and the viewport restores the disk value within one frame.

The writer's own writes are recognised by hash when the watcher reports them, so they cause no second reconcile write: last-applied values were updated at write time. `bevy-ts/level+watch` reports every external change, so abyme can refresh an open text buffer.

- *Beat: lock level files while edit mode is on.* That blocks agents and `git` for as long as someone has the viewport open.
- *Beat: last writer wins (overwrite with our version).* It silently drops an agent's edit.
- *Beat: a three-way text merge.* Name-path replay gives the same result for the cases that matter (different entities or fields), states the real conflict (same field) plainly, and needs no merge machinery.

### 9. TSX is rewritten only by "Apply to prefab" by default (operator, 2026-10-19)
Take an entity inside a level-instanced prefab (enemy `b`'s `#eye`) or a prefab's own field (its mesh colour). Two policies are possible, chosen by `levels.prefabEdits` in the project config. Both are specced, and both run on the owner chain of decision 7.

| | `override` (Unity's default; recommended) | `source` |
|---|---|---|
| Drag `#eye` on enemy `b` | `Children [ #eye Transform { … } ]` under `#EnemyB` in the level; other enemies unchanged | the editor rewrites `<Transform translation={[0, 0.6, 0.5]} />` in `enemy.tsx` when it is a literal; every enemy's eye moves |
| TSX rewritten | only through "Apply to prefab" | on every edit whose TSX prop is a literal; otherwise falls back to `override` |
| What this change must carry | child overrides by name (E3, E4), the apply transaction, override badges and revert | almost nothing beyond root placement; E3 is used only by hand-written files |
| Risk | levels pile up overrides nobody meant to keep; mitigated by override listing (`level_node`) and "Apply" | one drag changes every instance silently; one mistake is a TSX change rather than a level line |

`viewport`'s current spec routes entities inside a prefab as `tsx`. That spec is `source`. Under `override`, those entities route `bsn` too, which viewport must accept (V1 below). TSX scene roots that no level placed (an `app.scene`) keep the `tsx` route in both policies. Their fallback, when the editor finds no literal, is decision 11.

### 10. Hot reload follows tsx's rules, per level, inside generations
- A `.bsn` change re-parses that file and every level that includes it. It expands their prefab instances in one host call, then reconciles each affected level root at the next frame boundary.
- A TS generation re-expands every instance as part of the generation, because prefab code may have changed. A level that no longer loads under new code fails that generation (bridge rule 8).
- A file that fails to parse or resolve (unknown prefab, component, field or type mismatch) is rejected whole. Its entities stay as they were, and the error carries file, line and column.

The full rules are in "Hot-reload state rules" below.

### 11. A TSX scene's fallback overrides go to a sidecar level
The viewport ruling sends an edit to `.bsn` when the editor finds no literal. For an entity of an `app.scene` root (no level), that file is `levels/<scene>.bsn`. The writer creates it on the first such edit, and it layers over the scene by `tsx` key path, with E3 merge semantics on the scene's root entities. `app.scene(name, fn, { level: path })` names another file.

- *Beat: reject the edit (`computed`).* The ruling says such an edit goes to `.bsn`. Rejecting it would leave TSX-only scenes with no persistent placement.

### 12. Asset references are upstream's: a string in a `Handle` field
- A string in a `Handle<T>` field is an `AssetPath` relative to the asset root, labels included (`"models/crate.glb#Scene0"`).
- `./` and `../` resolve against the level file, as `AssetPath::resolve` does.
- Conversion uses `ReflectConvert` where the 0.20 registry has it, and bevy-ts's own string-to-handle conversion otherwise.
- A missing asset is a warning with file, line and column, and the entity loads with the handle unresolved.
- `bevy-ts levels check` lists broken references.
- The writer emits a path the way the asset browser gave it, relative to the asset root.

- *Beat: GUIDs in `.meta` files (Unity).* Bevy has none. Renames would need a project-wide rewrite either way, and a path is what an agent can read.

## Data model

**Rust, `bevy-ts-levels` (survives reloads; ECS state):**
- `BsnLevel` asset: the lossless tree, content hash, the extensions used, and diagnostics.
- `LevelInstance` component, on each level-placed entity (viewport's D5): the level path, the name path, and the owner chain (level frames outermost first, then the TSX source chain).
- `Levels` resource: open levels with base hashes; an include graph for reload fan-out; and a pending write queue with conflict state.
- `TsxNode` provenance gains `override { file, line, column }`.

**Wire:**
- To JS: `expand([(instance node id, prefab name, props JSON)])` through tsx's host entry (B2), returning descriptions.
- Protocol 1.2:
  - `bevy-ts/level_list`: open levels, hash, extensions and diagnostics.
  - `bevy-ts/level_node` (`entity` or name path): owner chain, overrides with locations, props.
  - `bevy-ts/level_write`: operations, returning hash, lines and the inverse.
  - `bevy-ts/level_apply` (`entity`, field, target frame): move an override one owner inward (decision 7).
  - `bevy-ts/level+watch`: loaded, failed, written, external change, rebased, conflict.

## Hot-reload state rules

On top of bridge's and tsx's rules:

1. A `.bsn` save reconciles every level that the file is, or includes, at one frame boundary. No frame mixes the old and new file.
2. Same name path: the entity stays. Its components are diffed as tsx rule 2.
3. An override whose value changed is written. An unchanged override is not, even if a system changed the field.
4. A removed override: the field takes the prefab's declared value, and it is written.
5. A changed prefab prop re-expands that instance. The expansion is reconciled with tsx's rules, so `initial*` fields keep their running values.
6. A new name path spawns. A removed name path despawns with descendants. A hand rename is a removal plus an addition. A writer `rename` keeps the entity.
7. An entity a system despawned is tombstoned (tsx rule 8), and a level reload does not respawn it. `restart: true` clears tombstones.
8. A file that fails to parse or resolve leaves its level, and every level including it, exactly as it was. The error streams on `level+watch` and the terminal.
9. A TS generation re-expands all instances and fails whole if any level fails under it.
10. The writer's own write is not reconciled twice. An external write while an edit is pending is decision 8.

## Depends on tsx and viewport

- **T1 (tsx).** Scene components (`class X extends SceneComponent` with `static scene(props)`) by name (a Rust-style path from the module, or a pinned `static path`), with fields and props read by the compile step, expandable from Rust through a host entry with `(name, props)` per instance.
- **T2 (tsx).** External roots: a Rust plugin can submit a description for a named root (`level:<path>`) built in Rust and partly expanded in JS. It is reconciled under the same rules and generation boundary as `app.scene` roots.
- **T3 (tsx).** Provenance `override` with location, and, before reconciling a level description, a lookup of which expanded fields are `bound` with their site location and expression, so levels can refuse overrides of them.
- **T4 (tsx).** An overlay root: a description layered over an `app.scene` root by key path, for decision 11.
- **V1 (viewport).** The `bsn` route payload is the owner chain plus the chosen target under `levels.prefabEdits`. Under `override`, entities inside a level-instanced prefab route `bsn`, not `tsx`. The `bsn` write confirmation is `level+watch`'s `written`, `rebased` or `conflict`.

## First demo: arena as a level

`examples/arena/` moves the enemies out of `game.tsx` into `assets/levels/arena.bsn`. `Enemy` becomes a registered prefab, and a `room.bsn` is included twice.

1. `bevy-ts dev`: the same two enemies, now from `arena.bsn`.
2. In edit mode, drag enemy `b` along x. `arena.bsn` gains one changed line in `git diff`. The cube does not jump on the reload.
3. Drag `#eye` of enemy `b` with the policy set to `override`. `Children [ #eye … ]` appears under `#EnemyB`, and enemy `a`'s eye is unchanged. "Apply to prefab" moves it into `enemy.tsx` through the editor and removes the `.bsn` lines.
4. An agent edits `hp: 250.0` to `400.0` by hand and saves. `max` becomes 400, and `hp` keeps its damaged value.
5. While enemy `b` is being dragged, the agent renames `#EnemyA`. The drag writes with `rebased`. If the agent deleted `#EnemyB` instead, it is `conflict` and `b` disappears.
6. Save a level with a typo in a component name. The terminal names `arena.bsn:12:5`, and the world is unchanged.
7. `bevy-ts levels check --upstream arena.bsn` lists E1 and E2 as used.

## Risks / Trade-offs

- [Upstream picks a different syntax for prefab calls or child overrides than E2 and E3] → Each extension is isolated in the parser and listed by `levels check`. The writer can rewrite files mechanically, because every use is a known tree shape. Name paths are kept either way.
- [Upstream's loader arrives with different semantics (no reconcile, respawn on reload)] → We adopt its parser and asset type, not its spawn path. Our reconciler stays because upstream does not reconcile. The deletion is our parser and loader. The writer, E2 and the reconciler stay until upstream has equivalents.
- [Splice edits can produce a file that parses to something else (a comment swallowing a line, a trailing comma rule)] → The writer re-parses after splicing and refuses any write whose tree differs by more than the operation, reporting `internal`.
- [A hand-written level with unnamed entities gets names inserted on first edit, which is a two-line diff] → Documented, and it happens once. `levels check` warns about unnamed entities up front.
- [Override build-up under the `override` policy] → `level_node` lists every override of an instance with its line. The viewport marks overridden fields, and "Apply" and "Revert" are one click.
- [Prefab expansion is one more host call per level reload] → One call per reconcile, never per frame. It is measured in task 6.3 at 1,000 instances.
- [Type-path ambiguity under short names (`Transform` in two crates)] → The reader errors with both candidates. The writer emits the full path for that type in that file.
- [`ReflectConvert` coverage in 0.20 for handle fields is unverified] → Task 2.4 checks. bevy-ts's own string-to-handle path covers `Handle<T>` for registered asset types.
- [Upstream's file lexer may already accept comments differently, for example as doc comments] → The differential test pins the behaviour at the vendored commit. We re-run it whenever we bump the vendored copy.

## Migration Plan

None for users: this is a new crate, new TS APIs, and protocol 1.2 (additions only). For Bevy's loader: bump the vendored `bevy_bsn`, run the differential test, and when upstream's `BsnLevel`-equivalent asset lands, swap our parser and loader for it behind decision 3's level root. Files using no extension need no change. Files using E1 to E4 are rewritten by `bevy-ts levels migrate` once upstream has the matching forms.

## Spike findings (task 1.1, 2026-10-20)

- **#26043's lexer has no comments.** `/` is an unexpected character (`bevy_bsn/src/lex.rs` at 81f44028). E1 stays an extension.
- **#26043's lexer has no negative numbers.** `-` and `--` are lexed before numbers, so `-4.0` is `Minus` then `Float(4.0)` and the parser rejects it; `+4.0` lexes as one float. The macro accepts negatives and levels need them, so bevy-ts accepts them and `levels check --upstream` lists them as extension `U1`. The differential test pins this.
- **Upstream's `ParseStream` reads a lex error as the end of input** (`peek_token` returns `None`), so its parser accepts the prefix of a file that fails to lex. The differential test lexes upstream's way first.
- **Upstream's root rule parses one entity** and stops at `--`. A level's top level is a scene-list body (the rule inside `Children [ … ]`), and the differential test compares it with that rule.
- **`@name` in an ordinary patch is refused**, as the macro refuses it ("Scene prop fields are not supported in normal component patches").

## Implementation notes (group 2)

- **An included file with several top-level entities is a group.** `#RoomA :"room.bsn"` takes `room.bsn`'s one top-level entity as its base; a file with several becomes the children of the including entity, keyed by their names.
- **Includes resolve as asset paths:** `:"levels/room.bsn"` from the asset root, `:"./room.bsn"` against the including file.
- **A level patch of a field the prefab gives as `initialF` makes it controlled:** the override wins and the `initial` prop goes.
- **Components with handles go through reflection (task 2.4).** Reflect-tier components, and layout-tier ones whose holes may own heap data (a `Handle` field), get a reflect target in tsx: inserted from their reflected `Default`, then written field by field, so an unchanged field is never rewritten. 0.20 registers no `ReflectConvert` from strings to handles, so a string in a field whose type has `ReflectHandle` is loaded with `load_erased` and made typed with `ReflectHandle::typed`. A failed asset load is a warning located at the field that named it.
- **The binding's expression** is not in the description, so the bound-field error names the binding's site (file, line, column) and not its expression text.

## Implementation notes (group 3)

- **Field paths reach lanes.** `set` on `Transform.translation.x` replaces that lane's bytes; a missing lane joins its typed value in the value's style; a missing path inside a prefab becomes `Children [ #eye Transform { … } ]` under the instance.
- **Inverses are batches by name path.** An inverse puts every value back but may place a re-added lane or entity after its siblings (the end of its list), so it restores the tree, not always the bytes.
- **The verify step compares entity heads** (each named entity's entries as tokens, without trivia) outside the touched paths; any other difference refuses the batch with `internal`.
- **Naming an unnamed entity moves the positional ids after it** (`#1` becomes `#0`); the writer reports those moves and the engine rekeys the retained tree (`tsx::rekey`), so no entity respawns. Renames rekey only in the level whose own file changed; a rename inside an included file respawns the includers' copies. `reparent` respawns the moved entity.
- **Conflicts** (decision 8): a disk hash that differs from the batch's base reloads the disk version first; a `set` or `revert` whose field text differs between the base and the disk is `changed on disk` (with the disk's text), one whose entity or field is gone is `target removed`, and the rest replay and report `rebased`.

## Operator rulings

- 2026-10-20: comments (E1) and negative numbers (U1) stay bevy-ts extensions for now; no upstream
  PRs. The differential test exempts them by name, so the switch to upstream stays a deletion once
  Bevy's lexer has them.

- 2026-10-20: placeable prefabs are scene components, `class X extends SceneComponent` with
  `static scene(props)`, placed with Bevy's own `@Comp { @prop: v, field: v }`. Plain function prefabs
  stay for TSX only (docs/research/dx.md).

- 2026-10-19: `levels.prefabEdits` defaults to `override`. A viewport edit is a per-instance `.bsn`
  override; "Apply to prefab" rewrites the TSX literal. `source` stays a setting a project may choose.

- 2026-10-19: TSX defines prefabs, what a thing is. Levels (where things are) and per-instance
  overrides are `.bsn` text files the IDE viewport writes, as Godot writes `.tscn` and Unity its
  scenes. Bevy 0.20 ships no `.bsn` loader, so bevy-ts loads and writes `.bsn` itself until
  Bevy's lands, staying as close to upstream BSN syntax and semantics as possible so the switch is
  a deletion. Viewport edits arrive routed by provenance; `bsn` routes are this change's.

- 2026-10-19 (tsx ruling, applied here): a bound prop has one owner, its binding. A `.bsn`
  override of a bound field is refused with a load error naming the file, line and binding; it
  does not pin the instance.

## Open Questions

- Whether upstream's eventual child-override form matches E3's merge by name. Watch #23637 and the loader PR.
