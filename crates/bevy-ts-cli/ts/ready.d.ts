// `ReadyHold` holds a run's ready frames while the script waits for data Bevy cannot see (a reply
// over a WebSocket). It is bevy-ts's own: Bevy has no such hold. While any name is held, a shot
// (`BEVY_TS_SCREENSHOT`, `bevy-ts/screenshot`) counts no ready frame and `BEVY_TS_FIXED_STEP=1` time
// does not step. Hold in a `Startup` system, so no step comes before it; release in the system that
// applies the data. A hold is no progress: one never released fails the shot after 60 s, naming it
// (`held by the script: <names>`). With no shot and no fixed step it changes nothing.
export {};
