## 1. Dump

- [x] 1.1 `crates/bevy-ts-remote/src/order_dump.rs`: `OrderDump`, from `BEVY_TS_ORDER_DUMP=1`; asked by `capture`; main-world lines in `Last`, render-world lines after `PhaseSort`, one file per shot
- [x] 1.2 `screenshot::materials`: each `MeshMaterial3d`/`MeshMaterial2d`'s asset hashed by its reflected fields, a handle by its path

## 2. Tests and docs

- [x] 2.1 `an_order_dump_is_the_same_every_run_and_only_when_asked` in `screenshot_ready.rs`: two runs of the glTF race give the same dump without ids; none without `OrderDump`
- [x] 2.2 `docs/build.md`: the variable, the file, and how to diff it
