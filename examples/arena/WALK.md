# Arena demo walk

The eight demo steps of `openspec/changes/archive/2026-10-21-tsx/design.md` ("First demo") against `examples/arena/src`, in V8, headless and in-process, by `crates/bevy-ts-cli/tests/arena_walk.rs`:

```
cargo static test --release -p bevy-ts-cli --test it arena_walk:: -- --nocapture
```

It runs in-process rather than against the binary so it can click the Spawn button (a triggered `PointerClick` on its entity) and read host calls per frame. Reload times are compile plus apply, with no file watcher. Release build, 2026-10-20, re-run after levels moved the enemies into `assets/levels/arena.bsn` (levels task 6.1), Apple M5 Max (shared with other agents' builds):

```
1 two enemies from arena.bsn and a player mounted, HUD "HP 100 / 100"; first load 21.7 ms
2 60 frames of damage: HUD "HP 80 / 100"; 2 host calls, on the 2 frames the player was hit (none on the other 58); 1 Text written
3 b @hp: 250.0 -> 400.0 in arena.bsn: Health (hp, max) = (250.0, 400.0), Transform untouched; reload 2.6 ms
4 recolour Enemy: both enemies share the new material; bobbed y kept (a.y 0.02); reload 8.8 ms
5 cull despawned a; an unrelated save left it tombstoned (8.6 ms); restart: true brought it back at hp 100 (23.6 ms)
6 add #EnemyC to arena.bsn: 2 spawned (enemy and eye), a and b unchanged; reload 2.2 ms
7 click Spawn: ["e78"] mounted under the runtime root in 1 host call
8 tsx_node b: source ["enemy.tsx:16", "arena.bsn:8"]; Health.hp {"provenance":"initial","value":400}, Health.max {"provenance":"controlled","value":400}
```

Notes:

- Step 2: `damage` hurts the player every 30 frames; the HUD's binding reads `player.get(Health)`, so the frames between make no call into JS, and `bob` moving the enemies every frame costs none either (the HUD does not read them).
- Step 5: `restart: true` remounts every scene as if first, so b's `Health.hp` in step 8 is the `initialHp` of the source at that point (400), with provenance `initial`.
- Steps 3 and 6 edit `assets/levels/arena.bsn` (the enemies' `@hp` and a new `#EnemyC`), not `game.tsx`: TSX says what an enemy is, the level where each one is. Each enemy is two entities now (the cube and its `eye`).
- Step 4: a generation re-expands its levels at the next frame boundary (the runtime is busy while a generation applies), so `bob` has run once more; x and z are the level's, y is never reset.
- Step 8: b's source chain is the `<entity>` in `enemy.tsx` (the scene component's `scene`), then its `@enemy::Enemy` entry in `arena.bsn`.
- A save through `bevy-ts dev`'s watcher adds its settle time (about 15 ms) and the next frame boundary, as in `examples/cube/WALK.md`.
