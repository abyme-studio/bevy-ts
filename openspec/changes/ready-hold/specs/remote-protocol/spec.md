## ADDED Requirements

### Requirement: The screenshot method waits out a ready hold
`bevy-ts/screenshot` SHALL count no ready update while the script holds `ReadyHold` (headless-run), and SHALL capture on the third ready update in a row after the release. A hold SHALL NOT be progress: after 60 seconds with no progress while held, the call SHALL fail naming the path and `held by the script: ` with the held names. With `BEVY_TS_FIXED_STEP=1`, time SHALL NOT step while held.

#### Scenario: A shot asked while held
- **WHEN** a headless client calls `bevy-ts/screenshot` while the script holds `"data"`, and the script releases it 30 updates later
- **THEN** no capture starts while held, and the answer's PNG shows the frame after the release

#### Scenario: A shot asked under a hold never released
- **WHEN** the script holds `"data"` for ever and a client calls `bevy-ts/screenshot`
- **THEN** after the no-progress limit the call fails with an error naming the path and `held by the script: data`
