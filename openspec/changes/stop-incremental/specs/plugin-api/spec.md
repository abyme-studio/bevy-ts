## ADDED Requirements

### Requirement: Start notification
When the game starts in a world (the first Play after an `--edit` start or a Stop, a step before the game started, or the first frame of a game booted in play), bevy-ts SHALL notify every plugin that registered for it once, before any `Startup` system runs. It SHALL NOT notify again until a restart or a Stop makes the game not started.

#### Scenario: Play then step
- **WHEN** a plugin registered for the start, an `--edit` engine steps one frame, then a client sets `play`
- **THEN** the plugin was notified once, before the step's `Startup` systems

#### Scenario: Again after a Stop
- **WHEN** a client sets `play`, `edit` and `play`
- **THEN** the plugin was notified twice, once per Play
