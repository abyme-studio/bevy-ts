## 1. Fix

- [x] 1.1 `headless.rs`: the highest-order offscreen camera holds `IsDefaultUiCamera` unless the game placed its own

## 2. Tests and docs

- [x] 2.1 `tests/headless_ui.rs`: the arena's Hud lays out at the image's size and its button's blue is in the screenshot
- [x] 2.2 `tests/headless_ui.rs`: a game's own `IsDefaultUiCamera` wins, and headless takes its marker back
- [x] 2.3 `docs/build.md` and the `headless-run` spec
