## ADDED Requirements

### Requirement: Passes are declared in TS with a param schema and a WGSL file
The `bevy-ts/material` module SHALL export `pass(name, { params, fragment, order? })`, where `params` takes the types material params take (`f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color`, and fixed arrays of these), `fragment` is a shader-file value from a relative `.wgsl` import, and `order` is a number, 0 by default. A WGSL source string SHALL be rejected with the pass name and the declaring file and line. Param names SHALL follow material param names' rules and SHALL NOT be `time`, `texel`, `size`, `view`, `screen`, `screen_sampler`, `params` or the generated struct's name; the params with the frame fields SHALL fit 256 bytes; at most four pass kinds SHALL be live in a generation. Any of these SHALL fail the load at the `pass()` call, naming the rule. The prebuilt engine SHALL draw such passes with no Rust per pass.

#### Scenario: A dusk pass with no Rust
- **WHEN** `game.tsx` imports `dusk.wgsl`, declares `pass("game::Dusk", { params: { amount: f32, tint: color }, fragment: dusk })`, and a `Camera2d` entity holds `<pass kind={Dusk} amount={0.6} tint="#f84" />`
- **THEN** the prebuilt engine draws the camera's image through that WGSL with those values

#### Scenario: A reserved name
- **WHEN** a pass declares `params: { time: f32 }`
- **THEN** the load fails at the `pass()` call, naming `time` as one the engine writes

#### Scenario: A fifth kind
- **WHEN** a generation declares five pass kinds
- **THEN** the load fails at the fifth `pass()` call, naming the limit of four

### Requirement: A pass runs on its camera's image after the main pass
A `<pass kind>` element SHALL sit under an entity holding `Camera2d` and SHALL run its kind's fragment over that camera's whole target in Bevy's `Core2d` schedule, in `Core2dSystems::PostProcess` after tonemapping and before upscaling, through Bevy 0.20's `FullscreenMaterial`. A camera's passes SHALL run in ascending `order`, then by kind name. A `<pass>` on an entity without `Camera2d`, or a second `<pass>` of one kind on one camera, SHALL fail the load at the element. Removing the element SHALL stop the pass the next frame. A camera with no pass SHALL draw as it did before this change.

#### Scenario: Two passes in order
- **WHEN** a camera holds `<pass kind={Tint} />` (order 0, multiplies by red) and `<pass kind={Invert} />` (order 1)
- **THEN** a white sprite reads back cyan

#### Scenario: A pass on a sprite
- **WHEN** `<pass kind={Dusk} />` sits under an entity holding a `Sprite` and no `Camera2d`
- **THEN** the load fails at the element, naming `Camera2d`

### Requirement: The WGSL gets the screen, its sampler and the params
The engine SHALL append, after the author's last line, the params struct, `@group(0) @binding(0) var screen: texture_2d<f32>`, `@group(0) @binding(1) var screen_sampler: sampler` and `@group(0) @binding(2) var<uniform> params: <Name>Params`. The struct SHALL start with `time: f32` at offset 0, `texel: f32` at 4, `size: vec2<f32>` at 8 and `view: vec2<f32>` at 16, then the kind's params by WGSL uniform rules. Each frame, before extraction, the engine SHALL write `time` as Bevy's `globals.time` (elapsed `Time`, wrapped), `size` as the camera's physical target size, `texel` as physical pixels per world unit, and `view` as the world point drawn at the target's centre, after the pixel camera's snap. The author's file SHALL NOT declare these names; one that does SHALL fail as a material's does.

#### Scenario: The frame fields under a pixel camera
- **WHEN** a `Camera2d` with `<PixelCamera zoom={2} />` at x 10.3 renders a 320×180 image with a pass
- **THEN** the pass reads `size` (320, 180), `texel` 2 and `view` x 10.5, the snapped centre

#### Scenario: Time pauses with the game
- **WHEN** virtual time is paused
- **THEN** `params.time` holds its value

### Requirement: A pass keeps pixel art crisp
The screen SHALL be sampled nearest. A pass whose fragment returns `textureSample(screen, screen_sampler, in.uv)` SHALL draw the same pixels as the camera without the pass, under `PixelCamera` at a whole zoom.

#### Scenario: A passthrough at zoom 2
- **WHEN** a 16×16 checker sprite draws under `PixelCamera` zoom 2, once with a passthrough pass and once without
- **THEN** both images are equal, pixel for pixel, each texel a whole 2×2 block

### Requirement: Pass params are per camera and may be bound
A `<pass>`'s props SHALL be the kind's params, encoded into the camera's own component; unset params SHALL be zero and colours white. A prop bound to a function SHALL write the camera's params the frame its value changes. A prop the kind lacks SHALL fail the load at the element, naming the kind's params.

#### Scenario: A bound amount
- **WHEN** `<pass kind={Dusk} amount={() => dusk()} />` is mounted and `setDusk(0.8)` runs
- **THEN** the camera's params hold `amount` 0.8 that frame, and no other camera's change

### Requirement: Passes and their shaders hot reload
A saved `.wgsl` file SHALL replace the shader of every pass kind naming it, in place, with no new generation. A changed params schema SHALL re-encode every camera's params by param name. A change in kinds or `order` that moves a kind to another slot SHALL move its shader and every camera's params with it in one world update. When a new schema's shader fails the check, the kind SHALL keep drawing as it was, and a later `.wgsl` save that fixes the file SHALL apply it with no new generation.

#### Scenario: Editing a pass shader
- **WHEN** the author changes `dusk.wgsl`'s tint and saves
- **THEN** the camera draws the new tint within a few frames, the generation number does not change and no entity respawns

#### Scenario: Reordering two passes
- **WHEN** a save swaps the `order` of `Tint` and `Invert`
- **THEN** the white sprite reads back black (inverted, then tinted) within a few frames, and each kind's params are unchanged

### Requirement: Pass WGSL errors point into the author's file
A WESL or WGSL error in a pass's file SHALL be reported in the terminal and on the protocol's log stream with the pass name, the author's file path, line and column. An error in the generated declarations SHALL name the pass's params schema in TS. The last good shader SHALL keep drawing; with none, the camera SHALL draw as without that pass.

#### Scenario: A WGSL error
- **WHEN** `dusk.wgsl` has a type error on its line 9 and is saved
- **THEN** the error names `game::Dusk` and `src/dusk.wgsl:9`, and the previous dusk keeps drawing

### Requirement: The pass's frame cost is measured
A bench in `bench/fullscreen-pass`, outside the workspace, SHALL time a 2D scene at 1920×1080 under `PixelCamera` with no pass, a passthrough pass and a grading pass, on textures the bench makes, and record the pass's GPU time and the frame time per arm in `bench/fullscreen-pass/results/README.md`. No Stardew Valley asset SHALL be committed.

#### Scenario: The bench's arms
- **WHEN** `bench/fullscreen-pass/run.sh` runs
- **THEN** it prints, per arm, the pass's GPU time and the frame time, median and p95
