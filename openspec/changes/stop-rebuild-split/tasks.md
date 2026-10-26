## 1. Split and measure (`crates/bevy-ts`, the bench)

- [x] 1.1 `apply_set`'s restart branch as steps: `reset`, `restart_engine`, `load`, `sections`, `apply`, `run_startups`; `rebuild` calls them in order; behaviour unchanged
- [x] 1.2 `stop_bench.rs` times each step per arm (full remount, none, 100, all touched) at 1k and 10k instances (2k and 20k entities), 3 runs, through `gate-slot`
- [x] 1.3 `docs/research/stop-cost.md`: the rebuild's table by step. If the time is not in the engine drop or the level check, stop here and report where it is (design decision 5)

## 2. Skip the kept levels' check (`crates/bevy-ts-levels`)

- [ ] 2.1 `check_section` skips the level paths in the `StopPlan`
- [ ] 2.2 Tests: a kept level is not prepared in a Stop's rebuild; a level saved during play is checked and remounts

## 3. The spare engine (`crates/bevy-ts`, `crates/bevy-ts-v8`)

- [ ] 3.1 `ScriptBackend::prepare_restart`; `restart` takes the held runtime or boots one; the old one retired
- [ ] 3.2 A `Last` system under `dev`: drops the retired runtime, then boots a spare when none is held; not in a shipped game
- [ ] 3.3 The inspector follows the taken runtime
- [ ] 3.4 The fake backend counts prepares, takes and drops
- [ ] 3.5 Tests: a global set before a restart is undefined after; two Stops with no frame between both get fresh instances; the debugger reattaches after a Stop

## 4. Write back (`crates/bevy-ts-tsx`, `crates/bevy-ts-levels`)

- [ ] 4.1 `restore::changed`: per changed node, the changed component ids and whether the archetype differs
- [ ] 4.2 The recomputed list beside `restore::derived` (`GlobalTransform`, `TransformTreeChanged`; not `InheritedVisibility`, design decision 4), each with a test that Bevy recomputes it
- [ ] 4.3 `write_back(world, root, nodes)`: inserts each changed tsx component from the kept resolved node
- [ ] 4.4 `levels::restore`: write back the nodes that qualify, respawn the rest, stamp only respawned ones
- [ ] 4.5 Tests, one per scenario: a moved enemy is written back and keeps its id; every enemy moved, nothing spawned; an added component respawns; a Rust-inserted component changed respawns; a written-back parent with a respawned child

## 5. Measure

- [ ] 5.1 The bench again, 3 runs per arm at 2k and 20k, through `gate-slot`; the table in `docs/research/stop-cost.md` with the step split
- [ ] 5.2 Each arm under 100 ms at 20k, or the table says which is not and why

## 6. Docs

- [ ] 6.1 `docs/viewport.md`: a moved level entity keeps its id after Stop
- [ ] 6.2 `docs/native.md`: write-back fires replace and insert hooks for the components written; an entity with a changed component a plugin inserted is respawned
- [ ] 6.3 Workspace gate once, last, through `gate-slot`
