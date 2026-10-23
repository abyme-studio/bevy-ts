## 1. Key numbers

- [ ] 1.1 `crates/bevy-ts/src/keys.rs`: `NAMED_KEYS` (every data-free `Key` variant, Rust name, number from `0x3000`), the character and dead-key bases, `key_of(n) -> Option<Key>` and `key_number(&Key) -> Option<u32>`
- [ ] 1.2 Tests: every named key and a sample of characters round-trip; reflected unit variants equal `NAMED_KEYS`; the `Key` ranges overlap no other device's

## 2. `LogicalKeys`

- [ ] 2.1 `crates/bevy-ts-cli/src/input.rs`: `LogicalKeysParam` over `Option<Res<ButtonInput<Key>>>`, `#[export(ts = "LogicalKeys")]` with `pressed`, `justPressed`, `justReleased`, throwing outside the `Key` ranges; `ts_param` it
- [ ] 2.2 The module's generated JS: `Key` with the named members, `Key.Character(c)` and `Key.Dead(c?)` (NFC, one code point, else throw naming the function and `c`)
- [ ] 2.3 The module's `.d.ts`: `Key` as a branded number type plus a namespace of members and the two functions; a type test that `Key.Entr` is an error

## 3. Tests, in viewport mode through `bevy-ts/viewport_input`

- [ ] 3.1 `code: "KeyY"`, `key: "z"`: `LogicalKeys.justPressed(Key.Character("z"))` true, `Keys.justPressed(KeyCode.KeyZ)` false
- [ ] 3.2 `Enter` down and up: `justPressed`, `pressed`, `justReleased` on `Key.Enter`
- [ ] 3.3 Shift then `N` with `key: "N"`: `"N"` pressed, `"n"` not
- [ ] 3.4 `Key.Character("ab")` throws; `Key.Character("é")` equals `Key.Character("é")`; `keys.pressed(KeyCode.KeyN)` on `LogicalKeys` throws naming it

## 4. Conditions (after script-input-devices lands its arms)

- [ ] 4.1 `crates/bevy-ts/src/systems.rs`: the four input condition arms decode the `Key` ranges and read `ButtonInput<Key>`
- [ ] 4.2 Test: `runIf(inputJustPressed(Key.Character("p")))` runs on the press frame only, no JS call for the condition; `inputToggleActive(false, Key.Escape)` flips per press

## 5. Docs

- [ ] 5.1 `docs/native.md` input section: `LogicalKeys` and `Key`, when to use it over `Keys`, the release caveat, and the coverage row for `ButtonInput<Key>` set to covered
- [ ] 5.2 `openspec validate logical-keys --strict` passes
