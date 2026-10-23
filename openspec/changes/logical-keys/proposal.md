## Why

`Keys` reads physical keys (`ButtonInput<KeyCode>`). A game that binds "the key that types `z`" or "`?`" on any layout needs Bevy's logical keys, `Res<ButtonInput<Key>>`, which a script cannot read today. The coverage table names it as text-input's follow-up (script-input-devices design, "Logical keys").

## What Changes

- `bevy-ts/input` gains the exported param `LogicalKeys`, over `Res<ButtonInput<Key>>`, with `pressed(key)`, `justPressed(key)` and `justReleased(key)`, as `Keys` has.
- `bevy-ts/input` gains `Key`, Bevy's logical key as one number: a generated member per named key (`Key.Enter`, `Key.Escape`, `Key.Shift`), `Key.Character("z")` for a character, and `Key.Dead("´")` / `Key.Dead()` for a dead key. `Key.Character` and `Key.Dead` are module JS that turn the character into a number; no string crosses into Rust.
- The numbers join the one input number space script-input-devices set up: named keys from `0x3000`, characters at `0x100000` plus the code point, dead keys at `0x300000` plus the code point.
- After script-input-devices lands its condition arms: `inputPressed`, `inputJustPressed`, `inputJustReleased` and `inputToggleActive` also take a `Key`, read against `ButtonInput<Key>`, as Bevy's generic `input_just_pressed::<Key>` does.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `script-input`: logical keys (`LogicalKeys`, `Key`), and the input conditions taking them.

## Impact

- `crates/bevy-ts/src/keys.rs`: a `NAMED_KEYS` list beside `KEY_CODES`, and the `Key` number codec (`key_of(n)`, `key_number(&Key)`).
- `crates/bevy-ts-cli/src/input.rs`: `LogicalKeysParam` and `Key` in the module's generated JS and `.d.ts`.
- `crates/bevy-ts/src/systems.rs`: the four condition arms decode the `Key` range (after script-input-devices).
- Tests in viewport mode through `bevy-ts/viewport_input`; `docs/native.md` input section.
- Shared files: `keys.rs`, `input.rs`, `systems.rs` and `docs/native.md` are also edited by script-input-devices. Separate items; whichever lands second rebases.
- Protocol: none.
- Operator go: none needed. No string arguments for exported params (a character is encoded as a number in the module's JS, an existing pattern), no new dependency, no AI surface, no data. Each call is one host call, as `Keys`.
