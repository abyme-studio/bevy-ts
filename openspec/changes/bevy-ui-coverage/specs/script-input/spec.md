## ADDED Requirements

### Requirement: Focus visibility and directional navigation in bevy-ts/input
`bevy-ts/input` SHALL also export `InputFocusVisible`, over `Res<InputFocusVisible>`, with `get()` returning `boolean`, and `DirectionalNavigation`, over Bevy's `DirectionalNavigation` system param, with `navigate(direction)` taking a `CompassOctant` (`"north"`, `"north-east"`, ..., exported as a TS enum) and returning the newly focused `Entity`, or `undefined` when there is no focus or no neighbour. They SHALL read and write Bevy's own resources.

#### Scenario: Arrow keys move focus
- **WHEN** a TS system runs `if (keys.justPressed(KeyCode.ArrowDown)) nav.navigate(CompassOctant.South)` over a column of `AutoDirectionalNavigation` buttons, one focused, and `ArrowDown` is pressed in viewport mode
- **THEN** `InputFocus` holds the button below

### Requirement: Every engine has tab and directional navigation
Every engine, project engine and shipped game SHALL add Bevy's `TabNavigationPlugin` and `DirectionalNavigationPlugin`. Tab navigation SHALL work in viewport mode and in a window; under `--headless`, which has no primary window, Tab SHALL do nothing.

#### Scenario: Tab in viewport mode
- **WHEN** an engine in viewport mode mounts two `<button tabIndex={0}>`s and the viewer sends `Tab`
- **THEN** `InputFocus` holds the first button

#### Scenario: Play, stop, play
- **WHEN** an edit-mode engine plays, stops and plays again, then the viewer sends `Tab` once
- **THEN** focus moves one step, not two
