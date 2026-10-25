## Context

Bevy's `Gizmos<Config, Clear>` (`bevy_gizmos/src/gizmos.rs:143`) is a `SystemParam` of a
`Deferred<GizmoBuffer>` and `Res<GizmoConfigStore>`; every drawing method lives on `GizmoBuffer`
(`gizmos.rs:361` onward, plus `arrows.rs`, `circles.rs`, `arcs.rs`, `grid.rs`, `curves.rs`,
`cross.rs`, `rounded_box.rs`, `stroke_text.rs`, `primitives/`). Shapes with settings return a
builder that draws on `Drop` (`ArrowBuilder`, `arrows.rs:15,61`). The buffer's four vectors are
public (`gizmos.rs:285-293`).

writable-native-params made an exported param able to write and defer: it is fetched with
`get_param` at the TS system's ticks, its `Deferred` state applies after the run in parameter
order (`docs/native.md:344-390`), and `native_writes.rs:866` already applies a Rust `Gizmos` export.
An export has one state shared by every TS system that takes it, so a `Local` in it is shared. A
call through a param the running system does not take throws (`crates/bevy-ts/src/host.rs:697`).

What crosses today: numbers, `bool`, `Entity`, `Vec2`, `Vec3`, `Quat`, records, fieldless enums,
strings, options, and numeric slices (`host.rs:37-53`). Gizmos need colours, isometries,
transforms, point lists and reflected values. Colours have one parser, `bevy_ts::color`
(`docs/native.md:158`). Reflected values have one writer, `bevy_ts::reflect_value`
(`docs/reflection.md`); the operator decided on 2026-10-23 that config and per-type fields go
through it, not through hand-wired fields.

The coverage table is `docs/research/script-gizmos.md`.

## Goals / Non-Goals

**Goals:**
- Every `GizmoBuffer` drawing method and builder setting from TS, with Bevy's names camelCased and
  Bevy's argument shapes, drawing exactly what the Rust call draws.
- Every config group Bevy registers in the engine, and any a Rust crate exposes.
- Config read and written through reflection; retained gizmos on reflect-assets' path.
- A measured cost for 1,000 lines a frame, and headless frames before and after.

**Non-Goals:**
- TS-defined config groups: a group is a Rust type (`GizmoConfigGroup: Reflect + TypePath +
  Default`, `config.rs:78`) keyed by `TypeId`.
- The global `gizmo()` (`global.rs:46`): TS draws only through a declared param.
- Custom `Clear` contexts, the transform gizmo in games, colour palettes (the table says why).
- Curves built from closures: a host call never calls back into JS.

## Decisions

### 1. One method table, one export per group

The gizmo methods are written once, in `crates/bevy-ts-native/src/gizmos/draw.rs` (behind a default `gizmos` feature, so a project crate can name them), as a table
generic over the group `C` and the clear type: each entry is the TS name, its `ArgSpec`s, its
result kind, its builder (decision 5) and `fn(&mut GizmoBuffer<C, ()>, &mut OpenBuilder, &mut
SlotReader, &mut [&mut [u8]]) -> Result<(), String>`, which decodes the arguments and makes the
Bevy call. A `macro_rules!` in that file lists each Bevy method once with its typed arguments and
expands to the entry, so the list cannot drift from the call.

`TsGizmos<'w, 's, C>` is `#[derive(SystemParam)] { gizmos: Gizmos<'w, 's, C>, open: Local<'s,
OpenBuilder> }`. It implements `ExportParam` by hand from the table (`spec()` maps each entry to a
`MethodSpec` whose `call` fetches the param with `host::fetch` and runs the entry on
`&mut *gizmos`), because `#[export]` takes no generic impls and the same table must also drive the
retained replay (decision 10). The JS glue and TS declarations come from the `ParamSpec` as for
any export.

Each registered group gets one export: TS name `Gizmos_<Short>` (never written by a game),
`stands_for = "Gizmos<Short>"`. The default group's export is named `Gizmos`, so its errors read
`Gizmos.line: ...`, and the reader takes bare `Gizmos` as it. Every group's export has the same
methods, so `ParamSpec.module_declares` keeps them out of the shared declarations and
`bevy-ts/gizmos` declares the one generic `interface Gizmos<G>` (`host::declaration_as`). The names are
leaked `&'static str`s, once per group at registration. *Alternative:* teach `#[export]` generic
impls. Rejected: the replay needs the table anyway, and generics in the macro serve only this.

### 2. Groups reach TS through `ts_gizmo_group`

`BevyTsNativeExt::ts_gizmo_group::<G>()` (in `crates/bevy-ts-native/src/lib.rs`) registers
`TsGizmos<G>` with `ts_param` and records `G`'s type path and short name for `bevy-ts/gizmos`. In
`finish`, a group missing from `GizmoConfigStore` (never `init_gizmo_group`ed) fails the app
naming the group. The engine's `GizmosPlugin` (`crates/bevy-ts-cli/src/gizmos.rs`, added beside
`CamerasPlugin` in `cli.rs:243` and `types.rs:33,754`) registers Bevy's five:
`DefaultGizmoConfigGroup`, `AabbGizmoConfigGroup`, `FrustumGizmoConfigGroup`,
`SkinnedMeshBoundsGizmoConfigGroup`, `LightGizmoConfigGroup`. bevy-ts's own `EditorGizmos` and
`OrientGizmos` are never registered.

`bevy-ts/gizmos` (a built-in module, `app.ts_module`) exports one constant per registered group,
its type path as a branded string (`LightGizmoConfigGroup: GizmoGroup<"LightGizmoConfigGroup",
LightGizmoConfigGroupValue>`), the value type taken from the group's reflected shape as
`native.d.ts` types reflected values, and `interface Gizmos<G extends GizmoGroup =
DefaultGizmoConfigGroup>`. The reader (`crates/bevy-ts-cli/src/reader.rs:1174`) resolves a param
written `Gizmos` or `Gizmos<X>` through the claims, as it does `Res`/`ResMut`; an unregistered `X`
fails the load naming it and the registered groups.

### 3. New argument kinds

Added to `host::Kind` with a `HostValue` impl each, so every export can use them:

| Kind | Rust | TS | Crossing |
|---|---|---|---|
| `Color` | `Color` | `ColorValue` | a tag slot then four channels: tags 0 to 12 are Bevy 0.20's 13 `Color` variants in declaration order, channels in reflected field order; an array is `Srgba`. A string is tag 13 then the string's slots, parsed in Rust by `bevy_ts::color`; any other object is tag 14, its JSON text, parsed there too. No colour text is parsed in JS. |
| `Isometry3d` | `Isometry3d` | `Vec3 \| Quat \| { rotation?, translation? }` | 7 slots. An array of 3 or an object with `z` and no `w` is a translation, of 4 or with `w` a rotation; Bevy's `From` impls. |
| `Isometry2d` | `Isometry2d` | `Vec2 \| Rot2 \| { rotation?, translation? }`, `Rot2` as `{ cos, sin }` | 4 slots; a number is not a `Rot2`. |
| `TransformPoint` | `Transform` | a `Transform` (`{ translation?, rotation?, scale? }`) or a `GlobalTransform` column value | 10 slots; a `GlobalTransform` is decomposed in Rust (`to_scale_rotation_translation`). |
| `UVec2`, `UVec3` | `UVec2`, `UVec3` | `[x, y]`, `[x, y, z]` or lane objects | 2 or 3 slots; a negative or fractional lane throws. |
| `Points1`, `Points2`, `Points3` | `HostPoints<f32>`, `HostPoints<Vec2>`, `HostPoints<Vec3>` | `number[]`/`Vec2[]`/`Vec3[]` or a `Float32Array` of 1, 2 or 3 per point | a slice: a `Float32Array` is passed as is; an array is packed by the glue into a reusable scratch `Float32Array`. Length not a whole number of points throws. |
| `List` | `HostList<T>` of a tuple of kinds | `[a, b][]` | a count slot then the elements, in the call's 4 KB of slots; past it throws naming the method and the room. |
| `Json` | `HostJson` (a `serde_json::Value`) with a TS type | that type | `JSON.stringify` as a string; parsed in Rust. Results the reverse, with `JSON.parse`. |
| `Aabb3d` | `#[derive(HostRecord)]` `{ min: Vec3, max: Vec3 }` in `draw.rs` | `{ min, max }` | a record. |

`cube` and `axes` take `impl TransformPoint`; TS passes a `Transform` or `GlobalTransform`, the
forms a TS system holds. *Alternative:* parse colours in JS for speed. Rejected: two parsers.

### 4. Lists keep Bevy's iterator shape

Bevy takes `impl IntoIterator<Item = Vec3>`; TS takes an array of points, the same shape, or a
`Float32Array` when the caller has one. Points cross as a slice (no length limit). Gradients and
text sections (`(Vec3, Color)`, `(f32, Color)`, `(&str, Color)`) cross as `List`, bounded by the
slots: about 60 3D gradient points with object colours. That is enough for debug strips; past it
the call throws and says to split the strip.

### 5. Builders: draw now, redraw on a setting

A method that returns a builder in Bevy draws at once with Bevy's defaults (the entry builds Bevy's
builder and drops it) and returns the param object itself, typed in TS as the builder
(`EllipseBuilder`, `ArrowBuilder`, ...). Before drawing, the entry records in `OpenBuilder` (the
`Local`) the buffer's four lengths, the arguments and the builder kind; after, the lengths again.
A setting (`resolution`, `withTipLength`, `outerEdges`, `skew`, `cornerRadius`, ...) checks the
buffer still ends where the builder left it and is of its kind, truncates the four vectors back to
the start lengths, and redraws with every setting so far. A setting called after anything else was
drawn, or by another system, throws `EllipseBuilder.resolution: call it right after the call that
returned the builder`.

`MethodSpec` gains `chain: Option<&'static str>` (the builder it returns) and `of: Option<&'static
str>` (the builder a setting belongs to). The glue returns `this` for both; declarations list each
builder as an interface of its settings, and leave settings off the param's interface. A chain of
k settings draws k + 1 times; settings are rare and shapes are small.

The `Local` is shared by every TS system using the export; the length check makes that safe,
since the buffer is applied and emptied after each run. *Alternatives:* a JS builder flushed at
the end of the run (needs a run-end hook and allocates per call); a trailing settings object
(`circle(iso, r, c, { resolution })`), not Bevy's shape.

### 6. Config through reflection

- `gizmos.config()` and `gizmos.configExt()` return `Gizmos::config` and `config_ext`
  (`gizmos.rs:150-152`) as `Json`, read by `reflect_value::read`.
- Stage 2: `TsGizmoConfigStore` claims `ResMut<GizmoConfigStore>` and `TsGizmoConfigStoreRead`
  claims `Res<GizmoConfigStore>`, one spelling per export (`docs/native.md:392`). `config(group)`
  reads `get_config_dyn` as `[config, ext]`; `insert(group, config, ext)` writes each with
  `reflect_value::write` over the current value (keys left out keep theirs), checks with
  `FromReflect`, and stores through `get_config_mut_dyn`. `groups()` lists the registered group
  names. A group that is not registered throws naming the registered ones. `config_mut` has no TS
  form: a JS copy cannot write back.
- `bevy-ts/gizmos` declares `class GizmoConfigStore` so the claims are typed
  (`types.rs` `claims_ts` takes module-declared names as declared).

### 7. Primitives as reflected values

`primitive2d(shape, isometry, color)` and `primitive3d` take `Json` typed as a union of one-key
objects, `{ Cuboid: { half_size: Vec3 } } | { Sphere: { radius } } | ...`, one per
`GizmoPrimitive2d`/`3d` impl (`primitives/dim2.rs:40-785`, `dim3.rs:37-899`). The key picks the
Rust type from a table in `crates/bevy-ts-native/src/gizmos/primitives.rs`; the value is written over
the type's default with `reflect_value::write` and taken with `FromReflect`; then the entry calls
`primitive_2d`/`primitive_3d`. Builder settings (`resolution`, `drawArrow`, `cellCount`, ...)
chain as in decision 5. TS types every 2D primitive's builder as one `Primitive2dBuilder` (and
3D as `Primitive3dBuilder`) with every setting any of them has, since the shape is only known from
the value; a setting the shape's builder lacks throws `Cuboid has no setting resolution`. An unknown key throws naming the supported ones. The TS union is generated
from the table and the types' reflected shapes.

### 8. Curves as reflected values

`curve2d`, `curve3d`, `curveGradient2d`, `curveGradient3d` take a curve as `Json`:
`{ EasingCurve: { start, end, ease_fn } }`, `{ SampleAutoCurve: ... }`,
`{ UnevenSampleAutoCurve: ... }` or `{ ConstantCurve: ... }`, each of `Vec2` or `Vec3`, written
through `reflect_value`; `GizmosPlugin` registers these eight generic types. `times` is `Points1`
(a `number[]` or `Float32Array`, one float each, decision 3's slice form); `timesWithColors` a
`List<(f32, Color)>`. Any other key throws naming the set.

### 9. Tests: TS draws what Rust draws

`crates/bevy-ts-cli/tests/it/gizmos.rs`, a module of the one test binary (`docs/testing.md`):

- `ts_draws_what_rust_draws`: a table of (TS call, Rust call) per method and builder setting. A TS
  system makes the call; the test reads `GizmoStorage<DefaultGizmoConfigGroup, ()>` after the run
  and compares its four vectors with a `GizmoBuffer` the Rust call filled. One table row per entry
  of decision 1's table, and a test fails when an entry has no row, so new methods need a row.
- Behaviour tests named in the coverage table: groups, fixed update, the undeclared-param throw,
  builder settings, config, primitives, curves, the retained asset.
- One render test (`gizmos_render_headless`, `common::render_app()`): a red `line2d` across a
  `Camera2d` image target gives red pixels on the line and none off it. Rule 5: a render bug is
  tested where it renders.
- `host.rs` unit tests: each new kind's codec, and glue for each input form.
- Type cases in `packages/bevy-ts/test/gizmos.ts` (tsc via `pnpm check`, as the claims' type
  tests), over a fixture of the generated `bevy-ts/gizmos` declaration that a Rust test keeps
  equal to the engine's: `Gizmos<LightGizmoConfigGroup>` types `configExt()`, a builder setting
  on the wrong builder is a type error, `Gizmos<Nope>` is a type error. `typescript.rs` runs no
  tsc, so the cases cannot live there.

### 10. Retained gizmos on reflect-assets' path

`GizmoAsset` in `bevy-ts/gizmos` is a JS class with the same drawing methods, generated from the
same `ParamSpec` by a second glue form, `host::recorder_glue`: each call writes its method index
and encoded arguments into the instance's growable `Float64Array` instead of `H.slots`; points are
copied inline. Builder settings record as calls too.

`assets.add(GizmoAsset, g)` goes through reflect-assets: a `HandMap` entry for `GizmoAsset` whose
write replays the record onto a fresh `GizmoAsset` (it derefs to `GizmoBuffer<ErasedGizmoConfigGroup,
()>`, `retained.rs:16-28`) through decision 1's table entries, with an `OpenBuilder` of its own.
Read returns the record the script last wrote (`{ calls: [...] }`), as animation-control's clip
entry does. `<Gizmo handle={ref} line_config={{ width: 4 }} depth_bias={-0.1} />` takes the ref
through reflect-assets' handle hook and `line_config` as a reflected hole prop.

`GizmoAsset` is `TypePath` only (`bevy_gizmos/src/lib.rs:325`), and `HandMap::write` returns a
`Box<dyn Reflect>` from an `asset_type` that needs `ReflectAsset`. Stage 3 needs reflect-assets to
widen `HandMap` for such a type: a typed `add`/`set` that inserts into `Assets<T>` directly and
mints the ref, registered from outside `bevy-ts` (`HAND_MAPS` is a `const` list in `bevy-ts`, which
cannot name `bevy-ts-native`'s table), for example `app.ts_hand_map(...)`. To ask of lead@reflect-assets (task 5.1); stage 3 waits on it.

### 11. Benchmark

`bench/gizmos/` (outside the gate's build, `docs/testing.md` rule 8), shaped like
`bench/diagnostics`:

- `rust/`: criterion, a Bevy app with `MinimalPlugins`, `AssetPlugin` and `GizmoPlugin`; arms
  `empty` and `lines_1000` (a Rust system calling `gizmos.line` 1,000 times per update); ns per
  update and per line.
- `src/game.ts` + `bench.mjs` + `summary.mjs`: a headless release engine running a TS system that
  draws 1,000 `line`s a frame, against the same system drawing none; `main` time per frame from
  diagnostics, 300 frames after 60 of warm-up, three runs. Arms: colours as `"#f00"`, as
  `srgb(...)` objects, and `linestrip` of 1,000 points as one call.
- Results to `bench/gizmos/results/`, the table to `docs/gizmos.md` "Cost". A process
  `bench-gizmos` in `.abyme/processes.ts`, cargo through `~/dev/abyme/scripts/gate-slot`.

### 12. Example and frames

`examples/gizmos` ports Bevy's `examples/gizmos/3d_gizmos.rs` and `light_gizmos.rs` call for call:
the same shapes, colours and config toggles (keys to cycle line style and joints, as Bevy's).
Three departures: a TS project cannot declare a group, so the "round" gizmos draw into
`FrustumGizmoConfigGroup`; a `FunctionCurve` is a closure, so its points are sampled into a
`linestripGradient`; and the example has no `example.json`, since a shipped example is
type-checked in a fresh project, which has `bevy-ts/gizmos` only after `bevy-ts types`.
Frames come from `bevy-ts play --headless` with `BEVY_TS_SCREENSHOT`
(`openspec/specs/headless-run/spec.md:9`): `before.png` is the example's scene with its gizmo
systems removed (main has no `Gizmos`, so the systems do not load), `after.png` the full example,
both in `~/dev/bevy-ts-frames/script-gizmos/`. Nothing opens on the operator's screen.

## Risks / Trade-offs

- [1,000 host calls a frame cost 0.15 to 0.26 ms in TS against 0.03 ms in Rust (measured)] → The
  bench states it; `linestrip` with a `Float32Array` draws many segments in one call at Rust's cost;
  the doc says so.
- [Colour objects cost more to encode than hex strings, or the reverse] → Measured: `[r, g, b]` is
  cheapest, a hex string and `srgb()` about 100 ns more; the doc names it.
- [List kinds are bounded by 4 KB of slots] → Only gradients and text sections use them; the throw
  names the room.
- [Builder redraw truncates the buffer] → Guarded by the length check; `ts_draws_what_rust_draws`
  covers every setting.
- [Stage 3 waits on another team] → Stages 1 and 2 land without it; if reflect-assets declines the
  widening, stage 3 becomes an export claiming `ResMut<Assets<GizmoAsset>>` with `add(g)`, and the
  proposal is amended before building it.
- [Bevy 0.21 adds or renames gizmo methods] → The same-buffer test fails for a missing row only for
  entries we have; the coverage table is re-walked at each Bevy bump.

## Migration Plan

Additive. No protocol schema change expected; the handshake lists new exports. Rollback is a
revert per stage.

## Open Questions

None for stages 1 and 2. Stage 3: reflect-assets' answer on widening `HandMap` (decision 10).
