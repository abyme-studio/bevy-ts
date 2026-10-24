## MODIFIED Requirements

### Requirement: Input events are protocol types carried on the channel
Viewport input SHALL be defined in `protocol/bevy-ts.openrpc.json` as `ViewportInput` events: `pointer` (`kind`: `move`, `down`, `up`, `enter`, `leave`; `x`, `y` in points from the hole's top left; `button`: `left`, `right`, `middle`, `back`, `forward`; `clicks`; `dx`, `dy` raw motion; `buttons` held; `mods`, with `shift`, `ctrl`, `alt`, `meta` and `caps`), `wheel` (`dx`, `dy`, `unit`: `line` or `pixel`, `phase`, `momentum`, `mods`), `gesture` (`kind`: `pinch`, `rotate`, `double_tap`; `delta`; `x`, `y`; `phase` as `wheel`'s), `key` (`state`: `down`, `up`; `code` as a W3C `KeyboardEvent.code`; `key` logical key; `text`; `repeat`; `mods`), `ime` (`kind`: `enabled`, `preedit`, `commit`, `disabled`; `text`; `cursor` as a UTF-8 byte range), `touch` (`kind`: `start`, `move`, `end`, `cancel`; `touches`, every touch down after the event, each `id`, `x`, `y` in points from the hole's top left, `rx`, `ry` radii in points and `force` 0-1; `mods`; `t`, when it happened in ms on the viewer device's own clock (protocol 1.20), only compared within one stream and not required to rise strictly), `gamepad` (`kind`: `connected`, `state`, `disconnected`; `index`; `id`; `buttons` as `{ pressed, value }` and `axes`, both in the W3C Gamepad API's standard mapping), `focus` (`on`) and `visible` (`on`). On the viewport's Mach channel the viewer SHALL send each event as one `input` notification whose params are that single event, one event per message, in the order they happened. `bevy-ts/viewport_input` (an array of events) SHALL accept the same events over HTTP, for tests and for clients without the channel. An embedding host SHALL pass the same events one per `bevy_ts_input` call (engine-embedding), with `x` and `y` from the top left of its surface, and they SHALL be applied to the embedding's window exactly as channel events are to the viewport's. The surface protocol is shared with abyme's browser host, so `buttons`, `caps` and a gesture's `x`, `y` and `phase` (protocol 1.9) SHALL be accepted, and the engine MAY ignore them.

#### Scenario: Input on the channel
- **WHEN** the viewer sends a pointer `down` and then `up` as two `input` notifications
- **THEN** the engine applies them as two events in that order, and a message holding an array or another method name for input is rejected as malformed

#### Scenario: A test drives the game over HTTP
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` down and up of `KeyW`
- **THEN** a system reading `ButtonInput<KeyCode>` sees `KeyW` just pressed on one frame and just released on a later one

#### Scenario: A touch from abyme's stream carries its time
- **WHEN** the viewer sends `touch` `start` with `t: 1234.5`, and another without `t`
- **THEN** both are accepted and become `TouchInput` `Started`, and the engine MAY ignore `t`

#### Scenario: A touch from an embedding host
- **WHEN** an embedding host passes `touch` `start` and then `end` for one touch, each as one `bevy_ts_input` call
- **THEN** the game sees `TouchInput` `Started` and then `Ended` on the embedding's window, as it would from the viewport's channel
