## ADDED Requirements

### Requirement: Logical keys
`bevy-ts/input` SHALL export the param `LogicalKeys`, over `Res<ButtonInput<Key>>`, with `pressed(key)`, `justPressed(key)` and `justReleased(key)`, each returning `boolean`, as `Keys` does for `KeyCode`. It SHALL read Bevy's own resource, which Bevy's `keyboard_input_system` fills from each `KeyboardInput`'s `logical_key`, so a character key reads as the character the layout and modifiers typed: Shift with `N` is `Key.Character("N")`, not `"n"`. With no such resource, it SHALL read as no key down.

#### Scenario: A key bound by what it types
- **WHEN** a system taking `keys: LogicalKeys` runs `if (keys.justPressed(Key.Character("z"))) count += 1`, and the viewer sends `key` down with `code: "KeyY"`, `key: "z"` (a German layout)
- **THEN** `count` is 1, and `Keys.justPressed(KeyCode.KeyZ)` is false that frame

#### Scenario: A named key
- **WHEN** the viewer sends `key` down for `Enter`
- **THEN** `keys.justPressed(Key.Enter)` is true that frame and `keys.pressed(Key.Enter)` stays true until the key up

#### Scenario: Case follows the layout
- **WHEN** the viewer sends `ShiftLeft` down, then `KeyN` down with `key: "N"`
- **THEN** `keys.pressed(Key.Character("N"))` is true and `keys.pressed(Key.Character("n"))` is false

### Requirement: Key is one number
`bevy-ts/input` SHALL export `Key`, Bevy 0.20's `Key` as a number in the input number space (script-input-devices):
- one member per `Key` variant that carries no data, named as in Rust (`Key.Enter`, `Key.Escape`, `Key.ArrowUp`, `Key.Shift`), numbered from `0x3000`;
- `Key.Character(c)`, `0x100000` plus the code point of `c` after NFC normalisation;
- `Key.Dead(c)`, `0x300000` plus the code point of `c`, and `Key.Dead()` for `Dead(None)`, `0x300000 + 0x110000`.

`Key.Character` and `Key.Dead` SHALL be the module's JS, so no string crosses into Rust; a `c` that is not exactly one code point after normalisation SHALL throw, naming `Key.Character` or `Key.Dead` and `c`. `Unidentified` keys SHALL have no member. The named list and the Rust table that maps numbers back to `Key` SHALL come from one generated source beside `KEY_CODES`, and a test SHALL fail when it differs from Bevy's data-free variants or when the ranges overlap another device's. A `LogicalKeys` method given a number outside the `Key` ranges SHALL throw, naming `LogicalKeys`, the method and the number.

#### Scenario: A character with two code points
- **WHEN** a system calls `Key.Character("ab")`
- **THEN** it throws naming `Key.Character` and `"ab"`

#### Scenario: A decomposed character
- **WHEN** a system calls `Key.Character("é")`
- **THEN** it returns the same number as `Key.Character("é")`

#### Scenario: A KeyCode given to LogicalKeys
- **WHEN** a system calls `keys.pressed(KeyCode.KeyN)` on a `LogicalKeys`
- **THEN** the call throws naming `LogicalKeys`, `pressed` and the number

### Requirement: Input conditions take logical keys
`inputPressed`, `inputJustPressed`, `inputJustReleased` and `inputToggleActive` SHALL also take a `Key` and evaluate it in Rust against `ButtonInput<Key>`, chosen by the number's range as for `KeyCode` and `MouseButton`, as Bevy's generic `input_just_pressed::<Key>` does.

#### Scenario: Pause on the key that types p
- **WHEN** `systems(pause).runIf(inputJustPressed(Key.Character("p")))` is registered and the viewer sends `key` down with `key: "p"`
- **THEN** `pause` runs on that frame only, and the condition makes no JS call
