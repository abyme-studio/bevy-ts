# Faster reflected data for scripts

Recommendation: keep stages 1 and 2 as designed (JSON). For stage 3, replace "host call plus
JSON per touched row per run" with a cache keyed by each row's changed tick, so unchanged rows
make no call. Fill a miss through a binary codec, compiled once per type from its `TypeInfo`.

Researched 2026-10-23. The JS numbers come from `bench/reflect-codec/codec.js` under node 26
(V8 14.6) and deno 2.9.7 (V8 15.0). The machine's load average was 64 to 78 at the time, so the
absolute times are rough. The two arms ran side by side in the same run. The host call cost is
in-repo (`docs/research/custom-relationships.md`, 84c2e62). The Rust side is an estimate: lead
asked for no cargo until the load is under 20.

## What others do

All the fast systems compute layouts or accessors once per type and then touch memory directly.
The slow ones reflect on every access. Every system that keeps JSON uses it for tools, never for
the hot path.

| System | How a script reaches reflected data | Cost class |
|---|---|---|
| bevy_mod_scripting 0.23 | `ReflectReference` (base plus `ReferencePath`). Each `Index` metamethod claims access through `WorldAccessGuard`, then walks the path by reflection. No value caching. README: "not yet optimized" | One crossing plus a reflect walk per field step |
| flecs meta | Each type compiles to a vector of `ecs_meta_op_t` (kind, offset, size) at registration. `ecs_meta_cursor` and `ecs_ptr_to_json` interpret the ops over raw memory at `base + offset`. JSON is the REST and explorer path. Native bindings (C#, Rust) use typed memory | Ops: native speed. JSON for tools only |
| Unity DOTS / Burst | Only unmanaged, blittable components go in chunks. `TypeManager` fixes layouts at startup. Managed (`class`) components sit in a managed array, outside Burst | Fixed layout, or no hot path at all |
| PuerTS (Unity) | Reflection mode, or generated static wrappers. `[BlittableCopy]` structs cross as a memcpy | Reflection is the slow default; the docs say to generate wrappers for shipping builds |
| PuerTS (Unreal) | One `FPropertyTranslator` per `FProperty`, built once from reflection. Getters use `ContainerPtrToValuePtr` (the offset). Structs pass by pointer or as a copy | Reflection metadata compiled to accessors once |
| Godot GDExtension | Variant `get`/`set` by `StringName` (boxed), or `ptrcall` and `variant_get_ptr_getter` over typed pointers. godot-rust and godot-cpp generate typed bindings from `extension_api.json` | Variant path slow, ptrcall fast |
| Unreal Verse / Blueprint | Compiled. Property access is by `FProperty` offset | Offsets |
| Cocos Creator (V8) | Node transforms in memory shared by C++ and JS, so no call per access | Zero-copy |
| Bevy 0.20 | `bevy_reflect` has no layout or offset API. `ParsedPath` caches parsed paths. Reflected functions (`DynamicFunction`) call functions; they do not lay out data. Dynamic queries (`QueryBuilder`, `FilteredEntityRef::get_by_id`) give a `Ptr`, not a layout. Offsets come only from the address trick `layout_reflect.rs` already uses | Build it ourselves |
| V8 | `JSON.stringify` got a side-effect-free fast path ("more than twice as fast", v8.dev). `JSON.parse` has no such fast path for small objects | See below |

## Measured: JS side per row (ns, median, 1,000 rows)

| Arm | node V8 14.6 | deno V8 15.0 |
|---|---|---|
| JSON.parse Color (`{Srgba:{…}}`) | 428 | 458 |
| JSON.stringify Color | 150 | 156 |
| JSON.parse Projection (Perspective) | 505 | 491 |
| JSON.stringify Projection | 244 | 178 |
| Generated decoder, Color | 24 | 22 |
| Generated encoder, Color | 11 | 11 |
| Generated decoder, Projection | 8 | 8 |
| Generated encoder, Projection | 5 | 4 |
| Tick cache hit (tick compare, cached object) | 11 | 9 |
| Layout-tier column, 4 lanes (baseline) | 7 | 12 |

On the JS side alone, a generated decoder is 20 to 60 times faster than `JSON.parse`. Its
output is the same plain object, with a stable hidden class.

## Options

Cost per row covers a read of one reflect-tier value: crossing, Rust, then JS. Rust figures are
estimates. Stage 3's bench (task 3.7) measures them.

| Option | Read, per row | Covers | Complexity | Changes stage 1/2 |
|---|---|---|---|---|
| A. Current plan: lazy host call, JSON, cache per run | about 35 crossing + 300–600 Rust (`serde_json::Value`, `to_string`, V8 string) + 450 parse ≈ **0.8–1.1 µs** | Everything `read` covers | Low | — |
| B. Lazy host call, binary codec compiled from `TypeInfo` (flecs-style ops). JS decoder generated from the same ops at load | 35 + 30–80 Rust reflect walk + 10–25 ≈ **80–140 ns** | Structs, tuples, arrays, enums, `Option`, math types, `Duration`, wide ints. Lists, maps and strings are length-prefixed (`TextDecoder` for strings). `Handle` as a path string. Opaque serde types as an embedded JSON op | Medium: one op compiler in `reflect_value`, one JS generator | None |
| C. Tick cache across runs. Expose the changed-tick column (already a zero-copy view for layout-tier writes) for reflect-tier terms. Cache entity → (tick, object) | **≈10 ns on a hit**. A miss costs A or B | Everything | Low. Invalidate on reload and despawn; a write refreshes the entry | None |
| D. Pack per table before the run (custom-relationships' pattern) | Rust encode for every matched row, touched or not: 30–80. No crossing | As B | Medium | None. It is the eager form decision 7 rejected |
| E. Zero-copy columns for "plain enums" (`Color`, `Val`): probe the tag and payload offsets, as fieldless enums are probed today | ≈ layout tier, 2–10 | Only enums whose every variant is plain data. Not `Projection` (`Custom` holds an `Arc`), `Handle`, `String` or `Vec` | High, and unsafe: Rust enum layout and niches are unspecified, and one JS write of a bad tag is UB | None. It contradicts the non-goal "holes as zero-copy columns" |
| F. Batch writes: queue `Mut` writes, one host call at the end of the run | Saves about 35 per written row | Everything | Low, but a write is no longer visible to Rust mid-run | None |

## Pick

C first, then B for the misses. Read-mostly data (cameras, lights, UI styles) then costs about
the same as the layout tier. A changed row costs about 0.1 µs instead of about 1 µs. D wastes
work on untouched rows. E trades a Bevy-wide gap for UB risk. F is small; add it only if a bench
shows write-heavy systems.

## What it changes

- Stage 1: nothing. JSON stays the shape for TSX, the protocol and `UNCOVERED`. B's decoder
  must produce exactly what `JSON.parse(read(v))` gives. The hole check (decision 3) can assert
  that for each type it covers.
- Stage 2: nothing. `OP_INSERT_REFLECT { entity, type index, json }` is internal between
  `core.js` and `commands.rs`. Both ship inside the engine, so a binary payload can replace JSON
  later without a compatibility step. Spawns and inserts are not a per-row-per-frame path.
- Stage 3: amend decision 7 and tasks 3.3 and 3.7. The cache outlives the run, keyed by the
  row's changed tick, which `systems.rs` exposes for reflect-tier terms as it does for
  layout-tier writes. A miss goes through a binary codec. Drop the non-goal "Binary encodings
  for reflected values", so stage 3 does not build a JSON path only to replace it.

## Sources

- bevy_mod_scripting: https://github.com/makspll/bevy_mod_scripting (`ReflectReference`, `WorldAccessGuard`, via DeepWiki)
- flecs meta and JSON: https://github.com/SanderMertens/flecs (`ecs_meta_op_t`, `flecs_json_ser_type_slice`, `flecs_script_eval_component`, via DeepWiki)
- PuerTS (Unity static wrappers, `BlittableCopy`; Unreal `FPropertyTranslator`): https://github.com/Tencent/puerts ; https://onejs.com/docs/tutorials/rainbow-bars
- Godot GDExtension `ptrcall` and Variant paths: https://github.com/godotengine/godot (`gdextension_interface`); https://docs.godotengine.org/en/4.x/tutorials/scripting/cpp/core_types.html
- V8 `JSON.stringify` fast path: https://v8.dev/blog/json-stringify
- Unity DOTS change filter and chunk layout: `docs/research/change-detection.md`
- Cocos shared memory, PuerTS engine split: `docs/research/js-runtime.md`
- Host call and per-table packing costs: `docs/research/custom-relationships.md` on branch custom-relationships (84c2e62)
