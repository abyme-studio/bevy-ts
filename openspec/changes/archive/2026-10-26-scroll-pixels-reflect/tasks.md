## 1. Reach the resource

- [x] 1.1 `crates/bevy-ts/src/type_data.rs`: `ReflectResource` for `MouseScrollPixelsPerLine`, with the data it depends on; unit test checks both
- [x] 1.2 Check `AccumulatedMouseMotion` and `AccumulatedMouseScroll` for the same gap: both reflect `Resource` already
- [x] 1.3 `scripts/gen-bevy-types.sh`: regenerate `bevy-ts/bevy`
- [x] 1.4 `crates/bevy-ts-cli/tests/it/input_devices.rs`: a script reads `Res<MouseScrollPixelsPerLine>` and sets it with `ResMut`

## 2. Docs

- [x] 2.1 `docs/native.md`: both coverage rows say `Res`/`ResMut`; both scroll examples read the resource, and `packages/bevy-ts/test/input_messages.ts` type-checks the message one
