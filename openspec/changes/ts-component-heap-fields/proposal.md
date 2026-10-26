## Why

A Bevy component holds a `String` or a `Vec<T>`. A TS component cannot: every field is plain bytes at
a fixed offset, and `name: string` fails the load. Games need a name, a label or a short list on an
entity. Research: `openspec/research/ts-component-heap-fields.md`.

## What Changes

- A TS component field may be `string` (stage 1), then `T[]` with `T` a number kind, `bool` or
  `string` (stage 2). A default comes from the initialiser; `declare` gives `""` or `[]`.
- The value lives in the component's own bytes, as Bevy keeps it: a Rust `String` or `Vec<T>` in the
  column. A component with a heap field registers a drop fn and a deep clone fn.
- A component with no heap field is unchanged: no drop fn, copy clone, same layout, same cost.
- TS systems read a heap field through a host call, cached per row by its changed tick, as a
  layout-tier hole is today. A `Mut<C>` assigns the whole field (`c.name = "x"`, `c.tags = [...]`);
  a list reads as a frozen array. In-place list edits (`c.tags.push`) are a later stage.
- Every path that builds or reads TS component bytes handles heap fields: commands (spawn, insert,
  clone), TSX props, `.bsn` levels, plugin API, reload migration, saves, and the protocol's
  `get_components`, `mutate_components` and schemas.
- `string` and `T[]` stay refused in `Local`, TS resources, messages and events, with a message
  naming the follow-up.
- Protocol: `FieldType` gains `string` and `list`. A new protocol minor version.
- **BREAKING** for nothing that loads today: only declarations that fail now start to load.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-components`: `string` and `T[]` fields are allowed on components; registration adds a drop and
  clone fn for components with heap fields; heap fields read and write through the host in TS
  systems; reload keeps them by moving.
- `world-saves`: a saved list migrates by item type, not length.
- `remote-protocol`: schemas and field edits cover `string` and `list` fields.

## Impact

- `crates/bevy-ts`: `layout.rs`, `components.rs`, `value.rs` (new heap-aware encode and decode),
  a new `heap.rs`, `commands.rs`, `reflect_codec.rs` (a TS heap target), `systems.rs`, `saves.rs`,
  `generation.rs`, `plugin_api.rs`.
- `crates/bevy-ts-cli/src/reader.rs` (field types), `types.rs` if it emits field types.
- `crates/bevy-ts-v8/js/core.js` (accessors, encoder, insert ops); `packages/bevy-ts` `index.d.ts`.
- `crates/bevy-ts-remote` (`protocol.rs`, `values.rs`), `protocol/bevy-ts.openrpc.json`.
- `crates/bevy-ts-tsx`, `crates/bevy-ts-levels`: only through `value.rs`.
- New bench `bench/ts-heap-fields/` and its process in `.abyme/processes.ts`.
- Docs: `docs/ecs.md`, `docs/scripts.md`, `docs/reflection.md`, `docs/saves.md`,
  `docs/research/bevy-coverage.md`.
- No new dependency.
