# script-gizmos Specification

## Purpose
Bevy's gizmos from TS: the `Gizmos` param with every drawing method and builder setting, config groups and their reflected config, primitives and curves, and retained `GizmoAsset`s shown by `<Gizmo />`, each drawing what the same Rust calls draw.

## Requirements

### Requirement: Gizmos is a system param
Every engine SHALL export Bevy's `Gizmos` to TS as a system param, one export per registered config group: a TS system param written `Gizmos` SHALL be `Gizmos<DefaultGizmoConfigGroup>`, and one written `Gizmos<G>` SHALL draw into group `G`'s storage. Drawing SHALL be deferred: the run's lines SHALL reach Bevy's `GizmoStorage` after the run, with the system's other deferred params, and a run that throws SHALL draw nothing. A TS system in `FixedUpdate` SHALL draw into Bevy's fixed context, as a Rust system does. A call through a `Gizmos` object the running system does not take as a param SHALL throw naming the param and method.

#### Scenario: A line
- **WHEN** a TS system taking `gizmos: Gizmos` calls `gizmos.line([0, 0, 0], [1, 0, 0], "#f00")`
- **THEN** after the run `GizmoStorage<DefaultGizmoConfigGroup, ()>` holds that segment in red, as the Rust call `gizmos.line(Vec3::ZERO, Vec3::X, RED)` leaves it

#### Scenario: A throw drops the drawing
- **WHEN** a TS system draws a line and then throws
- **THEN** the storage holds no line from that run

#### Scenario: A light gizmo group
- **WHEN** a TS system taking `gizmos: Gizmos<LightGizmoConfigGroup>` draws a line
- **THEN** the line is in `GizmoStorage<LightGizmoConfigGroup, ()>` and not in the default group's

#### Scenario: Kept and called from another system
- **WHEN** system A stores its `gizmos` object in a module variable and system B, which takes no `Gizmos`, calls `line` on it
- **THEN** the call throws `Gizmos.line: this system does not take Gizmos ...` and nothing is drawn

### Requirement: Every drawing method draws what Rust draws
Every drawing method of Bevy 0.20's `GizmoBuffer` SHALL be a method of TS `Gizmos`, named as Bevy names it in camelCase (`line2d`, `shortArc3dBetween`, `roundedCuboid`), taking Bevy's arguments in Bevy's order and shape: `line`, `lineGradient`, `ray`, `rayGradient`, `linestrip`, `lineloop`, `linestripGradient`, `rect`, `cube`, `aabb3d`, their 2D forms, `cross`, `cross2d`, `circle`, `circle2d`, `ellipse`, `ellipse2d`, `sphere`, `arc2d`, `arc3d`, `shortArc3dBetween`, `longArc3dBetween`, `shortArc2dBetween`, `longArc2dBetween`, `arrow`, `arrow2d`, `axes`, `axes2d`, `grid`, `grid2d`, `grid3d`, `roundedRect`, `roundedRect2d`, `roundedCuboid`, `text`, `text2d`, `textSections`, `textSections2d`, and (stage 2) `primitive2d`, `primitive3d`, `curve2d`, `curve3d`, `curveGradient2d`, `curveGradient3d`. Each call SHALL leave the gizmo buffer exactly as the same Rust call leaves it. Bevy's global `gizmo()` SHALL have no TS form.

#### Scenario: Every method against Rust
- **WHEN** the test calls each method from TS and the same method from Rust with equal arguments
- **THEN** the two buffers hold equal positions and colours, vertex for vertex, and a method with no row in the test's table fails the test

#### Scenario: A strip from a Float32Array
- **WHEN** a TS system calls `gizmos.linestrip(points, "#0f0")` with a `Float32Array` of 3,000 floats
- **THEN** the buffer holds a 1,000-point strip, drawn in one host call

### Requirement: Builder settings chain as in Rust
A method that returns a builder in Bevy SHALL return an object typed as that builder, whose methods are Bevy's settings for it in camelCase (`resolution`, `withTipLength`, `withDoubleEnd`, `skew`, `skewX`, `outerEdges`, `cornerRadius`, `arcResolution`, `edgeRadius`, and the primitives' settings). A chain SHALL leave the buffer as the same Rust chain leaves it. A setting called after any other drawing call, or from another system, SHALL throw naming the builder and setting and draw nothing more.

#### Scenario: A finer circle
- **WHEN** a TS system calls `gizmos.circle([0, 0, 0], 1, "#fff").resolution(64)`
- **THEN** the buffer holds one 64-segment circle and no 32-segment one

#### Scenario: A late setting
- **WHEN** a TS system keeps the builder from `circle(...)`, draws a line, then calls `.resolution(64)` on the builder
- **THEN** the call throws naming `EllipseBuilder.resolution`, and the buffer holds the default circle and the line

### Requirement: Config groups come from Rust
The engine SHALL expose Bevy's `DefaultGizmoConfigGroup`, `AabbGizmoConfigGroup`, `FrustumGizmoConfigGroup`, `SkinnedMeshBoundsGizmoConfigGroup` and `LightGizmoConfigGroup` from `bevy-ts/gizmos`. A Rust crate SHALL expose its own group with `app.ts_gizmo_group::<G>()`; a group exposed but never registered with Bevy's `init_gizmo_group` SHALL fail the app naming it. bevy-ts's editor groups SHALL NOT be exposed. TS SHALL NOT define a config group.

#### Scenario: A project group
- **WHEN** a project crate calls `app.init_gizmo_group::<PathGizmos>().ts_gizmo_group::<PathGizmos>()` and a TS system takes `gizmos: Gizmos<PathGizmos>`
- **THEN** it type-checks and draws into `PathGizmos`'s storage

#### Scenario: An editor group
- **WHEN** a TS system takes `gizmos: Gizmos<EditorGizmos>`
- **THEN** `tsc` reports the type missing and the load fails naming it

### Requirement: Config is read and written through reflection
`gizmos.config()` and `gizmos.configExt()` SHALL return the group's `GizmoConfig` and group value as Bevy's reflected JSON (docs/reflection.md). A TS system taking `ResMut<GizmoConfigStore>` SHALL read a group with `store.config(Group)`, giving `[config, ext]`, and write it with `store.insert(Group, config, ext)`, each written through `bevy_ts::reflect_value` over the current value, so keys left out keep their value. Every field SHALL be reachable this way: `enabled`, `line.width`, `line.perspective`, `line.style`, `line.joints`, `line.animation_offset`, `depth_bias`, `render_layers`, and each group's own fields. A bad field or value SHALL throw naming the group and field and change nothing. `Res<GizmoConfigStore>` SHALL read only. Bevy's `config_mut` SHALL have no TS form.

#### Scenario: Dashed and wide
- **WHEN** a TS system calls `store.insert(DefaultGizmoConfigGroup, { line: { width: 4, style: { Dashed: { gap_scale: 2, line_scale: 1 } } } }, {})`
- **THEN** the default group's `GizmoConfig` has width 4 and the dashed style, and its other fields are unchanged

#### Scenario: Drawing every light
- **WHEN** a TS system inserts `{ draw_all: true }` as `LightGizmoConfigGroup`'s ext
- **THEN** the next frame draws a gizmo for every light, as Bevy does with `draw_all` set in Rust

#### Scenario: Disabled
- **WHEN** the default group's `enabled` is set to `false` and a TS system draws a line
- **THEN** nothing reaches the storage

### Requirement: Primitives and curves are reflected values
`primitive2d(shape, isometry, color)` and `primitive3d` SHALL take a primitive as a one-key object naming a Bevy primitive with a `GizmoPrimitive2d` or `GizmoPrimitive3d` impl, its value Bevy's reflected shape written over the type's default, and SHALL cover every such impl in Bevy 0.20. `curve2d`, `curve3d`, `curveGradient2d` and `curveGradient3d` SHALL take Bevy's `EasingCurve`, `SampleAutoCurve`, `UnevenSampleAutoCurve` or `ConstantCurve` of `Vec2` or `Vec3` the same way. Any other key SHALL throw naming the supported ones. A curve built from a closure SHALL have no TS form.

#### Scenario: A cuboid
- **WHEN** a TS system calls `gizmos.primitive3d({ Cuboid: { half_size: [1, 1, 1] } }, [0, 1, 0], "#ff0")`
- **THEN** the buffer holds what `gizmos.primitive_3d(&Cuboid::new(2., 2., 2.), Vec3::Y, YELLOW)` draws

#### Scenario: An eased curve
- **WHEN** a TS system calls `gizmos.curve3d({ EasingCurve: { start: [0, 0, 0], end: [1, 1, 0], ease_fn: "CubicInOut" } }, times, "#fff")` with 20 times
- **THEN** the buffer holds the same 20-point strip as the Rust call

### Requirement: Retained gizmos
`bevy-ts/gizmos` SHALL export `GizmoAsset`, a class with every `Gizmos` drawing method and builder setting. `assets.add(GizmoAsset, g)` SHALL add a `GizmoAsset` holding exactly what those calls draw and return its `AssetRef<GizmoAsset>`; `assets.set(ref, g2)` SHALL replace its calls. `<Gizmo handle={ref} line_config={...} depth_bias={...} />` SHALL draw it, with `line_config` written through reflection.

#### Scenario: A retained sphere
- **WHEN** a script builds `g = new GizmoAsset()`, calls `g.sphere([0, 0, 0], 1, "#f00").resolution(64)`, adds it and mounts `<Gizmo handle={ref} />`
- **THEN** `Assets<GizmoAsset>` holds an asset whose buffer equals what the Rust calls leave, and the entity's `Gizmo.handle` points at it

### Requirement: Gizmo coverage is documented and measured
`docs/gizmos.md` SHALL list every public gizmo item of Bevy 0.20 (from `bevy_gizmos`, `bevy_gizmos_render` and `bevy_light`'s gizmos) with its TS form, status and test, and SHALL give the TS cost of drawing 1,000 lines a frame against the same drawing in Rust, measured by `bench/gizmos`. No row marked working SHALL name a test that does not exist.

#### Scenario: A stale row
- **WHEN** a row of `docs/gizmos.md` marked working names a test that is not in `crates/bevy-ts-cli/tests/it/gizmos.rs`
- **THEN** `docs_rows_name_real_tests` fails naming the row
