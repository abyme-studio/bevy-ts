# Levels demo walk

The seven demo steps of `openspec/changes/levels/design.md` ("First demo") against `examples/arena` (`src/` and `assets/levels/`), in V8, headless and in-process, under both `levels.prefabEdits` policies, by `crates/bevy-ts-cli/tests/levels_walk.rs`:

```
cargo static test --release -p bevy-ts-cli --test it levels_walk:: -- --nocapture
```

A gizmo drag is the viewport's release: the entity is moved, then `bevy_ts_levels::write_from` is called with the start value. The editor is played by the test: it rewrites `enemy.tsx` when Apply hands it the edit. File reloads are triggered by hand, with no file watcher. Each write's diff is printed under its step. Release build, 2026-10-20, Apple M5 Max (shared with other agents' builds):

```
1 two enemies from arena.bsn (and a guard in each of two rooms); load 61.2 ms
2 drag b along x: written (1.6 ms, the reconcile included); b stays at x 5
    -Transform { translation: Vec3 { x: 3.0, z: -4.0 } }
    +Transform { translation: Vec3 { x: 5.0, z: -4.0 } }
3 drag b's eye (override): Children [ #eye … ] under #EnemyB, a's eye unchanged
    +Children [
    +    #eye
    +    Transform { translation: Vec3 { y: 0.9 } }
    +]
  apply to prefab: the editor rewrote enemy.tsx:21 and level_write removed the override; both eyes at y 0.9
    -          <Transform translation={[0, 0.6, 0.5]} />
    +          <Transform translation={[0, 0.9, 0.5]} />
    -Children [
    -    #eye
    -    Transform { translation: Vec3 { y: 0.9 } }
    -]
  under source: b's eye routes tsx (the editor rewrites enemy.tsx); b itself still routes bsn
4 an agent sets @hp: 400.0: Health (hp, max) = (90.0, 400.0); reload 2.5 ms
    -@enemy::Enemy { @hp: 250.0 }
    +@enemy::Enemy { @hp: 400.0 }
5 rename #EnemyA mid-drag: rebased
    -Transform { translation: Vec3 { x: 5.0, z: -4.0 } }
    +Transform { translation: Vec3 { x: 6.0, z: -4.0 } }
  delete #EnemyB mid-drag: conflict (target removed); b despawned
6 a typo: levels/arena.bsn:5:1 levels: unknown component Transfrom; the world is unchanged
7 levels check --upstream arena.bsn:
    arena.bsn:1:1: E1 comment
    arena.bsn:4:1: E2 scene component instance
    arena.bsn:5:36: U1 negative number
    arena.bsn:5:45: U1 negative number
    arena.bsn:7:1: E1 comment
    arena.bsn:10:36: U1 negative number
    arena.bsn:10:46: U1 negative number
    arena.bsn:14:45: U1 negative number
```

Notes:

- Step 2 diffs the lanes against the drag's start, so `bob` having moved b's y writes nothing for y: one changed line.
- Step 3: "Apply to prefab" asks `bevy-ts/level_apply`, which answers `tsx` with the element (`enemy.tsx:21`), the prop and the value, and `then`, the `level_write` batch that removes the override. The editor sends `then` only after it wrote the TSX, so a rejected edit leaves the override in place.
- Step 4: `hp` is the prefab's `initialHp`, so the running value (90 after damage) stays; `max` is controlled and follows.
- Step 5: the rename lands on disk while b is dragged; the drag's batch names `#EnemyB`, so it replays on the new text and reports `rebased`. After the delete, the world follows the disk first and the batch's target is gone: `conflict`, reason `target removed`.
- Step 7, run on the file as step 6 left it (b deleted, a renamed `#Grunt`): `U1` is the negative-number extension (operator, 2026-10-20).
