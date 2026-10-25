## ADDED Requirements

### Requirement: A host drives the engine through a C ABI
`bevy-ts-embed` SHALL export a C ABI, declared in `crates/bevy-ts-embed/include/bevy_ts_embed.h`: `bevy_ts_create`, `bevy_ts_surface`, `bevy_ts_pause`, `bevy_ts_resume`, `bevy_ts_input`, `bevy_ts_destroy`, `bevy_ts_free_string`, `bevy_ts_abi` and `bevy_ts_version`, on an opaque `BevyTsEngine *` handle. The engine SHALL run on a thread of its own that it starts in `bevy_ts_create`. Every call SHALL be safe from any one host thread and SHALL return without waiting for a frame, except `bevy_ts_surface` with no window and `bevy_ts_destroy`, which wait as their requirements say. At most one engine SHALL be live in a process: `bevy_ts_create` while one is live SHALL fail, saying one engine is already running. `bevy_ts_abi()` SHALL return the ABI number, a `uint32_t` raised whenever a function, its arguments, the config keys or the event shapes change incompatibly, and kept across bevy-ts versions otherwise; `bevy_ts_version()` SHALL return the bevy-ts version as a static string.

#### Scenario: A second engine
- **WHEN** a host creates an engine and, before destroying it, creates another
- **THEN** the second `bevy_ts_create` returns null with the error "an engine is already running", and the first keeps playing

#### Scenario: Two versions, one ABI
- **WHEN** bevy-ts 0.4.0 and 0.4.1 change nothing in the C ABI
- **THEN** both libraries return the same `bevy_ts_abi()` and different `bevy_ts_version()`

#### Scenario: Calls do not wait for frames
- **WHEN** the engine is stuck in a long frame and the host calls `bevy_ts_input` and `bevy_ts_pause`
- **THEN** both return at once, and they apply at the next frame boundary

### Requirement: Create takes the game folder, the env and a surface
`bevy_ts_create(config_json, window, width, height, scale, on_event, user, error)` SHALL read `config_json` as `{ "game": <absolute path of the folder holding game.json>, "env": { <name>: <value> }, "headless": <bool, default false> }`. Before it returns it SHALL read and check `game.json` as a shipped game does (version, engine kind) and SHALL fail with the same messages, written to `*error` (freed with `bevy_ts_free_string`). It SHALL never look beside its executable or in the working folder. `env` SHALL take the place of the process environment for `${NAME}` in `net.allow` and for `process.env`. `window` MAY be null; `width` and `height` SHALL be physical pixels and `scale` the host's pixels per point. Assets SHALL be read from `<game>/assets/` by path, never from a platform asset store.

#### Scenario: A game from app storage
- **WHEN** a host creates an engine with `game` set to `/data/user/0/app.abyme.android/files/games/arena` and a window
- **THEN** the arena plays in that window, its levels read from that folder's `assets/levels/`

#### Scenario: A game built by another version
- **WHEN** the folder's `game.json` was built by another bevy-ts version
- **THEN** `bevy_ts_create` returns null and `*error` names both versions, and no thread is left running

#### Scenario: Env from the host
- **WHEN** `game.json` has `env: ["ABYME_URL", "ABYME_KEY"]` and `net.allow: ["${ABYME_URL}"]`, and the host passes `env: { "ABYME_URL": "https://mac.tail.ts.net:8443", "ABYME_KEY": "k", "HOME": "/x" }`
- **THEN** `process.env.ABYME_URL` and `process.env.ABYME_KEY` are those values, `process.env.HOME` is `undefined`, and a script may connect to `mac.tail.ts.net:8443`

### Requirement: The engine runs only while resumed with a surface
The engine's frame loop SHALL run only while it is resumed and has a window, or is resumed and `headless`. A new engine SHALL start resumed. `bevy_ts_pause` SHALL stop the loop after the current frame: no system runs and nothing is drawn, and the world, its scripts, connections and timers SHALL be kept. `bevy_ts_resume` SHALL start it again; the first frame's delta SHALL be capped by Bevy's `Time<Virtual>` maximum delta, so game time does not jump by the pause. A timer due during the pause SHALL fire once at the first frame after it, as timers delayed by a long frame do.

#### Scenario: Pausing stops frames
- **WHEN** a headless engine runs a system that counts frames, and the host pauses, waits 500 ms and resumes
- **THEN** the count does not rise while paused, and the first frame after resume sees a virtual delta no larger than Bevy's maximum

#### Scenario: A socket survives a pause
- **WHEN** a script holds an open `WebSocket` and the host pauses and resumes
- **THEN** the socket is still open after resume and messages received during the pause are delivered at the first frame

### Requirement: Surface changes and loss
`bevy_ts_surface(engine, window, width, height, scale)` with a window SHALL make it the engine's window at that size, replacing any previous one; the same window at a new size SHALL resize it, and the game SHALL see `WindowResized` and the new `Window` resolution. With a null window, the engine SHALL stop drawing, drop its GPU surface and every reference to the old window, and only then return, within 2 seconds; the world SHALL be kept, and drawing SHALL resume on the next window. The engine SHALL hold its own reference to a window it is given (on Android, `ANativeWindow_acquire`) and release it when done.

#### Scenario: Rotating the phone
- **WHEN** the host reports the same window at 2400 x 1080 after 1080 x 2400
- **THEN** a system reading the primary `Window` sees the new size, and the camera draws to the whole surface

#### Scenario: The app goes to the background
- **WHEN** the host calls `bevy_ts_surface` with a null window and later with a new one
- **THEN** the first call returns only after the engine stopped using the old window, and after the second the same world draws again, its entities unchanged

### Requirement: Input is one SURFACE.md event per call
`bevy_ts_input(engine, json, len)` SHALL take one `ViewportInput` event, exactly as the viewport's channel carries it in an `input` notification, with `x` and `y` in points from the surface's top left. The engine SHALL queue it and apply it as the viewport does (viewport-input), on the embedding's window. A malformed event SHALL be logged and dropped, never fatal.

#### Scenario: A tap
- **WHEN** the host sends `touch` `start` and `touch` `end` with one touch at a point over a button
- **THEN** the game sees `TouchInput` `Started` and `Ended` there, and the button's `Interaction` reads `Pressed` on a frame between

#### Scenario: Junk
- **WHEN** the host sends `{"type":"nope"}`
- **THEN** the log has a warning naming the event, and the game keeps playing

### Requirement: Events come back as JSON on one callback
The engine SHALL call `on_event(user, json, len)` from its own thread with one JSON object per call: `{"event":"started","inputs":[...],"js":"v8"}` after the first frame is presented (or the first frame runs, headless), `inputs` being the input types it reads; `{"event":"log","level","message","file"?,"line"?}` for each script log line and error, mapped through source maps; `{"event":"ime","kind":"enabled"|"disabled"}` when the game wants text input or no longer does; and `{"event":"exited","code","message"?}` once, last, when the engine stops for any reason. The host SHALL NOT call `bevy_ts_destroy` or `bevy_ts_surface` with a null window from inside the callback; other calls MAY be made from it.

#### Scenario: A script error reaches the host
- **WHEN** a system in `src/enemy.tsx` throws
- **THEN** the host gets a `log` event with level `error`, `file` `src/enemy.tsx` and the TS line

#### Scenario: The game asks for text
- **WHEN** the game focuses a text field
- **THEN** the host gets `{"event":"ime","kind":"enabled"}`

### Requirement: Destroy stops and joins
`bevy_ts_destroy(engine)` SHALL stop the loop, close every connection with 1001, drop the world and the JS engine, release the window, send `exited`, join the engine thread and return, within 5 seconds. The handle SHALL be invalid after it. A new engine MAY be created once it returns.

#### Scenario: Open, close, open
- **WHEN** a host creates a headless engine, destroys it and creates another on the same game
- **THEN** the second plays from the start, and the first sent `exited` with code 0 before `bevy_ts_destroy` returned
