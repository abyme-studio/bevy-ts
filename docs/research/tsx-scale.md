# TSX at scale: mount, reload and HUD cost

Task 8.3 of `openspec/changes/archive/2026-10-21-tsx`. Measured 2026-10-20 on the shared Apple M5 Max (other agents building), release, V8 15.0, Bevy 0.20, with `crates/bevy-ts-cli/tests/it/tsx_scale.rs`:

```
cargo test --release -p bevy-ts-cli --test it tsx_scale:: -- --ignored --nocapture --test-threads 1
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
