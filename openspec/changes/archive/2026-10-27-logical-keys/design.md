## Context

- Bevy 0.20's `InputPlugin` inits `ButtonInput<Key>`; `keyboard_input_system` presses and releases each `KeyboardInput`'s `logical_key` (bevy_input `keyboard.rs:162`). `Key` is `Clone + Eq + Hash`, not `Copy`: named unit variants, plus `Character(SmolStr)`, `Dead(Option<char>)` and `Unidentified(NativeKey)` (`keyboard.rs:816`).
- Exported params take numbers, `bool`, vectors, `Entity`, records and typed-array slices; no strings (native-functions). A slice argument must already be a typed array; the glue does not convert.
- `bevy-ts/input` has `Keys` over `ButtonInput<KeyCode>` and `KEY_CODES` in `bevy_ts::keys` (text-input). script-input-devices puts every input enum in one number space (`KeyCode` `0..`, `MouseButton` `0x1000`, `GamepadButton` `0x2000`, `GamepadAxis` `0x2100`), so its four input conditions decode one number by range.
- `bevy_ts::keys::logical_key` already maps W3C `key` strings to `Key` for the viewport; `key_name` maps back.

## Goals / Non-Goals

**Goals:**
- `ButtonInput<Key>` in TS with Bevy's methods and Bevy's `Key` names.
- Characters without a new export pattern.
- The input conditions take a `Key`, as Bevy's generic conditions do.

**Non-Goals:**
- String arguments for exported params.
- `Unidentified` keys (no stable identity to bind).
- `get_pressed` / `get_just_pressed` iteration (no list returns, as for `Touches` without an index form; a later change if a game needs it).
- Changing Bevy's release behaviour (see Risks).

## Decisions

1. A param of its own, `LogicalKeys`, beside `Keys`. Bevy reads them as two resources, `Res<ButtonInput<KeyCode>>` and `Res<ButtonInput<Key>>`; TS params are not generic, so each gets a name. `Keys` keeps its name from text-input. Alternative: one `Keys` taking both number ranges. Rejected: one param would hide which of Bevy's two resources a read hits.
2. A `Key` is one number. Named keys get a generated member each from `0x3000`; a character is `0x100000 + code point`, a dead key `0x300000 + code point`, `Dead(None)` `0x410000`. All fit a `u32` and the `f64` host slot, and sit clear of the other devices' ranges. Alternative: string arguments in `#[export]` (UTF-8 through a slice slot). Rejected for now: a new export pattern, ask-first; one code point covers keyboard characters.
3. `Key.Character(c)` and `Key.Dead(c)` are functions in the module's JS, not glue: NFC-normalise, require one code point, add the base. Rust rebuilds `Key::Character(char.to_string().into())`. Strings never reach Rust. `Key` is declared in the `.d.ts` as a namespace of named members plus the two functions, over a branded `number` type, so `keys.pressed(42)` is still allowed by type as `Keys` allows, and rejected at run time when out of range.
4. The named list lives in `bevy_ts::keys` as `NAMED_KEYS`, one macro-generated table like `KEY_CODES`, with `key_of(n) -> Option<Key>` and `key_number(&Key) -> Option<u32>`. `logical_key` and `key_name` stay as they are. A test walks `Key`'s reflected unit variants and fails on drift, and checks no range overlaps another device's.
5. Conditions: after script-input-devices lands `InputPressed { code }` and its siblings, `compile_condition` decodes the `Key` ranges too and `evaluate` reads `ButtonInput<Key>`. Ours, second, a small diff on their arms. Bevy-faithful: `input_just_pressed` is generic over `T`, and `input_just_pressed(Key::Character("p".into()))` is valid Bevy.

## Risks / Trade-offs

- [A character key's release can name a different character than its press (Shift let go first: press `"N"`, release `"n"`)] → Bevy's own behaviour; `ButtonInput<Key>` then holds `"N"` until focus loss clears it. Docs say so and point to `Keys` for held-key movement.
- [A multi-code-point `Key::Character` (some IME layouts, emoji)] → `Key.Character` throws for it; a game binding one needs string arguments, a later ask-first change.
- [Shared files with script-input-devices] → separate items in `keys.rs`, `input.rs` and the conditions; the second to land rebases. Task 4 waits on their arms.

## Migration Plan

Additive. No protocol change.

## Open Questions

None.
