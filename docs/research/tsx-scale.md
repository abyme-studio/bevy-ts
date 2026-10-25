# TSX at scale: mount, reload and HUD cost

Task 8.3 of `openspec/changes/archive/2026-10-21-tsx`. Measured 2026-10-20 on the shared Apple M5 Max (other agents building), release, V8 15.0, Bevy 0.20, with `crates/bevy-ts-cli/tests/it/tsx_scale.rs`:

```
cargo static test --release -p bevy-ts-cli --test it tsx_scale:: -- --ignored --nocapture --test-threads 1
```

The scene is N `Enemy` prefabs (`<entity name>`, `Transform`, a TS `Health` with `initialHp`/`max`, `<mesh shape="cuboid" color>`), a player, and a HUD whose `<text>` binds the player's `Health` with no hook. Reload times are `bevy-ts/reload`'s: compile, load (JS evaluates every scene and describes it), the generation's check and apply.

| enemies | mount, ms | first frame, ms | identical reload, ms | recolour every enemy, ms | HUD hit: tsx work, µs | quiet frame: tsx work, µs |
|---|---|---|---|---|---|---|
| 1,000 | 53 | 1.9 | 41 | 43 | 59 | 0.8 |
| 10,000 | 318 | 2.0 | 349 | 345 | 45 | 0.8 |

Where a 10,000-enemy reload goes: tsx's check 70 ms (26 of them parsing the JSON section), its apply 180 ms (resolving component elements 60 to 76 ms, the keyed diff 66 to 90 ms), the rest bridge's compile and the JS load. An identical reload writes nothing and moves no change tick; recolouring writes one material per enemy and keeps every `Transform` and `Health`.

What was fixed on the way. The first run took 369 ms for an identical reload of 1,000 enemies and 2.6 s for 10,000: every source location in the section (four or five per enemy) was made project-relative with a `std::env::current_dir()` call, and the section was parsed twice, once to check it and once to apply it. The working directory is now read once per section with each file's display path cached, and the checked section is kept for the apply step: 41 ms and 349 ms.

What is left, if 10,000-entity scenes become a target: the section is JSON that repeats each element's full file path per node (a file table would shrink it), and resolution re-encodes every component element (the template cache only hits on identical props, and enemies differ in `at`).

The HUD row is the per-frame tsx work in `PostUpdate` when the player's `Health` changed (one host call, one `Text` written) and when nothing it reads changed (no call: the cost of checking the subscribed pair's tick). It does not grow with the number of enemies, whose `Health` the HUD does not read.

Bindings on native-layout fields, per site (decision 11), are measured separately: 10,000 bound instances with 100 changed per frame cost 170 µs a frame against 125 µs for a TS system doing the same (`crates/bevy-ts-cli/tests/it/tsx_sites.rs`, design.md decision 11).

## `ref.has(C)`: presence costs no more than a field

Change `bevy-ui-coverage`, stage 3. Measured 2026-10-25 on the shared M5 Max (load 26 to 78), release, with `quiet_frames_with_a_thousand_has_or_get_reads` in `tsx_scale.rs`: 1,000 `<button>`s, each with one binding, on 200 quiet frames, 3 runs.

| 1,000 bindings reading | quiet frame: tsx work, µs (3 runs) | host calls in 200 quiet frames |
|---|---|---|
| `r.has(Pressed)` | 4.5, 4.6, 5.1 | 0 |
| `r.get(Health).hp > 50` | 7.4, 8.3, 7.7 | 0 |

A `has` pair is one archetype lookup (`contains_id`) against the presence last delivered; a `get` pair reads a change tick. Neither calls JS while nothing changes. The one-HUD quiet frame above stays where it was: 0.7, 0.6 and 1.5 µs over the same three runs (the last at load 78).

## `<For>`: one change costs what it changes

Change `tsx-keyed-for-linear`. Measured 2026-10-24 on the shared M5 Max (load 18 to 43), release, with `bench/tsx-for/`: one keyed `<For>` of n rows (`<entity>`, `Transform`, a TS `Row`) under one parent, a fresh headless engine per run, 3 runs per arm. Time is from the signal set in `Update` to the end of the frame the rows are all there.

```
cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/tsx-for-after
node bench/tsx-for/bench.mjs after target/tsx-for-after
node bench/tsx-for/summary.mjs before after
```

| arm | rows | before, ms | after, ms |
|---|---|---|---|
| grow 0 to n in one set | 1,000 | 98 | 30 |
| | 10,000 | 10,518 | 257 |
| | 100,000 | over 600 s | 2,524 |
| grow 1,000 a frame | 10,000 | 18,779 | 315 |
| | 100,000 | over 600 s | 3,871 (longest frame 56) |
| reverse | 10,000 | 3 (moves nothing) | 4 |
| | 100,000 | not reached | 41 |
| clear | 10,000 | 20,422 | 28 |
| | 100,000 | not reached | 207 |
| `commands.spawn` (reference) | 10,000 | 540 | 246 |
| | 100,000 | 3,068 | 2,236 |

Before, 100k timed out at 600 s in every arm but the reference; `reverse` and `clear` never finished mounting. After, a 10k grow is 1.04 times `commands.spawn` of the same rows, and 100k is 9.8 times 10k: linear. Before, `reverse` was fast because it did nothing: the entities kept their first order. Now they are sorted into item order, in one write of the parent's `Children`.

The cause was in Rust. Each `<For>` item was a group applied alone, and each apply took every node of the whole root as a despawn candidate and cloned its key path to test it: group k walked the k−1 before it. Unmounting all items had the same shape. Now a host call's groups for one slot are one reconcile, its despawn candidates come from the groups it was given (`SlotState.rows`), a spawned node's `ChildOf`, `TsxNode` and native-layout components go in with one `insert_by_ids`, and JS sends a slot's key order only when its entities would be out of it.

## Placement: `transform` instead of `style` (bevy-ui-coverage 3b)

Measured 2026-10-25 on the shared M5 Max under heavy load (load average 29 to 82), release.

1,000 4-px nodes, each moved every frame through a bound prop, laid out by the headless engine (`tsx_scale::a_thousand_nodes_moved_by_style_or_transform`, frame ms, median of 200 frames, 8 runs by turns):

| moved by | median of runs, ms | runs where it was faster |
|---|---|---|
| `style` `left`/`top` | 9.9 (8.5 to 19.5) | 0 of 8 |
| `transform.translation` | 9.0 (7.8 to 11.4) | 8 of 8 |

A `style` binding rebuilds `Node`, so `ui_layout_system` sends each node to taffy again; `transform` writes only `UiTransform`, applied after layout. `ui_placement.rs` shows the `Node` change tick never moves and `ComputedNode` stays put.

Agent town's label bench (`openspec/research/heimdall-labels/bench.mjs` at 75154d9, arm `ui`, `BENCH_N=1000`, this branch's release engine at 2c7817b3; `openspec/changes/bevy-ui-coverage/bench-town.sh`), 8 runs by turns:

| plates placed by | frame ms | CPU % |
|---|---|---|
| `style` (before) | 21.2 ± 1.3 | 55 ± 7 |
| `transform` (after) | 21.0 ± 1.3 | 56 ± 7 |

Within noise: the `ui` arm shows at most 48 plates (its budget), and 48 nodes laid out again cost too little to show under this load. The +37-point CPU rise agent town saw came from many more moved nodes; the 1,000-node rows above are that case. `transform`'s own write (an insert per change) did not show as a cost in either bench, so 3b.4's in-place set was not made.
