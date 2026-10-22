## ADDED Requirements

### Requirement: A headless pointer over HTTP
Under `bevy-ts dev --headless`, on every platform, `bevy-ts/viewport_input` SHALL be served, registered by `bevy-ts-viewport` as under `--viewport`, with the same params and result. Its `pointer` events SHALL drive one Bevy custom pointer (`PointerId::Custom`) whose location targets the headless offscreen image, with `x`, `y` in that image's pixels (scale 1): `move` and `enter` move it, `down` and `up` press and release `left`, `right` or `middle`, and `leave` cancels it and takes it off the image, so it hovers nothing until it moves again. A `down` or `up` away from the pointer first moves it there, a frame before the press or release. `back` and `forward` SHALL be accepted and ignored. The engine SHALL apply queued pointer events in order, at most one per frame, in `bevy_picking`'s input step, so hover from a `move` is in place before a following `down`. An event of any other type SHALL refuse the whole request with an error naming that type, and queue nothing. A `viewport` other than none or 0 SHALL be refused. `bevy-ts/hello` SHALL still report `viewport: false`.

#### Scenario: A test clicks a mesh headless
- **WHEN** a test runs `bevy-ts dev --headless` on a project with `"meshPicking": true` and a cube with `onPointerClick`, and posts `bevy-ts/viewport_input` with `move`, `down` and `up` (`button: left`) at the cube's pixel in the offscreen image
- **THEN** the answer is `{ queued: 3 }` and the handler fires once with the cube's entity

#### Scenario: A key headless
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` event under `--headless`
- **THEN** the call fails with an error naming `key`, and no event is applied

#### Scenario: On Linux
- **WHEN** the headless click test runs on Linux
- **THEN** it passes as on macOS
