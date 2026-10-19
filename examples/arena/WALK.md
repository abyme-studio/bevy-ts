# Arena demo walk

The eight demo steps of `openspec/changes/tsx/design.md` ("First demo") against `examples/arena/src`, in V8, headless and in-process, by `crates/bevy-ts-cli/tests/arena_walk.rs`:

```
cargo test --release -p bevy-ts-cli --test arena_walk -- --nocapture
```

It runs in-process rather than against the binary so it can click the Spawn button (a triggered `PointerClick` on its entity) and read host calls per frame. Reload times are compile plus apply, with no file watcher. Release build, 2026-10-20, Apple M5 Max (shared with other agents' builds):

```
1 two enemies and a player mounted, HUD "HP 100 / 100"; first load 12.6 ms
2 60 frames of damage: HUD "HP 80 / 100"; 2 host calls, on the 2 frames the player was hit (none on the other 58); 1 Text written
3 b hp={250} -> {400}: Health (hp, max) = (250.0, 400.0), Transform untouched; reload 4.4 ms
4 recolour Enemy: both enemies share the new material; bobbed y kept (a.y 0.04); reload 4.0 ms
5 cull despawned a; an unrelated save left it tombstoned (4.0 ms); restart: true brought it back at hp 100 (13.5 ms)
6 add <Enemy key="c">: 1 spawned, a and b unchanged; reload 4.2 ms
7 click Spawn: ["e71"] mounted under the runtime root in 1 host call
8 tsx_node b: source ["enemy.tsx:13", "game.tsx:41"]; Health.hp {"provenance":"initial","value":400}, Health.max {"provenance":"controlled","value":400}
```

Notes:

- Step 2: `damage` hurts the player every 30 frames; the HUD's binding reads `player.get(Health)`, so the frames between make no call into JS, and `bob` moving the enemies every frame costs none either (the HUD does not read them).
- Step 5: `restart: true` remounts every scene as if first, so b's `Health.hp` in step 8 is the `initialHp` of the source at that point (400), with provenance `initial`.
- Step 8: b's source chain is the `<entity>` in `enemy.tsx` (the prefab), then the `<Enemy key="b">` call site in `game.tsx`, which an editor would rewrite.
- A save through `bevy-ts dev`'s watcher adds its settle time (about 15 ms) and the next frame boundary, as in `examples/cube/WALK.md`.
