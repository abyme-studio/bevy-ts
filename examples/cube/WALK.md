# Cube demo walk

`node examples/cube/walk.mjs target/release/bevy-ts` drives the engine headless through the six demo steps of openspec/changes/bridge/design.md over the file system, the protocol and CDP. Release build, 2026-10-20, Apple M5 Max:

```
PASS 1 window and bobbing cube: one cube, y 0.996 -> 1.370 (headless, offscreen target)
PASS 2 edit bob to move x: applied gen 2 in 42.6 ms from save (engine 1.0 ms); same entity, x=0.60, still one cube
PASS 3 add phase: f32: "game::Mover migrated on 1 entities" (also in the terminal: true); speed 1, amplitude 2, phase 0; 38.2 ms
PASS 4 mutate .amplitude = 4: watch streamed amplitude 4 after 12.5 ms; |y| reaches 4.00 (> 2)
PASS 5 save a syntax error: reload+watch: failed at game.ts:26:4; the cube keeps moving (2.24 -> 1.44)
PASS 6 break in bob over the inspector: paused at game.ts?gen=4 line 47; the frame waited 297 ms; after a save (gen 5) it hit again at game.ts?gen=5 on the same session

all six steps pass
```

Reload latency from save to applied: 42.6 ms (edit) and 38.2 ms (layout change with migration); the engine's own share is about 1 ms, the rest is the watcher's 15 ms settle and the next frame boundary.
