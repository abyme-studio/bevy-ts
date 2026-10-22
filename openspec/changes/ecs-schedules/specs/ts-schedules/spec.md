## ADDED Requirements

### Requirement: Every main and fixed schedule
`bevy-ts` SHALL export a `Schedule` constant for each of Bevy 0.20's main and fixed schedules that a Bevy app adds systems to: `PreStartup`, `Startup`, `PostStartup`, `First`, `PreUpdate`, `RunFixedMainLoop`, `Update`, `SpawnScene`, `PostUpdate`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` and `FixedLast`, with Bevy's names. `app.addSystems(schedule, ...)` SHALL run those systems in that Bevy schedule, in a bevy-ts stage: one exclusive system in set `BevyTsSet::<Stage>` of that schedule. `StateTransition`, `Main` and `FixedMain` SHALL NOT be offered: bevy-ts drives transitions itself (ts-states), and the other two only run schedules.

The stages of `PreUpdate`, `Update`, `PostUpdate` and `FixedUpdate` SHALL exist from startup, as now. Every other stage SHALL be made the first time a generation registers a system in its schedule, as one schedule edit applied with that generation at the frame boundary, and SHALL stay for the life of the app, skipped while empty. A schedule no generation has named SHALL have no bevy-ts stage. The `First` stage SHALL run after the generation apply and the runtime pump; the `RunFixedMainLoop` stage SHALL run in `RunFixedMainLoopSystems::BeforeFixedMainLoop` unless the system orders against `bevy:fixed-main-loop` (native-ordering). A system in a fixed schedule SHALL run once per fixed step and read `Time<Fixed>` as `Res<Time>`.

#### Scenario: A system in Last
- **WHEN** `app.addSystems(Last, tally)` is registered and the file loads
- **THEN** from that frame `tally` runs once per frame after every `PostUpdate` system, and before the load the `Last` schedule had no bevy-ts system

#### Scenario: Fixed schedules in order
- **WHEN** systems are registered in `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` and `FixedLast`, and a frame runs two fixed steps
- **THEN** each runs twice, and within each step in that order

#### Scenario: An unused schedule costs nothing
- **WHEN** no generation registers a system in `SpawnScene`
- **THEN** the `SpawnScene` schedule contains no bevy-ts system

### Requirement: PreStartup and PostStartup order inside TS startup
`PreStartup` and `PostStartup` systems SHALL run with `Startup` systems, at the generation apply where TS startup runs today, once per system identity per world (hot-reload): every due `PreStartup` system, then every due `Startup` system, then every due `PostStartup` system, each list's commands applied before the next list runs. They SHALL add no per-frame stage.

#### Scenario: Startup order
- **WHEN** `a` is in `PostStartup`, `b` in `Startup` and `c` in `PreStartup`, and `c` inserts a resource that `b` reads
- **THEN** on the first play they run `c`, `b`, `a`, each once; `b` sees the resource; a reload runs none of them again

### Requirement: Custom schedules
`class Waves extends ScheduleLabel {}` SHALL declare a schedule label, named like a component (a Rust-style path from its module, such as `game::Waves`, or `static path`). `app.initSchedule(Waves)` SHALL create the schedule if absent. `app.insertScheduleAfter(after, Waves)` and `app.insertScheduleBefore(before, Waves)` SHALL insert it into `MainScheduleOrder`, `app.insertFixedScheduleAfter/Before` into `FixedMainScheduleOrder`, and `app.insertStartupScheduleAfter/Before` into the startup labels, as Bevy's methods of those names. `app.addSystems(Waves, ...)` SHALL create the schedule if absent and run the systems in a bevy-ts stage made in it on first use. In Rust the label SHALL be `bevy_ts::TsSchedule` with the declared path, so a Rust plugin can add systems to it or run it.

An insertion SHALL be applied at the frame boundary with its generation. A later generation that inserts the label at another position SHALL move it in that one edit; one that no longer inserts it SHALL remove it from the order. The schedule itself and its stage SHALL stay for the life of the app. A label inserted relative to a schedule that is not in the order SHALL fail the load naming both.

#### Scenario: A schedule after Update
- **WHEN** a module declares `Waves`, calls `app.insertScheduleAfter(Update, Waves)` and `app.addSystems(Waves, spawnWave)`
- **THEN** each frame `spawnWave` runs after every `Update` system and before `PostUpdate`

#### Scenario: Moving it on reload
- **WHEN** the call changes to `app.insertScheduleBefore(Update, Waves)` and the file is saved
- **THEN** from the next frame `spawnWave` runs before `Update`, and `Waves` appears once in `MainScheduleOrder`

### Requirement: Running a schedule from commands
`commands.runSchedule(label)` SHALL run the schedule once when the caller's commands are applied, as Bevy's `Commands::run_schedule`, nested inside the current stage, before the next system of the stage runs. `label` SHALL be a TS `ScheduleLabel` class or a schedule a Rust crate exposed (`schedule("port:name")`). Running a schedule that is already running SHALL log an error naming the schedule and the system, and skip it.

#### Scenario: A schedule run on demand
- **WHEN** `Waves` is created with `app.initSchedule(Waves)` and not inserted, holds `spawnWave`, and an `Update` system calls `commands.runSchedule(Waves)` once
- **THEN** `spawnWave` runs once, before the next `Update` system, and not on later frames

#### Scenario: Re-entry
- **WHEN** a system in `Waves` calls `commands.runSchedule(Waves)`
- **THEN** the nested run is skipped and an error names `game::Waves` and the system

### Requirement: Rust exposes schedules under stable names
A crate SHALL be able to expose a Bevy schedule label to TS with `app.ts_schedule("port:name", Label)`, with the naming and uniqueness rules of `ts_set` (native-ordering). TS SHALL name it with `schedule("port:name")`, usable wherever a `Schedule` is. Exposed schedules SHALL be listed in the generated `native.d.ts` as the `NativeSchedule` string union.

#### Scenario: A port's schedule
- **WHEN** a port calls `app.ts_schedule("net:tick", NetTick)` and a module calls `app.addSystems(schedule("net:tick"), sync)`
- **THEN** `sync` runs each time the port runs `NetTick`, and `"net:tick"` is in `NativeSchedule`

### Requirement: One-shot systems
`app.registerSystem(fn)` SHALL register an exported function as a one-shot system, read by the compile step like any system, and return its `SystemId`. `commands.runSystem(target)` SHALL run it once when the caller's commands are applied, nested inside the current stage, as Bevy's `Commands::run_system`; `target` SHALL be the `SystemId` or the function itself. `commands.runSystemWith(target, input)` SHALL do the same with an input: the system's first param is `In<E>`, `E` an `Event` class, and `input` is an `E` value, encoded as `commands.trigger` encodes events. Running a function that was not registered SHALL throw "register it with app.registerSystem" with its name. A one-shot system SHALL keep its `Local`, message cursors and last-run tick per identity, as any system. Rust SHALL run one by identity with the command `bevy_ts::RunTsSystem::new("game::waves#spawnWave")`, and with an input with `RunTsSystem::with(identity, bytes)`.

#### Scenario: Run from a system
- **WHEN** `spawnWave` is registered and an `Update` system calls `commands.runSystem(spawnWave)` twice in one run
- **THEN** `spawnWave` runs twice after that system's commands apply, before the next system

#### Scenario: With an input
- **WHEN** `function spawnAt(at: In<SpawnAt>, commands: Commands)` is registered and a system calls `commands.runSystemWith(spawnAt, SpawnAt.of({ x: 3 }))`
- **THEN** `spawnAt` runs once and reads `at.x === 3`

#### Scenario: Not registered
- **WHEN** a system calls `commands.runSystem(other)` and `other` was never passed to `registerSystem`
- **THEN** the call throws "register it with app.registerSystem" naming `other`

### Requirement: Schedules in the protocol
`bevy-ts/systems` SHALL report each system's `stage` as one of `preStartup`, `startup`, `postStartup`, `first`, `preUpdate`, `runFixedMainLoop`, `update`, `spawnScene`, `postUpdate`, `last`, `fixedFirst`, `fixedPreUpdate`, `fixedUpdate`, `fixedPostUpdate`, `fixedLast`, `onEnter`, `onExit`, `observer`, `oneShot` or `schedule`; for `schedule`, a new optional `schedule` field SHALL name the label (`game::Waves`, `net:tick`). These SHALL be additions in the next free protocol minor at landing.

#### Scenario: Listing a custom-schedule system
- **WHEN** a client calls `bevy-ts/systems` while `spawnWave` runs in `Waves`
- **THEN** its entry has `stage: "schedule"` and `schedule: "game::Waves"`
