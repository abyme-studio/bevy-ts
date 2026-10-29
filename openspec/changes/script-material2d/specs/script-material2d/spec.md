## ADDED Requirements

### Requirement: 2D materials are declared in TS with a param schema and WGSL files
The `bevy-ts/material` module SHALL export `material2d(name, { params, instance, textures, fragment, vertex?, alphaMode? })`. `params`, `instance` and `textures` SHALL take what `material()` takes, laid out and named by the same rules and limits. `fragment` and `vertex` SHALL be shader-file values from relative `.wgsl` imports; a WGSL source string SHALL be rejected with the kind name and the declaring file and line. `alphaMode` SHALL be `"opaque"` (default), `"blend"` or `{ mask: n }`; any other SHALL fail at the call naming the three. A 2D kind SHALL NOT share its name with a 3D kind or a pass. Each of these SHALL fail the load at the `material2d()` call. The prebuilt engine SHALL draw such kinds with no Rust per kind.

#### Scenario: A planet with no Rust
- **WHEN** `game.tsx` imports `planet.wgsl`, declares `material2d("game::Planet", { params: { sea: color }, instance: { seed: f32 }, fragment: planet })`, and an entity holds `<mesh2d shape="rectangle" width={48} height={48} />` and `<material2d kind={Planet} sea="#24a" seed={3} />` under a `Camera2d`
- **THEN** the prebuilt engine draws the rectangle through that WGSL with those values

#### Scenario: A 3D alpha mode
- **WHEN** a 2D kind declares `alphaMode: "add"`
- **THEN** the load fails at the `material2d()` call, naming `"opaque"`, `"blend"` and `{ mask }`

#### Scenario: One name in two spaces
- **WHEN** a generation declares `material("game::Glow", ...)` and `material2d("game::Glow", ...)`
- **THEN** the load fails at the second call, naming the first

### Requirement: `<material2d kind>` draws a `<mesh2d>`
A `<material2d kind>` element SHALL draw its entity's `Mesh2d` with the kind, through Bevy 0.20's 2D material pipeline, in the 2D phase its alpha mode names. Its params and textures SHALL be keyed props: equal props SHALL share one asset, new props SHALL take another, and an asset no entity holds SHALL be freed at the end of the frame. Params and textures SHALL NOT be bindable; instance props SHALL be, writing the entity's row and never the asset. A prop the kind lacks, a 3D kind, or the element on an entity holding `Sprite` SHALL fail the load at the element, naming what to use instead. Removing the element SHALL stop drawing the mesh with the kind.

#### Scenario: Equal props share one asset
- **WHEN** 100 entities hold `<material2d kind={Planet} sea="#24a" seed={i} />`
- **THEN** they hold one `ScriptMaterial2d` asset, and each entity's row holds its own `seed`

#### Scenario: A bound instance prop
- **WHEN** `<material2d kind={Planet} land={() => cutoff()} />` is mounted and `setCutoff(0.4)` runs
- **THEN** that entity's row holds `land` 0.4 that frame, and the asset is unchanged

#### Scenario: On a sprite
- **WHEN** `<material2d kind={Planet} />` sits on an entity holding `Sprite`
- **THEN** the load fails at the element, naming `<mesh2d shape="rectangle">` with a texture param

### Requirement: The 2D WGSL gets its params, textures and instances
The engine SHALL append, after the author's last line, the params struct and `params` uniform, each texture and its `<name>_sampler`, and, for a kind with instance params, the instance struct, the `instances` storage array and `fn instance(i: u32)`, all at `@group(2)`, in 3D's order and binding numbers. A kind with instance params SHALL have Bevy's 2D `get_tag` imported at the start of the author's first line as `bevy_ts_get_tag`, keeping that line's columns.

#### Scenario: Reading an instance param
- **WHEN** the fragment returns `params.sea * instance(in.instance_index).seed / 10.0` for two quads with `seed` 5 and 10
- **THEN** the first reads back half the second's colour

### Requirement: A 2D material keeps pixel art crisp
Under `PixelCamera` at a whole zoom, a fragment whose colour depends only on `floor(in.world_position.xy)` SHALL draw each world unit as one whole `zoom × zoom` block of one colour, wherever the mesh is. A fragment whose colour depends only on `floor(in.uv * pixels)`, on a rectangle of `pixels` world units whose corners sit on whole world units, SHALL do the same. A texture SHALL sample with its image's sampler. A `<YSort>` on an entity with a script 2D material SHALL set its draw order as on a sprite.

#### Scenario: World-space art pixels at a fractional position
- **WHEN** a 16×16 rectangle at x 10.3 draws a checker of `floor(in.world_position.xy)` under `PixelCamera` zoom 3
- **THEN** every drawn world unit is a whole 3×3 block of one colour

#### Scenario: Quad-space art pixels on whole units
- **WHEN** a 16×16 rectangle at x 10 draws a checker of `floor(in.uv * 16.0)` under `PixelCamera` zoom 3
- **THEN** it covers 48×48 pixels in whole 3×3 blocks

#### Scenario: Y-sorted planets
- **WHEN** two overlapping quads with one kind and `<YSort layer={1} />` stand at y 0 and y -10
- **THEN** the one at y -10 draws in front, and after a bound y swaps them the other does within a frame

### Requirement: 2D materials and their shaders hot reload
A saved `.wgsl` file SHALL replace the shader of every 2D kind naming it, in place, with no new generation. A changed schema SHALL re-encode every asset and instance row of the kind by name. When a new schema's shader fails the check, the kind SHALL keep drawing as it was, and a later `.wgsl` save that fixes the file SHALL apply it with no new generation.

#### Scenario: Editing a planet shader
- **WHEN** the author changes `planet.wgsl`'s sea colour and saves
- **THEN** the planets draw the new colour within a few frames, the generation number does not change and no entity respawns

### Requirement: 2D WGSL errors point into the author's file
A WESL or WGSL error in a 2D kind's file SHALL be reported in the terminal and on the protocol's log stream with the kind name, the author's file path, line and column. An error in the generated declarations SHALL name the kind's schema in TS. The last good shader SHALL keep drawing; with none, the mesh SHALL draw nothing until fixed. A renderer failure SHALL go through the same pipeline watch as 3D, never reported against a newer source.

#### Scenario: A WGSL error
- **WHEN** `planet.wgsl` has a type error on its line 12 and is saved
- **THEN** the error names `game::Planet` and `src/planet.wgsl:12`, and the previous planet keeps drawing

### Requirement: The 2D material's frame cost is measured
A bench in `bench/material2d`, outside the workspace, SHALL draw a `Camera2d` under `PixelCamera` zoom 3 to a 1920×1080 image, headless, with arms `off`, `one` (a 200-art-px planet of three layers), and `n20`, `n100`, `n200`, `n500` (48-art-px planets of three layers, a ring on every fifth), on shaders and textures the bench makes, 600 frames after 120 warm-up, arms alternated. It SHALL record per arm the median and p95 frame time, the machine's load and the commit in `bench/material2d/results/README.md`. `n200` is recorded for learn-planets and decides nothing. `n100` SHALL add under 2 ms to `off`'s median frame in a run at load under 10.

#### Scenario: The bench's arms
- **WHEN** `bench/material2d/run.sh 8` runs
- **THEN** it prints, per arm and run, the median and p95 frame time, and `uptime` before and after
