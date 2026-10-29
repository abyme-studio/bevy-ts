## 1. One mapping

- [x] 1.1 `crates/bevy-ts-viewport/src/mouse.rs` (every platform): `pointer`, `wheel` and `gesture` from `input.rs::apply_one`, each returning where the cursor moved
- [x] 1.2 `input.rs::apply_one` calls `mouse.rs`, and tells the cursor feedback where the viewer put the cursor

## 2. Headless

- [x] 2.1 `offscreen.rs`: accept `wheel` and `gesture`; still refuse any other type naming it
- [x] 2.2 `apply`: every event onto the primary window through `mouse.rs` and `keyboard.rs`; a wheel scrolls the headless pointer, a placed wheel away from it moves it first

## 3. Tests

- [x] 3.1 `crates/bevy-ts-viewport/tests/it/headless.rs`: a placed and an unplaced wheel are `MouseWheel` and the pointer's `Scroll`, and move the window's cursor
- [x] 3.2 Pinch, rotate and double tap are Bevy's gestures
- [x] 3.3 A move sets the window's cursor, a leave clears it and writes `CursorLeft`
- [x] 3.4 `touch` is refused headless; `headless_pointer.rs` refuses `touch`, not `wheel`

## 4. Docs

- [x] 4.1 `docs/picking.md` Clicking headless and `docs/viewport.md`: the headless event types, the window's cursor
- [x] 4.2 `openspec validate headless-wheel --strict` passes
