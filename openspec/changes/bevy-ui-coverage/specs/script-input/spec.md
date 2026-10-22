## ADDED Requirements

### Requirement: Focus visibility and directional navigation in bevy-ts/input
`bevy-ts/input` SHALL also export `InputFocusVisible`, over `Res<InputFocusVisible>`, with `get()` returning `boolean`, and `DirectionalNavigation`, over Bevy's `DirectionalNavigation` system param, with `navigate(direction)` taking a `CompassOctant` (`"north"`, `"north-east"`, ..., exported as a TS enum) and returning the newly focused `Entity`, or `undefined` when there is no focus or no neighbour. They SHALL read and write Bevy's own resources.

#### Scenario: Arrow keys move focus
- **WHEN** a project with `"directionalNavigation": true` has a TS system that runs `if (keys.justPressed(KeyCode.ArrowDown)) nav.navigate(CompassOctant.South)` over a column of `AutoDirectionalNavigation` buttons, one focused, and `ArrowDown` is pressed in viewport mode
- **THEN** `InputFocus` holds the button below

### Requirement: A project opts into tab and directional navigation
`bevy-ts.json` SHALL accept `tabNavigation` and `directionalNavigation`, bools, default `false`, through the same path as `meshPicking`. `tabNavigation: true` SHALL add Bevy's `TabNavigationPlugin`, and `directionalNavigation: true` Bevy's `DirectionalNavigationPlugin`, under `bevy-ts dev` in a window, `--viewport` and `--headless` alike, and in a built game. When a switch is `false` or absent the engine SHALL NOT add its plugin. Neither switch SHALL be an engine hash input or start `cargo`. A value that is not a bool SHALL fail `bevy-ts dev` with a message naming the field and `bevy-ts.json`. Tab navigation SHALL work in viewport mode and in a window; under `--headless`, which has no primary window, Tab SHALL do nothing.

#### Scenario: Tab in viewport mode
- **WHEN** a project with `"tabNavigation": true` runs in viewport mode, mounts two `<button tabIndex={0}>`s and the viewer sends `Tab`
- **THEN** `InputFocus` holds the first button

#### Scenario: Off by default
- **WHEN** a project's `bevy-ts.json` has neither switch, runs in viewport mode, mounts two `<button tabIndex={0}>`s and the viewer sends `Tab`
- **THEN** `InputFocus` stays empty, and the app has no `DirectionalNavigationMap`

#### Scenario: A wrong type
- **WHEN** `bevy-ts.json` holds `"tabNavigation": "yes"`
- **THEN** `bevy-ts dev` exits 1 with a message naming `tabNavigation` and `bevy-ts.json`

#### Scenario: Built and played
- **WHEN** a project with both switches `true` is built and its folder played
- **THEN** `game.json` holds both as `true` and the played app has both plugins

#### Scenario: Play, stop, play
- **WHEN** an edit-mode engine with `"tabNavigation": true` plays, stops and plays again, then the viewer sends `Tab` once
- **THEN** focus moves one step, not two

### Requirement: Navigation used with its switch off is named
When a `tabIndex` or `tabGroup` prop mounts and `TabNavigationPlugin` was not added, the engine SHALL log one warning per process that names the entity and says to set `"tabNavigation": true` in `bevy-ts.json`; it SHALL NOT warn for a `TabIndex` Bevy writes itself (a `<tab>`), nor when a native plugin added `TabNavigationPlugin` itself. `DirectionalNavigation.navigate` SHALL throw, naming `"directionalNavigation": true`, when the app has no `DirectionalNavigationMap`.

#### Scenario: A tab index with the switch off
- **WHEN** a game without `tabNavigation` mounts two `<button tabIndex={0}>`s
- **THEN** one warning naming the first button and `"tabNavigation": true` is logged, and no second

#### Scenario: navigate with the switch off
- **WHEN** a TS system calls `nav.navigate(CompassOctant.South)` in a game without `directionalNavigation`
- **THEN** the call throws an error naming `"directionalNavigation": true`
