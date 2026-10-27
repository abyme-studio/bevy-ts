## Why

A layout-tier component's hole props (`<DistanceFog color>`, `<Sprite color>`, `<DirectionalLight color>`) take a value only. Reflect-tier props bind since reflect-bindable, and a column field always did, so a fog colour that follows the time of day needs a TS system instead of one prop. The dev WARN for two writers checks per component, so a system that writes `Sprite.texture_atlas` warns because the sprite's `color` is bound, which trains people to ignore it.

## What Changes

- **Every layout-tier hole prop binds.** `Color`, enums with data, `Option`, nested structs, lists, sets, maps, handles and opaque serde holes: `<DistanceFog color={() => ...}>` type-checks (`tsxProps` field typed `T | (() => T)`, as the enum `value` prop is) and the runtime writes each value through reflection when it changes, checked as it arrives. One writer for all kinds (`bevy_ts::reflect_value`), Bevy's field names, no code per type. A hole on the uncovered list still fails the load.
- **The two-writer WARN checks fields, per entity.** Under `bevy-ts dev`, a TS system that changes a bound field on an entity warns, naming the system, `Component.field`, the entity and the binding's site, once per system, field and site per generation. Writing another field of the same component never warns. The load-time warning by component goes.
- Docs: `docs/native.md` (TSX section), `docs/research/bevy-coverage.md` rows. Spec deltas below.

Systems' reads and writes of holes are not part of this: a TS system already reads a hole with its getter and writes it with its setter (`light.color`), through the same writer. Only TSX binding was missing.

| Hole kind | Binds in TSX | Reads in systems | Writes in systems |
|---|---|---|---|
| `Color` (`DistanceFog.color`, `Sprite.color`) | this change | yes, `ColorRead` | yes, `ColorValue` |
| Enum with data (`DistanceFog.falloff`) | this change | yes | yes |
| `Option<T>` (`Sprite.texture_atlas`, `soft_shadow_size`) | this change | yes | yes |
| Nested struct | this change | yes | yes |
| List, set, map | this change, written whole | yes | yes, whole or in place |
| `Handle<T>` (`Sprite.image`) | this change, a path or a ref | yes | yes |
| Opaque with serde (`usize`, `Duration`) | this change | yes | yes |
| On the uncovered list | no, fails the load | no | no |

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: "Reflect-tier TSX props take a binding" drops the layout-hole exception; a new requirement says hole props bind.
- `reactive-ui`: a new requirement for the dev WARN when a TS system writes a bound field.

## Impact

- `crates/bevy-ts-tsx/src/resolve.rs` (`resolve`: a hole prop binds), `crates/bevy-ts-tsx/src/host.rs` (`patch`: a hole writes through `reflected::write`).
- `crates/bevy-ts-cli/src/types.rs` `hole_props`; regenerated `packages/bevy-ts/bevy.d.ts`, `packages/bevy-ts/test/engine/native.d.ts`.
- WARN: `crates/bevy-ts-tsx/src/lib.rs` (`warn_two_writers` goes; bound targets published to a new `bevy_ts::bound::BoundFields` resource), `crates/bevy-ts/src/bevy_system.rs` (dev-only check around each TS system's run), new `crates/bevy-ts/src/bound.rs`.
- Tests: `crates/bevy-ts-cli/tests/it/reflect_values.rs`, `crates/bevy-ts-cli/tests/it/tsx_sites.rs`, `packages/bevy-ts/test/tsx/reflect.tsx`.
- Docs: `docs/native.md`, `docs/research/bevy-coverage.md`.
- No new dependency, infrastructure or service. Ask-first: the WARN adds work to every TS system run under `bevy-ts dev` that writes a component with bound fields (a snapshot and compare per bound entity). Release builds and `dev(false)` pay nothing. Budget and bench in design.md.
