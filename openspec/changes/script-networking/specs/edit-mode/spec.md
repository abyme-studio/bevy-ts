## MODIFIED Requirements

### Requirement: Edit mode runs no game code
While in edit mode, a TS trigger or lifecycle event that would run a TS observer SHALL be dropped, not queued for play. An edit made in edit mode SHALL therefore fire no TS observer when play resumes. No script timer SHALL fire and no socket event SHALL be delivered in edit mode, and leaving play SHALL close the game's connections; script-networking says how.

#### Scenario: Dragging an enemy in edit
- **WHEN** a TS observer watches `Insert<Transform>` on enemies, and in edit mode the operator drags enemy `b` and then sets `play`
- **THEN** the observer does not run for the drag, before or after play

#### Scenario: A timer set before edit
- **WHEN** a playing script sets `setTimeout(f, 100)` and a client sets `edit` at once, waits one second, then sets `play`
- **THEN** `f` does not run in edit mode and runs at the first frame of play
