## 1. Method

- [x] 1.1 `UpdateFrame`, noted in `First`; `pipelines_waiting` reads the render before it; a unit test records renders an update late, as a pipelined render thread does
- [x] 1.2 `Progress::Waiting { ready, at }`: capture at `METHOD_FRAME` (3) ready updates in a row, counted once per update
- [x] 1.3 Summary in the contract and `docs/build.md`

## 2. Tests

- [x] 2.1 `engine_with` returns the watchdog; tests bind it before the app; `HANG` 300 s
- [x] 2.2 The method's answer and `until_shot` wait with `common::until`
- [x] 2.3 Six copies of the slow-disk method test at once pass at load 40+
