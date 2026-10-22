## ADDED Requirements

### Requirement: A built-in input module
Every engine SHALL ship the built-in module `bevy-ts/input`, registered with `ts_module`, its declarations written to `native-modules.d.ts` by `bevy-ts types`. It SHALL export two exported params (native-functions):
- `Keys`, over `Res<ButtonInput<KeyCode>>`, with `pressed(code)`, `justPressed(code)` and `justReleased(code)`, each returning `boolean`.
- `InputFocus`, over `Res<InputFocus>`, with `get()` returning the focused `Entity`, or `undefined` while nothing has focus.

They SHALL read Bevy's own resources, so they agree with Rust systems in the same frame, and SHALL NOT add a resource or a system of their own.

#### Scenario: A key read while nothing has focus
- **WHEN** a system taking `keys: Keys, focus: InputFocus` runs `if (keys.justPressed(KeyCode.KeyN) && focus.get() === undefined) count += 1`, and `N` is pressed in viewport mode with no field focused
- **THEN** `count` is 1 after that frame and stays 1 on the next

#### Scenario: A key typed into a field
- **WHEN** a field has focus and `N` is pressed
- **THEN** `keys.justPressed(KeyCode.KeyN)` is still true, as in Bevy, `focus.get()` is the field, and `count` stays 0

#### Scenario: After Escape
- **WHEN** a focused field receives `Escape` and `N` is pressed on a later frame
- **THEN** `focus.get()` is `undefined` and `count` is 1

### Requirement: KeyCode is a generated enum
`bevy-ts/input` SHALL export `KeyCode`, a TS enum with one member per Bevy 0.20 `KeyCode` variant that carries no data, named as in Rust (`KeyCode.KeyN`, `KeyCode.Enter`, `KeyCode.ArrowUp`), each a number. The TS enum and the Rust table that maps numbers back to `KeyCode` SHALL come from one generated source, and a test SHALL fail when they differ from Bevy's variants. A method given a number that is not a member SHALL throw, naming the param, the method and the number.

#### Scenario: A typo
- **WHEN** a system writes `keys.justPressed(KeyCode.KeyNN)`
- **THEN** `tsc` reports that `KeyNN` does not exist on `KeyCode`

#### Scenario: A bad number
- **WHEN** a system calls `keys.pressed(9999)`
- **THEN** the call throws naming `Keys`, `pressed` and `9999`, and the system's commands from that run are discarded
