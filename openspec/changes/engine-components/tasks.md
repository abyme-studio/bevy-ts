## 1. Contract

- [x] 1.1 Add `EditorInfo` (`origin`, `internal`, `addable`, `category`, all required; `origin` and `category` as string enums) to `protocol/bevy-ts.openrpc.json` and an optional `x-bevy-ts-editor` property on `ComponentSchema`; bump `info.version` to 1.11.0
- [x] 1.2 Regenerate the Rust and TS types (`bevy-ts-protocol-gen`, `packages/bevy-ts/scripts/gen-protocol.mjs`) and check the drift check passes

## 2. Engine

- [x] 2.1 Add `crates/bevy-ts-remote/src/editor.rs` with `BEVY_CRATES`, `INTERNAL` and `CATEGORIES`, and one function computing `EditorInfo` for a reflected type and one for a TS component
- [x] 2.2 Complete `INTERNAL` from Bevy 0.20's registered components in the cube example: every component a system writes and no user sets
- [x] 2.3 Decide `game` origin from the native crate in `EngineIdentity` (package name, `-` as `_`)
- [x] 2.4 In `values.rs`, add `x-bevy-ts-editor` to every TS and Rust entry `bevy-ts/schema` returns

## 3. Tests

- [x] 3.1 In `crates/bevy-ts-cli/tests/protocol.rs`: GlobalTransform internal and not addable, Transform addable with category transform, `game::Mover` game/addable, an undeclared TS component not addable
- [x] 3.2 A bare engine (DefaultPlugins, no ports) has no component with origin `plugin`, and no `INTERNAL` path is unknown to the registry
- [x] 3.3 The rapier example's `RigidBody` is origin `plugin`, category `physics`; a native crate's component is origin `game`

## 4. Docs

- [ ] 4.1 Sync the `remote-protocol` delta into `openspec/specs/remote-protocol/spec.md` on archive, and run `cargo test --workspace` and `pnpm check` and `pnpm test` in `packages/bevy-ts` green
