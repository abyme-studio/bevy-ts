## MODIFIED Requirements

### Requirement: Mouse motion and scroll as resources
Mouse motion and scroll SHALL be read through `Res<T>` of Bevy's own resources, with no param of `bevy-ts/input`: `Res<AccumulatedMouseMotion>` (`delta`), and `Res<AccumulatedMouseScroll>` (`unit`, `delta`), through the `Res<T>` path of Rust resources. `MouseScrollPixelsPerLine` SHALL be read with `Res<MouseScrollPixelsPerLine>` and written with `ResMut<MouseScrollPixelsPerLine>`: Bevy 0.20 reflects it without `reflect(Resource)`, so bevy-ts registers its `ReflectResource`.

#### Scenario: Mouse look
- **WHEN** Bevy receives `MouseMotion { delta: (3, -2) }` and a system reads `motion: Res<AccumulatedMouseMotion>`
- **THEN** `motion.delta` is `Vec2(3, -2)` that frame and `Vec2(0, 0)` the next

#### Scenario: Pixels per scroll line
- **WHEN** a system reads `Res<MouseScrollPixelsPerLine>` and another sets it to 40 with `ResMut<MouseScrollPixelsPerLine>`
- **THEN** the first reads Bevy's default, 100, and Bevy's `MouseScrollPixelsPerLine` is 40 after the frame
